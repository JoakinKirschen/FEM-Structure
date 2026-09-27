//! Deterministic reference surface mesher.
//!
//! The implementation is intentionally conservative: it tessellates supported
//! planar B-rep faces, then applies conforming uniform 1-to-4 triangle
//! refinement. It is a validation/reference implementation, not a production
//! tetrahedral or boundary-layer mesher.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use structural_geometry_api::{
    GeometryDocument, GeometryKernel, MeshVertex, Point3, TessellationOptions, Triangle,
    TriangleMesh,
};
use structural_geometry_simple::SimpleGeometryKernel;
use structural_mesh_api::*;
use uuid::Uuid;

const MESH_NAMESPACE: Uuid =
    Uuid::from_u128(0x8b0a8826_27ba_48c7_9c31_3be6cb2a973d);

#[derive(Debug, Default)]
pub struct SimpleSurfaceMesher;

impl SurfaceMeshGenerator for SimpleSurfaceMesher {
    fn id(&self) -> &'static str {
        "sweco.structural.mesh.simple"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn generate(
        &self,
        geometry: &GeometryDocument,
        options: SurfaceMeshOptions,
    ) -> Result<MeshGenerationOutcome> {
        if !options.is_valid() {
            bail!("invalid surface mesh options");
        }

        let kernel = SimpleGeometryKernel;
        let geometry_report = kernel.validate(geometry);
        if geometry_report.has_errors() {
            bail!("geometry contains errors and cannot be meshed");
        }

        let mut meshes = if geometry.meshes.is_empty() {
            kernel.tessellate(
                geometry,
                TessellationOptions {
                    chord_tolerance_m: geometry.tolerance.linear_m,
                    angular_tolerance_rad: geometry.tolerance.angular_rad,
                    maximum_edge_length_m: Some(options.maximum_edge_length_m),
                    deterministic: options.deterministic,
                },
            )?
        } else {
            geometry.meshes.clone()
        };

        if meshes.is_empty() {
            bail!("geometry contains no supported faces or source meshes");
        }

        let mut levels = 0u8;
        while levels < options.maximum_refinement_levels
            && maximum_edge(&meshes)? > options.maximum_edge_length_m.metres()
        {
            meshes = meshes
                .iter()
                .map(refine_uniform)
                .collect::<Result<Vec<_>>>()?;
            levels += 1;
        }

        let mut report = self.assess(&meshes, options);
        report.refinement_levels_used = levels;
        if maximum_edge(&meshes)? > options.maximum_edge_length_m.metres() {
            for mesh in &meshes {
                report.issues.push(MeshIssue {
                    severity: MeshIssueSeverity::Error,
                    code: "maximum_refinement_reached".to_owned(),
                    message: "maximum edge length was not reached within the refinement limit"
                        .to_owned(),
                    mesh_id: mesh.id,
                    triangle_id: None,
                    measured_value: report.statistics.maximum_edge_length_m,
                    required_value: Some(options.maximum_edge_length_m.metres()),
                });
            }
        }

        Ok(MeshGenerationOutcome { meshes, report })
    }

    fn assess(
        &self,
        meshes: &[TriangleMesh],
        options: SurfaceMeshOptions,
    ) -> MeshQualityReport {
        let mut issues = Vec::new();
        let mut edge_lengths = Vec::new();
        let mut angles = Vec::new();
        let mut aspects = Vec::new();
        let mut areas = Vec::new();
        let mut vertex_count = 0usize;
        let mut triangle_count = 0usize;

        for mesh in meshes {
            vertex_count += mesh.vertices.len();
            triangle_count += mesh.triangles.len();
            let points: HashMap<_, _> =
                mesh.vertices.iter().map(|vertex| (vertex.id, vertex.point)).collect();

            for triangle in &mesh.triangles {
                let Some(a) = points.get(&triangle.vertex_ids[0]).copied() else {
                    issues.push(missing_vertex(mesh.id, triangle.id));
                    continue;
                };
                let Some(b) = points.get(&triangle.vertex_ids[1]).copied() else {
                    issues.push(missing_vertex(mesh.id, triangle.id));
                    continue;
                };
                let Some(c) = points.get(&triangle.vertex_ids[2]).copied() else {
                    issues.push(missing_vertex(mesh.id, triangle.id));
                    continue;
                };
                let lengths = [
                    distance(a, b),
                    distance(b, c),
                    distance(c, a),
                ];
                let area = triangle_area(a, b, c);
                if !area.is_finite() || area <= f64::EPSILON {
                    issues.push(MeshIssue {
                        severity: MeshIssueSeverity::Error,
                        code: "degenerate_triangle".to_owned(),
                        message: "triangle has zero or non-finite area".to_owned(),
                        mesh_id: mesh.id,
                        triangle_id: Some(triangle.id),
                        measured_value: Some(area),
                        required_value: None,
                    });
                    continue;
                }

                let triangle_angles = angles_deg(lengths);
                let min_angle = triangle_angles
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min);
                let aspect = lengths.iter().map(|v| v * v).sum::<f64>()
                    / (4.0 * 3.0_f64.sqrt() * area);
                let max_edge = lengths.iter().copied().fold(0.0, f64::max);

                edge_lengths.extend(lengths);
                angles.extend(triangle_angles);
                aspects.push(aspect);
                areas.push(area);

                if max_edge > options.maximum_edge_length_m.metres() {
                    issues.push(MeshIssue {
                        severity: MeshIssueSeverity::Error,
                        code: "edge_too_long".to_owned(),
                        message: "triangle exceeds the configured maximum edge length".to_owned(),
                        mesh_id: mesh.id,
                        triangle_id: Some(triangle.id),
                        measured_value: Some(max_edge),
                        required_value: Some(options.maximum_edge_length_m.metres()),
                    });
                }
                if min_angle < options.minimum_angle_deg {
                    issues.push(MeshIssue {
                        severity: MeshIssueSeverity::Warning,
                        code: "minimum_angle_not_met".to_owned(),
                        message: "triangle is sharper than the configured quality threshold"
                            .to_owned(),
                        mesh_id: mesh.id,
                        triangle_id: Some(triangle.id),
                        measured_value: Some(min_angle),
                        required_value: Some(options.minimum_angle_deg),
                    });
                }
                if aspect > options.maximum_aspect_ratio {
                    issues.push(MeshIssue {
                        severity: MeshIssueSeverity::Warning,
                        code: "maximum_aspect_ratio_exceeded".to_owned(),
                        message: "triangle exceeds the configured aspect-ratio threshold"
                            .to_owned(),
                        mesh_id: mesh.id,
                        triangle_id: Some(triangle.id),
                        measured_value: Some(aspect),
                        required_value: Some(options.maximum_aspect_ratio),
                    });
                }
            }
        }

        MeshQualityReport {
            schema_version: MESH_SCHEMA_VERSION.to_owned(),
            generator_id: SurfaceMeshGenerator::id(self).to_owned(),
            generator_version: SurfaceMeshGenerator::version(self).to_owned(),
            options,
            refinement_levels_used: 0,
            statistics: MeshStatistics {
                mesh_count: meshes.len(),
                vertex_count,
                triangle_count,
                minimum_edge_length_m: finite_min(&edge_lengths),
                maximum_edge_length_m: finite_max(&edge_lengths),
                minimum_angle_deg: finite_min(&angles),
                maximum_aspect_ratio: finite_max(&aspects),
                minimum_area_m2: finite_min(&areas),
                maximum_area_m2: finite_max(&areas),
            },
            issues,
        }
    }
}

fn refine_uniform(mesh: &TriangleMesh) -> Result<TriangleMesh> {
    let source_points: HashMap<_, _> =
        mesh.vertices.iter().map(|vertex| (vertex.id, vertex.point)).collect();
    let mut vertices = mesh.vertices.clone();
    let mut midpoint_ids: HashMap<(Uuid, Uuid), Uuid> = HashMap::new();
    let mut triangles = Vec::with_capacity(mesh.triangles.len() * 4);

    for triangle in &mesh.triangles {
        let [a, b, c] = triangle.vertex_ids;
        let ab = midpoint(mesh.id, a, b, &source_points, &mut vertices, &mut midpoint_ids)?;
        let bc = midpoint(mesh.id, b, c, &source_points, &mut vertices, &mut midpoint_ids)?;
        let ca = midpoint(mesh.id, c, a, &source_points, &mut vertices, &mut midpoint_ids)?;
        for (index, ids) in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]
            .into_iter()
            .enumerate()
        {
            triangles.push(Triangle {
                id: stable_uuid(&format!("{}:{}:child:{}", mesh.id, triangle.id, index)),
                vertex_ids: ids,
                source_face_id: triangle.source_face_id,
            });
        }
    }

    Ok(TriangleMesh {
        id: mesh.id,
        vertices,
        triangles,
        provenance: mesh.provenance.clone(),
    })
}

fn midpoint(
    mesh_id: Uuid,
    first: Uuid,
    second: Uuid,
    points: &HashMap<Uuid, Point3>,
    vertices: &mut Vec<MeshVertex>,
    cache: &mut HashMap<(Uuid, Uuid), Uuid>,
) -> Result<Uuid> {
    let key = if first.as_bytes() <= second.as_bytes() {
        (first, second)
    } else {
        (second, first)
    };
    if let Some(id) = cache.get(&key) {
        return Ok(*id);
    }
    let a = points.get(&first).context("triangle references missing first vertex")?;
    let b = points.get(&second).context("triangle references missing second vertex")?;
    let id = stable_uuid(&format!("{}:midpoint:{}:{}", mesh_id, key.0, key.1));
    vertices.push(MeshVertex {
        id,
        point: Point3::from_metres(
            (a.xyz_m[0].metres() + b.xyz_m[0].metres()) * 0.5,
            (a.xyz_m[1].metres() + b.xyz_m[1].metres()) * 0.5,
            (a.xyz_m[2].metres() + b.xyz_m[2].metres()) * 0.5,
        ),
        source_vertex_id: None,
    });
    cache.insert(key, id);
    Ok(id)
}

fn maximum_edge(meshes: &[TriangleMesh]) -> Result<f64> {
    let mut maximum = 0.0_f64;
    for mesh in meshes {
        let points: HashMap<_, _> =
            mesh.vertices.iter().map(|vertex| (vertex.id, vertex.point)).collect();
        for triangle in &mesh.triangles {
            let a = points.get(&triangle.vertex_ids[0]).context("missing triangle vertex")?;
            let b = points.get(&triangle.vertex_ids[1]).context("missing triangle vertex")?;
            let c = points.get(&triangle.vertex_ids[2]).context("missing triangle vertex")?;
            maximum = maximum.max(distance(*a, *b));
            maximum = maximum.max(distance(*b, *c));
            maximum = maximum.max(distance(*c, *a));
        }
    }
    Ok(maximum)
}

fn distance(a: Point3, b: Point3) -> f64 {
    a.distance_to(b).metres()
}

fn triangle_area(a: Point3, b: Point3, c: Point3) -> f64 {
    let ab = [
        b.xyz_m[0].metres() - a.xyz_m[0].metres(),
        b.xyz_m[1].metres() - a.xyz_m[1].metres(),
        b.xyz_m[2].metres() - a.xyz_m[2].metres(),
    ];
    let ac = [
        c.xyz_m[0].metres() - a.xyz_m[0].metres(),
        c.xyz_m[1].metres() - a.xyz_m[1].metres(),
        c.xyz_m[2].metres() - a.xyz_m[2].metres(),
    ];
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt()
}

fn angles_deg(lengths: [f64; 3]) -> [f64; 3] {
    fn angle(opposite: f64, side_a: f64, side_b: f64) -> f64 {
        let denominator = 2.0 * side_a * side_b;
        if denominator <= f64::EPSILON {
            return 0.0;
        }
        let cosine =
            ((side_a * side_a + side_b * side_b - opposite * opposite) / denominator)
                .clamp(-1.0, 1.0);
        cosine.acos().to_degrees()
    }
    [
        angle(lengths[1], lengths[0], lengths[2]),
        angle(lengths[2], lengths[0], lengths[1]),
        angle(lengths[0], lengths[1], lengths[2]),
    ]
}

fn missing_vertex(mesh_id: Uuid, triangle_id: Uuid) -> MeshIssue {
    MeshIssue {
        severity: MeshIssueSeverity::Error,
        code: "missing_triangle_vertex".to_owned(),
        message: "triangle references a vertex not present in its mesh".to_owned(),
        mesh_id,
        triangle_id: Some(triangle_id),
        measured_value: None,
        required_value: None,
    }
}

fn finite_min(values: &[f64]) -> Option<f64> {
    values.iter().copied().filter(|v| v.is_finite()).reduce(f64::min)
}

fn finite_max(values: &[f64]) -> Option<f64> {
    values.iter().copied().filter(|v| v.is_finite()).reduce(f64::max)
}

fn stable_uuid(value: &str) -> Uuid {
    Uuid::new_v5(&MESH_NAMESPACE, value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use structural_geometry_api::{GeometryProvenance, Triangle};
    use structural_units::Length;

    fn coarse_mesh() -> TriangleMesh {
        let a = stable_uuid("a");
        let b = stable_uuid("b");
        let c = stable_uuid("c");
        TriangleMesh {
            id: stable_uuid("mesh"),
            vertices: vec![
                MeshVertex { id: a, point: Point3::from_metres(0.0, 0.0, 0.0), source_vertex_id: None },
                MeshVertex { id: b, point: Point3::from_metres(2.0, 0.0, 0.0), source_vertex_id: None },
                MeshVertex { id: c, point: Point3::from_metres(0.0, 2.0, 0.0), source_vertex_id: None },
            ],
            triangles: vec![Triangle {
                id: stable_uuid("triangle"),
                vertex_ids: [a, b, c],
                source_face_id: None,
            }],
            provenance: None::<GeometryProvenance>,
        }
    }

    #[test]
    fn uniform_refinement_is_conforming_and_deterministic() {
        let once = refine_uniform(&coarse_mesh()).unwrap();
        let twice = refine_uniform(&coarse_mesh()).unwrap();
        assert_eq!(once, twice);
        assert_eq!(once.vertices.len(), 6);
        assert_eq!(once.triangles.len(), 4);
    }

    #[test]
    fn generator_refines_until_edge_target_is_met() {
        use structural_geometry_api::{BrepModel, GeometryDocument, GeometryTolerance};

        let document = GeometryDocument {
            schema_version: structural_geometry_api::GEOMETRY_SCHEMA_VERSION.to_owned(),
            document_id: stable_uuid("document"),
            revision_id: stable_uuid("revision"),
            name: "coarse imported mesh".to_owned(),
            tolerance: GeometryTolerance::engineering_default(),
            brep: BrepModel {
                vertices: vec![],
                edges: vec![],
                wires: vec![],
                faces: vec![],
                shells: vec![],
                bodies: vec![],
            },
            meshes: vec![coarse_mesh()],
            provenance: None,
        };
        let mesher = SimpleSurfaceMesher;
        let outcome = mesher.generate(
            &document,
            SurfaceMeshOptions {
                maximum_edge_length_m: Length::from_metres(1.0),
                ..SurfaceMeshOptions::engineering_default()
            },
        ).unwrap();
        assert!(!outcome.report.has_errors());
        assert!(outcome.report.refinement_levels_used > 0);
        assert!(outcome.report.statistics.maximum_edge_length_m.unwrap() <= 1.0);
    }

    #[test]
    fn quality_report_detects_long_edges() {
        let mesher = SimpleSurfaceMesher;
        let report = mesher.assess(
            &[coarse_mesh()],
            SurfaceMeshOptions {
                maximum_edge_length_m: Length::from_metres(1.0),
                ..SurfaceMeshOptions::engineering_default()
            },
        );
        assert!(report.issues.iter().any(|issue| issue.code == "edge_too_long"));
    }
}


// ---- Pass 8 generalized line/surface/volume mesher -----------------------

impl GeneralMeshGenerator for SimpleSurfaceMesher {
    fn id(&self) -> &'static str {
        "sweco.structural.mesh.simple.general"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn generate_general(
        &self,
        geometry: &GeometryDocument,
        options: GeneralMeshOptions,
    ) -> Result<GeneralMeshGenerationOutcome> {
        if !options.is_valid() {
            bail!("invalid generalized mesh options");
        }
        let kernel = SimpleGeometryKernel;
        if kernel.validate(geometry).has_errors() {
            bail!("geometry contains errors and cannot be meshed");
        }

        let meshes = match options.target {
            MeshTarget::Line => vec![generate_line_mesh(geometry, options)?],
            MeshTarget::Surface => generate_surface_analysis_meshes(geometry, options)?,
            MeshTarget::ExtrudedVolume => {
                let extrusion = options.extrusion.context("volume target requires extrusion")?;
                let surface_options = GeneralMeshOptions {
                    target: MeshTarget::Surface,
                    extrusion: None,
                    ..options
                };
                generate_surface_analysis_meshes(geometry, surface_options)?
                    .iter()
                    .map(|mesh| extrude_mesh(mesh, extrusion))
                    .collect::<Result<Vec<_>>>()?
            }
        };
        if meshes.iter().all(|mesh| mesh.elements.is_empty()) {
            bail!("geometry contains no entities supported by the selected mesh target");
        }
        let report = self.assess_general(&meshes, options);
        Ok(GeneralMeshGenerationOutcome { meshes, report })
    }

    fn assess_general(
        &self,
        meshes: &[AnalysisMesh],
        options: GeneralMeshOptions,
    ) -> GeneralMeshQualityReport {
        let mut issues = Vec::new();
        let mut qualities = Vec::new();
        let mut counts = std::collections::BTreeMap::new();
        let mut all_min_edges = Vec::new();
        let mut all_max_edges = Vec::new();
        let mut measures = Vec::new();
        let mut aspects = Vec::new();
        let mut node_count = 0usize;

        for mesh in meshes {
            node_count += mesh.nodes.len();
            let points: HashMap<_, _> =
                mesh.nodes.iter().map(|node| (node.id, node.point)).collect();
            for element in &mesh.elements {
                *counts.entry(element.kind).or_insert(0) += 1;
                if element.node_ids.len() != element.kind.node_count() {
                    issues.push(general_issue(
                        MeshIssueSeverity::Error,
                        "invalid_element_node_count",
                        "element connectivity does not match its declared kind",
                        mesh.id,
                        Some(element.id),
                        Some(element.node_ids.len() as f64),
                        Some(element.kind.node_count() as f64),
                    ));
                    continue;
                }
                let mut element_points = Vec::with_capacity(element.node_ids.len());
                let mut missing = false;
                for node_id in &element.node_ids {
                    if let Some(point) = points.get(node_id) {
                        element_points.push(*point);
                    } else {
                        missing = true;
                    }
                }
                if missing {
                    issues.push(general_issue(
                        MeshIssueSeverity::Error,
                        "missing_element_node",
                        "element references a node not present in its mesh",
                        mesh.id,
                        Some(element.id),
                        None,
                        None,
                    ));
                    continue;
                }
                let edges = element_edge_lengths(element.kind, &element_points);
                let min_edge = edges.iter().copied().fold(f64::INFINITY, f64::min);
                let max_edge = edges.iter().copied().fold(0.0_f64, f64::max);
                let aspect = if min_edge > f64::EPSILON {
                    max_edge / min_edge
                } else {
                    f64::INFINITY
                };
                let (measure, unit) = element_measure(element.kind, &element_points);
                let quality = ElementQuality {
                    element_id: element.id,
                    kind: element.kind,
                    minimum_edge_length_m: min_edge,
                    maximum_edge_length_m: max_edge,
                    aspect_ratio: aspect,
                    measure,
                    measure_unit: unit.to_owned(),
                };
                qualities.push(quality);
                all_min_edges.push(min_edge);
                all_max_edges.push(max_edge);
                measures.push(measure);
                aspects.push(aspect);

                if !measure.is_finite() || measure <= f64::EPSILON {
                    issues.push(general_issue(
                        MeshIssueSeverity::Error,
                        "degenerate_element",
                        "element has zero or non-finite length, area, or volume",
                        mesh.id,
                        Some(element.id),
                        Some(measure),
                        None,
                    ));
                }
                if max_edge > options.maximum_edge_length_m.metres() * (1.0 + 1.0e-12) {
                    issues.push(general_issue(
                        MeshIssueSeverity::Error,
                        "edge_too_long",
                        "element exceeds the configured maximum edge length",
                        mesh.id,
                        Some(element.id),
                        Some(max_edge),
                        Some(options.maximum_edge_length_m.metres()),
                    ));
                }
                if aspect > options.maximum_aspect_ratio {
                    issues.push(general_issue(
                        MeshIssueSeverity::Warning,
                        "maximum_aspect_ratio_exceeded",
                        "element exceeds the configured edge aspect-ratio threshold",
                        mesh.id,
                        Some(element.id),
                        Some(aspect),
                        Some(options.maximum_aspect_ratio),
                    ));
                }
            }
        }

        GeneralMeshQualityReport {
            schema_version: MESH_SCHEMA_VERSION.to_owned(),
            generator_id: GeneralMeshGenerator::id(self).to_owned(),
            generator_version: GeneralMeshGenerator::version(self).to_owned(),
            options,
            statistics: GeneralMeshStatistics {
                mesh_count: meshes.len(),
                node_count,
                element_count: counts.values().sum(),
                element_counts: counts,
                minimum_edge_length_m: finite_min(&all_min_edges),
                maximum_edge_length_m: finite_max(&all_max_edges),
                minimum_measure: finite_min(&measures),
                maximum_measure: finite_max(&measures),
                maximum_aspect_ratio: finite_max(&aspects),
            },
            element_quality: qualities,
            issues,
        }
    }
}

fn generate_line_mesh(
    geometry: &GeometryDocument,
    options: GeneralMeshOptions,
) -> Result<AnalysisMesh> {
    let vertices: HashMap<_, _> = geometry
        .brep
        .vertices
        .iter()
        .map(|vertex| (vertex.id, vertex.point))
        .collect();
    let mut nodes = Vec::new();
    let mut node_points: HashMap<Uuid, Point3> = HashMap::new();
    let mut elements = Vec::new();

    for edge in &geometry.brep.edges {
        if edge.curve_kind != structural_geometry_api::CurveKind::Line {
            continue;
        }
        let start = *vertices.get(&edge.start_vertex_id).context("edge start vertex missing")?;
        let end = *vertices.get(&edge.end_vertex_id).context("edge end vertex missing")?;
        let length = distance(start, end);
        let segments = (length / options.maximum_edge_length_m.metres()).ceil().max(1.0) as usize;
        let mut previous = stable_uuid(&format!("line-vertex-node:{}", edge.start_vertex_id));
        insert_analysis_node(
            &mut nodes,
            &mut node_points,
            previous,
            start,
            Some(edge.start_vertex_id),
        );
        for index in 1..=segments {
            let t = index as f64 / segments as f64;
            let point = interpolate(start, end, t);
            let node_id = if index == segments {
                stable_uuid(&format!("line-vertex-node:{}", edge.end_vertex_id))
            } else {
                line_node_id(edge.id, index, segments)
            };
            insert_analysis_node(
                &mut nodes,
                &mut node_points,
                node_id,
                point,
                if index == segments { Some(edge.end_vertex_id) } else { None },
            );
            elements.push(AnalysisElement {
                id: stable_uuid(&format!("line-element:{}:{}", edge.id, index - 1)),
                kind: ElementKind::Line2,
                node_ids: vec![previous, node_id],
                source_geometry_id: Some(edge.id),
            });
            previous = node_id;
        }
    }

    Ok(AnalysisMesh {
        schema_version: MESH_SCHEMA_VERSION.to_owned(),
        id: stable_uuid(&format!("line-mesh:{}", geometry.document_id)),
        nodes,
        elements,
        provenance: geometry.provenance.clone(),
    })
}

fn line_node_id(edge_id: Uuid, index: usize, segments: usize) -> Uuid {
    stable_uuid(&format!("line-node:{}:{}:{}", edge_id, index, segments))
}

fn insert_analysis_node(
    nodes: &mut Vec<AnalysisMeshNode>,
    cache: &mut HashMap<Uuid, Point3>,
    id: Uuid,
    point: Point3,
    source_vertex_id: Option<Uuid>,
) {
    if cache.insert(id, point).is_none() {
        nodes.push(AnalysisMeshNode { id, point, source_vertex_id });
    }
}

fn interpolate(a: Point3, b: Point3, t: f64) -> Point3 {
    Point3::from_metres(
        a.xyz_m[0].metres() + (b.xyz_m[0].metres() - a.xyz_m[0].metres()) * t,
        a.xyz_m[1].metres() + (b.xyz_m[1].metres() - a.xyz_m[1].metres()) * t,
        a.xyz_m[2].metres() + (b.xyz_m[2].metres() - a.xyz_m[2].metres()) * t,
    )
}

fn generate_surface_analysis_meshes(
    geometry: &GeometryDocument,
    options: GeneralMeshOptions,
) -> Result<Vec<AnalysisMesh>> {
    let quad_mesh = if options.prefer_quadrilaterals {
        quadrilateral_faces(geometry)?
    } else {
        None
    };
    if let Some(mesh) = quad_mesh {
        if !mesh.elements.is_empty() && mesh.elements.len() == geometry.brep.faces.len() {
            return Ok(vec![mesh]);
        }
    }

    let legacy = SimpleSurfaceMesher;
    let outcome = SurfaceMeshGenerator::generate(
        &legacy,
        geometry,
        SurfaceMeshOptions {
            maximum_edge_length_m: options.maximum_edge_length_m,
            maximum_aspect_ratio: options.maximum_aspect_ratio,
            ..SurfaceMeshOptions::engineering_default()
        },
    )?;
    Ok(outcome.meshes.iter().map(triangle_mesh_to_analysis).collect())
}

fn quadrilateral_faces(geometry: &GeometryDocument) -> Result<Option<AnalysisMesh>> {
    if geometry.brep.faces.is_empty() {
        return Ok(None);
    }
    let vertices: HashMap<_, _> = geometry.brep.vertices.iter().map(|v| (v.id, v)).collect();
    let edges: HashMap<_, _> = geometry.brep.edges.iter().map(|e| (e.id, e)).collect();
    let wires: HashMap<_, _> = geometry.brep.wires.iter().map(|w| (w.id, w)).collect();
    let mut nodes = Vec::new();
    let mut seen = HashMap::new();
    let mut elements = Vec::new();

    for face in &geometry.brep.faces {
        if face.surface_kind != structural_geometry_api::SurfaceKind::Plane || face.wire_ids.len() != 1 {
            return Ok(None);
        }
        let wire = wires.get(&face.wire_ids[0]).context("face wire missing")?;
        if !wire.closed || wire.edges.len() != 4 {
            return Ok(None);
        }
        let mut corner_ids = Vec::new();
        for oriented in &wire.edges {
            let edge = edges.get(&oriented.edge_id).context("wire edge missing")?;
            if edge.curve_kind != structural_geometry_api::CurveKind::Line {
                return Ok(None);
            }
            corner_ids.push(if oriented.reversed {
                edge.end_vertex_id
            } else {
                edge.start_vertex_id
            });
        }
        if face.orientation_reversed {
            corner_ids.reverse();
        }
        let mut node_ids = Vec::new();
        for vertex_id in corner_ids {
            let vertex = vertices.get(&vertex_id).context("quad vertex missing")?;
            let node_id = stable_uuid(&format!("surface-node:{}", vertex_id));
            insert_analysis_node(&mut nodes, &mut seen, node_id, vertex.point, Some(vertex_id));
            node_ids.push(node_id);
        }
        elements.push(AnalysisElement {
            id: stable_uuid(&format!("quad-element:{}", face.id)),
            kind: ElementKind::Quadrilateral4,
            node_ids,
            source_geometry_id: Some(face.id),
        });
    }
    Ok(Some(AnalysisMesh {
        schema_version: MESH_SCHEMA_VERSION.to_owned(),
        id: stable_uuid(&format!("quad-mesh:{}", geometry.document_id)),
        nodes,
        elements,
        provenance: geometry.provenance.clone(),
    }))
}

fn triangle_mesh_to_analysis(mesh: &TriangleMesh) -> AnalysisMesh {
    AnalysisMesh {
        schema_version: MESH_SCHEMA_VERSION.to_owned(),
        id: mesh.id,
        nodes: mesh
            .vertices
            .iter()
            .map(|vertex| AnalysisMeshNode {
                id: vertex.id,
                point: vertex.point,
                source_vertex_id: vertex.source_vertex_id,
            })
            .collect(),
        elements: mesh
            .triangles
            .iter()
            .map(|triangle| AnalysisElement {
                id: triangle.id,
                kind: ElementKind::Triangle3,
                node_ids: triangle.vertex_ids.to_vec(),
                source_geometry_id: triangle.source_face_id,
            })
            .collect(),
        provenance: mesh.provenance.clone(),
    }
}

fn extrude_mesh(mesh: &AnalysisMesh, extrusion: ExtrusionOptions) -> Result<AnalysisMesh> {
    if !extrusion.is_valid() {
        bail!("invalid extrusion options");
    }
    let mut nodes = Vec::with_capacity(mesh.nodes.len() * (extrusion.layers as usize + 1));
    let mut layer_node_ids: HashMap<(Uuid, u16), Uuid> = HashMap::new();
    for layer in 0..=extrusion.layers {
        let t = layer as f64 / extrusion.layers as f64;
        for node in &mesh.nodes {
            let id = stable_uuid(&format!("extruded-node:{}:{}", node.id, layer));
            layer_node_ids.insert((node.id, layer), id);
            nodes.push(AnalysisMeshNode {
                id,
                point: Point3::from_metres(
                    node.point.xyz_m[0].metres() + extrusion.vector_m[0].metres() * t,
                    node.point.xyz_m[1].metres() + extrusion.vector_m[1].metres() * t,
                    node.point.xyz_m[2].metres() + extrusion.vector_m[2].metres() * t,
                ),
                source_vertex_id: node.source_vertex_id,
            });
        }
    }

    let mut elements = Vec::new();
    for layer in 0..extrusion.layers {
        for element in &mesh.elements {
            let kind = match element.kind {
                ElementKind::Triangle3 => ElementKind::Wedge6,
                ElementKind::Quadrilateral4 => ElementKind::Hexahedron8,
                _ => bail!("only triangular and quadrilateral surface elements can be extruded"),
            };
            let mut connectivity = Vec::with_capacity(kind.node_count());
            for node_id in &element.node_ids {
                connectivity.push(layer_node_ids[&(*node_id, layer)]);
            }
            for node_id in &element.node_ids {
                connectivity.push(layer_node_ids[&(*node_id, layer + 1)]);
            }
            elements.push(AnalysisElement {
                id: stable_uuid(&format!("extruded-element:{}:{}", element.id, layer)),
                kind,
                node_ids: connectivity,
                source_geometry_id: element.source_geometry_id,
            });
        }
    }

    Ok(AnalysisMesh {
        schema_version: MESH_SCHEMA_VERSION.to_owned(),
        id: stable_uuid(&format!("volume-mesh:{}", mesh.id)),
        nodes,
        elements,
        provenance: mesh.provenance.clone(),
    })
}

fn element_edge_lengths(kind: ElementKind, points: &[Point3]) -> Vec<f64> {
    let pairs: &[(usize, usize)] = match kind {
        ElementKind::Line2 => &[(0, 1)],
        ElementKind::Triangle3 => &[(0, 1), (1, 2), (2, 0)],
        ElementKind::Quadrilateral4 => &[(0, 1), (1, 2), (2, 3), (3, 0)],
        ElementKind::Tetrahedron4 => &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)],
        ElementKind::Pyramid5 => &[(0, 1), (1, 2), (2, 3), (3, 0), (0, 4), (1, 4), (2, 4), (3, 4)],
        ElementKind::Wedge6 => &[(0, 1), (1, 2), (2, 0), (3, 4), (4, 5), (5, 3), (0, 3), (1, 4), (2, 5)],
        ElementKind::Hexahedron8 => &[(0, 1), (1, 2), (2, 3), (3, 0), (4, 5), (5, 6), (6, 7), (7, 4), (0, 4), (1, 5), (2, 6), (3, 7)],
    };
    pairs.iter().map(|(a, b)| distance(points[*a], points[*b])).collect()
}

fn element_measure(kind: ElementKind, p: &[Point3]) -> (f64, &'static str) {
    match kind {
        ElementKind::Line2 => (distance(p[0], p[1]), "m"),
        ElementKind::Triangle3 => (triangle_area(p[0], p[1], p[2]), "m2"),
        ElementKind::Quadrilateral4 => (
            triangle_area(p[0], p[1], p[2]) + triangle_area(p[0], p[2], p[3]),
            "m2",
        ),
        ElementKind::Tetrahedron4 => (tetra_volume(p[0], p[1], p[2], p[3]), "m3"),
        ElementKind::Pyramid5 => (
            tetra_volume(p[0], p[1], p[2], p[4]) + tetra_volume(p[0], p[2], p[3], p[4]),
            "m3",
        ),
        ElementKind::Wedge6 => (
            tetra_volume(p[0], p[1], p[2], p[3])
                + tetra_volume(p[1], p[2], p[3], p[4])
                + tetra_volume(p[2], p[3], p[4], p[5]),
            "m3",
        ),
        ElementKind::Hexahedron8 => (
            tetra_volume(p[0], p[1], p[3], p[4])
                + tetra_volume(p[1], p[2], p[3], p[6])
                + tetra_volume(p[1], p[3], p[4], p[6])
                + tetra_volume(p[1], p[4], p[5], p[6])
                + tetra_volume(p[3], p[4], p[6], p[7]),
            "m3",
        ),
    }
}

fn tetra_volume(a: Point3, b: Point3, c: Point3, d: Point3) -> f64 {
    let ab = vector(a, b);
    let ac = vector(a, c);
    let ad = vector(a, d);
    ((ab[0] * (ac[1] * ad[2] - ac[2] * ad[1])
        - ab[1] * (ac[0] * ad[2] - ac[2] * ad[0])
        + ab[2] * (ac[0] * ad[1] - ac[1] * ad[0]))
        / 6.0)
        .abs()
}

fn vector(a: Point3, b: Point3) -> [f64; 3] {
    [
        b.xyz_m[0].metres() - a.xyz_m[0].metres(),
        b.xyz_m[1].metres() - a.xyz_m[1].metres(),
        b.xyz_m[2].metres() - a.xyz_m[2].metres(),
    ]
}

fn general_issue(
    severity: MeshIssueSeverity,
    code: &str,
    message: &str,
    mesh_id: Uuid,
    element_id: Option<Uuid>,
    measured_value: Option<f64>,
    required_value: Option<f64>,
) -> GeneralMeshIssue {
    GeneralMeshIssue {
        severity,
        code: code.to_owned(),
        message: message.to_owned(),
        mesh_id,
        element_id,
        measured_value,
        required_value,
    }
}

#[cfg(test)]
mod pass8_tests {
    use super::*;
    use structural_geometry_api::{
        Body, BrepModel, Edge, Face, GeometryDocument, GeometryTolerance, OrientedEdge, Shell,
        SurfaceKind, Vertex, Wire, GEOMETRY_SCHEMA_VERSION,
    };
    use structural_units::Length;

    fn quad_document() -> GeometryDocument {
        let ids = [
            stable_uuid("q0"),
            stable_uuid("q1"),
            stable_uuid("q2"),
            stable_uuid("q3"),
        ];
        let edge_ids = [
            stable_uuid("qe0"),
            stable_uuid("qe1"),
            stable_uuid("qe2"),
            stable_uuid("qe3"),
        ];
        let wire_id = stable_uuid("qwire");
        let face_id = stable_uuid("qface");
        let shell_id = stable_uuid("qshell");
        GeometryDocument {
            schema_version: GEOMETRY_SCHEMA_VERSION.to_owned(),
            document_id: stable_uuid("qdoc"),
            revision_id: stable_uuid("qrev"),
            name: "unit square".to_owned(),
            tolerance: GeometryTolerance::engineering_default(),
            brep: BrepModel {
                vertices: vec![
                    Vertex { id: ids[0], point: Point3::from_metres(0.0, 0.0, 0.0), tolerance_m: Length::ZERO, provenance: None },
                    Vertex { id: ids[1], point: Point3::from_metres(1.0, 0.0, 0.0), tolerance_m: Length::ZERO, provenance: None },
                    Vertex { id: ids[2], point: Point3::from_metres(1.0, 1.0, 0.0), tolerance_m: Length::ZERO, provenance: None },
                    Vertex { id: ids[3], point: Point3::from_metres(0.0, 1.0, 0.0), tolerance_m: Length::ZERO, provenance: None },
                ],
                edges: (0..4).map(|i| Edge {
                    id: edge_ids[i],
                    start_vertex_id: ids[i],
                    end_vertex_id: ids[(i + 1) % 4],
                    curve_kind: structural_geometry_api::CurveKind::Line,
                    provenance: None,
                }).collect(),
                wires: vec![Wire {
                    id: wire_id,
                    edges: edge_ids.iter().map(|id| OrientedEdge { edge_id: *id, reversed: false }).collect(),
                    closed: true,
                    provenance: None,
                }],
                faces: vec![Face {
                    id: face_id,
                    wire_ids: vec![wire_id],
                    surface_kind: SurfaceKind::Plane,
                    orientation_reversed: false,
                    provenance: None,
                }],
                shells: vec![Shell { id: shell_id, face_ids: vec![face_id], closed: false, provenance: None }],
                bodies: vec![Body { id: stable_uuid("qbody"), name: None, shell_ids: vec![shell_id], is_solid: false, provenance: None }],
            },
            meshes: vec![],
            provenance: None,
        }
    }

    #[test]
    fn creates_quadrilateral_surface_element() {
        let generator = SimpleSurfaceMesher;
        let outcome = generator
            .generate_general(&quad_document(), GeneralMeshOptions::surface_default())
            .unwrap();
        assert_eq!(outcome.meshes[0].elements.len(), 1);
        assert_eq!(outcome.meshes[0].elements[0].kind, ElementKind::Quadrilateral4);
        assert!(!outcome.report.has_errors());
    }

    #[test]
    fn extrudes_quadrilateral_to_hexahedron() {
        let generator = SimpleSurfaceMesher;
        let outcome = generator
            .generate_general(&quad_document(), GeneralMeshOptions::volume_default())
            .unwrap();
        assert_eq!(outcome.meshes[0].elements.len(), 1);
        assert_eq!(outcome.meshes[0].elements[0].kind, ElementKind::Hexahedron8);
        assert_eq!(outcome.meshes[0].nodes.len(), 8);
        assert!((outcome.report.element_quality[0].measure - 1.0).abs() < 1.0e-12);
        assert!(!outcome.report.has_errors());
    }

    #[test]
    fn creates_subdivided_line_elements() {
        let generator = SimpleSurfaceMesher;
        let mut options = GeneralMeshOptions::line_default();
        options.maximum_edge_length_m = Length::from_metres(0.4);
        let outcome = generator.generate_general(&quad_document(), options).unwrap();
        assert_eq!(outcome.meshes[0].elements.len(), 12);
        assert!(outcome.meshes[0].elements.iter().all(|e| e.kind == ElementKind::Line2));
        assert!(!outcome.report.has_errors());
    }

    #[test]
    fn generalized_schema_declares_all_common_first_order_topologies() {
        assert_eq!(ElementKind::Tetrahedron4.node_count(), 4);
        assert_eq!(ElementKind::Pyramid5.node_count(), 5);
        assert_eq!(ElementKind::Wedge6.node_count(), 6);
        assert_eq!(ElementKind::Hexahedron8.node_count(), 8);
    }
}
