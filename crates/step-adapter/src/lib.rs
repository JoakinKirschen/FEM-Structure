//! Auditable STEP AP203/AP214/AP242 reference adapter.
//!
//! This dependency-light implementation supports Cartesian points, polygon
//! loops, face bounds and planar advanced faces. Unsupported curved and solid
//! representations are diagnosed rather than approximated silently.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use structural_geometry_api::*;
use structural_units::Length;
use uuid::Uuid;

const STEP_NAMESPACE: Uuid =
    Uuid::from_u128(0x74f786f0_b7ea_4c49_bed8_8c514fe2c519);

#[derive(Debug, Default)]
pub struct StepAdapter;

#[derive(Debug, Clone)]
struct Record {
    id: u64,
    kind: String,
    args: String,
    normalized: String,
}

impl GeometryImportAdapter for StepAdapter {
    fn id(&self) -> &'static str { "structural-step-spf" }
    fn version(&self) -> &'static str { env!("CARGO_PKG_VERSION") }
    fn source_format(&self) -> &'static str { "STEP-SPF" }

    fn import(
        &self,
        bytes: &[u8],
        source_name: Option<&str>,
        options: GeometryImportOptions,
    ) -> Result<GeometryImportOutcome> {
        if !options.tolerance.is_valid() { bail!("invalid geometry tolerance"); }
        let source = std::str::from_utf8(bytes).context("STEP-SPF must be UTF-8")?;
        let records = parse_records(source)?;
        let source_sha256 = sha256_hex(bytes);
        let schema = parse_schema(source);
        let (scale, unit_name) = detect_length_unit(&records);
        let by_id: HashMap<_, _> = records.iter().map(|record| (record.id, record)).collect();

        let provenance = |record: &Record| GeometryProvenance {
            source_format: "STEP-SPF".to_owned(),
            source_document: source_name.map(str::to_owned),
            source_entity_id: Some(format!("#{}", record.id)),
            source_revision: schema.clone(),
            source_units: Some(unit_name.clone()),
            adapter_id: self.id().to_owned(),
            adapter_version: self.version().to_owned(),
            source_sha256: Some(source_sha256.clone()),
        };

        let mut vertices = Vec::new();
        let mut point_ids = HashMap::new();
        let mut edges = Vec::new();
        let mut wires = Vec::new();
        let mut loop_wires = HashMap::new();
        let mut faces = Vec::new();
        let mut mappings: HashMap<u64, Vec<Uuid>> = HashMap::new();
        let mut diagnostics = Vec::new();

        for record in &records {
            if record.kind == "CARTESIAN_POINT" {
                match parse_point(&record.args) {
                    Ok([x, y, z]) => {
                        let id = entity_uuid(record.id, "vertex");
                        point_ids.insert(record.id, id);
                        vertices.push(Vertex {
                            id,
                            point: Point3::from_metres(x * scale, y * scale, z * scale),
                            tolerance_m: Length::ZERO,
                            provenance: Some(provenance(record)),
                        });
                        mappings.entry(record.id).or_default().push(id);
                    }
                    Err(error) => diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "step_invalid_cartesian_point",
                        error.to_string(),
                        record.id,
                    )),
                }
            }
        }

        for record in &records {
            if record.kind == "POLY_LOOP" {
                let refs = extract_refs(&record.args);
                if refs.len() < 3 {
                    diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "step_polygon_too_short",
                        "POLY_LOOP requires at least three points".to_owned(),
                        record.id,
                    ));
                    continue;
                }
                let mut oriented = Vec::new();
                let mut valid = true;
                for index in 0..refs.len() {
                    let start_ref = refs[index];
                    let end_ref = refs[(index + 1) % refs.len()];
                    let (Some(start), Some(end)) =
                        (point_ids.get(&start_ref), point_ids.get(&end_ref))
                    else {
                        diagnostics.push(diag(
                            ImportDiagnosticSeverity::Error,
                            "step_polygon_missing_point",
                            "POLY_LOOP references an unavailable CARTESIAN_POINT".to_owned(),
                            record.id,
                        ));
                        valid = false;
                        break;
                    };
                    let edge_id = stable_uuid(&format!(
                        "loop:{}:edge:{}:{}",
                        record.id, start_ref, end_ref
                    ));
                    edges.push(Edge {
                        id: edge_id,
                        start_vertex_id: *start,
                        end_vertex_id: *end,
                        curve_kind: CurveKind::Line,
                        provenance: Some(provenance(record)),
                    });
                    oriented.push(OrientedEdge { edge_id, reversed: false });
                    mappings.entry(record.id).or_default().push(edge_id);
                }
                if valid {
                    let wire_id = entity_uuid(record.id, "wire");
                    wires.push(Wire {
                        id: wire_id,
                        edges: oriented,
                        closed: true,
                        provenance: Some(provenance(record)),
                    });
                    loop_wires.insert(record.id, wire_id);
                    mappings.entry(record.id).or_default().push(wire_id);
                }
            }
        }

        let mut bound_wires = HashMap::new();
        for record in &records {
            if matches!(record.kind.as_str(), "FACE_OUTER_BOUND" | "FACE_BOUND") {
                if let Some(loop_ref) = extract_refs(&record.args).first() {
                    if let Some(wire_id) = loop_wires.get(loop_ref) {
                        bound_wires.insert(record.id, *wire_id);
                        mappings.entry(record.id).or_default().push(*wire_id);
                    } else {
                        diagnostics.push(diag(
                            ImportDiagnosticSeverity::Error,
                            "step_face_bound_missing_loop",
                            format!("face bound references unsupported loop #{}", loop_ref),
                            record.id,
                        ));
                    }
                }
            }
        }

        for record in &records {
            if record.kind == "ADVANCED_FACE" {
                let wire_ids: Vec<_> = extract_refs(&record.args)
                    .into_iter()
                    .filter_map(|reference| bound_wires.get(&reference).copied())
                    .collect();
                if wire_ids.is_empty() {
                    diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "step_face_without_supported_bound",
                        "ADVANCED_FACE has no supported polygonal bound".to_owned(),
                        record.id,
                    ));
                    continue;
                }
                let id = entity_uuid(record.id, "face");
                faces.push(Face {
                    id,
                    wire_ids,
                    surface_kind: SurfaceKind::Plane,
                    orientation_reversed: record.args.trim_end().ends_with(".F."),
                    provenance: Some(provenance(record)),
                });
                mappings.entry(record.id).or_default().push(id);
            }
        }

        let supported: BTreeSet<&str> = [
            "CARTESIAN_POINT", "POLY_LOOP", "FACE_OUTER_BOUND", "FACE_BOUND",
            "ADVANCED_FACE", "PLANE", "AXIS2_PLACEMENT_3D", "DIRECTION",
            "SI_UNIT", "LENGTH_UNIT", "NAMED_UNIT",
        ].into_iter().collect();
        let geometry_markers = [
            "B_SPLINE", "CYLINDRICAL_SURFACE", "CONICAL_SURFACE", "TOROIDAL_SURFACE",
            "MANIFOLD_SOLID_BREP", "CLOSED_SHELL", "OPEN_SHELL", "EDGE_CURVE",
        ];
        let unsupported: BTreeSet<String> = records.iter()
            .filter(|record| !supported.contains(record.kind.as_str())
                && geometry_markers.iter().any(|marker| record.kind.contains(marker)))
            .map(|record| record.kind.clone())
            .collect();
        for kind in &unsupported {
            diagnostics.push(ImportDiagnostic {
                severity: if options.fail_on_unsupported_geometry {
                    ImportDiagnosticSeverity::Error
                } else {
                    ImportDiagnosticSeverity::Warning
                },
                code: "step_unsupported_geometry".to_owned(),
                message: format!("{} is not mapped by the reference STEP adapter", kind),
                source_entity_id: None,
            });
        }

        let mapping_records = records.iter()
            .filter_map(|record| {
                let targets = mappings.get(&record.id)?.clone();
                Some(SourceEntityMapping {
                    source_entity_id: format!("#{}", record.id),
                    source_entity_type: record.kind.clone(),
                    source_global_id: None,
                    source_name: first_string(&record.args),
                    target_entity_ids: targets,
                    source_record_sha256: sha256_hex(record.normalized.as_bytes()),
                    normalized_source_record: options.preserve_source_records
                        .then(|| record.normalized.clone()),
                    properties: BTreeMap::new(),
                })
            })
            .collect::<Vec<_>>();

        let face_ids = faces.iter().map(|face| face.id).collect::<Vec<_>>();
        let shell_id = stable_uuid("imported-open-shell");
        let body_id = stable_uuid("imported-step-body");
        let shells = if face_ids.is_empty() { vec![] } else {
            vec![Shell {
                id: shell_id,
                face_ids,
                closed: false,
                provenance: None,
            }]
        };
        let bodies = if shells.is_empty() { vec![] } else {
            vec![Body {
                id: body_id,
                name: Some("Imported STEP surface model".to_owned()),
                shell_ids: vec![shell_id],
                is_solid: false,
                provenance: None,
            }]
        };

        let report = GeometryImportReport {
            adapter_id: self.id().to_owned(),
            adapter_version: self.version().to_owned(),
            source_format: self.source_format().to_owned(),
            source_schema: schema.clone(),
            source_sha256: source_sha256.clone(),
            source_length_unit: Some(unit_name.clone()),
            source_length_scale_to_m: scale,
            parsed_entity_count: records.len(),
            mapped_entity_count: mapping_records.len(),
            unsupported_entity_types: unsupported.into_iter().collect(),
            diagnostics,
            mappings: mapping_records,
        };
        let document = GeometryDocument {
            schema_version: GEOMETRY_SCHEMA_VERSION.to_owned(),
            document_id: stable_uuid(&format!("document:{}", source_sha256)),
            revision_id: stable_uuid(&format!("revision:{}", source_sha256)),
            name: source_name.unwrap_or("Imported STEP model").to_owned(),
            tolerance: options.tolerance,
            brep: BrepModel { vertices, edges, wires, faces, shells, bodies },
            meshes: vec![],
            provenance: Some(GeometryProvenance {
                source_format: "STEP-SPF".to_owned(),
                source_document: source_name.map(str::to_owned),
                source_entity_id: None,
                source_revision: schema,
                source_units: Some(unit_name),
                adapter_id: self.id().to_owned(),
                adapter_version: self.version().to_owned(),
                source_sha256: Some(source_sha256),
            }),
        };
        let _ = by_id;
        Ok(GeometryImportOutcome { document, report })
    }
}

fn parse_records(source: &str) -> Result<Vec<Record>> {
    let upper = source.to_ascii_uppercase();
    let start = upper.find("DATA;").context("missing DATA section")? + 5;
    let end = upper[start..].find("ENDSEC;").map(|v| start + v)
        .context("missing DATA ENDSEC")?;
    let mut records = Vec::new();
    for statement in split_statements(&source[start..end]) {
        let normalized = statement.split_whitespace().collect::<String>();
        if !normalized.starts_with('#') { continue; }
        let (left, right) = normalized.trim_end_matches(';')
            .split_once('=').context("invalid STEP assignment")?;
        let id = left[1..].parse::<u64>().context("invalid STEP id")?;
        let open = right.find('(').context("missing STEP argument list")?;
        let close = right.rfind(')').context("unclosed STEP argument list")?;
        records.push(Record {
            id,
            kind: right[..open].to_ascii_uppercase(),
            args: right[open + 1..close].to_owned(),
            normalized,
        });
    }
    records.sort_by_key(|record| record.id);
    Ok(records)
}

fn split_statements(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        current.push(ch);
        if ch == '\'' {
            if quoted && chars.peek() == Some(&'\'') {
                current.push(chars.next().unwrap());
            } else {
                quoted = !quoted;
            }
        } else if ch == ';' && !quoted {
            result.push(current.trim().to_owned());
            current.clear();
        }
    }
    result
}

fn parse_schema(source: &str) -> Option<String> {
    let tail = &source[source.to_ascii_uppercase().find("FILE_SCHEMA")?..];
    let first = tail.find('\'')? + 1;
    let second = tail[first..].find('\'')? + first;
    Some(tail[first..second].to_ascii_uppercase())
}

fn detect_length_unit(records: &[Record]) -> (f64, String) {
    for record in records {
        if record.kind == "SI_UNIT" && record.args.to_ascii_uppercase().contains(".METRE.") {
            let upper = record.args.to_ascii_uppercase();
            if upper.contains(".MILLI.") { return (1.0e-3, "mm".to_owned()); }
            if upper.contains(".CENTI.") { return (1.0e-2, "cm".to_owned()); }
            return (1.0, "m".to_owned());
        }
    }
    (1.0, "m (assumed)".to_owned())
}

fn parse_point(args: &str) -> Result<[f64; 3]> {
    let open = args.rfind('(').context("point coordinate tuple missing")?;
    let close = args[open..].find(')').map(|v| open + v)
        .context("point coordinate tuple unclosed")?;
    let values = args[open + 1..close].split(',')
        .map(|value| value.trim().parse::<f64>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match values.as_slice() {
        [x, y] => Ok([*x, *y, 0.0]),
        [x, y, z, ..] => Ok([*x, *y, *z]),
        _ => bail!("point requires two or three coordinates"),
    }
}

fn extract_refs(input: &str) -> Vec<u64> {
    let bytes = input.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() { index += 1; }
            if let Ok(value) = input[start..index].parse() { output.push(value); }
        } else { index += 1; }
    }
    output
}

fn first_string(input: &str) -> Option<String> {
    let first = input.find('\'')? + 1;
    let second = input[first..].find('\'')? + first;
    Some(input[first..second].replace("''", "'"))
}

fn diag(
    severity: ImportDiagnosticSeverity,
    code: &str,
    message: String,
    id: u64,
) -> ImportDiagnostic {
    ImportDiagnostic {
        severity,
        code: code.to_owned(),
        message,
        source_entity_id: Some(format!("#{}", id)),
    }
}

fn entity_uuid(id: u64, purpose: &str) -> Uuid {
    stable_uuid(&format!("entity:{}:{}", id, purpose))
}

fn stable_uuid(value: &str) -> Uuid {
    Uuid::new_v5(&STEP_NAMESPACE, value.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_polygonal_advanced_face() {
        let bytes = include_bytes!("../tests/fixtures/ap242-triangle.stp");
        let outcome = StepAdapter.import(
            bytes,
            Some("ap242-triangle.stp"),
            GeometryImportOptions::default(),
        ).unwrap();
        assert!(!outcome.report.has_errors());
        assert_eq!(outcome.document.brep.faces.len(), 1);
        assert_eq!(outcome.document.brep.vertices.len(), 3);
    }
}
