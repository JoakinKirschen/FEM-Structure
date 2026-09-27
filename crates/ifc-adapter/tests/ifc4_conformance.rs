use structural_geometry_api::{
    GeometryImportAdapter, GeometryImportOptions, ImportDiagnosticSeverity,
};
use structural_ifc_adapter::IfcAdapter;

const FIXTURE: &[u8] = include_bytes!("fixtures/ifc4-triangle.ifc");

#[test]
fn imports_ifc4_tessellation_with_units_and_traceability() {
    let outcome = IfcAdapter
        .import(
            FIXTURE,
            Some("ifc4-triangle.ifc"),
            GeometryImportOptions::default(),
        )
        .expect("fixture import");

    assert_eq!(outcome.report.source_schema.as_deref(), Some("IFC4"));
    assert_eq!(outcome.report.source_length_unit.as_deref(), Some("mm"));
    assert!((outcome.report.source_length_scale_to_m - 0.001).abs() < 1.0e-12);
    assert_eq!(outcome.document.meshes.len(), 1);
    assert_eq!(outcome.document.meshes[0].triangles.len(), 1);
    assert!((outcome.document.meshes[0].vertices[1].point.xyz_m[0].metres() - 5.0).abs() < 1.0e-12);
    assert!(!outcome
        .report
        .diagnostics
        .iter()
        .any(|item| item.severity == ImportDiagnosticSeverity::Error));

    let product = outcome
        .report
        .mappings
        .iter()
        .find(|item| item.source_entity_id == "#40")
        .expect("product mapping");
    assert_eq!(
        product.source_global_id.as_deref(),
        Some("0YvctVUKr0kugbFTf53O9L")
    );
    assert_eq!(product.source_name.as_deref(), Some("Imported panel"));
    assert_eq!(
        product.properties.get("Pset_Test.Reference").map(String::as_str),
        Some("TRI-01")
    );
}

#[test]
fn import_is_deterministic_for_identical_bytes_and_name() {
    let adapter = IfcAdapter;
    let first = adapter
        .import(FIXTURE, Some("same.ifc"), GeometryImportOptions::default())
        .unwrap();
    let second = adapter
        .import(FIXTURE, Some("same.ifc"), GeometryImportOptions::default())
        .unwrap();
    assert_eq!(first.document, second.document);
    assert_eq!(first.report, second.report);
}


#[test]
fn imports_cartesian_points_and_polyline() {
    let source = br#"ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((0.,0.,0.));
#11=IFCCARTESIANPOINT((2.,0.,0.));
#12=IFCPOLYLINE((#10,#11));
ENDSEC;
END-ISO-10303-21;"#;
    let outcome = IfcAdapter
        .import(source, Some("line.ifc"), GeometryImportOptions::default())
        .unwrap();
    assert_eq!(outcome.document.brep.vertices.len(), 2);
    assert_eq!(outcome.document.brep.edges.len(), 1);
    assert_eq!(outcome.document.brep.wires.len(), 1);
    assert!((outcome.document.brep.vertices[1].point.xyz_m[0].metres() - 2.0).abs() < 1.0e-12);
}
