use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::{env, fs, path::PathBuf};
use structural_collaboration::{ModelSnapshot, ObjectChange, Repository};
use structural_assurance::{build_portal, AssuranceCatalog};
use structural_audit::{
    canonical_json_bytes, sha256_hex, ArtifactRef, ExecutionEnvironment, RunManifest,
};
use structural_domain::{
    AnalysisInput, CoordinateSystem, LoadCase, LoadCaseCategory, Material, MaterialKind,
    Member, NodalLoad, Node, Provenance, RuleSetRef, Section, SectionProperties,
    SpringElement, StructuralModel, MODEL_SCHEMA_VERSION,
};
use structural_execution::{
    compare_json, ArtifactStore, ComparisonProfile, ContentArtifact, ExecutionEnvelope,
    ResourceRequest, RetryPolicy, ToolchainIdentity, EXECUTION_SCHEMA_VERSION,
};
use structural_geometry_api::*;
use structural_geometry_simple::SimpleGeometryKernel;
use structural_ifc_adapter::IfcAdapter;
use structural_step_adapter::StepAdapter;
use structural_mesh_api::{
    GeneralMeshGenerator, GeneralMeshOptions, MeshTarget, SurfaceMeshGenerator,
    SurfaceMeshOptions, TetrahedralizationOptions, VolumeMeshGenerator, MESH_SCHEMA_VERSION,
};
use structural_mesh_simple::SimpleSurfaceMesher;
use structural_mesh_tet::ReferenceTetMesher;
use structural_rules_api::{RuleEvaluationInput, RulePackage};
use structural_rules_engine::evaluate as evaluate_rules;
use structural_release::{verify_release, ReleaseCandidate, ReleaseTrustStore};
use structural_solver_api::{
    BucklingAnalysisInput, DynamicStructuralSolver, EigenExecutionOptions, ExecutionOptions,
    LinearDynamicSystem, NonlinearAnalysisInput, NonlinearExecutionOptions,
    NonlinearRestartPoint, NonlinearStructuralSolver, StructuralSolver,
    TimeHistoryAnalysisInput, TimeHistoryExecutionOptions,
};
use structural_solver_linear::LinearSpringSolver;
use structural_solver_nonlinear::ReferenceNonlinearSpringSolver;
use structural_solver_dynamics::ReferenceDynamicSolver;
use structural_units::{
    Area, Force, Length, MassDensity, Moment, SecondMomentOfArea, Stiffness, Stress,
};
use uuid::Uuid;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_, command, output] if command == "demo-run" => demo_run(output.into()),
        [_, command, output] if command == "geometry-demo" => geometry_demo(output.into()),
        [_, command, input, output] if command == "ifc-import" => {
            ifc_import(input.into(), output.into())
        },
        [_, command, input, output] if command == "step-import" => {
            step_import(input.into(), output.into())
        },
        [_, command, input, output] if command == "mesh-generate" => {
            mesh_generate(input.into(), output.into())
        },
        [_, command, input, target, output] if command == "mesh-generate-general" => {
            mesh_generate_general(input.into(), target, output.into())
        },
        [_, command, input, output] if command == "tet-generate" => {
            tet_generate(input.into(), output.into())
        },
        [_, command, package, input, output] if command == "rules-evaluate" => {
            rules_evaluate(package.into(), input.into(), output.into())
        },
        [_, command, input, options, output] if command == "modal-run" => {
            modal_run(input.into(), options.into(), output.into())
        },
        [_, command, input, options, output] if command == "buckling-run" => {
            buckling_run(input.into(), options.into(), output.into())
        },
        [_, command, input, options, output] if command == "time-history-run" => {
            time_history_run(input.into(), options.into(), output.into())
        },
        [_, command, operation, input, output] if command == "execution-package" => {
            execution_package(operation, input.into(), output.into())
        },
        [_, command, local, remote, profile, output] if command == "execution-compare" => {
            execution_compare(local.into(), remote.into(), profile.into(), output.into())
        },
        [_, command, store, digest, destination] if command == "execution-artifact-get" => {
            execution_artifact_get(store.into(), digest, destination.into())
        },
        [_, command, snapshot, owner, output] if command == "collab-init" => {
            collab_init(snapshot.into(), owner, output.into())
        },
        [_, command, repository, branch, actor, changes, message, output]
            if command == "collab-commit" =>
        {
            collab_commit(
                repository.into(),
                branch,
                actor,
                changes.into(),
                message,
                output.into(),
            )
        },
        [_, command, repository, actor, known, output] if command == "collab-sync-export" => {
            collab_sync_export(
                repository.into(),
                actor,
                known.into(),
                output.into(),
            )
        },
        [_, command, repository, actor, bundle, output] if command == "collab-sync-import" => {
            collab_sync_import(
                repository.into(),
                actor,
                bundle.into(),
                output.into(),
            )
        },
        [_, command, catalog, source, output] if command == "assurance-build" => {
            assurance_build(catalog.into(), source.into(), output.into())
        },
        [_, command, candidate, source, trust, output] if command == "release-verify" => {
            release_verify(candidate.into(), source.into(), trust.into(), output.into())
        },
        [_, command, input, options, output] if command == "nonlinear-run" => {
            nonlinear_run(input.into(), options.into(), None, output.into())
        },
        [_, command, input, options, restart, output] if command == "nonlinear-resume" => {
            nonlinear_run(input.into(), options.into(), Some(restart.into()), output.into())
        },
        _ => {
            eprintln!(
                "Usage:\n  structural-cli demo-run <output-directory>\n  structural-cli geometry-demo <output-directory>\n  structural-cli ifc-import <input.ifc> <output-directory>\n  structural-cli step-import <input.stp> <output-directory>\n  structural-cli mesh-generate <geometry.json> <output-directory>\n  structural-cli mesh-generate-general <geometry.json> <line|surface|volume> <output-directory>\n  structural-cli tet-generate <geometry.json> <output-directory>\n  structural-cli rules-evaluate <package.json> <evaluation-input.json> <output-directory>\n  structural-cli modal-run <system.json> <options.json> <output-directory>\n  structural-cli buckling-run <input.json> <options.json> <output-directory>\n  structural-cli time-history-run <input.json> <options.json> <output-directory>\n  structural-cli execution-package <operation> <input.json> <output-directory>\n  structural-cli execution-compare <local.json> <cloud.json> <profile.json> <output.json>\n  structural-cli execution-artifact-get <store-directory> <sha256> <destination>\n  structural-cli collab-init <snapshot.json> <owner> <repository.json>\n  structural-cli collab-commit <repository.json> <branch> <actor> <changes.json> <message> <output.json>\n  structural-cli collab-sync-export <repository.json> <actor> <known-revisions.json> <bundle.json>\n  structural-cli collab-sync-import <repository.json> <actor> <bundle.json> <output.json>\n  structural-cli assurance-build <catalog.json> <source-directory> <portal-directory>\n  structural-cli release-verify <candidate.json> <source-directory> <trust-store.json> <report.json>\n  structural-cli nonlinear-run <input.json> <options.json> <output-directory>\n  structural-cli nonlinear-resume <input.json> <options.json> <restart.json> <output-directory>"
            );
            bail!("invalid command line")
        }
    }
}

fn demo_run(output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;

    let cs_id = Uuid::new_v4();
    let fixed_node_id = Uuid::new_v4();
    let loaded_node_id = Uuid::new_v4();
    let material_id = Uuid::new_v4();
    let section_id = Uuid::new_v4();
    let load_case_id = Uuid::new_v4();

    let input = AnalysisInput {
        schema_version: MODEL_SCHEMA_VERSION.to_owned(),
        model: StructuralModel {
            model_id: Uuid::new_v4(),
            revision_id: Uuid::new_v4(),
            name: "Pass 20 structural-domain demonstration".to_owned(),
            global_coordinate_system_id: cs_id,
            coordinate_systems: vec![CoordinateSystem::global(cs_id)],
            nodes: vec![
                Node {
                    id: fixed_node_id,
                    name: Some("N1".to_owned()),
                    xyz_m: [Length::ZERO; 3],
                    coordinate_system_id: None,
                    provenance: None,
                },
                Node {
                    id: loaded_node_id,
                    name: Some("N2".to_owned()),
                    xyz_m: [Length::from_metres(5.0), Length::ZERO, Length::ZERO],
                    coordinate_system_id: None,
                    provenance: None,
                },
            ],
            materials: vec![Material {
                id: material_id,
                name: "Demonstration steel".to_owned(),
                kind: MaterialKind::Steel,
                elastic_modulus_pa: Stress::from_gigapascals(210.0),
                poisson_ratio: 0.3,
                mass_density_kg_per_m3: MassDensity::from_kilograms_per_cubic_metre(7_850.0),
                yield_strength_pa: Some(Stress::from_megapascals(355.0)),
                provenance: Some(Provenance {
                    source_system: "demo".to_owned(),
                    source_document: None,
                    source_entity_id: Some("material-1".to_owned()),
                    source_revision: Some("1".to_owned()),
                    importer_id: None,
                }),
            }],
            sections: vec![Section {
                id: section_id,
                name: "Generic demonstration section".to_owned(),
                catalogue_reference: None,
                properties: SectionProperties {
                    area_m2: Area::from_square_millimetres(5_000.0),
                    iy_m4: SecondMomentOfArea::from_millimetres_to_fourth(20.0e6),
                    iz_m4: SecondMomentOfArea::from_millimetres_to_fourth(8.0e6),
                    torsion_constant_m4: None,
                },
                provenance: None,
            }],
            members: vec![Member {
                id: Uuid::new_v4(),
                name: Some("M1".to_owned()),
                start_node_id: fixed_node_id,
                end_node_id: loaded_node_id,
                material_id,
                section_id,
                local_coordinate_system_id: None,
                provenance: None,
            }],
            shells: vec![],
            solids: vec![],
            // Temporary spring keeps the Pass 1/2 demonstrator solver operational.
            springs: vec![
                SpringElement {
                    id: Uuid::new_v4(),
                    node_id: fixed_node_id,
                    stiffness_n_per_m: Stiffness::from_newtons_per_metre(20_000_000.0),
                    provenance: None,
                },
                SpringElement {
                    id: Uuid::new_v4(),
                    node_id: loaded_node_id,
                    stiffness_n_per_m: Stiffness::from_newtons_per_metre(20_000_000.0),
                    provenance: None,
                },
            ],
            supports: vec![],
            load_cases: vec![LoadCase {
                id: load_case_id,
                name: "Demonstration load".to_owned(),
                category: LoadCaseCategory::Variable,
                self_weight_factor: 0.0,
                nodal_loads: vec![NodalLoad {
                    id: Uuid::new_v4(),
                    node_id: loaded_node_id,
                    coordinate_system_id: None,
                    force_n: [Force::from_kilonewtons(100.0), Force::ZERO, Force::ZERO],
                    moment_nm: [Moment::ZERO; 3],
                    provenance: None,
                }],
                provenance: None,
            }],
            load_combinations: vec![],
            provenance: None,
        },
        selected_load_case_ids: vec![load_case_id],
        rule_sets: vec![RuleSetRef {
            authority: "DEMO".to_owned(),
            identifier: "NO-DESIGN-RULES".to_owned(),
            edition: "0".to_owned(),
            national_annex: None,
            package_sha256: "not-applicable-in-pass-4".to_owned(),
        }],
    };

    input.validate_or_error()?;
    let options = ExecutionOptions::default();
    let solver = LinearSpringSolver;
    let result = solver.solve(&input, options)?;

    let input_bytes = canonical_json_bytes(&input)?;
    let result_bytes = canonical_json_bytes(&result)?;
    fs::write(output_dir.join("input.json"), pretty_json(&input)?)?;
    fs::write(output_dir.join("result.json"), pretty_json(&result)?)?;

    let manifest = RunManifest {
        manifest_schema_version: "0.17".to_owned(),
        run_id: Uuid::new_v4(),
        parent_run_id: None,
        created_at_utc: Utc::now(),
        input: ArtifactRef {
            media_type: "application/vnd.structural.input+json".to_owned(),
            sha256: sha256_hex(&input_bytes),
        },
        result: ArtifactRef {
            media_type: "application/vnd.structural.result+json".to_owned(),
            sha256: sha256_hex(&result_bytes),
        },
        solver_id: solver.id().to_owned(),
        solver_version: solver.version().to_owned(),
        solver_commit: option_env!("GIT_COMMIT").map(str::to_owned),
        relative_tolerance: options.relative_tolerance,
        deterministic_profile: options.deterministic_profile,
        random_seed: None,
        unit_system: "SI".to_owned(),
        presentation_rounding: None,
        environment: ExecutionEnvironment {
            operating_system: env::consts::OS.to_owned(),
            architecture: env::consts::ARCH.to_owned(),
            runtime: format!("rust/{}", env!("CARGO_PKG_VERSION")),
            hardware_summary: None,
            container_digest: None,
        },
        signature: None,
    };

    fs::write(output_dir.join("run-manifest.json"), pretty_json(&manifest)?)?;
    println!("Created auditable Pass 17 demo run in {}", output_dir.display());
    Ok(())
}


fn geometry_demo(output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;

    let provenance = GeometryProvenance {
        source_format: "native-demo".to_owned(),
        source_document: Some("generated-by-cli".to_owned()),
        source_entity_id: None,
        source_revision: Some("1".to_owned()),
        source_units: Some("m".to_owned()),
        adapter_id: "structural-cli".to_owned(),
        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
        source_sha256: None,
    };

    let v1 = Uuid::new_v4();
    let v2 = Uuid::new_v4();
    let v3 = Uuid::new_v4();
    let e1 = Uuid::new_v4();
    let e2 = Uuid::new_v4();
    let e3 = Uuid::new_v4();
    let wire_id = Uuid::new_v4();
    let face_id = Uuid::new_v4();
    let shell_id = Uuid::new_v4();

    let document = GeometryDocument {
        schema_version: GEOMETRY_SCHEMA_VERSION.to_owned(),
        document_id: Uuid::new_v4(),
        revision_id: Uuid::new_v4(),
        name: "Pass 4 planar geometry demonstration".to_owned(),
        tolerance: GeometryTolerance::engineering_default(),
        brep: BrepModel {
            vertices: vec![
                geometry_vertex(v1, 0.0, 0.0, provenance.clone()),
                geometry_vertex(v2, 5.0, 0.0, provenance.clone()),
                geometry_vertex(v3, 0.0, 3.0, provenance.clone()),
            ],
            edges: vec![
                geometry_edge(e1, v1, v2, provenance.clone()),
                geometry_edge(e2, v2, v3, provenance.clone()),
                geometry_edge(e3, v3, v1, provenance.clone()),
            ],
            wires: vec![Wire {
                id: wire_id,
                edges: vec![
                    OrientedEdge { edge_id: e1, reversed: false },
                    OrientedEdge { edge_id: e2, reversed: false },
                    OrientedEdge { edge_id: e3, reversed: false },
                ],
                closed: true,
                provenance: Some(provenance.clone()),
            }],
            faces: vec![Face {
                id: face_id,
                wire_ids: vec![wire_id],
                surface_kind: SurfaceKind::Plane,
                orientation_reversed: false,
                provenance: Some(provenance.clone()),
            }],
            shells: vec![Shell {
                id: shell_id,
                face_ids: vec![face_id],
                closed: false,
                provenance: Some(provenance.clone()),
            }],
            bodies: vec![Body {
                id: Uuid::new_v4(),
                name: Some("Triangular surface".to_owned()),
                shell_ids: vec![shell_id],
                is_solid: false,
                provenance: Some(provenance.clone()),
            }],
        },
        meshes: vec![],
        provenance: Some(provenance),
    };

    let kernel = SimpleGeometryKernel;
    let validation = kernel.validate(&document);
    if validation.has_errors() {
        bail!("generated geometry unexpectedly failed validation");
    }
    let healing = kernel.heal(&document, document.tolerance)?;
    let meshes = kernel.tessellate(
        &healing.document,
        TessellationOptions::engineering_default(),
    )?;

    fs::write(output_dir.join("geometry.json"), pretty_json(&document)?)?;
    fs::write(
        output_dir.join("geometry-validation.json"),
        pretty_json(&validation)?,
    )?;
    fs::write(
        output_dir.join("geometry-healing.json"),
        pretty_json(&healing.report)?,
    )?;
    fs::write(output_dir.join("geometry-meshes.json"), pretty_json(&meshes)?)?;

    let evidence = serde_json::json!({
        "schema_version": "0.1",
        "kernel_id": kernel.id(),
        "kernel_version": kernel.version(),
        "geometry_sha256": sha256_hex(&canonical_json_bytes(&document)?),
        "validation_sha256": sha256_hex(&canonical_json_bytes(&validation)?),
        "healing_report_sha256": sha256_hex(&canonical_json_bytes(&healing.report)?),
        "meshes_sha256": sha256_hex(&canonical_json_bytes(&meshes)?)
    });
    fs::write(
        output_dir.join("geometry-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;

    println!(
        "Created validated Pass 8 geometry artifacts in {}",
        output_dir.display()
    );
    Ok(())
}

fn geometry_vertex(
    id: Uuid,
    x: f64,
    y: f64,
    provenance: GeometryProvenance,
) -> Vertex {
    Vertex {
        id,
        point: Point3::from_metres(x, y, 0.0),
        tolerance_m: Length::ZERO,
        provenance: Some(provenance),
    }
}

fn geometry_edge(
    id: Uuid,
    start_vertex_id: Uuid,
    end_vertex_id: Uuid,
    provenance: GeometryProvenance,
) -> Edge {
    Edge {
        id,
        start_vertex_id,
        end_vertex_id,
        curve_kind: CurveKind::Line,
        provenance: Some(provenance),
    }
}


fn ifc_import(input_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let bytes = fs::read(&input_path)
        .with_context(|| format!("cannot read {}", input_path.display()))?;
    let source_name = input_path.file_name().and_then(|value| value.to_str());
    let adapter = IfcAdapter;
    let outcome = adapter.import(
        &bytes,
        source_name,
        GeometryImportOptions::default(),
    )?;

    fs::write(
        output_dir.join("ifc-geometry.json"),
        pretty_json(&outcome.document)?,
    )?;
    fs::write(
        output_dir.join("ifc-import-report.json"),
        pretty_json(&outcome.report)?,
    )?;
    fs::write(
        output_dir.join("ifc-entity-map.json"),
        pretty_json(&outcome.report.mappings)?,
    )?;

    let evidence = serde_json::json!({
        "schema_version": "0.1",
        "adapter_id": adapter.id(),
        "adapter_version": adapter.version(),
        "source_sha256": outcome.report.source_sha256.clone(),
        "geometry_sha256": sha256_hex(&canonical_json_bytes(&outcome.document)?),
        "report_sha256": sha256_hex(&canonical_json_bytes(&outcome.report)?),
        "has_errors": outcome.report.has_errors()
    });
    fs::write(
        output_dir.join("ifc-import-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;

    if outcome.report.has_errors() {
        bail!(
            "IFC import produced diagnostic errors; evidence was written to {}",
            output_dir.display()
        );
    }
    println!(
        "Imported IFC with deterministic traceability into {}",
        output_dir.display()
    );
    Ok(())
}


fn step_import(input_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let bytes = fs::read(&input_path)
        .with_context(|| format!("cannot read {}", input_path.display()))?;
    let source_name = input_path.file_name().and_then(|value| value.to_str());
    let adapter = StepAdapter;
    let outcome = adapter.import(
        &bytes,
        source_name,
        GeometryImportOptions::default(),
    )?;

    fs::write(
        output_dir.join("step-geometry.json"),
        pretty_json(&outcome.document)?,
    )?;
    fs::write(
        output_dir.join("step-import-report.json"),
        pretty_json(&outcome.report)?,
    )?;
    fs::write(
        output_dir.join("step-entity-map.json"),
        pretty_json(&outcome.report.mappings)?,
    )?;
    let evidence = serde_json::json!({
        "schema_version": "0.1",
        "adapter_id": adapter.id(),
        "adapter_version": adapter.version(),
        "source_sha256": outcome.report.source_sha256.clone(),
        "geometry_sha256": sha256_hex(&canonical_json_bytes(&outcome.document)?),
        "report_sha256": sha256_hex(&canonical_json_bytes(&outcome.report)?),
        "has_errors": outcome.report.has_errors()
    });
    fs::write(
        output_dir.join("step-import-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;

    if outcome.report.has_errors() {
        bail!(
            "STEP import produced diagnostic errors; evidence was written to {}",
            output_dir.display()
        );
    }
    println!(
        "Imported STEP with deterministic traceability into {}",
        output_dir.display()
    );
    Ok(())
}

fn mesh_generate(input_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input_bytes = fs::read(&input_path)
        .with_context(|| format!("cannot read {}", input_path.display()))?;
    let document: GeometryDocument = serde_json::from_slice(&input_bytes)
        .with_context(|| format!("cannot parse {}", input_path.display()))?;
    let generator = SimpleSurfaceMesher;
    let options = SurfaceMeshOptions::engineering_default();
    let outcome = generator.generate(&document, options)?;

    fs::write(
        output_dir.join("surface-meshes.json"),
        pretty_json(&outcome.meshes)?,
    )?;
    fs::write(
        output_dir.join("mesh-quality-report.json"),
        pretty_json(&outcome.report)?,
    )?;
    let evidence = serde_json::json!({
        "schema_version": "0.1",
        "generator_id": SurfaceMeshGenerator::id(&generator),
        "generator_version": SurfaceMeshGenerator::version(&generator),
        "source_geometry_sha256": sha256_hex(&canonical_json_bytes(&document)?),
        "meshes_sha256": sha256_hex(&canonical_json_bytes(&outcome.meshes)?),
        "quality_report_sha256": sha256_hex(&canonical_json_bytes(&outcome.report)?),
        "has_errors": outcome.report.has_errors()
    });
    fs::write(
        output_dir.join("mesh-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;

    if outcome.report.has_errors() {
        bail!(
            "mesh generation did not meet mandatory criteria; evidence was written to {}",
            output_dir.display()
        );
    }
    println!(
        "Generated and assessed deterministic surface meshes in {}",
        output_dir.display()
    );
    Ok(())
}


fn mesh_generate_general(
    input_path: PathBuf,
    target_name: &str,
    output_dir: PathBuf,
) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input_bytes = fs::read(&input_path)
        .with_context(|| format!("cannot read {}", input_path.display()))?;
    let document: GeometryDocument = serde_json::from_slice(&input_bytes)
        .with_context(|| format!("cannot parse {}", input_path.display()))?;
    let options = match target_name {
        "line" => GeneralMeshOptions::line_default(),
        "surface" => GeneralMeshOptions::surface_default(),
        "volume" => GeneralMeshOptions::volume_default(),
        other => bail!("unsupported mesh target {other}; expected line, surface, or volume"),
    };
    let generator = SimpleSurfaceMesher;
    let outcome = generator.generate_general(&document, options)?;

    fs::write(
        output_dir.join("analysis-meshes.json"),
        pretty_json(&outcome.meshes)?,
    )?;
    fs::write(
        output_dir.join("general-mesh-quality-report.json"),
        pretty_json(&outcome.report)?,
    )?;
    let evidence = serde_json::json!({
        "schema_version": MESH_SCHEMA_VERSION,
        "generator_id": GeneralMeshGenerator::id(&generator),
        "generator_version": GeneralMeshGenerator::version(&generator),
        "target": match options.target {
            MeshTarget::Line => "line",
            MeshTarget::Surface => "surface",
            MeshTarget::ExtrudedVolume => "extruded_volume",
        },
        "source_geometry_sha256": sha256_hex(&canonical_json_bytes(&document)?),
        "meshes_sha256": sha256_hex(&canonical_json_bytes(&outcome.meshes)?),
        "quality_report_sha256": sha256_hex(&canonical_json_bytes(&outcome.report)?),
        "has_errors": outcome.report.has_errors()
    });
    fs::write(
        output_dir.join("general-mesh-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;

    if outcome.report.has_errors() {
        bail!(
            "general mesh generation did not meet mandatory criteria; evidence was written to {}",
            output_dir.display()
        );
    }
    println!(
        "Generated {:?} analysis mesh elements in {}",
        options.target,
        output_dir.display()
    );
    Ok(())
}


fn tet_generate(input_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input_bytes = fs::read(&input_path)
        .with_context(|| format!("cannot read {}", input_path.display()))?;
    let document: GeometryDocument = serde_json::from_slice(&input_bytes)
        .with_context(|| format!("cannot parse {}", input_path.display()))?;
    let options = TetrahedralizationOptions::engineering_default();
    let generator = ReferenceTetMesher;
    let outcome = generator.tetrahedralize(&document, options.clone())?;

    fs::write(output_dir.join("tetrahedral-mesh.json"), pretty_json(&outcome.mesh)?)?;
    fs::write(
        output_dir.join("boundary-validation.json"),
        pretty_json(&outcome.boundary_report)?,
    )?;
    fs::write(
        output_dir.join("tetrahedral-quality-report.json"),
        pretty_json(&outcome.quality_report)?,
    )?;
    fs::write(
        output_dir.join("boundary-facet-map.json"),
        pretty_json(&outcome.boundary_mapping)?,
    )?;
    let evidence = serde_json::json!({
        "schema_version": "0.3",
        "backend": outcome.backend,
        "options": options,
        "source_geometry_sha256": sha256_hex(&canonical_json_bytes(&document)?),
        "mesh_sha256": sha256_hex(&canonical_json_bytes(&outcome.mesh)?),
        "boundary_report_sha256": sha256_hex(&canonical_json_bytes(&outcome.boundary_report)?),
        "quality_report_sha256": sha256_hex(&canonical_json_bytes(&outcome.quality_report)?),
        "boundary_mapping_sha256": sha256_hex(&canonical_json_bytes(&outcome.boundary_mapping)?),
        "has_errors": outcome.boundary_report.has_errors() || outcome.quality_report.has_errors(),
        "environment": {
            "operating_system": env::consts::OS,
            "architecture": env::consts::ARCH
        }
    });
    fs::write(
        output_dir.join("tetrahedral-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;

    if outcome.quality_report.has_errors() {
        bail!(
            "tetrahedralization did not meet mandatory criteria; evidence was written to {}",
            output_dir.display()
        );
    }
    println!(
        "Generated {} tetrahedra in {}",
        outcome.mesh.elements.len(),
        output_dir.display()
    );
    Ok(())
}


fn pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(value)?)
}


fn rules_evaluate(package_path: PathBuf, input_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let package: RulePackage = serde_json::from_slice(
        &fs::read(&package_path)
            .with_context(|| format!("cannot read {}", package_path.display()))?,
    )
    .with_context(|| format!("invalid rule package {}", package_path.display()))?;
    let input: RuleEvaluationInput = serde_json::from_slice(
        &fs::read(&input_path)
            .with_context(|| format!("cannot read {}", input_path.display()))?,
    )
    .with_context(|| format!("invalid rule input {}", input_path.display()))?;

    let (report, evidence) = evaluate_rules(&package, &input)?;
    fs::write(output_dir.join("rule-evaluation-report.json"), pretty_json(&report)?)?;
    fs::write(output_dir.join("rule-evaluation-evidence.json"), pretty_json(&evidence)?)?;
    fs::write(
        output_dir.join("rule-package-snapshot.json"),
        pretty_json(&package)?,
    )?;
    println!(
        "Evaluated {} checks: {} pass, {} fail, {} not applicable, {} blocked, {} error",
        report.checks.len(),
        report.pass_count,
        report.fail_count,
        report.not_applicable_count,
        report.blocked_count,
        report.error_count
    );
    Ok(())
}


fn modal_run(input_path: PathBuf, options_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input: LinearDynamicSystem = read_json(&input_path, "modal system")?;
    let options: EigenExecutionOptions = read_json(&options_path, "modal options")?;
    let solver = ReferenceDynamicSolver;
    let result = solver.solve_modal(&input, options)?;
    write_analysis_artifacts(
        &output_dir,
        "modal-result.json",
        "modal-evidence.json",
        &input,
        &options,
        &result,
        solver.id(),
        solver.version(),
    )?;
    println!("Extracted {} modes in {}", result.modes.len(), output_dir.display());
    Ok(())
}

fn buckling_run(input_path: PathBuf, options_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input: BucklingAnalysisInput = read_json(&input_path, "buckling input")?;
    let options: EigenExecutionOptions = read_json(&options_path, "buckling options")?;
    let solver = ReferenceDynamicSolver;
    let result = solver.solve_buckling(&input, options)?;
    write_analysis_artifacts(
        &output_dir,
        "buckling-result.json",
        "buckling-evidence.json",
        &input,
        &options,
        &result,
        solver.id(),
        solver.version(),
    )?;
    println!(
        "Extracted {} linearized buckling modes in {}",
        result.modes.len(),
        output_dir.display()
    );
    Ok(())
}

fn time_history_run(
    input_path: PathBuf,
    options_path: PathBuf,
    output_dir: PathBuf,
) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input: TimeHistoryAnalysisInput = read_json(&input_path, "time-history input")?;
    let options: TimeHistoryExecutionOptions =
        read_json(&options_path, "time-history options")?;
    let solver = ReferenceDynamicSolver;
    let result = solver.solve_time_history(&input, options)?;
    write_analysis_artifacts(
        &output_dir,
        "time-history-result.json",
        "time-history-evidence.json",
        &input,
        &options,
        &result,
        solver.id(),
        solver.version(),
    )?;
    println!(
        "Integrated {} time-history samples in {}",
        result.steps.len(),
        output_dir.display()
    );
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf, description: &str) -> Result<T> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
    )
    .with_context(|| format!("invalid {} {}", description, path.display()))
}

fn write_analysis_artifacts<I: serde::Serialize, O: serde::Serialize, R: serde::Serialize>(
    output_dir: &PathBuf,
    result_name: &str,
    evidence_name: &str,
    input: &I,
    options: &O,
    result: &R,
    solver_id: &str,
    solver_version: &str,
) -> Result<()> {
    fs::write(output_dir.join(result_name), pretty_json(result)?)?;
    let evidence = serde_json::json!({
        "schema_version": "0.1",
        "solver_id": solver_id,
        "solver_version": solver_version,
        "input_sha256": sha256_hex(&canonical_json_bytes(input)?),
        "options_sha256": sha256_hex(&canonical_json_bytes(options)?),
        "result_sha256": sha256_hex(&canonical_json_bytes(result)?)
    });
    fs::write(output_dir.join(evidence_name), pretty_json(&evidence)?)?;
    Ok(())
}

fn nonlinear_run(
    input_path: PathBuf,
    options_path: PathBuf,
    restart_path: Option<PathBuf>,
    output_dir: PathBuf,
) -> Result<()> {
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input: NonlinearAnalysisInput = serde_json::from_slice(
        &fs::read(&input_path)
            .with_context(|| format!("cannot read {}", input_path.display()))?,
    )
    .with_context(|| format!("invalid nonlinear input {}", input_path.display()))?;
    let options: NonlinearExecutionOptions = serde_json::from_slice(
        &fs::read(&options_path)
            .with_context(|| format!("cannot read {}", options_path.display()))?,
    )
    .with_context(|| format!("invalid nonlinear options {}", options_path.display()))?;
    let restart: Option<NonlinearRestartPoint> = restart_path
        .as_ref()
        .map(|path| {
            serde_json::from_slice(
                &fs::read(path).with_context(|| format!("cannot read {}", path.display()))?,
            )
            .with_context(|| format!("invalid nonlinear restart {}", path.display()))
        })
        .transpose()?;

    let solver = ReferenceNonlinearSpringSolver;
    let result = solver.solve_nonlinear(&input, options, restart.as_ref())?;

    fs::write(output_dir.join("nonlinear-result.json"), pretty_json(&result)?)?;
    fs::write(
        output_dir.join("nonlinear-diagnostics.json"),
        pretty_json(&result.diagnostics)?,
    )?;
    fs::write(
        output_dir.join("nonlinear-steps.json"),
        pretty_json(&result.steps)?,
    )?;
    if let Some(point) = result.restart_points.last() {
        fs::write(
            output_dir.join("nonlinear-restart.json"),
            pretty_json(point)?,
        )?;
    }

    let evidence = serde_json::json!({
        "schema_version": "0.1",
        "solver_id": solver.id(),
        "solver_version": solver.version(),
        "input_sha256": sha256_hex(&canonical_json_bytes(&input)?),
        "options_sha256": sha256_hex(&canonical_json_bytes(&options)?),
        "restart_input_sha256": restart
            .as_ref()
            .map(|point| canonical_json_bytes(point))
            .transpose()?
            .map(|bytes| sha256_hex(&bytes)),
        "result_sha256": sha256_hex(&canonical_json_bytes(&result)?),
        "termination": result.termination,
        "converged_load_factor": result.converged_load_factor,
        "deterministic_profile": options.deterministic_profile
    });
    fs::write(
        output_dir.join("nonlinear-evidence.json"),
        pretty_json(&evidence)?,
    )?;

    if !result.converged() {
        bail!(
            concat!(
                "nonlinear analysis stopped at load factor {} with {:?}; ",
                "diagnostics were written to {}"
            ),
            result.converged_load_factor,
            result.termination,
            output_dir.display()
        );
    }

    println!(
        "Completed {} nonlinear load steps in {}",
        result.steps.len(),
        output_dir.display()
    );
    Ok(())
}


fn execution_package(operation: &str, input_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    if operation.trim().is_empty() {
        bail!("execution operation must not be empty");
    }
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("cannot create {}", output_dir.display()))?;
    let input_bytes = fs::read(&input_path)
        .with_context(|| format!("cannot read {}", input_path.display()))?;
    // Require valid JSON now so cloud workers cannot disagree about malformed payloads.
    let _: serde_json::Value = serde_json::from_slice(&input_bytes)
        .with_context(|| format!("input is not valid JSON: {}", input_path.display()))?;

    let store = ArtifactStore::open(output_dir.join("artifacts"))?;
    let digest = store.upload(&input_bytes)?;
    let artifact = ContentArtifact {
        logical_name: input_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("input.json")
            .to_owned(),
        media_type: "application/json".to_owned(),
        sha256: digest,
        byte_length: input_bytes.len() as u64,
    };
    let target = option_env!("TARGET")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}-{}", env::consts::ARCH, env::consts::OS));
    let envelope = ExecutionEnvelope {
        schema_version: EXECUTION_SCHEMA_VERSION.to_owned(),
        job_id: Uuid::new_v4(),
        created_at_utc: Utc::now(),
        operation: operation.to_owned(),
        parameters: serde_json::json!({}),
        inputs: vec![artifact],
        expected_output_media_types: vec!["application/json".to_owned()],
        toolchain: ToolchainIdentity {
            application_id: "structural-cli".to_owned(),
            application_version: env!("CARGO_PKG_VERSION").to_owned(),
            source_commit: option_env!("GIT_COMMIT").map(str::to_owned),
            target_triple: target,
            compiler: option_env!("RUSTC_VERSION")
                .unwrap_or("rustc version not embedded")
                .to_owned(),
            dependency_lock_sha256: None,
            executable_sha256: env::current_exe()
                .ok()
                .and_then(|path| fs::read(path).ok())
                .map(|bytes| sha256_hex(&bytes)),
            container_digest: env::var("STRUCTURAL_CONTAINER_DIGEST").ok(),
        },
        resources: ResourceRequest {
            cpu_cores: 1,
            memory_bytes: 1024 * 1024 * 1024,
            maximum_runtime_seconds: 3600,
        },
        retry: RetryPolicy::default(),
        cancellation_id: Uuid::new_v4(),
        deterministic_profile: true,
        random_seed: None,
    };
    envelope.validate()?;
    fs::write(
        output_dir.join("execution-envelope.json"),
        pretty_json(&envelope)?,
    )?;
    let evidence = serde_json::json!({
        "schema_version": "structural-execution-package-evidence/1.0",
        "envelope_sha256": envelope.sha256()?,
        "artifact_store": "artifacts/sha256",
        "input_sha256": envelope.inputs[0].sha256,
        "input_byte_length": envelope.inputs[0].byte_length
    });
    fs::write(
        output_dir.join("execution-package-evidence.json"),
        pretty_json(&evidence)?,
    )?;
    println!(
        "Packaged execution job {} in {}",
        envelope.job_id,
        output_dir.display()
    );
    Ok(())
}

fn execution_compare(
    local_path: PathBuf,
    remote_path: PathBuf,
    profile_path: PathBuf,
    output_path: PathBuf,
) -> Result<()> {
    let local: serde_json::Value = serde_json::from_slice(
        &fs::read(&local_path)
            .with_context(|| format!("cannot read {}", local_path.display()))?,
    )
    .with_context(|| format!("invalid JSON {}", local_path.display()))?;
    let remote: serde_json::Value = serde_json::from_slice(
        &fs::read(&remote_path)
            .with_context(|| format!("cannot read {}", remote_path.display()))?,
    )
    .with_context(|| format!("invalid JSON {}", remote_path.display()))?;
    let profile: ComparisonProfile = serde_json::from_slice(
        &fs::read(&profile_path)
            .with_context(|| format!("cannot read {}", profile_path.display()))?,
    )
    .with_context(|| format!("invalid comparison profile {}", profile_path.display()))?;

    let report = compare_json(&local, &remote, profile)?;
    if let Some(parent) = output_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, pretty_json(&report)?)?;
    println!(
        "Execution parity: {} ({} differences), report {}",
        if report.equivalent { "equivalent" } else { "different" },
        report.differences.len(),
        output_path.display()
    );
    if !report.equivalent {
        bail!("local and cloud results differ outside the declared tolerance");
    }
    Ok(())
}

fn execution_artifact_get(store_path: PathBuf, digest: &str, destination: PathBuf) -> Result<()> {
    let store = ArtifactStore::open(store_path)?;
    let bytes = store.download(digest)?;
    if let Some(parent) = destination.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&destination, bytes)?;
    println!("Verified and downloaded artifact to {}", destination.display());
    Ok(())
}


fn collab_init(snapshot_path: PathBuf, owner: &str, output_path: PathBuf) -> Result<()> {
    let snapshot: ModelSnapshot = serde_json::from_slice(
        &fs::read(&snapshot_path)
            .with_context(|| format!("cannot read {}", snapshot_path.display()))?,
    )
    .with_context(|| format!("invalid collaboration snapshot {}", snapshot_path.display()))?;
    let repository = Repository::initialize(Uuid::new_v4(), owner, snapshot, Utc::now())?;
    repository.validate()?;
    if let Some(parent) = output_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, pretty_json(&repository)?)?;
    println!(
        "Initialized collaboration repository {} at {}",
        repository.repository_id,
        output_path.display()
    );
    Ok(())
}

fn collab_commit(
    repository_path: PathBuf,
    branch: &str,
    actor: &str,
    changes_path: PathBuf,
    message: &str,
    output_path: PathBuf,
) -> Result<()> {
    let mut repository: Repository = serde_json::from_slice(
        &fs::read(&repository_path)
            .with_context(|| format!("cannot read {}", repository_path.display()))?,
    )
    .with_context(|| format!("invalid repository {}", repository_path.display()))?;
    repository.validate()?;
    let changes: Vec<ObjectChange> = serde_json::from_slice(
        &fs::read(&changes_path)
            .with_context(|| format!("cannot read {}", changes_path.display()))?,
    )
    .with_context(|| format!("invalid object changes {}", changes_path.display()))?;
    let expected_head = repository.head(branch)?.revision_id.clone();
    let revision = repository.commit(
        actor,
        branch,
        &expected_head,
        &changes,
        message,
        Utc::now(),
    )?;
    repository.validate()?;
    if let Some(parent) = output_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, pretty_json(&repository)?)?;
    println!("Committed revision {revision} to branch '{branch}'");
    Ok(())
}

fn collab_sync_export(
    repository_path: PathBuf,
    actor: &str,
    known_path: PathBuf,
    output_path: PathBuf,
) -> Result<()> {
    let repository: Repository = serde_json::from_slice(
        &fs::read(&repository_path)
            .with_context(|| format!("cannot read {}", repository_path.display()))?,
    )
    .with_context(|| format!("invalid repository {}", repository_path.display()))?;
    repository.validate()?;
    let known: std::collections::BTreeSet<String> = serde_json::from_slice(
        &fs::read(&known_path)
            .with_context(|| format!("cannot read {}", known_path.display()))?,
    )
    .with_context(|| format!("invalid known revision list {}", known_path.display()))?;
    let bundle = repository.export_sync_bundle(actor, &known)?;
    if let Some(parent) = output_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, pretty_json(&bundle)?)?;
    println!(
        "Exported {} immutable revisions to {}",
        bundle.revisions.len(),
        output_path.display()
    );
    Ok(())
}

fn collab_sync_import(
    repository_path: PathBuf,
    actor: &str,
    bundle_path: PathBuf,
    output_path: PathBuf,
) -> Result<()> {
    let mut repository: Repository = serde_json::from_slice(
        &fs::read(&repository_path)
            .with_context(|| format!("cannot read {}", repository_path.display()))?,
    )
    .with_context(|| format!("invalid repository {}", repository_path.display()))?;
    repository.validate()?;
    let bundle = serde_json::from_slice(
        &fs::read(&bundle_path)
            .with_context(|| format!("cannot read {}", bundle_path.display()))?,
    )
    .with_context(|| format!("invalid sync bundle {}", bundle_path.display()))?;
    let report = repository.import_sync_bundle(actor, bundle)?;
    repository.validate()?;
    if let Some(parent) = output_path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, pretty_json(&repository)?)?;
    let report_path = output_path.with_extension("sync-report.json");
    fs::write(&report_path, pretty_json(&report)?)?;
    println!(
        "Imported {} revisions; {} branch conflicts; report {}",
        report.imported_revisions.len(),
        report.branch_conflicts.len(),
        report_path.display()
    );
    Ok(())
}


fn assurance_build(catalog_path: PathBuf, source_root: PathBuf, output_root: PathBuf) -> Result<()> {
    let catalog: AssuranceCatalog = serde_json::from_slice(
        &fs::read(&catalog_path)
            .with_context(|| format!("cannot read {}", catalog_path.display()))?,
    )
    .with_context(|| format!("invalid assurance catalog {}", catalog_path.display()))?;
    let manifest = build_portal(&catalog, &source_root, &output_root, Utc::now())?;
    println!(
        "Built assurance portal with {} verified artifacts at {} (bundle {})",
        manifest.artifacts.len(),
        output_root.display(),
        manifest.bundle_sha256
    );
    Ok(())
}


fn release_verify(
    candidate_path: PathBuf,
    source_directory: PathBuf,
    trust_path: PathBuf,
    output_path: PathBuf,
) -> Result<()> {
    let candidate: ReleaseCandidate = serde_json::from_slice(
        &fs::read(&candidate_path)
            .with_context(|| format!("cannot read {}", candidate_path.display()))?,
    )
    .with_context(|| format!("invalid release candidate {}", candidate_path.display()))?;
    let trust: ReleaseTrustStore = serde_json::from_slice(
        &fs::read(&trust_path)
            .with_context(|| format!("cannot read {}", trust_path.display()))?,
    )
    .with_context(|| format!("invalid release trust store {}", trust_path.display()))?;
    let report = verify_release(&candidate, &source_directory, &trust, Utc::now())?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::write(&output_path, pretty_json(&report)?)
        .with_context(|| format!("cannot write {}", output_path.display()))?;
    if !report.accepted {
        bail!(
            "release candidate failed {} gate(s); report written to {}",
            report.issues.len(),
            output_path.display()
        );
    }
    println!(
        "Verified release {} with {} artifacts and {} trusted signature(s)",
        report.version,
        report.verified_artifacts.len(),
        report.verified_signature_key_ids.len()
    );
    Ok(())
}
