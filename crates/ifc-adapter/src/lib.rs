//! Deterministic, dependency-light IFC STEP Physical File import adapter.
//!
//! Pass 5 intentionally supports a constrained, auditable subset:
//! IFC2X3/IFC4 headers, SI length units, Cartesian points, polylines,
//! IfcCartesianPointList3D and IfcTriangulatedFaceSet. Unsupported geometry is
//! diagnosed instead of guessed. A production adapter may replace this parser
//! behind the same `GeometryImportAdapter` contract.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use structural_geometry_api::*;
use structural_units::Length;
use uuid::Uuid;

const IFC_NAMESPACE: Uuid =
    Uuid::from_u128(0x90d7142f_61cd_49df_ae95_813d4ed75ea5);

#[derive(Debug, Default)]
pub struct IfcAdapter;

#[derive(Debug, Clone)]
struct Record {
    step_id: u64,
    entity_type: String,
    arguments: String,
    normalized: String,
}

impl GeometryImportAdapter for IfcAdapter {
    fn id(&self) -> &'static str {
        "structural-ifc-spf"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn source_format(&self) -> &'static str {
        "IFC-SPF"
    }

    fn import(
        &self,
        bytes: &[u8],
        source_name: Option<&str>,
        options: GeometryImportOptions,
    ) -> Result<GeometryImportOutcome> {
        if !options.tolerance.is_valid() {
            bail!("invalid geometry tolerance");
        }
        let source = std::str::from_utf8(bytes).context("IFC-SPF must be UTF-8 for this adapter")?;
        let source_sha256 = sha256_hex(bytes);
        let schema = parse_schema(source);
        let records = parse_records(source)?;
        if records.is_empty() {
            bail!("IFC file contains no parsed DATA entities");
        }

        let by_id: HashMap<u64, &Record> = records.iter().map(|r| (r.step_id, r)).collect();
        let (length_scale, length_name) = detect_length_unit(&records);
        let revision_id = stable_uuid(&format!("revision:{source_sha256}"));
        let document_id = stable_uuid(&format!(
            "document:{}:{}",
            source_name.unwrap_or("unnamed.ifc"),
            source_sha256
        ));

        let base_provenance = |record: &Record| GeometryProvenance {
            source_format: "IFC-SPF".to_owned(),
            source_document: source_name.map(str::to_owned),
            source_entity_id: Some(format!("#{}", record.step_id)),
            source_revision: schema.clone(),
            source_units: Some(length_name.clone()),
            adapter_id: self.id().to_owned(),
            adapter_version: self.version().to_owned(),
            source_sha256: Some(source_sha256.clone()),
        };

        let mut diagnostics = Vec::new();
        let mut vertices = Vec::new();
        let mut edges = Vec::new();
        let mut wires = Vec::new();
        let mut meshes = Vec::new();
        let mut mapped_targets: HashMap<u64, Vec<Uuid>> = HashMap::new();
        let mut point_vertex_ids: HashMap<u64, Uuid> = HashMap::new();

        // Standalone Cartesian points and polyline references.
        for record in &records {
            if record.entity_type == "IFCCARTESIANPOINT" {
                match parse_cartesian_point(&record.arguments) {
                    Ok(coords) => {
                        let id = entity_uuid(record.step_id, "vertex");
                        point_vertex_ids.insert(record.step_id, id);
                        vertices.push(Vertex {
                            id,
                            point: scaled_point(coords, length_scale),
                            tolerance_m: Length::ZERO,
                            provenance: Some(base_provenance(record)),
                        });
                        mapped_targets.entry(record.step_id).or_default().push(id);
                    }
                    Err(error) => diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "ifc_invalid_cartesian_point",
                        error.to_string(),
                        record,
                    )),
                }
            }
        }

        for record in &records {
            if record.entity_type == "IFCPOLYLINE" {
                match parse_reference_list(&record.arguments) {
                    Ok(point_refs) if point_refs.len() >= 2 => {
                        let mut oriented = Vec::new();
                        let mut failed = false;
                        for pair in point_refs.windows(2) {
                            let Some(start) = point_vertex_ids.get(&pair[0]).copied() else {
                                diagnostics.push(diag(
                                    ImportDiagnosticSeverity::Error,
                                    "ifc_polyline_missing_point",
                                    format!("polyline references missing point #{}", pair[0]),
                                    record,
                                ));
                                failed = true;
                                break;
                            };
                            let Some(end) = point_vertex_ids.get(&pair[1]).copied() else {
                                diagnostics.push(diag(
                                    ImportDiagnosticSeverity::Error,
                                    "ifc_polyline_missing_point",
                                    format!("polyline references missing point #{}", pair[1]),
                                    record,
                                ));
                                failed = true;
                                break;
                            };
                            let edge_id = stable_uuid(&format!(
                                "ifc-edge:{}:{}:{}",
                                record.step_id, pair[0], pair[1]
                            ));
                            edges.push(Edge {
                                id: edge_id,
                                start_vertex_id: start,
                                end_vertex_id: end,
                                curve_kind: CurveKind::Line,
                                provenance: Some(base_provenance(record)),
                            });
                            oriented.push(OrientedEdge {
                                edge_id,
                                reversed: false,
                            });
                            mapped_targets.entry(record.step_id).or_default().push(edge_id);
                        }
                        if !failed {
                            let wire_id = entity_uuid(record.step_id, "wire");
                            let closed = point_refs.first() == point_refs.last();
                            wires.push(Wire {
                                id: wire_id,
                                edges: oriented,
                                closed,
                                provenance: Some(base_provenance(record)),
                            });
                            mapped_targets.entry(record.step_id).or_default().push(wire_id);
                        }
                    }
                    Ok(_) => diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "ifc_polyline_too_short",
                        "IfcPolyline must contain at least two points".to_owned(),
                        record,
                    )),
                    Err(error) => diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "ifc_invalid_polyline",
                        error.to_string(),
                        record,
                    )),
                }
            }
        }

        // IFC4 tessellated geometry.
        let mut point_lists: HashMap<u64, Vec<[f64; 3]>> = HashMap::new();
        for record in &records {
            if record.entity_type == "IFCCARTESIANPOINTLIST3D" {
                match parse_point_list_3d(&record.arguments) {
                    Ok(points) => {
                        point_lists.insert(record.step_id, points);
                    }
                    Err(error) => diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "ifc_invalid_point_list_3d",
                        error.to_string(),
                        record,
                    )),
                }
            }
        }

        for record in &records {
            if record.entity_type == "IFCTRIANGULATEDFACESET" {
                match parse_triangulated_face_set(&record.arguments) {
                    Ok((point_list_ref, indices)) => {
                        let Some(points) = point_lists.get(&point_list_ref) else {
                            diagnostics.push(diag(
                                ImportDiagnosticSeverity::Error,
                                "ifc_face_set_missing_point_list",
                                format!("missing IfcCartesianPointList3D #{}", point_list_ref),
                                record,
                            ));
                            continue;
                        };
                        let mesh_id = entity_uuid(record.step_id, "mesh");
                        let mesh_vertices: Vec<MeshVertex> = points
                            .iter()
                            .enumerate()
                            .map(|(index, coordinates)| MeshVertex {
                                id: stable_uuid(&format!(
                                    "ifc-mesh-vertex:{}:{}",
                                    record.step_id,
                                    index + 1
                                )),
                                point: scaled_point(*coordinates, length_scale),
                                source_vertex_id: None,
                            })
                            .collect();
                        let mut triangles = Vec::new();
                        for (triangle_index, item) in indices.iter().enumerate() {
                            if item.iter().any(|value| *value == 0 || *value > mesh_vertices.len()) {
                                diagnostics.push(diag(
                                    ImportDiagnosticSeverity::Error,
                                    "ifc_triangle_index_out_of_range",
                                    format!("triangle {} has an invalid 1-based index", triangle_index + 1),
                                    record,
                                ));
                                continue;
                            }
                            triangles.push(Triangle {
                                id: stable_uuid(&format!(
                                    "ifc-triangle:{}:{}",
                                    record.step_id,
                                    triangle_index + 1
                                )),
                                vertex_ids: [
                                    mesh_vertices[item[0] - 1].id,
                                    mesh_vertices[item[1] - 1].id,
                                    mesh_vertices[item[2] - 1].id,
                                ],
                                source_face_id: None,
                            });
                        }
                        meshes.push(TriangleMesh {
                            id: mesh_id,
                            vertices: mesh_vertices,
                            triangles,
                            provenance: Some(base_provenance(record)),
                        });
                        mapped_targets.entry(record.step_id).or_default().push(mesh_id);
                        mapped_targets
                            .entry(point_list_ref)
                            .or_default()
                            .push(mesh_id);
                    }
                    Err(error) => diagnostics.push(diag(
                        ImportDiagnosticSeverity::Error,
                        "ifc_invalid_triangulated_face_set",
                        error.to_string(),
                        record,
                    )),
                }
            }
        }

        let unsupported_geometry_types = unsupported_geometry_types(&records);
        for entity_type in &unsupported_geometry_types {
            diagnostics.push(ImportDiagnostic {
                severity: if options.fail_on_unsupported_geometry {
                    ImportDiagnosticSeverity::Error
                } else {
                    ImportDiagnosticSeverity::Warning
                },
                code: "ifc_unsupported_geometry".to_owned(),
                message: format!(
                    "{} is present but is not mapped by the Pass 5 reference adapter",
                    entity_type
                ),
                source_entity_id: None,
            });
        }

        let properties = collect_properties(&records, &by_id);
        let mut mappings = Vec::new();
        for record in &records {
            let targets = mapped_targets.get(&record.step_id).cloned().unwrap_or_default();
            let identity = parse_root_identity(record);
            let entity_properties = properties
                .get(&record.step_id)
                .cloned()
                .unwrap_or_default();
            if !targets.is_empty() || identity.0.is_some() || !entity_properties.is_empty() {
                mappings.push(SourceEntityMapping {
                    source_entity_id: format!("#{}", record.step_id),
                    source_entity_type: record.entity_type.clone(),
                    source_global_id: identity.0,
                    source_name: identity.1,
                    target_entity_ids: targets,
                    source_record_sha256: sha256_hex(record.normalized.as_bytes()),
                    normalized_source_record: options
                        .preserve_source_records
                        .then(|| record.normalized.clone()),
                    properties: entity_properties,
                });
            }
        }
        mappings.sort_by_key(|item| parse_step_id(&item.source_entity_id));

        let report = GeometryImportReport {
            adapter_id: self.id().to_owned(),
            adapter_version: self.version().to_owned(),
            source_format: self.source_format().to_owned(),
            source_schema: schema.clone(),
            source_sha256: source_sha256.clone(),
            source_length_unit: Some(length_name),
            source_length_scale_to_m: length_scale,
            parsed_entity_count: records.len(),
            mapped_entity_count: mappings.len(),
            unsupported_entity_types: unsupported_geometry_types,
            diagnostics,
            mappings,
        };

        let document = GeometryDocument {
            schema_version: GEOMETRY_SCHEMA_VERSION.to_owned(),
            document_id,
            revision_id,
            name: source_name.unwrap_or("Imported IFC model").to_owned(),
            tolerance: options.tolerance,
            brep: BrepModel {
                vertices,
                edges,
                wires,
                faces: vec![],
                shells: vec![],
                bodies: vec![],
            },
            meshes,
            provenance: Some(GeometryProvenance {
                source_format: "IFC-SPF".to_owned(),
                source_document: source_name.map(str::to_owned),
                source_entity_id: None,
                source_revision: schema,
                source_units: report.source_length_unit.clone(),
                adapter_id: self.id().to_owned(),
                adapter_version: self.version().to_owned(),
                source_sha256: Some(source_sha256),
            }),
        };

        if report.has_errors() {
            // Return the evidence-rich outcome for mapping diagnostics. Only malformed
            // container-level input is a hard Rust error.
        }
        Ok(GeometryImportOutcome { document, report })
    }
}

fn parse_records(source: &str) -> Result<Vec<Record>> {
    let upper = source.to_ascii_uppercase();
    let data_start = upper.find("DATA;").context("missing DATA section")? + 5;
    let data_end = upper[data_start..]
        .find("ENDSEC;")
        .map(|index| data_start + index)
        .context("missing DATA ENDSEC")?;
    let data = strip_block_comments(&source[data_start..data_end]);
    let mut records = Vec::new();
    for statement in split_statements(&data) {
        let normalized = normalize_statement(&statement);
        if !normalized.starts_with('#') {
            continue;
        }
        let (left, right) = normalized
            .split_once('=')
            .context("invalid IFC entity assignment")?;
        let step_id = left[1..].parse::<u64>().context("invalid IFC STEP id")?;
        let open = right.find('(').context("missing entity argument list")?;
        let close = right.rfind(')').context("unclosed entity argument list")?;
        let entity_type = right[..open].trim().to_ascii_uppercase();
        records.push(Record {
            step_id,
            entity_type,
            arguments: right[open + 1..close].to_owned(),
            normalized: format!("{};", normalized),
        });
    }
    records.sort_by_key(|record| record.step_id);
    Ok(records)
}

fn split_statements(input: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        current.push(ch);
        if ch == '\'' {
            if in_string && chars.peek() == Some(&'\'') {
                current.push(chars.next().unwrap());
            } else {
                in_string = !in_string;
            }
        } else if ch == ';' && !in_string {
            output.push(current.trim().to_owned());
            current.clear();
        }
    }
    output
}

fn strip_block_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if index + 1 < bytes.len() && bytes[index] == b'/' && bytes[index + 1] == b'*' {
            index += 2;
            while index + 1 < bytes.len()
                && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
            {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
        } else {
            output.push(bytes[index] as char);
            index += 1;
        }
    }
    output
}

fn normalize_statement(input: &str) -> String {
    let mut output = String::new();
    let mut in_string = false;
    let mut pending_space = false;
    for ch in input.trim().trim_end_matches(';').chars() {
        if ch == '\'' {
            in_string = !in_string;
            output.push(ch);
            pending_space = false;
        } else if ch.is_whitespace() && !in_string {
            pending_space = true;
        } else {
            if pending_space && !matches!(ch, ',' | ')' | '(' | '=') {
                output.push(' ');
            }
            pending_space = false;
            output.push(ch);
        }
    }
    output
}

fn parse_schema(source: &str) -> Option<String> {
    let upper = source.to_ascii_uppercase();
    let marker = "FILE_SCHEMA";
    let start = upper.find(marker)?;
    let tail = &source[start + marker.len()..];
    let first_quote = tail.find('\'')?;
    let remainder = &tail[first_quote + 1..];
    let second_quote = remainder.find('\'')?;
    Some(remainder[..second_quote].to_ascii_uppercase())
}

fn detect_length_unit(records: &[Record]) -> (f64, String) {
    for record in records {
        if record.entity_type == "IFCSIUNIT"
            && record.arguments.to_ascii_uppercase().contains(".LENGTHUNIT.")
            && record.arguments.to_ascii_uppercase().contains(".METRE.")
        {
            let upper = record.arguments.to_ascii_uppercase();
            if upper.contains(".MILLI.") {
                return (1.0e-3, "mm".to_owned());
            }
            if upper.contains(".CENTI.") {
                return (1.0e-2, "cm".to_owned());
            }
            if upper.contains(".DECI.") {
                return (1.0e-1, "dm".to_owned());
            }
            if upper.contains(".KILO.") {
                return (1.0e3, "km".to_owned());
            }
            return (1.0, "m".to_owned());
        }
    }
    (1.0, "m (assumed; no supported IfcSIUnit found)".to_owned())
}

fn parse_cartesian_point(arguments: &str) -> Result<[f64; 3]> {
    let groups = nested_numeric_groups(arguments)?;
    let values = groups.first().context("point has no coordinate tuple")?;
    match values.as_slice() {
        [x, y] => Ok([*x, *y, 0.0]),
        [x, y, z, ..] => Ok([*x, *y, *z]),
        _ => bail!("point must contain two or three coordinates"),
    }
}

fn parse_point_list_3d(arguments: &str) -> Result<Vec<[f64; 3]>> {
    let groups = nested_numeric_groups(arguments)?;
    let points: Vec<[f64; 3]> = groups
        .into_iter()
        .filter(|values| values.len() >= 3)
        .map(|values| [values[0], values[1], values[2]])
        .collect();
    if points.is_empty() {
        bail!("IfcCartesianPointList3D contains no 3D coordinates");
    }
    Ok(points)
}

fn parse_reference_list(arguments: &str) -> Result<Vec<u64>> {
    let refs = extract_references(arguments);
    if refs.is_empty() {
        bail!("expected at least one STEP reference");
    }
    Ok(refs)
}

fn parse_triangulated_face_set(arguments: &str) -> Result<(u64, Vec<[usize; 3]>)> {
    let fields = split_top_level(arguments);
    let point_list_ref = fields
        .first()
        .and_then(|field| extract_references(field).first().copied())
        .context("IfcTriangulatedFaceSet has no coordinate-list reference")?;
    let coordinate_index = fields
        .iter()
        .rev()
        .find_map(|field| {
            let groups = nested_integer_groups(field).ok()?;
            let triangles: Vec<[usize; 3]> = groups
                .into_iter()
                .filter(|values| values.len() == 3)
                .map(|values| [values[0], values[1], values[2]])
                .collect();
            (!triangles.is_empty()).then_some(triangles)
        })
        .context("IfcTriangulatedFaceSet has no triangle index list")?;
    Ok((point_list_ref, coordinate_index))
}

fn nested_numeric_groups(input: &str) -> Result<Vec<Vec<f64>>> {
    // Collect innermost parenthesized numeric tuples. This handles both
    // IfcCartesianPoint((x,y,z)) at record level (arguments are "(x,y,z)")
    // and nested lists such as "((x,y,z),(x,y,z))".
    let mut groups = Vec::new();
    let mut starts = Vec::new();
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => starts.push(index + ch.len_utf8()),
            ')' => {
                if let Some(start) = starts.pop() {
                    let content = &input[start..index];
                    if !content.contains('(') && !content.contains(')') {
                        let tokens: Vec<&str> = content
                            .split(',')
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .collect();
                        if !tokens.is_empty() {
                            let mut values = Vec::with_capacity(tokens.len());
                            let mut numeric = true;
                            for token in tokens {
                                match parse_real(token) {
                                    Ok(value) => values.push(value),
                                    Err(_) => {
                                        numeric = false;
                                        break;
                                    }
                                }
                            }
                            if numeric {
                                groups.push(values);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(groups)
}

fn nested_integer_groups(input: &str) -> Result<Vec<Vec<usize>>> {
    Ok(nested_numeric_groups(input)?
        .into_iter()
        .map(|group| group.into_iter().map(|value| value as usize).collect())
        .collect())
}

fn parse_real(value: &str) -> Result<f64> {
    value
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid IFC real '{}'", value.trim()))
}

fn split_top_level(input: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                current.push(ch);
                if in_string && chars.peek() == Some(&'\'') {
                    current.push(chars.next().unwrap());
                } else {
                    in_string = !in_string;
                }
            }
            '(' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_string => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if !in_string && depth == 0 => {
                fields.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    fields.push(current.trim().to_owned());
    fields
}

fn extract_references(input: &str) -> Vec<u64> {
    let bytes = input.as_bytes();
    let mut refs = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if let Ok(value) = input[start..index].parse() {
                refs.push(value);
            }
        } else {
            index += 1;
        }
    }
    refs
}

fn collect_properties(
    records: &[Record],
    by_id: &HashMap<u64, &Record>,
) -> HashMap<u64, BTreeMap<String, String>> {
    let mut property_values: HashMap<u64, (String, String)> = HashMap::new();
    for record in records {
        if record.entity_type == "IFCPROPERTYSINGLEVALUE" {
            let fields = split_top_level(&record.arguments);
            if let Some(name) = fields.first().and_then(|field| parse_string(field)) {
                let value = fields
                    .get(2)
                    .and_then(|field| parse_typed_or_plain_value(field))
                    .unwrap_or_else(|| "<unset>".to_owned());
                property_values.insert(record.step_id, (name, value));
            }
        }
    }

    let mut property_sets: HashMap<u64, BTreeMap<String, String>> = HashMap::new();
    for record in records {
        if record.entity_type == "IFCPROPERTYSET" {
            let fields = split_top_level(&record.arguments);
            let set_name = fields
                .get(2)
                .and_then(|field| parse_string(field))
                .unwrap_or_else(|| format!("#{}", record.step_id));
            let mut values = BTreeMap::new();
            if let Some(reference_field) = fields.get(4) {
                for property_ref in extract_references(reference_field) {
                    if let Some((name, value)) = property_values.get(&property_ref) {
                        values.insert(format!("{}.{}", set_name, name), value.clone());
                    }
                }
            }
            property_sets.insert(record.step_id, values);
        }
    }

    let mut output: HashMap<u64, BTreeMap<String, String>> = HashMap::new();
    for record in records {
        if record.entity_type == "IFCRELDEFINESBYPROPERTIES" {
            let fields = split_top_level(&record.arguments);
            if fields.len() < 6 {
                continue;
            }
            let related = extract_references(&fields[4]);
            let set_ref = extract_references(&fields[5]).first().copied();
            if let Some(values) = set_ref.and_then(|id| property_sets.get(&id)) {
                for object_ref in related {
                    output.entry(object_ref).or_default().extend(values.clone());
                }
            }
        }
    }

    // Keep the reference map meaningfully used and ready for richer relationship traversal.
    let _known_record_count = by_id.len();
    output
}

fn parse_root_identity(record: &Record) -> (Option<String>, Option<String>) {
    let non_root = matches!(
        record.entity_type.as_str(),
        "IFCCARTESIANPOINT"
            | "IFCCARTESIANPOINTLIST3D"
            | "IFCTRIANGULATEDFACESET"
            | "IFCPOLYLINE"
            | "IFCSIUNIT"
            | "IFCPROPERTYSINGLEVALUE"
    );
    if non_root {
        return (None, None);
    }
    let fields = split_top_level(&record.arguments);
    let global_id = fields.first().and_then(|field| parse_string(field));
    let name = fields.get(2).and_then(|field| parse_string(field));
    (global_id, name)
}

fn parse_string(field: &str) -> Option<String> {
    let trimmed = field.trim();
    if !trimmed.starts_with('\'') || !trimmed.ends_with('\'') || trimmed.len() < 2 {
        return None;
    }
    Some(trimmed[1..trimmed.len() - 1].replace("''", "'"))
}

fn parse_typed_or_plain_value(field: &str) -> Option<String> {
    let trimmed = field.trim();
    if let Some(value) = parse_string(trimmed) {
        return Some(value);
    }
    if let (Some(open), Some(close)) = (trimmed.find('('), trimmed.rfind(')')) {
        return parse_string(&trimmed[open + 1..close])
            .or_else(|| Some(trimmed[open + 1..close].to_owned()));
    }
    (!matches!(trimmed, "$" | "*")).then(|| trimmed.to_owned())
}

fn unsupported_geometry_types(records: &[Record]) -> Vec<String> {
    const UNSUPPORTED: &[&str] = &[
        "IFCEXTRUDEDAREASOLID",
        "IFCREVOLVEDAREASOLID",
        "IFCFACETEDBREP",
        "IFCADVANCEDBREP",
        "IFCBOOLEANCLIPPINGRESULT",
        "IFCBOOLEANRESULT",
        "IFCMAPPEDITEM",
        "IFCPOLYGONALFACESET",
        "IFCSHELLBASEDSURFACEMODEL",
    ];
    let present: BTreeSet<String> = records
        .iter()
        .filter(|record| UNSUPPORTED.contains(&record.entity_type.as_str()))
        .map(|record| record.entity_type.clone())
        .collect();
    present.into_iter().collect()
}

fn scaled_point(value: [f64; 3], scale: f64) -> Point3 {
    Point3::from_metres(value[0] * scale, value[1] * scale, value[2] * scale)
}

fn entity_uuid(step_id: u64, role: &str) -> Uuid {
    stable_uuid(&format!("ifc:{step_id}:{role}"))
}

fn stable_uuid(value: &str) -> Uuid {
    Uuid::new_v5(&IFC_NAMESPACE, value.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn diag(
    severity: ImportDiagnosticSeverity,
    code: &str,
    message: String,
    record: &Record,
) -> ImportDiagnostic {
    ImportDiagnostic {
        severity,
        code: code.to_owned(),
        message,
        source_entity_id: Some(format!("#{}", record.step_id)),
    }
}

fn parse_step_id(value: &str) -> u64 {
    value.trim_start_matches('#').parse().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_keep_semicolons_inside_strings() {
        let input = "#1=IFCPROPERTYSINGLEVALUE('a;b',$,IFCLABEL('x'),$);#2=IFCCARTESIANPOINT((0.,0.));";
        let statements = split_statements(input);
        assert_eq!(statements.len(), 2);
    }

    #[test]
    fn top_level_split_ignores_nested_commas() {
        let fields = split_top_level("#1,$,.T.,((1,2,3),(1,3,4)),$");
        assert_eq!(fields.len(), 5);
    }

    #[test]
    fn deterministic_entity_identifiers() {
        assert_eq!(entity_uuid(20, "mesh"), entity_uuid(20, "mesh"));
        assert_ne!(entity_uuid(20, "mesh"), entity_uuid(21, "mesh"));
    }
}
