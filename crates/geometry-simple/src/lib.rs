//! Deterministic reference geometry kernel.
//!
//! This is deliberately not a production CAD kernel. It proves the abstraction,
//! validation, healing-report and tessellation contracts without binding the
//! structural domain to OpenCascade, Parasolid, ACIS, or another vendor.

use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use structural_geometry_api::*;
use structural_units::Length;
use uuid::Uuid;

pub struct SimpleGeometryKernel;

impl GeometryKernel for SimpleGeometryKernel {
    fn id(&self) -> &'static str {
        "sweco.structural.geometry.simple"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn validate(&self, document: &GeometryDocument) -> GeometryValidationReport {
        let mut issues = Vec::new();
        if document.schema_version != GEOMETRY_SCHEMA_VERSION {
            issues.push(error(
                "unsupported_geometry_schema",
                format!(
                    "expected geometry schema {}, got {}",
                    GEOMETRY_SCHEMA_VERSION, document.schema_version
                ),
                None,
            ));
        }
        if !document.tolerance.is_valid() {
            issues.push(error(
                "invalid_geometry_tolerance",
                "geometry tolerance must contain positive finite values".to_owned(),
                None,
            ));
        }

        let mut all_ids = HashSet::new();
        macro_rules! register {
            ($items:expr, $kind:literal) => {
                for item in $items {
                    if !all_ids.insert(item.id) {
                        issues.push(error(
                            "duplicate_geometry_id",
                            format!("duplicate {} identifier {}", $kind, item.id),
                            Some(item.id),
                        ));
                    }
                }
            };
        }
        register!(&document.brep.vertices, "vertex");
        register!(&document.brep.edges, "edge");
        register!(&document.brep.wires, "wire");
        register!(&document.brep.faces, "face");
        register!(&document.brep.shells, "shell");
        register!(&document.brep.bodies, "body");
        register!(&document.meshes, "mesh");
        for mesh in &document.meshes {
            register!(&mesh.vertices, "mesh vertex");
            register!(&mesh.triangles, "triangle");
        }

        let vertices: HashMap<_, _> = document
            .brep
            .vertices
            .iter()
            .map(|item| (item.id, item))
            .collect();
        let edges: HashMap<_, _> = document
            .brep
            .edges
            .iter()
            .map(|item| (item.id, item))
            .collect();
        let wires: HashMap<_, _> = document
            .brep
            .wires
            .iter()
            .map(|item| (item.id, item))
            .collect();
        let faces: HashMap<_, _> = document
            .brep
            .faces
            .iter()
            .map(|item| (item.id, item))
            .collect();
        let shells: HashMap<_, _> = document
            .brep
            .shells
            .iter()
            .map(|item| (item.id, item))
            .collect();

        for vertex in &document.brep.vertices {
            if !vertex.point.is_finite() {
                issues.push(error(
                    "non_finite_vertex",
                    "vertex contains a non-finite coordinate".to_owned(),
                    Some(vertex.id),
                ));
            }
            if !vertex.tolerance_m.is_finite() || vertex.tolerance_m.metres() < 0.0 {
                issues.push(error(
                    "invalid_vertex_tolerance",
                    "vertex tolerance must be finite and non-negative".to_owned(),
                    Some(vertex.id),
                ));
            }
        }

        for edge in &document.brep.edges {
            let start = vertices.get(&edge.start_vertex_id);
            let end = vertices.get(&edge.end_vertex_id);
            if start.is_none() {
                issues.push(error(
                    "missing_edge_vertex",
                    format!("edge references missing start vertex {}", edge.start_vertex_id),
                    Some(edge.id),
                ));
            }
            if end.is_none() {
                issues.push(error(
                    "missing_edge_vertex",
                    format!("edge references missing end vertex {}", edge.end_vertex_id),
                    Some(edge.id),
                ));
            }
            if edge.start_vertex_id == edge.end_vertex_id {
                issues.push(error(
                    "degenerate_edge_topology",
                    "edge start and end identifiers are equal".to_owned(),
                    Some(edge.id),
                ));
            } else if let (Some(start), Some(end)) = (start, end) {
                if start.point.distance_to(end.point).metres()
                    <= document.tolerance.linear_m.metres()
                {
                    issues.push(error(
                        "degenerate_edge_geometry",
                        "edge length is within the document linear tolerance".to_owned(),
                        Some(edge.id),
                    ));
                }
            }
        }

        for wire in &document.brep.wires {
            if wire.edges.is_empty() {
                issues.push(error(
                    "empty_wire",
                    "wire contains no oriented edges".to_owned(),
                    Some(wire.id),
                ));
                continue;
            }
            for oriented in &wire.edges {
                if !edges.contains_key(&oriented.edge_id) {
                    issues.push(error(
                        "missing_wire_edge",
                        format!("wire references missing edge {}", oriented.edge_id),
                        Some(wire.id),
                    ));
                }
            }
            if wire.edges.iter().all(|item| edges.contains_key(&item.edge_id)) {
                let endpoints: Vec<_> = wire
                    .edges
                    .iter()
                    .map(|item| oriented_endpoints(edges[&item.edge_id], item.reversed))
                    .collect();
                for pair in endpoints.windows(2) {
                    if pair[0].1 != pair[1].0 {
                        issues.push(error(
                            "disconnected_wire",
                            "consecutive oriented edges do not share a vertex".to_owned(),
                            Some(wire.id),
                        ));
                    }
                }
                if wire.closed && endpoints.last().unwrap().1 != endpoints[0].0 {
                    issues.push(error(
                        "open_wire_marked_closed",
                        "wire is marked closed but its final endpoint does not meet its start"
                            .to_owned(),
                        Some(wire.id),
                    ));
                }
            }
        }

        for face in &document.brep.faces {
            if face.wire_ids.is_empty() {
                issues.push(error(
                    "face_without_boundary",
                    "face contains no boundary wire".to_owned(),
                    Some(face.id),
                ));
            }
            for wire_id in &face.wire_ids {
                match wires.get(wire_id) {
                    None => issues.push(error(
                        "missing_face_wire",
                        format!("face references missing wire {wire_id}"),
                        Some(face.id),
                    )),
                    Some(wire) if !wire.closed => issues.push(error(
                        "face_boundary_not_closed",
                        "face boundary wire must be closed".to_owned(),
                        Some(face.id),
                    )),
                    _ => {}
                }
            }
        }

        for shell in &document.brep.shells {
            if shell.face_ids.is_empty() {
                issues.push(error(
                    "empty_shell",
                    "shell contains no faces".to_owned(),
                    Some(shell.id),
                ));
            }
            for face_id in &shell.face_ids {
                if !faces.contains_key(face_id) {
                    issues.push(error(
                        "missing_shell_face",
                        format!("shell references missing face {face_id}"),
                        Some(shell.id),
                    ));
                }
            }
            if shell.closed {
                let mut usage: HashMap<Uuid, usize> = HashMap::new();
                for face_id in &shell.face_ids {
                    if let Some(face) = faces.get(face_id) {
                        for wire_id in &face.wire_ids {
                            if let Some(wire) = wires.get(wire_id) {
                                for oriented in &wire.edges {
                                    *usage.entry(oriented.edge_id).or_default() += 1;
                                }
                            }
                        }
                    }
                }
                if usage.values().any(|count| *count != 2) {
                    issues.push(error(
                        "non_manifold_closed_shell",
                        "every edge of a closed shell must be used exactly twice".to_owned(),
                        Some(shell.id),
                    ));
                }
            }
        }

        for body in &document.brep.bodies {
            if body.shell_ids.is_empty() {
                issues.push(error(
                    "body_without_shell",
                    "body contains no shells".to_owned(),
                    Some(body.id),
                ));
            }
            for shell_id in &body.shell_ids {
                match shells.get(shell_id) {
                    None => issues.push(error(
                        "missing_body_shell",
                        format!("body references missing shell {shell_id}"),
                        Some(body.id),
                    )),
                    Some(shell) if body.is_solid && !shell.closed => issues.push(error(
                        "solid_body_has_open_shell",
                        "solid body references a shell that is not closed".to_owned(),
                        Some(body.id),
                    )),
                    _ => {}
                }
            }
        }

        for mesh in &document.meshes {
            validate_mesh(mesh, document.tolerance, &mut issues);
        }

        GeometryValidationReport {
            kernel_id: self.id().to_owned(),
            kernel_version: self.version().to_owned(),
            tolerance: document.tolerance,
            statistics: statistics(document),
            issues,
        }
    }

    fn heal(
        &self,
        document: &GeometryDocument,
        tolerance: GeometryTolerance,
    ) -> Result<HealingOutcome> {
        if !tolerance.is_valid() {
            bail!("healing tolerance must contain positive finite values");
        }

        let mut healed = document.clone();
        healed.tolerance = tolerance;
        let mut actions = Vec::new();
        let mut canonical: HashMap<Uuid, Uuid> = HashMap::new();
        let mut retained: Vec<Vertex> = Vec::new();

        // Input order is retained, making the representative choice deterministic
        // for a versioned input artifact.
        for vertex in &healed.brep.vertices {
            let duplicate = retained.iter().find(|candidate| {
                candidate.point.distance_to(vertex.point).metres()
                    <= tolerance.linear_m.metres()
            });
            if let Some(representative) = duplicate {
                canonical.insert(vertex.id, representative.id);
                actions.push(HealingAction {
                    kind: HealingActionKind::MergeCoincidentVertices,
                    entity_id: Some(vertex.id),
                    message: format!(
                        "merged vertex {} into representative {}",
                        vertex.id, representative.id
                    ),
                    displacement_m: vertex.point.distance_to(representative.point),
                });
            } else {
                canonical.insert(vertex.id, vertex.id);
                retained.push(vertex.clone());
            }
        }

        healed.brep.vertices = retained;
        for edge in &mut healed.brep.edges {
            let old_start = edge.start_vertex_id;
            let old_end = edge.end_vertex_id;
            edge.start_vertex_id = *canonical.get(&old_start).unwrap_or(&old_start);
            edge.end_vertex_id = *canonical.get(&old_end).unwrap_or(&old_end);
            if old_start != edge.start_vertex_id || old_end != edge.end_vertex_id {
                actions.push(HealingAction {
                    kind: HealingActionKind::RewriteEdgeEndpoint,
                    entity_id: Some(edge.id),
                    message: format!(
                        "rewrote edge endpoints from ({old_start}, {old_end}) to ({}, {})",
                        edge.start_vertex_id, edge.end_vertex_id
                    ),
                    displacement_m: Length::ZERO,
                });
            }
        }

        let validation = self.validate(&healed);
        Ok(HealingOutcome {
            document: healed,
            report: HealingReport {
                kernel_id: self.id().to_owned(),
                kernel_version: self.version().to_owned(),
                tolerance,
                actions,
                unresolved_issues: validation.issues,
            },
        })
    }

    fn tessellate(
        &self,
        document: &GeometryDocument,
        options: TessellationOptions,
    ) -> Result<Vec<TriangleMesh>> {
        if !options.is_valid() {
            bail!("invalid tessellation options");
        }
        let validation = self.validate(document);
        if validation.has_errors() {
            bail!("geometry must validate before tessellation");
        }

        let vertices: HashMap<_, _> = document
            .brep
            .vertices
            .iter()
            .map(|item| (item.id, item))
            .collect();
        let edges: HashMap<_, _> = document
            .brep
            .edges
            .iter()
            .map(|item| (item.id, item))
            .collect();
        let wires: HashMap<_, _> = document
            .brep
            .wires
            .iter()
            .map(|item| (item.id, item))
            .collect();

        let mut result = Vec::new();
        for face in &document.brep.faces {
            if face.surface_kind != SurfaceKind::Plane {
                bail!("reference kernel tessellates planar faces only");
            }
            if face.wire_ids.len() != 1 {
                bail!("reference kernel does not tessellate face openings");
            }
            let wire = wires
                .get(&face.wire_ids[0])
                .context("validated face wire disappeared")?;
            let polygon_ids: Vec<Uuid> = wire
                .edges
                .iter()
                .map(|item| oriented_endpoints(edges[&item.edge_id], item.reversed).0)
                .collect();
            if polygon_ids.len() < 3 {
                bail!("face {} has fewer than three polygon vertices", face.id);
            }

            let mesh_vertices: Vec<MeshVertex> = polygon_ids
                .iter()
                .map(|source_id| MeshVertex {
                    id: derived_id(face.id, &format!("vertex:{source_id}")),
                    point: vertices[source_id].point,
                    source_vertex_id: Some(*source_id),
                })
                .collect();

            let mut triangles = Vec::new();
            for index in 1..(mesh_vertices.len() - 1) {
                let mut ids = [
                    mesh_vertices[0].id,
                    mesh_vertices[index].id,
                    mesh_vertices[index + 1].id,
                ];
                if face.orientation_reversed {
                    ids.swap(1, 2);
                }
                triangles.push(Triangle {
                    id: derived_id(face.id, &format!("triangle:{index}")),
                    vertex_ids: ids,
                    source_face_id: Some(face.id),
                });
            }
            result.push(TriangleMesh {
                id: derived_id(face.id, "mesh"),
                vertices: mesh_vertices,
                triangles,
                provenance: face.provenance.clone(),
            });
        }
        Ok(result)
    }
}

fn oriented_endpoints(edge: &Edge, reversed: bool) -> (Uuid, Uuid) {
    if reversed {
        (edge.end_vertex_id, edge.start_vertex_id)
    } else {
        (edge.start_vertex_id, edge.end_vertex_id)
    }
}

fn validate_mesh(
    mesh: &TriangleMesh,
    tolerance: GeometryTolerance,
    issues: &mut Vec<GeometryIssue>,
) {
    let vertices: HashMap<_, _> = mesh.vertices.iter().map(|item| (item.id, item)).collect();
    for vertex in &mesh.vertices {
        if !vertex.point.is_finite() {
            issues.push(error(
                "non_finite_mesh_vertex",
                "mesh vertex contains a non-finite coordinate".to_owned(),
                Some(vertex.id),
            ));
        }
    }
    for triangle in &mesh.triangles {
        if triangle.vertex_ids.iter().collect::<HashSet<_>>().len() != 3 {
            issues.push(error(
                "degenerate_triangle_topology",
                "triangle must reference three distinct vertices".to_owned(),
                Some(triangle.id),
            ));
            continue;
        }
        let points: Option<Vec<_>> = triangle
            .vertex_ids
            .iter()
            .map(|id| vertices.get(id).map(|vertex| vertex.point))
            .collect();
        match points {
            None => issues.push(error(
                "missing_triangle_vertex",
                "triangle references a missing mesh vertex".to_owned(),
                Some(triangle.id),
            )),
            Some(points) if triangle_area_twice(points[0], points[1], points[2])
                <= tolerance.linear_m.metres().powi(2) =>
            {
                issues.push(error(
                    "degenerate_triangle_geometry",
                    "triangle area is within the geometry tolerance".to_owned(),
                    Some(triangle.id),
                ))
            }
            _ => {}
        }
    }
}

fn triangle_area_twice(a: Point3, b: Point3, c: Point3) -> f64 {
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
    (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt()
}

fn statistics(document: &GeometryDocument) -> TopologyStatistics {
    TopologyStatistics {
        vertices: document.brep.vertices.len(),
        edges: document.brep.edges.len(),
        wires: document.brep.wires.len(),
        faces: document.brep.faces.len(),
        shells: document.brep.shells.len(),
        bodies: document.brep.bodies.len(),
        meshes: document.meshes.len(),
        triangles: document
            .meshes
            .iter()
            .map(|mesh| mesh.triangles.len())
            .sum(),
    }
}

fn error(code: &str, message: String, entity_id: Option<Uuid>) -> GeometryIssue {
    GeometryIssue {
        severity: GeometryIssueSeverity::Error,
        code: code.to_owned(),
        message,
        entity_id,
    }
}

fn derived_id(parent: Uuid, purpose: &str) -> Uuid {
    const NAMESPACE: Uuid =
        Uuid::from_u128(0xc2f17731_41f1_46d1_a887_1c358ba50ec4);
    Uuid::new_v5(&NAMESPACE, format!("{parent}:{purpose}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_document() -> GeometryDocument {
        let v1 = Uuid::new_v4();
        let v2 = Uuid::new_v4();
        let v3 = Uuid::new_v4();
        let e1 = Uuid::new_v4();
        let e2 = Uuid::new_v4();
        let e3 = Uuid::new_v4();
        let wire = Uuid::new_v4();
        let face = Uuid::new_v4();
        let shell = Uuid::new_v4();
        let body = Uuid::new_v4();
        GeometryDocument {
            schema_version: GEOMETRY_SCHEMA_VERSION.to_owned(),
            document_id: Uuid::new_v4(),
            revision_id: Uuid::new_v4(),
            name: "Triangle".to_owned(),
            tolerance: GeometryTolerance::engineering_default(),
            brep: BrepModel {
                vertices: vec![
                    vertex(v1, 0.0, 0.0),
                    vertex(v2, 1.0, 0.0),
                    vertex(v3, 0.0, 1.0),
                ],
                edges: vec![
                    edge(e1, v1, v2),
                    edge(e2, v2, v3),
                    edge(e3, v3, v1),
                ],
                wires: vec![Wire {
                    id: wire,
                    edges: vec![
                        OrientedEdge { edge_id: e1, reversed: false },
                        OrientedEdge { edge_id: e2, reversed: false },
                        OrientedEdge { edge_id: e3, reversed: false },
                    ],
                    closed: true,
                    provenance: None,
                }],
                faces: vec![Face {
                    id: face,
                    wire_ids: vec![wire],
                    surface_kind: SurfaceKind::Plane,
                    orientation_reversed: false,
                    provenance: None,
                }],
                shells: vec![Shell {
                    id: shell,
                    face_ids: vec![face],
                    closed: false,
                    provenance: None,
                }],
                bodies: vec![Body {
                    id: body,
                    name: None,
                    shell_ids: vec![shell],
                    is_solid: false,
                    provenance: None,
                }],
            },
            meshes: vec![],
            provenance: None,
        }
    }

    fn vertex(id: Uuid, x: f64, y: f64) -> Vertex {
        Vertex {
            id,
            point: Point3::from_metres(x, y, 0.0),
            tolerance_m: Length::ZERO,
            provenance: None,
        }
    }

    fn edge(id: Uuid, start: Uuid, end: Uuid) -> Edge {
        Edge {
            id,
            start_vertex_id: start,
            end_vertex_id: end,
            curve_kind: CurveKind::Line,
            provenance: None,
        }
    }

    #[test]
    fn validates_and_tessellates_triangle() {
        let kernel = SimpleGeometryKernel;
        let document = triangle_document();
        assert!(!kernel.validate(&document).has_errors());
        let meshes = kernel
            .tessellate(&document, TessellationOptions::engineering_default())
            .unwrap();
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].triangles.len(), 1);
    }

    #[test]
    fn reports_disconnected_wire() {
        let kernel = SimpleGeometryKernel;
        let mut document = triangle_document();
        document.brep.wires[0].edges.swap(0, 1);
        let report = kernel.validate(&document);
        assert!(report.issues.iter().any(|issue| issue.code == "disconnected_wire"));
    }

    #[test]
    fn healing_merges_coincident_vertices_and_reports_remaining_degeneracy() {
        let kernel = SimpleGeometryKernel;
        let mut document = triangle_document();
        let duplicate = Uuid::new_v4();
        document.brep.vertices.push(vertex(duplicate, 0.0, 0.0));
        let extra_edge = Uuid::new_v4();
        document.brep.edges.push(edge(
            extra_edge,
            document.brep.vertices[0].id,
            duplicate,
        ));
        let outcome = kernel
            .heal(&document, GeometryTolerance::engineering_default())
            .unwrap();
        assert_eq!(outcome.document.brep.vertices.len(), 3);
        assert!(outcome
            .report
            .actions
            .iter()
            .any(|action| action.kind == HealingActionKind::MergeCoincidentVertices));
        assert!(outcome
            .report
            .unresolved_issues
            .iter()
            .any(|issue| issue.code == "degenerate_edge_topology"));
    }
}
