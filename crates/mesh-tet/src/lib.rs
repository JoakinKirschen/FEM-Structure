//! Pass 9 automatic tetrahedralization backend.
//!
//! `ReferenceTetMesher` is deterministic and dependency-free. It validates a
//! closed triangular boundary and creates a centroid fan, with optional
//! volume-driven 1-to-4 refinement. This is useful for convex and star-shaped
//! solids. Production arbitrary/non-star-shaped CAD should use another
//! `VolumeMeshGenerator` implementation (for example a TetGen/Gmsh/CGAL adapter).

use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use structural_geometry_api::{
    GeometryDocument, GeometryKernel, Point3, TessellationOptions, TriangleMesh,
};
use structural_geometry_simple::SimpleGeometryKernel;
use structural_mesh_api::*;
use uuid::Uuid;

const TET_NAMESPACE: Uuid =
    Uuid::from_u128(0xc7811dd4_3815_4ab4_bcb0_d5b813bb4809);

#[derive(Debug, Default)]
pub struct ReferenceTetMesher;

#[derive(Clone)]
struct TetRecord {
    id: Uuid,
    nodes: [Uuid; 4],
    source_mesh_id: Uuid,
    source_triangle_id: Uuid,
    source_face_id: Option<Uuid>,
}

impl VolumeMeshGenerator for ReferenceTetMesher {
    fn id(&self) -> &'static str {
        "sweco.structural.mesh.tet.reference"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn algorithm(&self) -> &'static str {
        "validated-star-centroid-fan-with-1-to-4-refinement"
    }

    fn validate_boundary(
        &self,
        boundary: &[TriangleMesh],
        options: &TetrahedralizationOptions,
    ) -> BoundaryValidationReport {
        validate_boundary_impl(boundary, options)
    }

    fn tetrahedralize(
        &self,
        geometry: &GeometryDocument,
        options: TetrahedralizationOptions,
    ) -> Result<TetrahedralizationOutcome> {
        if !options.is_valid() {
            bail!("invalid tetrahedralization options");
        }
        if !options.cavity_seeds.is_empty() {
            bail!("reference tetrahedralizer does not support cavities; use an external backend");
        }
        if options.region_seeds.len() > 1 {
            bail!("reference tetrahedralizer supports at most one material region");
        }

        let boundary = boundary_meshes(geometry, &options)?;
        if boundary.is_empty() {
            bail!("geometry contains no triangular solid boundary");
        }
        let boundary_report = self.validate_boundary(&boundary, &options);
        if boundary_report.has_errors() {
            bail!("MESH_BOUNDARY_INVALID: boundary validation failed");
        }

        let mut nodes = collect_nodes(&boundary)?;
        let original_boundary_node_count = nodes.len();
        let original_boundary_node_ids: HashSet<_> = nodes.keys().copied().collect();
        let centre = centroid(nodes.values().copied());
        if !point_inside_boundary(centre, &boundary, &nodes) {
            bail!("MESH_REFERENCE_BACKEND_CENTRE_OUTSIDE: boundary is not supported by the reference backend");
        }
        let centre_id = stable_uuid(&format!(
            "centre:{}:{}",
            geometry.document_id, options.deterministic_seed
        ));
        nodes.insert(centre_id, centre);

        let mut tets = Vec::new();
        for mesh in &boundary {
            for triangle in &mesh.triangles {
                let mut ids = [
                    centre_id,
                    triangle.vertex_ids[0],
                    triangle.vertex_ids[1],
                    triangle.vertex_ids[2],
                ];
                let points = tet_points(ids, &nodes)?;
                let signed = signed_tet_volume(points);
                if signed.abs() <= options.merge_tolerance_m.metres().powi(3) {
                    bail!("MESH_DEGENERATE_TET: boundary facet creates a zero-volume tetrahedron");
                }
                if signed < 0.0 {
                    ids.swap(2, 3);
                }
                tets.push(TetRecord {
                    id: stable_uuid(&format!("tet:{}:{}", mesh.id, triangle.id)),
                    nodes: ids,
                    source_mesh_id: mesh.id,
                    source_triangle_id: triangle.id,
                    source_face_id: triangle.source_face_id,
                });
            }
        }

        ensure_star_fan_is_conforming(&tets, &nodes, &options)?;

        if let Some(maximum) = options.maximum_element_volume_m3 {
            if !options.allow_steiner_points
                && tets.iter().any(|tet| tet_volume(tet, &nodes).unwrap_or(f64::INFINITY) > maximum)
            {
                bail!("MESH_STEINER_POINTS_REQUIRED: volume target requires interior points");
            }
            for level in 0..options.maximum_refinement_levels {
                if !tets.iter().any(|tet| tet_volume(tet, &nodes).unwrap_or(f64::INFINITY) > maximum) {
                    break;
                }
                let (next, added) = refine_large_tets(tets, &mut nodes, maximum, level)?;
                tets = next;
                if !added {
                    break;
                }
            }
        }

        let mesh_id = stable_uuid(&format!(
            "tet-mesh:{}:{}",
            geometry.document_id, options.deterministic_seed
        ));
        let mut mesh_nodes: Vec<_> = nodes
            .into_iter()
            .map(|(id, point)| AnalysisMeshNode {
                id,
                point,
                source_vertex_id: if original_boundary_node_ids.contains(&id) { Some(id) } else { None },
            })
            .collect();
        mesh_nodes.sort_by_key(|node| *node.id.as_bytes());

        let elements = tets
            .iter()
            .map(|tet| AnalysisElement {
                id: tet.id,
                kind: ElementKind::Tetrahedron4,
                node_ids: tet.nodes.to_vec(),
                source_geometry_id: tet.source_face_id,
            })
            .collect();
        let mesh = AnalysisMesh {
            schema_version: MESH_SCHEMA_VERSION.to_owned(),
            id: mesh_id,
            nodes: mesh_nodes,
            elements,
            provenance: geometry.provenance.clone(),
        };

        let mut mapping_by_facet: BTreeMap<(Uuid, Uuid), BoundaryFacetMapping> = BTreeMap::new();
        for tet in &tets {
            mapping_by_facet
                .entry((tet.source_mesh_id, tet.source_triangle_id))
                .or_insert_with(|| BoundaryFacetMapping {
                    boundary_mesh_id: tet.source_mesh_id,
                    boundary_triangle_id: tet.source_triangle_id,
                    source_face_id: tet.source_face_id,
                    generated_tetrahedron_ids: Vec::new(),
                })
                .generated_tetrahedron_ids
                .push(tet.id);
        }
        let boundary_mapping = mapping_by_facet.into_values().collect();
        let quality_report = self.assess_volume(
            &mesh,
            boundary_report.statistics.facet_count,
            original_boundary_node_count,
            &options,
        );

        Ok(TetrahedralizationOutcome {
            mesh,
            boundary_report,
            quality_report,
            boundary_mapping,
            backend: VolumeMesherBackend {
                id: self.id().to_owned(),
                version: self.version().to_owned(),
                algorithm: self.algorithm().to_owned(),
                deterministic: true,
                native_library: None,
            },
        })
    }

    fn assess_volume(
        &self,
        mesh: &AnalysisMesh,
        boundary_facet_count: usize,
        original_boundary_node_count: usize,
        options: &TetrahedralizationOptions,
    ) -> VolumeMeshQualityReport {
        let points: HashMap<_, _> = mesh.nodes.iter().map(|node| (node.id, node.point)).collect();
        let mut issues = Vec::new();
        let mut qualities = Vec::new();

        for element in &mesh.elements {
            if element.kind != ElementKind::Tetrahedron4 || element.node_ids.len() != 4 {
                issues.push(issue(
                    MeshIssueSeverity::Error,
                    "MESH_INVALID_TET_CONNECTIVITY",
                    "volume mesh contains a non-tetrahedral or malformed element",
                    Some(element.id),
                    None,
                    Some(4.0),
                ));
                continue;
            }
            let ids = [
                element.node_ids[0],
                element.node_ids[1],
                element.node_ids[2],
                element.node_ids[3],
            ];
            let Ok(p) = tet_points(ids, &points) else {
                issues.push(issue(
                    MeshIssueSeverity::Error,
                    "MESH_MISSING_TET_NODE",
                    "tetrahedron references a missing node",
                    Some(element.id),
                    None,
                    None,
                ));
                continue;
            };
            let volume = signed_tet_volume(p);
            let edges = tet_edges(p);
            let min_edge = edges.iter().copied().fold(f64::INFINITY, f64::min);
            let max_edge = edges.iter().copied().fold(0.0_f64, f64::max);
            let aspect = max_edge / min_edge;
            let min_dihedral = minimum_dihedral_angle_deg(p);
            let min_jacobian = minimum_scaled_jacobian(p);
            qualities.push(TetrahedronQuality {
                element_id: element.id,
                signed_volume_m3: volume,
                minimum_edge_length_m: min_edge,
                maximum_edge_length_m: max_edge,
                edge_aspect_ratio: aspect,
                minimum_dihedral_angle_deg: min_dihedral,
                minimum_scaled_jacobian: min_jacobian,
            });
            if !volume.is_finite() || volume <= 0.0 {
                issues.push(issue(
                    MeshIssueSeverity::Error,
                    "MESH_NEGATIVE_TET_VOLUME",
                    "tetrahedron has zero, negative, or non-finite signed volume",
                    Some(element.id),
                    Some(volume),
                    Some(0.0),
                ));
            }
            if let Some(maximum) = options.maximum_element_volume_m3 {
                if volume > maximum * (1.0 + 1.0e-12) {
                    issues.push(issue(
                        MeshIssueSeverity::Error,
                        "MESH_MAXIMUM_TET_VOLUME_EXCEEDED",
                        "tetrahedron exceeds the configured maximum volume",
                        Some(element.id),
                        Some(volume),
                        Some(maximum),
                    ));
                }
            }
            if let Some(maximum) = options.maximum_edge_length_m {
                if max_edge > maximum.metres() * (1.0 + 1.0e-12) {
                    issues.push(issue(
                        MeshIssueSeverity::Error,
                        "MESH_MAXIMUM_EDGE_LENGTH_EXCEEDED",
                        "tetrahedron exceeds the configured maximum edge length",
                        Some(element.id),
                        Some(max_edge),
                        Some(maximum.metres()),
                    ));
                }
            }
            if min_dihedral < options.minimum_dihedral_angle_deg {
                issues.push(issue(
                    MeshIssueSeverity::Warning,
                    "MESH_MINIMUM_DIHEDRAL_ANGLE_NOT_MET",
                    "tetrahedron is sharper than the configured quality threshold",
                    Some(element.id),
                    Some(min_dihedral),
                    Some(options.minimum_dihedral_angle_deg),
                ));
            }
            if min_jacobian < options.minimum_scaled_jacobian {
                issues.push(issue(
                    MeshIssueSeverity::Warning,
                    "MESH_SCALED_JACOBIAN_BELOW_LIMIT",
                    "tetrahedron is below the configured scaled-Jacobian threshold",
                    Some(element.id),
                    Some(min_jacobian),
                    Some(options.minimum_scaled_jacobian),
                ));
            }
        }

        let volumes: Vec<_> = qualities.iter().map(|q| q.signed_volume_m3).collect();
        let angles: Vec<_> = qualities.iter().map(|q| q.minimum_dihedral_angle_deg).collect();
        let jacobians: Vec<_> = qualities.iter().map(|q| q.minimum_scaled_jacobian).collect();
        let edges: Vec<_> = qualities.iter().map(|q| q.maximum_edge_length_m).collect();

        VolumeMeshQualityReport {
            schema_version: MESH_SCHEMA_VERSION.to_owned(),
            generator_id: self.id().to_owned(),
            generator_version: self.version().to_owned(),
            statistics: VolumeMeshStatistics {
                node_count: mesh.nodes.len(),
                tetrahedron_count: qualities.len(),
                boundary_facet_count,
                steiner_point_count: mesh.nodes.len().saturating_sub(original_boundary_node_count),
                total_volume_m3: volumes.iter().sum(),
                minimum_tetrahedron_volume_m3: finite_min(&volumes),
                maximum_tetrahedron_volume_m3: finite_max(&volumes),
                minimum_dihedral_angle_deg: finite_min(&angles),
                minimum_scaled_jacobian: finite_min(&jacobians),
                maximum_edge_length_m: finite_max(&edges),
            },
            tetrahedra: qualities,
            issues,
        }
    }
}

fn boundary_meshes(
    geometry: &GeometryDocument,
    options: &TetrahedralizationOptions,
) -> Result<Vec<TriangleMesh>> {
    if !geometry.meshes.is_empty() {
        return Ok(geometry.meshes.clone());
    }
    let kernel = SimpleGeometryKernel;
    kernel.tessellate(
        geometry,
        TessellationOptions {
            chord_tolerance_m: geometry.tolerance.linear_m,
            angular_tolerance_rad: geometry.tolerance.angular_rad,
            maximum_edge_length_m: options.maximum_edge_length_m,
            deterministic: true,
        },
    )
}

fn validate_boundary_impl(
    boundary: &[TriangleMesh],
    options: &TetrahedralizationOptions,
) -> BoundaryValidationReport {
    let mut issues = Vec::new();
    let mut vertices = HashMap::<Uuid, Point3>::new();
    let mut facets = HashSet::<[Uuid; 3]>::new();
    let mut edge_uses = HashMap::<(Uuid, Uuid), usize>::new();
    let mut adjacency = HashMap::<Uuid, Vec<Uuid>>::new();
    let mut facet_count = 0usize;

    for mesh in boundary {
        for vertex in &mesh.vertices {
            if let Some(existing) = vertices.insert(vertex.id, vertex.point) {
                if existing.distance_to(vertex.point).metres() > options.merge_tolerance_m.metres() {
                    issues.push(issue(
                        MeshIssueSeverity::Error,
                        "MESH_DUPLICATE_VERTEX_ID",
                        "the same boundary vertex identifier has inconsistent coordinates",
                        Some(vertex.id),
                        None,
                        None,
                    ));
                }
            }
        }
    }
    for mesh in boundary {
        for triangle in &mesh.triangles {
            facet_count += 1;
            if triangle.vertex_ids.iter().any(|id| !vertices.contains_key(id)) {
                issues.push(issue(
                    MeshIssueSeverity::Error,
                    "MESH_BOUNDARY_MISSING_VERTEX",
                    "boundary facet references a missing vertex",
                    Some(triangle.id),
                    None,
                    None,
                ));
                continue;
            }
            let mut key = triangle.vertex_ids;
            key.sort_by_key(|id| *id.as_bytes());
            if !facets.insert(key) {
                issues.push(issue(
                    MeshIssueSeverity::Error,
                    "MESH_BOUNDARY_DUPLICATE_FACET",
                    "boundary contains a duplicate triangular facet",
                    Some(triangle.id),
                    None,
                    None,
                ));
            }
            let p = [
                vertices[&triangle.vertex_ids[0]],
                vertices[&triangle.vertex_ids[1]],
                vertices[&triangle.vertex_ids[2]],
            ];
            if triangle_area(p[0], p[1], p[2]) <= options.merge_tolerance_m.metres().powi(2) {
                issues.push(issue(
                    MeshIssueSeverity::Error,
                    "MESH_BOUNDARY_DEGENERATE_FACET",
                    "boundary contains a zero-area or tolerance-scale facet",
                    Some(triangle.id),
                    None,
                    None,
                ));
            }
            for (a, b) in [
                (triangle.vertex_ids[0], triangle.vertex_ids[1]),
                (triangle.vertex_ids[1], triangle.vertex_ids[2]),
                (triangle.vertex_ids[2], triangle.vertex_ids[0]),
            ] {
                let edge = ordered_pair(a, b);
                *edge_uses.entry(edge).or_insert(0) += 1;
                adjacency.entry(a).or_default().push(b);
                adjacency.entry(b).or_default().push(a);
            }
        }
    }
    for ((a, b), count) in &edge_uses {
        if *count == 1 {
            issues.push(issue(
                MeshIssueSeverity::Error,
                "MESH_BOUNDARY_OPEN",
                "boundary edge is used by only one facet",
                Some(stable_uuid(&format!("edge:{a}:{b}"))),
                Some(*count as f64),
                Some(2.0),
            ));
        } else if *count != 2 {
            issues.push(issue(
                MeshIssueSeverity::Error,
                "MESH_BOUNDARY_NON_MANIFOLD",
                "boundary edge is not used by exactly two facets",
                Some(stable_uuid(&format!("edge:{a}:{b}"))),
                Some(*count as f64),
                Some(2.0),
            ));
        }
    }
    let components = connected_components(vertices.keys().copied(), &adjacency);
    if options.require_single_region && components > 1 {
        issues.push(issue(
            MeshIssueSeverity::Error,
            "MESH_BOUNDARY_DISCONNECTED",
            "reference backend requires one connected closed boundary",
            None,
            Some(components as f64),
            Some(1.0),
        ));
    }
    issues.push(issue(
        MeshIssueSeverity::Information,
        "MESH_BOUNDARY_SELF_INTERSECTION_CHECK_LIMITED",
        "reference backend performs topology checks but not a robust exact-predicate global self-intersection test",
        None,
        None,
        None,
    ));

    BoundaryValidationReport {
        schema_version: MESH_SCHEMA_VERSION.to_owned(),
        statistics: BoundaryValidationStatistics {
            mesh_count: boundary.len(),
            vertex_count: vertices.len(),
            facet_count,
            unique_edge_count: edge_uses.len(),
            connected_component_count: components,
        },
        issues,
    }
}

fn collect_nodes(boundary: &[TriangleMesh]) -> Result<HashMap<Uuid, Point3>> {
    let mut result = HashMap::new();
    for mesh in boundary {
        for vertex in &mesh.vertices {
            if let Some(existing) = result.insert(vertex.id, vertex.point) {
                if existing != vertex.point {
                    bail!("duplicate vertex identifier has inconsistent coordinates");
                }
            }
        }
    }
    Ok(result)
}

fn centroid(points: impl Iterator<Item = Point3>) -> Point3 {
    let mut sum = [0.0; 3];
    let mut count = 0usize;
    for point in points {
        for (index, value) in sum.iter_mut().enumerate() {
            *value += point.xyz_m[index].metres();
        }
        count += 1;
    }
    Point3::from_metres(
        sum[0] / count as f64,
        sum[1] / count as f64,
        sum[2] / count as f64,
    )
}

fn point_inside_boundary(
    point: Point3,
    boundary: &[TriangleMesh],
    points: &HashMap<Uuid, Point3>,
) -> bool {
    let origin = xyz(point);
    let direction = normalize([1.0, 0.3713906763541037, 0.217832]);
    let mut hits = Vec::new();
    for mesh in boundary {
        for triangle in &mesh.triangles {
            let a = xyz(points[&triangle.vertex_ids[0]]);
            let b = xyz(points[&triangle.vertex_ids[1]]);
            let c = xyz(points[&triangle.vertex_ids[2]]);
            if let Some(t) = ray_triangle(origin, direction, a, b, c) {
                if t > 1.0e-10 && !hits.iter().any(|other: &f64| (*other - t).abs() < 1.0e-9) {
                    hits.push(t);
                }
            }
        }
    }
    hits.len() % 2 == 1
}

fn ray_triangle(o: [f64; 3], d: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<f64> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let h = cross(d, e2);
    let det = dot(e1, h);
    if det.abs() < 1.0e-12 { return None; }
    let inv = 1.0 / det;
    let s = sub(o, a);
    let u = inv * dot(s, h);
    if !(0.0..=1.0).contains(&u) { return None; }
    let q = cross(s, e1);
    let v = inv * dot(d, q);
    if v < 0.0 || u + v > 1.0 { return None; }
    Some(inv * dot(e2, q))
}

fn ensure_star_fan_is_conforming(
    tets: &[TetRecord],
    _points: &HashMap<Uuid, Point3>,
    _options: &TetrahedralizationOptions,
) -> Result<()> {
    let mut directed = HashMap::<(Uuid, Uuid), (usize, usize)>::new();
    for tet in tets {
        let tri = [tet.nodes[1], tet.nodes[2], tet.nodes[3]];
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = ordered_pair(a, b);
            let entry = directed.entry(key).or_insert((0, 0));
            if key == (a, b) { entry.0 += 1; } else { entry.1 += 1; }
        }
    }
    if directed.values().any(|(forward, reverse)| *forward != 1 || *reverse != 1) {
        bail!("MESH_REFERENCE_BACKEND_NON_STAR_SHAPED: centroid fan orientation is inconsistent; use a constrained-Delaunay backend");
    }
    Ok(())
}

fn refine_large_tets(
    tets: Vec<TetRecord>,
    nodes: &mut HashMap<Uuid, Point3>,
    maximum: f64,
    level: u8,
) -> Result<(Vec<TetRecord>, bool)> {
    let mut output = Vec::new();
    let mut added = false;
    for tet in tets {
        if tet_volume(&tet, nodes)? <= maximum * (1.0 + 1.0e-12) {
            output.push(tet);
            continue;
        }
        added = true;
        let p = tet_points(tet.nodes, nodes)?;
        let centre = centroid(p.into_iter());
        let centre_id = stable_uuid(&format!("tet-centre:{}:{level}", tet.id));
        nodes.insert(centre_id, centre);
        let faces = [
            [tet.nodes[1], tet.nodes[2], tet.nodes[3]],
            [tet.nodes[0], tet.nodes[3], tet.nodes[2]],
            [tet.nodes[0], tet.nodes[1], tet.nodes[3]],
            [tet.nodes[0], tet.nodes[2], tet.nodes[1]],
        ];
        for (index, face) in faces.into_iter().enumerate() {
            let mut ids = [centre_id, face[0], face[1], face[2]];
            if signed_tet_volume(tet_points(ids, nodes)?) < 0.0 {
                ids.swap(2, 3);
            }
            output.push(TetRecord {
                id: stable_uuid(&format!("tet-child:{}:{level}:{index}", tet.id)),
                nodes: ids,
                source_mesh_id: tet.source_mesh_id,
                source_triangle_id: tet.source_triangle_id,
                source_face_id: tet.source_face_id,
            });
        }
    }
    Ok((output, added))
}

fn tet_volume(tet: &TetRecord, points: &HashMap<Uuid, Point3>) -> Result<f64> {
    Ok(signed_tet_volume(tet_points(tet.nodes, points)?))
}

fn tet_points(ids: [Uuid; 4], points: &HashMap<Uuid, Point3>) -> Result<[Point3; 4]> {
    Ok([
        *points.get(&ids[0]).context("tetrahedron node missing")?,
        *points.get(&ids[1]).context("tetrahedron node missing")?,
        *points.get(&ids[2]).context("tetrahedron node missing")?,
        *points.get(&ids[3]).context("tetrahedron node missing")?,
    ])
}

fn signed_tet_volume(p: [Point3; 4]) -> f64 {
    dot(
        sub(xyz(p[1]), xyz(p[0])),
        cross(sub(xyz(p[2]), xyz(p[0])), sub(xyz(p[3]), xyz(p[0]))),
    ) / 6.0
}

fn tet_edges(p: [Point3; 4]) -> [f64; 6] {
    [
        distance(p[0], p[1]), distance(p[0], p[2]), distance(p[0], p[3]),
        distance(p[1], p[2]), distance(p[1], p[3]), distance(p[2], p[3]),
    ]
}

fn minimum_scaled_jacobian(p: [Point3; 4]) -> f64 {
    let corners = [
        (0, 1, 2, 3), (1, 0, 3, 2), (2, 0, 1, 3), (3, 0, 2, 1),
    ];
    corners.into_iter().map(|(o, a, b, c)| {
        let va = sub(xyz(p[a]), xyz(p[o]));
        let vb = sub(xyz(p[b]), xyz(p[o]));
        let vc = sub(xyz(p[c]), xyz(p[o]));
        (dot(va, cross(vb, vc)) / (norm(va) * norm(vb) * norm(vc))).abs()
    }).fold(f64::INFINITY, f64::min)
}

fn minimum_dihedral_angle_deg(p: [Point3; 4]) -> f64 {
    let faces = [(0,1,2), (0,3,1), (0,2,3), (1,3,2)];
    let normals: Vec<_> = faces.into_iter().map(|(a,b,c)| {
        normalize(cross(sub(xyz(p[b]), xyz(p[a])), sub(xyz(p[c]), xyz(p[a]))))
    }).collect();
    let pairs = [(0,1), (0,2), (0,3), (1,2), (1,3), (2,3)];
    pairs.into_iter()
        .map(|(a,b)| dot(normals[a], normals[b]).abs().clamp(-1.0, 1.0).acos().to_degrees())
        .fold(f64::INFINITY, f64::min)
}

fn triangle_area(a: Point3, b: Point3, c: Point3) -> f64 {
    0.5 * norm(cross(sub(xyz(b), xyz(a)), sub(xyz(c), xyz(a))))
}

fn connected_components(
    vertices: impl Iterator<Item = Uuid>,
    adjacency: &HashMap<Uuid, Vec<Uuid>>,
) -> usize {
    let mut remaining: HashSet<_> = vertices.collect();
    let mut count = 0;
    while let Some(start) = remaining.iter().next().copied() {
        count += 1;
        let mut queue = VecDeque::from([start]);
        remaining.remove(&start);
        while let Some(current) = queue.pop_front() {
            for next in adjacency.get(&current).into_iter().flatten() {
                if remaining.remove(next) {
                    queue.push_back(*next);
                }
            }
        }
    }
    count
}

fn issue(
    severity: MeshIssueSeverity,
    code: &str,
    message: &str,
    entity_id: Option<Uuid>,
    measured_value: Option<f64>,
    required_value: Option<f64>,
) -> VolumeMeshingIssue {
    VolumeMeshingIssue {
        severity,
        code: code.to_owned(),
        message: message.to_owned(),
        entity_id,
        measured_value,
        required_value,
    }
}

fn ordered_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a.as_bytes() <= b.as_bytes() { (a, b) } else { (b, a) }
}
fn stable_uuid(value: &str) -> Uuid { Uuid::new_v5(&TET_NAMESPACE, value.as_bytes()) }
fn xyz(p: Point3) -> [f64;3] { [p.xyz_m[0].metres(), p.xyz_m[1].metres(), p.xyz_m[2].metres()] }
fn sub(a: [f64;3], b: [f64;3]) -> [f64;3] { [a[0]-b[0], a[1]-b[1], a[2]-b[2]] }
fn dot(a: [f64;3], b: [f64;3]) -> f64 { a[0]*b[0] + a[1]*b[1] + a[2]*b[2] }
fn cross(a: [f64;3], b: [f64;3]) -> [f64;3] {
    [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
}
fn norm(a: [f64;3]) -> f64 { dot(a,a).sqrt() }
fn normalize(a: [f64;3]) -> [f64;3] {
    let n = norm(a);
    if n <= f64::EPSILON { [0.0;3] } else { [a[0]/n, a[1]/n, a[2]/n] }
}
fn distance(a: Point3, b: Point3) -> f64 { a.distance_to(b).metres() }
fn finite_min(values: &[f64]) -> Option<f64> {
    values.iter().copied().filter(|v| v.is_finite()).reduce(f64::min)
}
fn finite_max(values: &[f64]) -> Option<f64> {
    values.iter().copied().filter(|v| v.is_finite()).reduce(f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use structural_geometry_api::{
        BrepModel, GeometryTolerance, MeshVertex, Triangle, GEOMETRY_SCHEMA_VERSION,
    };

    fn tetrahedron_document(open: bool) -> GeometryDocument {
        let ids = [
            stable_uuid("a"), stable_uuid("b"), stable_uuid("c"), stable_uuid("d"),
        ];
        let vertices = vec![
            MeshVertex { id: ids[0], point: Point3::from_metres(0.0,0.0,0.0), source_vertex_id: None },
            MeshVertex { id: ids[1], point: Point3::from_metres(1.0,0.0,0.0), source_vertex_id: None },
            MeshVertex { id: ids[2], point: Point3::from_metres(0.0,1.0,0.0), source_vertex_id: None },
            MeshVertex { id: ids[3], point: Point3::from_metres(0.0,0.0,1.0), source_vertex_id: None },
        ];
        let mut faces = vec![[ids[0],ids[2],ids[1]], [ids[0],ids[1],ids[3]], [ids[1],ids[2],ids[3]], [ids[2],ids[0],ids[3]]];
        if open { faces.pop(); }
        let triangles = faces.into_iter().enumerate().map(|(i, vertex_ids)| Triangle {
            id: stable_uuid(&format!("face-{i}")), vertex_ids, source_face_id: None,
        }).collect();
        GeometryDocument {
            schema_version: GEOMETRY_SCHEMA_VERSION.to_owned(),
            document_id: stable_uuid("document"),
            revision_id: stable_uuid("revision"),
            name: "closed tetrahedron".to_owned(),
            tolerance: GeometryTolerance::engineering_default(),
            brep: BrepModel { vertices: vec![], edges: vec![], wires: vec![], faces: vec![], shells: vec![], bodies: vec![] },
            meshes: vec![TriangleMesh { id: stable_uuid("boundary"), vertices, triangles, provenance: None }],
            provenance: None,
        }
    }

    #[test]
    fn rejects_open_boundary() {
        let mesher = ReferenceTetMesher;
        let document = tetrahedron_document(true);
        let report = mesher.validate_boundary(&document.meshes, &TetrahedralizationOptions::engineering_default());
        assert!(report.has_errors());
        assert!(report.issues.iter().any(|issue| issue.code == "MESH_BOUNDARY_OPEN"));
    }

    #[test]
    fn tetrahedralizes_closed_boundary_deterministically() {
        let mesher = ReferenceTetMesher;
        let document = tetrahedron_document(false);
        let first = mesher.tetrahedralize(&document, TetrahedralizationOptions::engineering_default()).unwrap();
        let second = mesher.tetrahedralize(&document, TetrahedralizationOptions::engineering_default()).unwrap();
        assert_eq!(first.mesh, second.mesh);
        assert_eq!(first.mesh.elements.len(), 4);
        assert!(!first.quality_report.has_errors());
        assert!((first.quality_report.statistics.total_volume_m3 - 1.0/6.0).abs() < 1.0e-12);
    }

    #[test]
    fn volume_refinement_adds_steiner_points() {
        let mesher = ReferenceTetMesher;
        let document = tetrahedron_document(false);
        let mut options = TetrahedralizationOptions::engineering_default();
        options.maximum_element_volume_m3 = Some(0.02);
        let outcome = mesher.tetrahedralize(&document, options).unwrap();
        assert!(outcome.mesh.elements.len() > 4);
        assert!(outcome.quality_report.statistics.steiner_point_count > 1);
        assert!(!outcome.quality_report.has_errors());
    }
}
