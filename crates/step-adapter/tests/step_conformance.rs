use structural_geometry_api::{GeometryImportAdapter, GeometryImportOptions};
use structural_step_adapter::StepAdapter;

#[test]
fn ap242_fixture_maps_to_a_valid_planar_face() {
    let outcome = StepAdapter
        .import(
            include_bytes!("fixtures/ap242-triangle.stp"),
            Some("ap242-triangle.stp"),
            GeometryImportOptions::default(),
        )
        .unwrap();
    assert!(!outcome.report.has_errors());
    assert_eq!(outcome.report.source_length_scale_to_m, 0.001);
    assert_eq!(outcome.document.brep.faces.len(), 1);
}
