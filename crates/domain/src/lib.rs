//! Versioned structural-domain contracts.
//!
//! This crate deliberately contains engineering data and validation rules, but no
//! geometry-kernel or FEM implementation. External formats must enter through
//! explicit adapters and migrations.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use structural_units::{
    Area, Force, Length, MassDensity, Moment, SecondMomentOfArea, Stiffness, Stress,
};
use uuid::Uuid;

pub const MODEL_SCHEMA_VERSION: &str = "0.3";
const MIGRATION_NAMESPACE: Uuid =
    Uuid::from_u128(0x6f214bb9_5c8f_4b38_945f_3650f4464cf1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub source_system: String,
    pub source_document: Option<String>,
    pub source_entity_id: Option<String>,
    pub source_revision: Option<String>,
    pub importer_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoordinateSystem {
    pub id: Uuid,
    pub name: String,
    pub origin_m: [Length; 3],
    /// Local unit axes expressed in global coordinates: [x_axis, y_axis, z_axis].
    pub axes: [[f64; 3]; 3],
    pub provenance: Option<Provenance>,
}

impl CoordinateSystem {
    pub fn global(id: Uuid) -> Self {
        Self {
            id,
            name: "Global".to_owned(),
            origin_m: [Length::ZERO; 3],
            axes: [
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            provenance: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: Uuid,
    pub name: Option<String>,
    /// SI metres on the wire; coordinates are expressed in `coordinate_system_id`.
    pub xyz_m: [Length; 3],
    /// `None` means the model's global coordinate system.
    pub coordinate_system_id: Option<Uuid>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    Steel,
    Concrete,
    Timber,
    Masonry,
    Aluminium,
    Composite,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Material {
    pub id: Uuid,
    pub name: String,
    pub kind: MaterialKind,
    pub elastic_modulus_pa: Stress,
    pub poisson_ratio: f64,
    pub mass_density_kg_per_m3: MassDensity,
    pub yield_strength_pa: Option<Stress>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectionProperties {
    pub area_m2: Area,
    pub iy_m4: SecondMomentOfArea,
    pub iz_m4: SecondMomentOfArea,
    pub torsion_constant_m4: Option<SecondMomentOfArea>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Section {
    pub id: Uuid,
    pub name: String,
    pub catalogue_reference: Option<String>,
    pub properties: SectionProperties,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub id: Uuid,
    pub name: Option<String>,
    pub start_node_id: Uuid,
    pub end_node_id: Uuid,
    pub material_id: Uuid,
    pub section_id: Uuid,
    pub local_coordinate_system_id: Option<Uuid>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Shell {
    pub id: Uuid,
    pub name: Option<String>,
    /// Ordered boundary nodes. Triangles and quadrilaterals are supported initially.
    pub node_ids: Vec<Uuid>,
    pub material_id: Uuid,
    pub thickness_m: Length,
    pub local_coordinate_system_id: Option<Uuid>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Solid {
    pub id: Uuid,
    pub name: Option<String>,
    /// Ordered topology is interpreted by the later meshing/element adapter.
    pub node_ids: Vec<Uuid>,
    pub material_id: Uuid,
    pub provenance: Option<Provenance>,
}

/// Temporary demonstrator element retained until a later solver-focused FEM pass replaces it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpringElement {
    pub id: Uuid,
    pub node_id: Uuid,
    pub stiffness_n_per_m: Stiffness,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranslationalRestraint {
    Free,
    Fixed,
    Spring,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranslationDof {
    pub restraint: TranslationalRestraint,
    pub spring_stiffness_n_per_m: Option<Stiffness>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Support {
    pub id: Uuid,
    pub node_id: Uuid,
    pub coordinate_system_id: Option<Uuid>,
    pub translations: [TranslationDof; 3],
    pub rotations_fixed: [bool; 3],
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoadCaseCategory {
    Permanent,
    Variable,
    Wind,
    Snow,
    Thermal,
    Seismic,
    Accidental,
    Construction,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodalLoad {
    pub id: Uuid,
    pub node_id: Uuid,
    pub coordinate_system_id: Option<Uuid>,
    pub force_n: [Force; 3],
    pub moment_nm: [Moment; 3],
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoadCase {
    pub id: Uuid,
    pub name: String,
    pub category: LoadCaseCategory,
    pub self_weight_factor: f64,
    pub nodal_loads: Vec<NodalLoad>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CombinationTerm {
    pub load_case_id: Uuid,
    pub factor: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoadCombination {
    pub id: Uuid,
    pub name: String,
    pub terms: Vec<CombinationTerm>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuralModel {
    pub model_id: Uuid,
    pub revision_id: Uuid,
    pub name: String,
    pub global_coordinate_system_id: Uuid,
    pub coordinate_systems: Vec<CoordinateSystem>,
    pub nodes: Vec<Node>,
    pub materials: Vec<Material>,
    pub sections: Vec<Section>,
    pub members: Vec<Member>,
    pub shells: Vec<Shell>,
    pub solids: Vec<Solid>,
    pub springs: Vec<SpringElement>,
    pub supports: Vec<Support>,
    pub load_cases: Vec<LoadCase>,
    pub load_combinations: Vec<LoadCombination>,
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleSetRef {
    pub authority: String,
    pub identifier: String,
    pub edition: String,
    pub national_annex: Option<String>,
    pub package_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisInput {
    pub schema_version: String,
    pub model: StructuralModel,
    /// Empty means all load cases. The demonstrator solver requires exactly one.
    pub selected_load_case_ids: Vec<Uuid>,
    pub rule_sets: Vec<RuleSetRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeResult {
    pub node_id: Uuid,
    pub displacement_m: Length,
    pub reaction_n: Force,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalysisResult {
    pub schema_version: String,
    pub solver_id: String,
    pub solver_version: String,
    pub warnings: Vec<String>,
    pub nodes: Vec<NodeResult>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelIssue {
    pub severity: IssueSeverity,
    pub code: String,
    pub message: String,
    pub entity_id: Option<Uuid>,
}

impl StructuralModel {
    pub fn validate(&self) -> Vec<ModelIssue> {
        let mut issues = Vec::new();
        let mut ids = std::collections::HashSet::new();

        macro_rules! register_ids {
            ($items:expr, $kind:literal) => {
                for item in $items {
                    if !ids.insert(item.id) {
                        issues.push(error(
                            "duplicate_id",
                            format!("duplicate {} identifier {}", $kind, item.id),
                            Some(item.id),
                        ));
                    }
                }
            };
        }

        register_ids!(&self.coordinate_systems, "coordinate-system");
        register_ids!(&self.nodes, "node");
        register_ids!(&self.materials, "material");
        register_ids!(&self.sections, "section");
        register_ids!(&self.members, "member");
        register_ids!(&self.shells, "shell");
        register_ids!(&self.solids, "solid");
        register_ids!(&self.springs, "spring");
        register_ids!(&self.supports, "support");
        register_ids!(&self.load_cases, "load-case");
        register_ids!(&self.load_combinations, "load-combination");

        let node_ids = id_set(self.nodes.iter().map(|x| x.id));
        let material_ids = id_set(self.materials.iter().map(|x| x.id));
        let section_ids = id_set(self.sections.iter().map(|x| x.id));
        let cs_ids = id_set(self.coordinate_systems.iter().map(|x| x.id));
        let load_case_ids = id_set(self.load_cases.iter().map(|x| x.id));

        if !cs_ids.contains(&self.global_coordinate_system_id) {
            issues.push(error(
                "missing_global_coordinate_system",
                "global coordinate-system reference does not exist".to_owned(),
                Some(self.global_coordinate_system_id),
            ));
        }

        for cs in &self.coordinate_systems {
            if !coordinate_axes_are_orthonormal(cs.axes, 1.0e-9) {
                issues.push(error(
                    "invalid_coordinate_system_axes",
                    format!("coordinate system '{}' is not right-handed orthonormal", cs.name),
                    Some(cs.id),
                ));
            }
        }

        for node in &self.nodes {
            validate_optional_ref(
                &mut issues,
                node.coordinate_system_id,
                &cs_ids,
                "missing_coordinate_system",
                node.id,
            );
            if node.xyz_m.iter().any(|v| !v.is_finite()) {
                issues.push(error(
                    "non_finite_node_coordinate",
                    "node contains a non-finite coordinate".to_owned(),
                    Some(node.id),
                ));
            }
        }

        for material in &self.materials {
            if !material.elastic_modulus_pa.is_finite()
                || material.elastic_modulus_pa.pascals() <= 0.0
            {
                issues.push(error(
                    "invalid_elastic_modulus",
                    "elastic modulus must be positive and finite".to_owned(),
                    Some(material.id),
                ));
            }
            if !(-1.0..0.5).contains(&material.poisson_ratio) {
                issues.push(error(
                    "invalid_poisson_ratio",
                    "Poisson ratio must satisfy -1 < ν < 0.5".to_owned(),
                    Some(material.id),
                ));
            }
            if !material.mass_density_kg_per_m3.is_finite()
                || material.mass_density_kg_per_m3.kilograms_per_cubic_metre() <= 0.0
            {
                issues.push(error(
                    "invalid_mass_density",
                    "mass density must be positive and finite".to_owned(),
                    Some(material.id),
                ));
            }
        }

        for section in &self.sections {
            if section.properties.area_m2.square_metres() <= 0.0
                || !section.properties.area_m2.is_finite()
            {
                issues.push(error(
                    "invalid_section_area",
                    "section area must be positive and finite".to_owned(),
                    Some(section.id),
                ));
            }
        }

        for member in &self.members {
            require_ref(&mut issues, member.start_node_id, &node_ids, "missing_node", member.id);
            require_ref(&mut issues, member.end_node_id, &node_ids, "missing_node", member.id);
            require_ref(
                &mut issues,
                member.material_id,
                &material_ids,
                "missing_material",
                member.id,
            );
            require_ref(
                &mut issues,
                member.section_id,
                &section_ids,
                "missing_section",
                member.id,
            );
            validate_optional_ref(
                &mut issues,
                member.local_coordinate_system_id,
                &cs_ids,
                "missing_coordinate_system",
                member.id,
            );
            if member.start_node_id == member.end_node_id {
                issues.push(error(
                    "zero_length_member_topology",
                    "member start and end node must differ".to_owned(),
                    Some(member.id),
                ));
            }
        }

        for shell in &self.shells {
            if !(3..=4).contains(&shell.node_ids.len()) {
                issues.push(error(
                    "unsupported_shell_topology",
                    "shell must currently contain three or four ordered nodes".to_owned(),
                    Some(shell.id),
                ));
            }
            for node_id in &shell.node_ids {
                require_ref(&mut issues, *node_id, &node_ids, "missing_node", shell.id);
            }
            require_ref(
                &mut issues,
                shell.material_id,
                &material_ids,
                "missing_material",
                shell.id,
            );
            if !shell.thickness_m.is_finite() || shell.thickness_m.metres() <= 0.0 {
                issues.push(error(
                    "invalid_shell_thickness",
                    "shell thickness must be positive and finite".to_owned(),
                    Some(shell.id),
                ));
            }
        }

        for solid in &self.solids {
            if solid.node_ids.len() < 4 {
                issues.push(error(
                    "unsupported_solid_topology",
                    "solid must contain at least four ordered nodes".to_owned(),
                    Some(solid.id),
                ));
            }
            for node_id in &solid.node_ids {
                require_ref(&mut issues, *node_id, &node_ids, "missing_node", solid.id);
            }
            require_ref(
                &mut issues,
                solid.material_id,
                &material_ids,
                "missing_material",
                solid.id,
            );
        }

        for spring in &self.springs {
            require_ref(&mut issues, spring.node_id, &node_ids, "missing_node", spring.id);
            if !spring.stiffness_n_per_m.is_finite()
                || spring.stiffness_n_per_m.newtons_per_metre() <= 0.0
            {
                issues.push(error(
                    "invalid_spring_stiffness",
                    "spring stiffness must be positive and finite".to_owned(),
                    Some(spring.id),
                ));
            }
        }

        for support in &self.supports {
            require_ref(&mut issues, support.node_id, &node_ids, "missing_node", support.id);
            validate_optional_ref(
                &mut issues,
                support.coordinate_system_id,
                &cs_ids,
                "missing_coordinate_system",
                support.id,
            );
            for dof in &support.translations {
                match (dof.restraint, dof.spring_stiffness_n_per_m) {
                    (TranslationalRestraint::Spring, Some(k))
                        if k.is_finite() && k.newtons_per_metre() > 0.0 => {}
                    (TranslationalRestraint::Spring, _) => issues.push(error(
                        "missing_support_spring_stiffness",
                        "spring restraint requires positive finite stiffness".to_owned(),
                        Some(support.id),
                    )),
                    (_, Some(_)) => issues.push(warning(
                        "unused_support_spring_stiffness",
                        "stiffness is ignored unless restraint is 'spring'".to_owned(),
                        Some(support.id),
                    )),
                    _ => {}
                }
            }
        }

        for load_case in &self.load_cases {
            if !load_case.self_weight_factor.is_finite() {
                issues.push(error(
                    "invalid_self_weight_factor",
                    "self-weight factor must be finite".to_owned(),
                    Some(load_case.id),
                ));
            }
            for load in &load_case.nodal_loads {
                require_ref(&mut issues, load.node_id, &node_ids, "missing_node", load.id);
                validate_optional_ref(
                    &mut issues,
                    load.coordinate_system_id,
                    &cs_ids,
                    "missing_coordinate_system",
                    load.id,
                );
                if load.force_n.iter().any(|v| !v.is_finite())
                    || load.moment_nm.iter().any(|v| !v.is_finite())
                {
                    issues.push(error(
                        "non_finite_nodal_load",
                        "nodal load contains a non-finite component".to_owned(),
                        Some(load.id),
                    ));
                }
            }
        }

        for combination in &self.load_combinations {
            if combination.terms.is_empty() {
                issues.push(warning(
                    "empty_load_combination",
                    "load combination contains no terms".to_owned(),
                    Some(combination.id),
                ));
            }
            for term in &combination.terms {
                require_ref(
                    &mut issues,
                    term.load_case_id,
                    &load_case_ids,
                    "missing_load_case",
                    combination.id,
                );
                if !term.factor.is_finite() {
                    issues.push(error(
                        "invalid_combination_factor",
                        "load-combination factor must be finite".to_owned(),
                        Some(combination.id),
                    ));
                }
            }
        }

        issues
    }

    pub fn validate_or_error(&self) -> Result<()> {
        let errors: Vec<_> = self
            .validate()
            .into_iter()
            .filter(|x| x.severity == IssueSeverity::Error)
            .collect();
        if errors.is_empty() {
            Ok(())
        } else {
            bail!(
                "model validation failed: {}",
                errors
                    .iter()
                    .map(|x| format!("{}: {}", x.code, x.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        }
    }
}

impl AnalysisInput {
    pub fn validate_or_error(&self) -> Result<()> {
        if self.schema_version != MODEL_SCHEMA_VERSION {
            bail!(
                "analysis input must be migrated to schema {}, got {}",
                MODEL_SCHEMA_VERSION,
                self.schema_version
            );
        }
        self.model.validate_or_error()?;
        let known = id_set(self.model.load_cases.iter().map(|x| x.id));
        for id in &self.selected_load_case_ids {
            if !known.contains(id) {
                bail!("selected load case {} does not exist", id);
            }
        }
        Ok(())
    }
}

fn id_set(values: impl Iterator<Item = Uuid>) -> std::collections::HashSet<Uuid> {
    values.collect()
}

fn require_ref(
    issues: &mut Vec<ModelIssue>,
    referenced: Uuid,
    known: &std::collections::HashSet<Uuid>,
    code: &str,
    owner: Uuid,
) {
    if !known.contains(&referenced) {
        issues.push(error(
            code,
            format!("entity {} references missing identifier {}", owner, referenced),
            Some(owner),
        ));
    }
}

fn validate_optional_ref(
    issues: &mut Vec<ModelIssue>,
    referenced: Option<Uuid>,
    known: &std::collections::HashSet<Uuid>,
    code: &str,
    owner: Uuid,
) {
    if let Some(id) = referenced {
        require_ref(issues, id, known, code, owner);
    }
}

fn error(code: &str, message: String, entity_id: Option<Uuid>) -> ModelIssue {
    ModelIssue {
        severity: IssueSeverity::Error,
        code: code.to_owned(),
        message,
        entity_id,
    }
}

fn warning(code: &str, message: String, entity_id: Option<Uuid>) -> ModelIssue {
    ModelIssue {
        severity: IssueSeverity::Warning,
        code: code.to_owned(),
        message,
        entity_id,
    }
}

fn coordinate_axes_are_orthonormal(axes: [[f64; 3]; 3], tolerance: f64) -> bool {
    if axes.iter().flatten().any(|x| !x.is_finite()) {
        return false;
    }
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let unit = axes
        .iter()
        .all(|axis| (dot(*axis, *axis) - 1.0).abs() <= tolerance);
    let perpendicular = dot(axes[0], axes[1]).abs() <= tolerance
        && dot(axes[0], axes[2]).abs() <= tolerance
        && dot(axes[1], axes[2]).abs() <= tolerance;
    let handedness = {
        let xy = cross(axes[0], axes[1]);
        (dot(xy, axes[2]) - 1.0).abs() <= tolerance
    };
    unit && perpendicular && handedness
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationStep {
    pub from_schema: String,
    pub to_schema: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationOutcome {
    pub input: AnalysisInput,
    pub steps: Vec<MigrationStep>,
}

/// Reads current or legacy Pass 1/2 JSON and returns a current Pass 3 model.
/// Migration never silently rewrites the original artifact.
pub fn migrate_analysis_input_json(json: &str) -> Result<MigrationOutcome> {
    let value: serde_json::Value = serde_json::from_str(json).context("invalid JSON")?;
    let schema = value
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        .context("missing string schema_version")?
        .to_owned();

    match schema.as_str() {
        MODEL_SCHEMA_VERSION => {
            let input: AnalysisInput =
                serde_json::from_value(value).context("invalid Pass 3 analysis input")?;
            input.validate_or_error()?;
            Ok(MigrationOutcome {
                input,
                steps: vec![],
            })
        }
        "0.1" | "0.2" => migrate_legacy(value, &schema),
        other => bail!("unsupported model schema version {other}"),
    }
}

#[derive(Debug, Deserialize)]
struct LegacyInput {
    schema_version: String,
    model_id: Uuid,
    revision_id: Uuid,
    nodes: Vec<LegacyNode>,
    springs: Vec<LegacySpring>,
    loads: Vec<LegacyLoad>,
    rule_sets: Vec<RuleSetRef>,
}

#[derive(Debug, Deserialize)]
struct LegacyNode {
    id: Uuid,
    xyz_m: [Length; 3],
}

#[derive(Debug, Deserialize)]
struct LegacySpring {
    id: Uuid,
    node_id: Uuid,
    stiffness_n_per_m: Stiffness,
}

#[derive(Debug, Deserialize)]
struct LegacyLoad {
    node_id: Uuid,
    force_n: Force,
}

fn migrate_legacy(value: serde_json::Value, source_schema: &str) -> Result<MigrationOutcome> {
    let legacy: LegacyInput =
        serde_json::from_value(value).context("invalid legacy analysis input")?;
    if legacy.schema_version != source_schema {
        bail!("legacy schema marker changed during migration");
    }

    let global_cs_id = derived_id(legacy.model_id, "global-coordinate-system");
    let load_case_id = derived_id(legacy.model_id, "legacy-load-case");

    let input = AnalysisInput {
        schema_version: MODEL_SCHEMA_VERSION.to_owned(),
        model: StructuralModel {
            model_id: legacy.model_id,
            revision_id: legacy.revision_id,
            name: "Migrated legacy model".to_owned(),
            global_coordinate_system_id: global_cs_id,
            coordinate_systems: vec![CoordinateSystem::global(global_cs_id)],
            nodes: legacy
                .nodes
                .into_iter()
                .map(|node| Node {
                    id: node.id,
                    name: None,
                    xyz_m: node.xyz_m,
                    coordinate_system_id: None,
                    provenance: Some(Provenance {
                        source_system: format!("structural-platform-schema-{source_schema}"),
                        source_document: None,
                        source_entity_id: Some(node.id.to_string()),
                        source_revision: None,
                        importer_id: Some("structural-domain-migrator-0.3".to_owned()),
                    }),
                })
                .collect(),
            materials: vec![],
            sections: vec![],
            members: vec![],
            shells: vec![],
            solids: vec![],
            springs: legacy
                .springs
                .into_iter()
                .map(|spring| SpringElement {
                    id: spring.id,
                    node_id: spring.node_id,
                    stiffness_n_per_m: spring.stiffness_n_per_m,
                    provenance: None,
                })
                .collect(),
            supports: vec![],
            load_cases: vec![LoadCase {
                id: load_case_id,
                name: "Migrated legacy loads".to_owned(),
                category: LoadCaseCategory::Other,
                self_weight_factor: 0.0,
                nodal_loads: legacy
                    .loads
                    .into_iter()
                    .enumerate()
                    .map(|(index, load)| NodalLoad {
                        id: derived_id(
                            legacy.model_id,
                            &format!("legacy-load-{index}-{}", load.node_id),
                        ),
                        node_id: load.node_id,
                        coordinate_system_id: None,
                        // Legacy scalar demonstrator acted along its sole translational DOF.
                        force_n: [load.force_n, Force::ZERO, Force::ZERO],
                        moment_nm: [Moment::ZERO; 3],
                        provenance: None,
                    })
                    .collect(),
                provenance: None,
            }],
            load_combinations: vec![],
            provenance: Some(Provenance {
                source_system: format!("structural-platform-schema-{source_schema}"),
                source_document: None,
                source_entity_id: Some(legacy.model_id.to_string()),
                source_revision: Some(legacy.revision_id.to_string()),
                importer_id: Some("structural-domain-migrator-0.3".to_owned()),
            }),
        },
        selected_load_case_ids: vec![load_case_id],
        rule_sets: legacy.rule_sets,
    };

    input.validate_or_error()?;
    Ok(MigrationOutcome {
        input,
        steps: vec![MigrationStep {
            from_schema: source_schema.to_owned(),
            to_schema: MODEL_SCHEMA_VERSION.to_owned(),
            note: "Wrapped legacy nodes/springs/loads in the Pass 3 structural model; scalar loads map to global X.".to_owned(),
        }],
    })
}

fn derived_id(model_id: Uuid, purpose: &str) -> Uuid {
    Uuid::new_v5(&MIGRATION_NAMESPACE, format!("{model_id}:{purpose}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_model() -> StructuralModel {
        let cs = Uuid::new_v4();
        StructuralModel {
            model_id: Uuid::new_v4(),
            revision_id: Uuid::new_v4(),
            name: "Test".to_owned(),
            global_coordinate_system_id: cs,
            coordinate_systems: vec![CoordinateSystem::global(cs)],
            nodes: vec![],
            materials: vec![],
            sections: vec![],
            members: vec![],
            shells: vec![],
            solids: vec![],
            springs: vec![],
            supports: vec![],
            load_cases: vec![],
            load_combinations: vec![],
            provenance: None,
        }
    }

    #[test]
    fn global_coordinate_system_is_valid() {
        assert!(empty_model().validate().is_empty());
    }

    #[test]
    fn catches_dangling_member_references() {
        let mut model = empty_model();
        model.members.push(Member {
            id: Uuid::new_v4(),
            name: None,
            start_node_id: Uuid::new_v4(),
            end_node_id: Uuid::new_v4(),
            material_id: Uuid::new_v4(),
            section_id: Uuid::new_v4(),
            local_coordinate_system_id: None,
            provenance: None,
        });
        let issues = model.validate();
        assert!(issues.iter().any(|x| x.code == "missing_node"));
        assert!(issues.iter().any(|x| x.code == "missing_material"));
        assert!(issues.iter().any(|x| x.code == "missing_section"));
    }

    #[test]
    fn rejects_left_handed_coordinate_system() {
        let mut model = empty_model();
        model.coordinate_systems[0].axes[2] = [0.0, 0.0, -1.0];
        assert!(model
            .validate()
            .iter()
            .any(|x| x.code == "invalid_coordinate_system_axes"));
    }
}
