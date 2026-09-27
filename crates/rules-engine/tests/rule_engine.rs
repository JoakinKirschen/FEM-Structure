use structural_rules_api::{CheckStatus, RuleEvaluationInput, RulePackage};
use structural_rules_engine::{evaluate, resolve_rule_order};

fn fixtures() -> (RulePackage, RuleEvaluationInput) {
    let package = serde_json::from_str(include_str!("fixtures/demo-be-package.json")).unwrap();
    let input = serde_json::from_str(include_str!("fixtures/demo-evaluation-input.json")).unwrap();
    (package, input)
}

fn check<'a>(
    report: &'a structural_rules_api::RuleEvaluationReport,
    id: &str,
) -> &'a structural_rules_api::CheckResult {
    report.checks.iter().find(|check| check.rule_id == id).unwrap()
}

#[test]
fn resolves_rules_and_clauses_with_full_traceability() {
    let (package, input) = fixtures();
    let (report, evidence) = evaluate(&package, &input).unwrap();

    assert_eq!(
        report.evaluation_order,
        vec!["MODEL-SPAN-001", "SLS-DISP-001"]
    );
    assert_eq!(report.pass_count, 2);

    let displacement = check(&report, "SLS-DISP-001");
    assert_eq!(displacement.status, CheckStatus::Pass);
    assert_eq!(displacement.input_references.len(), 1);
    assert_eq!(displacement.resolved_parameters.len(), 1);
    assert_eq!(displacement.dependencies.len(), 1);
    assert!(displacement.dependencies[0].satisfied);

    let clause_ids: Vec<_> = displacement
        .resolved_clauses
        .iter()
        .map(|clause| clause.id.as_str())
        .collect();
    assert_eq!(clause_ids, vec!["basis", "na-limit", "scope", "sls-check"]);
    assert!(displacement
        .clause_refs
        .iter()
        .any(|reference| reference.clause == "SLS.2"));

    assert_eq!(report.package_sha256, evidence.package_sha256);
    assert_eq!(report.clause_graph_sha256, evidence.clause_graph_sha256);
}

#[test]
fn blocks_downstream_rule_when_required_rule_fails() {
    let (package, mut input) = fixtures();
    input.numeric_inputs.get_mut("member_span").unwrap().value = 0.0;

    let (report, _) = evaluate(&package, &input).unwrap();
    assert_eq!(report.fail_count, 1);
    assert_eq!(report.blocked_count, 1);
    assert_eq!(check(&report, "MODEL-SPAN-001").status, CheckStatus::Fail);
    assert_eq!(check(&report, "SLS-DISP-001").status, CheckStatus::Blocked);
}

#[test]
fn fails_limit_without_blocking_completed_prerequisite() {
    let (mut package, mut input) = fixtures();
    package.rules[1].depends_on[0].requirement =
        structural_rules_api::DependencyRequirement::Completed;
    input.numeric_inputs.get_mut("member_span").unwrap().value = 0.0;
    input
        .numeric_inputs
        .get_mut("maximum_displacement")
        .unwrap()
        .value = 0.021;

    let (report, _) = evaluate(&package, &input).unwrap();
    assert_eq!(report.fail_count, 2);
    assert_eq!(report.blocked_count, 0);
}

#[test]
fn rejects_rule_dependency_cycles() {
    let (mut package, _) = fixtures();
    package.rules[0]
        .depends_on
        .push(structural_rules_api::RuleDependency {
            rule_id: "SLS-DISP-001".to_owned(),
            requirement: structural_rules_api::DependencyRequirement::Pass,
            reason: "Deliberate test cycle.".to_owned(),
        });

    assert!(package.validate().is_err());
    assert!(resolve_rule_order(&package).is_err());
}

#[test]
fn rejects_clause_dependency_cycles() {
    let (mut package, _) = fixtures();
    package
        .clause_graph
        .nodes
        .get_mut("scope")
        .unwrap()
        .dependencies
        .push("sls-check".to_owned());

    assert!(package.validate().is_err());
}

#[test]
fn is_deterministic() {
    let (package, input) = fixtures();
    assert_eq!(
        evaluate(&package, &input).unwrap(),
        evaluate(&package, &input).unwrap()
    );
}

#[test]
fn rejects_implicit_unit_conversion() {
    let (package, mut input) = fixtures();
    input
        .numeric_inputs
        .get_mut("maximum_displacement")
        .unwrap()
        .unit = "mm".to_owned();

    let (report, _) = evaluate(&package, &input).unwrap();
    assert_eq!(report.error_count, 1);
    assert_eq!(check(&report, "SLS-DISP-001").status, CheckStatus::Error);
}
