use structural_domain::{migrate_analysis_input_json, MODEL_SCHEMA_VERSION};

#[test]
fn migrates_pass1_numeric_si_payload() {
    let json = include_str!("fixtures/pass1-input.json");
    let migrated = migrate_analysis_input_json(json).unwrap();

    assert_eq!(migrated.input.schema_version, MODEL_SCHEMA_VERSION);
    assert_eq!(migrated.steps.len(), 1);
    assert_eq!(migrated.input.model.nodes[0].xyz_m[0].metres(), 1.0);
    assert_eq!(
        migrated.input.model.springs[0]
            .stiffness_n_per_m
            .newtons_per_metre(),
        20_000.0
    );
    assert_eq!(
        migrated.input.model.load_cases[0].nodal_loads[0].force_n[0].newtons(),
        2_000.0
    );
}

#[test]
fn migration_is_deterministic() {
    let json = include_str!("fixtures/pass1-input.json");
    let first = migrate_analysis_input_json(json).unwrap();
    let second = migrate_analysis_input_json(json).unwrap();

    assert_eq!(
        first.input.model.global_coordinate_system_id,
        second.input.model.global_coordinate_system_id
    );
    assert_eq!(
        first.input.model.load_cases[0].id,
        second.input.model.load_cases[0].id
    );
}
