use serde_json::json;
use structural_automation_api::{
    ApiRequest, ApiStatus, Capability, AUTOMATION_API_VERSION, AUTOMATION_SCHEMA_VERSION,
};
use structural_automation_host::{describe, invoke, CancellationToken};
use uuid::Uuid;

#[test]
fn v1_description_matches_checked_in_contract() {
    let description = describe();
    assert_eq!(description.api_version, AUTOMATION_API_VERSION);
    assert_eq!(description.schema_version, AUTOMATION_SCHEMA_VERSION);
    assert_eq!(description.minimum_client_api_version, "1.0");
    assert_eq!(description.operations.len(), 5);
}

#[test]
fn v1_request_round_trip_remains_compatible() {
    let fixture = include_str!("fixtures/v1-describe-request.json");
    let request: ApiRequest = serde_json::from_str(fixture).unwrap();
    let encoded = serde_json::to_value(&request).unwrap();
    assert_eq!(encoded["schema_version"], AUTOMATION_SCHEMA_VERSION);
    let response = invoke(&request, &CancellationToken::default());
    assert_eq!(response.status, ApiStatus::Ok);
}

#[test]
fn unknown_methods_fail_without_panicking() {
    let response = invoke(
        &ApiRequest {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: Uuid::nil(),
            method: "future.unknown".to_owned(),
            params: json!({}),
            capabilities: vec![Capability::InspectSystem],
        },
        &CancellationToken::default(),
    );
    assert_eq!(response.status, ApiStatus::Error);
    assert_eq!(response.error.unwrap().code, "method_not_found");
}


#[test]
fn api_1_1_runs_nonlinear_analysis_with_explicit_capability() {
    let input: serde_json::Value =
        serde_json::from_str(include_str!("../../solver-nonlinear/tests/fixtures/linear-elastic.json"))
            .unwrap();
    let response = invoke(
        &ApiRequest {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: Uuid::from_u128(15),
            method: "analysis.solve_nonlinear".to_owned(),
            params: json!({
                "input": input,
                "options": {
                    "target_load_factor": 1.0,
                    "initial_load_increment": 0.1,
                    "minimum_load_increment": 0.0001,
                    "maximum_load_increment": 0.25,
                    "maximum_steps": 100,
                    "maximum_iterations_per_step": 25,
                    "maximum_cutbacks": 12,
                    "residual_absolute_tolerance_n": 1.0e-8,
                    "residual_relative_tolerance": 1.0e-9,
                    "displacement_increment_tolerance_m": 1.0e-12,
                    "restart_interval": 1,
                    "deterministic_profile": true
                }
            }),
            capabilities: vec![Capability::RunAnalysis],
        },
        &CancellationToken::default(),
    );
    assert_eq!(response.status, ApiStatus::Ok);
    let result = response.result.unwrap();
    assert_eq!(result["termination"], "converged");
}
