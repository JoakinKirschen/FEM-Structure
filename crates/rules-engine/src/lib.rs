//! Deterministic reference evaluator for versioned rule packages.
//!
//! Pass 11 validates and resolves clause/rule dependency DAGs, evaluates rules in
//! stable topological order, and records why dependent checks ran or were blocked.

use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use structural_audit::hash_json;
use structural_rules_api::*;

pub const ENGINE_ID: &str = "structural-rules-reference";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn evaluate(
    package: &RulePackage,
    input: &RuleEvaluationInput,
) -> Result<(RuleEvaluationReport, RuleEvaluationEvidence)> {
    package.validate()?;
    if input.schema_version != RULE_EVALUATION_SCHEMA_VERSION {
        bail!(
            "unsupported rule-evaluation input schema '{}'",
            input.schema_version
        );
    }

    let package_sha256 = hash_json(package)?;
    let clause_graph_sha256 = hash_json(&package.clause_graph)?;
    let input_sha256 = hash_json(input)?;
    let (package_status, package_details) =
        evaluate_applicability(&package.applicability, input);
    let evaluation_order = resolve_rule_order(package)?;

    let rules: BTreeMap<&str, &LimitCheckRule> = package
        .rules
        .iter()
        .map(|rule| (rule.id.as_str(), rule))
        .collect();
    let mut statuses = BTreeMap::new();
    let mut checks = Vec::with_capacity(package.rules.len());

    for rule_id in &evaluation_order {
        let rule = rules
            .get(rule_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("resolved rule '{}' is missing", rule_id))?;
        let result = evaluate_rule(package, input, rule, package_status, &statuses);
        statuses.insert(rule_id.clone(), result.status);
        checks.push(result);
    }

    let pass_count = count(&checks, CheckStatus::Pass);
    let fail_count = count(&checks, CheckStatus::Fail);
    let not_applicable_count = count(&checks, CheckStatus::NotApplicable);
    let blocked_count = count(&checks, CheckStatus::Blocked);
    let error_count = count(&checks, CheckStatus::Error);

    let report = RuleEvaluationReport {
        schema_version: RULE_EVALUATION_SCHEMA_VERSION.to_owned(),
        evaluation_id: input.evaluation_id,
        package_id: package.metadata.package_id.clone(),
        package_version: package.metadata.package_version.clone(),
        package_sha256: package_sha256.clone(),
        clause_graph_sha256: clause_graph_sha256.clone(),
        national_annex: package.metadata.national_annex.clone(),
        package_applicability: package_status,
        package_applicability_details: package_details,
        evaluation_order,
        checks,
        pass_count,
        fail_count,
        not_applicable_count,
        blocked_count,
        error_count,
    };
    let report_sha256 = hash_json(&report)?;
    let evidence = RuleEvaluationEvidence {
        evidence_schema_version: "0.2".to_owned(),
        engine_id: ENGINE_ID.to_owned(),
        engine_version: ENGINE_VERSION.to_owned(),
        package_sha256,
        clause_graph_sha256,
        input_sha256,
        report_sha256,
        deterministic: true,
    };
    Ok((report, evidence))
}

/// Returns a stable topological order. Lexically smaller ready rule IDs run first.
pub fn resolve_rule_order(package: &RulePackage) -> Result<Vec<String>> {
    package.validate()?;

    let mut indegree: BTreeMap<String, usize> = package
        .rules
        .iter()
        .map(|rule| (rule.id.clone(), rule.depends_on.len()))
        .collect();
    let mut dependents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for rule in &package.rules {
        for dependency in &rule.depends_on {
            dependents
                .entry(dependency.rule_id.clone())
                .or_default()
                .insert(rule.id.clone());
        }
    }

    let mut ready: BTreeSet<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut order = Vec::with_capacity(package.rules.len());

    while let Some(id) = ready.iter().next().cloned() {
        ready.remove(&id);
        order.push(id.clone());
        if let Some(next) = dependents.get(&id) {
            for dependent in next {
                let degree = indegree
                    .get_mut(dependent)
                    .ok_or_else(|| anyhow::anyhow!("missing indegree for '{}'", dependent))?;
                *degree -= 1;
                if *degree == 0 {
                    ready.insert(dependent.clone());
                }
            }
        }
    }

    if order.len() != package.rules.len() {
        bail!("rule dependency graph could not be topologically resolved");
    }
    Ok(order)
}

fn count(checks: &[CheckResult], status: CheckStatus) -> usize {
    checks.iter().filter(|check| check.status == status).count()
}

fn evaluate_rule(
    package: &RulePackage,
    input: &RuleEvaluationInput,
    rule: &LimitCheckRule,
    package_status: ApplicabilityStatus,
    statuses: &BTreeMap<String, CheckStatus>,
) -> CheckResult {
    let (rule_status, applicability) = evaluate_applicability(&rule.applicability, input);
    let effective_status = combine_applicability(package_status, rule_status);
    let trace = resolve_clause_trace(package, rule);

    if effective_status == ApplicabilityStatus::NotApplicable {
        return result_without_values(
            rule,
            CheckStatus::NotApplicable,
            "Rule is outside the declared package or rule applicability.",
            applicability,
            Vec::new(),
            trace,
        );
    }
    if effective_status == ApplicabilityStatus::Indeterminate {
        return result_without_values(
            rule,
            CheckStatus::Error,
            "Applicability could not be determined because context is missing or incompatible.",
            applicability,
            Vec::new(),
            trace,
        );
    }

    let dependency_results = resolve_rule_dependencies(rule, statuses);
    if dependency_results
        .iter()
        .any(|dependency| !dependency.satisfied)
    {
        let blocked_by = dependency_results
            .iter()
            .filter(|dependency| !dependency.satisfied)
            .map(|dependency| dependency.rule_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return result_without_values(
            rule,
            CheckStatus::Blocked,
            &format!("Rule evaluation blocked by dependencies: {}.", blocked_by),
            applicability,
            dependency_results,
            trace,
        );
    }

    let Some(actual_input) = input.numeric_inputs.get(&rule.actual_input_key) else {
        return result_without_values(
            rule,
            CheckStatus::Error,
            &format!(
                "Required numeric input '{}' is missing.",
                rule.actual_input_key
            ),
            applicability,
            dependency_results,
            trace,
        );
    };

    let (limit, limit_unit, resolved_parameters) = match &rule.limit {
        LimitSource::Literal { value, unit } => (*value, unit.clone(), Vec::new()),
        LimitSource::Parameter { key } => {
            let Some(parameter) = package.parameters.get(key) else {
                return result_without_values(
                    rule,
                    CheckStatus::Error,
                    &format!("Required parameter '{}' is missing.", key),
                    applicability,
                    dependency_results,
                    trace,
                );
            };
            (
                parameter.value,
                parameter.unit.clone(),
                vec![ResolvedParameter {
                    key: parameter.key.clone(),
                    value: parameter.value,
                    unit: parameter.unit.clone(),
                    source: parameter.source.clone(),
                }],
            )
        }
    };

    if actual_input.unit != limit_unit {
        return CheckResult {
            rule_id: rule.id.clone(),
            title: rule.title.clone(),
            status: CheckStatus::Error,
            message: format!(
                "Unit mismatch: input uses '{}' while limit uses '{}'; implicit conversion is forbidden.",
                actual_input.unit, limit_unit
            ),
            actual: Some(actual_input.value),
            limit: Some(limit),
            unit: None,
            utilization: None,
            applicability,
            input_references: actual_input.references.clone(),
            resolved_parameters,
            clause_refs: trace.references,
            interpretation_note_ids: trace.interpretation_note_ids,
            direct_clause_node_ids: rule.clause_node_ids.clone(),
            resolved_clauses: trace.clauses,
            dependencies: dependency_results,
        };
    }

    let actual = if rule.use_absolute_actual {
        actual_input.value.abs()
    } else {
        actual_input.value
    };
    let passed = compare(actual, limit, &rule.operator);
    let utilization = if limit.abs() > f64::EPSILON {
        Some(actual / limit)
    } else {
        None
    };

    CheckResult {
        rule_id: rule.id.clone(),
        title: rule.title.clone(),
        status: if passed {
            CheckStatus::Pass
        } else {
            CheckStatus::Fail
        },
        message: format!(
            "Actual {} {} limit {} {}.",
            actual,
            operator_text(&rule.operator),
            limit,
            limit_unit
        ),
        actual: Some(actual),
        limit: Some(limit),
        unit: Some(limit_unit),
        utilization,
        applicability,
        input_references: actual_input.references.clone(),
        resolved_parameters,
        clause_refs: trace.references,
        interpretation_note_ids: trace.interpretation_note_ids,
        direct_clause_node_ids: rule.clause_node_ids.clone(),
        resolved_clauses: trace.clauses,
        dependencies: dependency_results,
    }
}

fn resolve_rule_dependencies(
    rule: &LimitCheckRule,
    statuses: &BTreeMap<String, CheckStatus>,
) -> Vec<ResolvedRuleDependency> {
    let mut dependencies = rule.depends_on.clone();
    dependencies.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));
    dependencies
        .into_iter()
        .map(|dependency| {
            let actual_status = statuses
                .get(&dependency.rule_id)
                .copied()
                .unwrap_or(CheckStatus::Error);
            let satisfied = match dependency.requirement {
                DependencyRequirement::Pass => actual_status == CheckStatus::Pass,
                DependencyRequirement::PassOrNotApplicable => matches!(
                    actual_status,
                    CheckStatus::Pass | CheckStatus::NotApplicable
                ),
                DependencyRequirement::Completed => matches!(
                    actual_status,
                    CheckStatus::Pass | CheckStatus::Fail | CheckStatus::NotApplicable
                ),
            };
            ResolvedRuleDependency {
                rule_id: dependency.rule_id,
                requirement: dependency.requirement,
                actual_status,
                satisfied,
                reason: dependency.reason,
            }
        })
        .collect()
}

struct ClauseTrace {
    clauses: Vec<ResolvedClause>,
    references: Vec<ClauseReference>,
    interpretation_note_ids: Vec<String>,
}

fn resolve_clause_trace(package: &RulePackage, rule: &LimitCheckRule) -> ClauseTrace {
    fn collect(id: &str, graph: &ClauseGraph, visited: &mut BTreeSet<String>) {
        if !visited.insert(id.to_owned()) {
            return;
        }
        if let Some(node) = graph.nodes.get(id) {
            let mut dependencies = node.dependencies.clone();
            dependencies.sort();
            for dependency in dependencies {
                collect(&dependency, graph, visited);
            }
        }
    }

    let mut visited = BTreeSet::new();
    let mut roots = rule.clause_node_ids.clone();
    roots.sort();
    for id in roots {
        collect(&id, &package.clause_graph, &mut visited);
    }

    let mut clauses = Vec::new();
    let mut references = rule.clause_refs.clone();
    let mut interpretation_note_ids = rule.interpretation_note_ids.clone();

    for id in visited {
        if let Some(node) = package.clause_graph.nodes.get(&id) {
            clauses.push(ResolvedClause {
                id: node.id.clone(),
                kind: node.kind.clone(),
                title: node.title.clone(),
                references: node.references.clone(),
            });
            for reference in &node.references {
                if !references.contains(reference) {
                    references.push(reference.clone());
                }
            }
            for note_id in &node.interpretation_note_ids {
                if !interpretation_note_ids.contains(note_id) {
                    interpretation_note_ids.push(note_id.clone());
                }
            }
        }
    }

    references.sort_by(|left, right| {
        (&left.document, &left.clause, &left.title, &left.uri)
            .cmp(&(&right.document, &right.clause, &right.title, &right.uri))
    });
    interpretation_note_ids.sort();
    interpretation_note_ids.dedup();

    ClauseTrace {
        clauses,
        references,
        interpretation_note_ids,
    }
}

fn result_without_values(
    rule: &LimitCheckRule,
    status: CheckStatus,
    message: &str,
    applicability: Vec<PredicateResult>,
    dependencies: Vec<ResolvedRuleDependency>,
    trace: ClauseTrace,
) -> CheckResult {
    CheckResult {
        rule_id: rule.id.clone(),
        title: rule.title.clone(),
        status,
        message: message.to_owned(),
        actual: None,
        limit: None,
        unit: None,
        utilization: None,
        applicability,
        input_references: Vec::new(),
        resolved_parameters: Vec::new(),
        clause_refs: trace.references,
        interpretation_note_ids: trace.interpretation_note_ids,
        direct_clause_node_ids: rule.clause_node_ids.clone(),
        resolved_clauses: trace.clauses,
        dependencies,
    }
}

fn compare(actual: f64, limit: f64, operator: &ComparisonOperator) -> bool {
    match operator {
        ComparisonOperator::LessThan => actual < limit,
        ComparisonOperator::LessThanOrEqual => actual <= limit,
        ComparisonOperator::GreaterThan => actual > limit,
        ComparisonOperator::GreaterThanOrEqual => actual >= limit,
    }
}

fn operator_text(operator: &ComparisonOperator) -> &'static str {
    match operator {
        ComparisonOperator::LessThan => "<",
        ComparisonOperator::LessThanOrEqual => "<=",
        ComparisonOperator::GreaterThan => ">",
        ComparisonOperator::GreaterThanOrEqual => ">=",
    }
}

fn combine_applicability(
    package: ApplicabilityStatus,
    rule: ApplicabilityStatus,
) -> ApplicabilityStatus {
    if package == ApplicabilityStatus::NotApplicable
        || rule == ApplicabilityStatus::NotApplicable
    {
        ApplicabilityStatus::NotApplicable
    } else if package == ApplicabilityStatus::Indeterminate
        || rule == ApplicabilityStatus::Indeterminate
    {
        ApplicabilityStatus::Indeterminate
    } else {
        ApplicabilityStatus::Applicable
    }
}

fn evaluate_applicability(
    applicability: &Applicability,
    input: &RuleEvaluationInput,
) -> (ApplicabilityStatus, Vec<PredicateResult>) {
    let mut overall = ApplicabilityStatus::Applicable;
    let mut details = Vec::new();
    for predicate in &applicability.all {
        let status = evaluate_predicate(predicate, input);
        if status == ApplicabilityStatus::NotApplicable {
            overall = ApplicabilityStatus::NotApplicable;
        } else if status == ApplicabilityStatus::Indeterminate
            && overall != ApplicabilityStatus::NotApplicable
        {
            overall = ApplicabilityStatus::Indeterminate;
        }
        details.push(PredicateResult {
            context_key: predicate.context_key.clone(),
            status,
            reason: predicate.reason.clone(),
            clause_refs: predicate.clause_refs.clone(),
        });
    }
    (overall, details)
}

fn evaluate_predicate(
    predicate: &ApplicabilityPredicate,
    input: &RuleEvaluationInput,
) -> ApplicabilityStatus {
    let actual = input.context.get(&predicate.context_key);
    match predicate.operator {
        PredicateOperator::Exists => {
            if actual.is_some() {
                ApplicabilityStatus::Applicable
            } else {
                ApplicabilityStatus::Indeterminate
            }
        }
        PredicateOperator::Equals => match (actual, predicate.expected.first()) {
            (Some(a), Some(e)) if context_equal(a, e) => ApplicabilityStatus::Applicable,
            (Some(_), Some(_)) => ApplicabilityStatus::NotApplicable,
            _ => ApplicabilityStatus::Indeterminate,
        },
        PredicateOperator::NotEquals => match (actual, predicate.expected.first()) {
            (Some(a), Some(e)) if !context_equal(a, e) => ApplicabilityStatus::Applicable,
            (Some(_), Some(_)) => ApplicabilityStatus::NotApplicable,
            _ => ApplicabilityStatus::Indeterminate,
        },
        PredicateOperator::In => match actual {
            Some(a) if predicate.expected.iter().any(|e| context_equal(a, e)) => {
                ApplicabilityStatus::Applicable
            }
            Some(_) => ApplicabilityStatus::NotApplicable,
            None => ApplicabilityStatus::Indeterminate,
        },
    }
}

fn context_equal(left: &ContextValue, right: &ContextValue) -> bool {
    match (left, right) {
        (ContextValue::Text { value: a }, ContextValue::Text { value: b }) => a == b,
        (ContextValue::Boolean { value: a }, ContextValue::Boolean { value: b }) => a == b,
        (
            ContextValue::Number { value: a, unit: ua },
            ContextValue::Number { value: b, unit: ub },
        ) => ua == ub && a == b,
        _ => false,
    }
}
