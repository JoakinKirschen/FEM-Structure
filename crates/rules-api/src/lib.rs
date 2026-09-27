//! Backend-neutral, versioned contracts for structural design-rule packages.
//!
//! Pass 11 adds a navigable clause graph and explicit inter-rule dependencies.
//! Rule authors still supply reviewed, data-only packages; engines validate and
//! deterministically resolve the graphs before evaluating checks.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub const RULE_PACKAGE_SCHEMA_VERSION: &str = "0.2";
pub const RULE_EVALUATION_SCHEMA_VERSION: &str = "0.2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PackageStatus {
    Draft,
    Reviewed,
    Approved,
    Withdrawn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RulePackageMetadata {
    pub package_id: String,
    pub package_version: String,
    pub authority: String,
    pub standard_identifier: String,
    pub standard_edition: String,
    pub national_annex: Option<String>,
    pub jurisdiction: Option<String>,
    pub language: String,
    pub status: PackageStatus,
    pub supersedes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClauseReference {
    pub document: String,
    pub clause: String,
    pub title: Option<String>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClauseKind {
    Scope,
    Normative,
    Informative,
    NationalAnnex,
    Interpretation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClauseNode {
    pub id: String,
    pub kind: ClauseKind,
    pub title: String,
    pub references: Vec<ClauseReference>,
    /// Clause nodes that must be read/resolved before this node.
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub interpretation_note_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClauseRelation {
    Requires,
    Refines,
    Overrides,
    CrossReferences,
    Interprets,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClauseLink {
    pub from: String,
    pub to: String,
    pub relation: ClauseRelation,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClauseGraph {
    /// BTreeMap ensures stable traversal and serialization.
    #[serde(default)]
    pub nodes: BTreeMap<String, ClauseNode>,
    /// Semantic links are auditable; ordering is normalized during validation.
    #[serde(default)]
    pub links: Vec<ClauseLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterpretationStatus {
    Draft,
    Reviewed,
    Approved,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterpretationNote {
    pub id: String,
    pub title: String,
    pub text: String,
    pub status: InterpretationStatus,
    pub clause_refs: Vec<ClauseReference>,
    pub supersedes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleParameter {
    pub key: String,
    pub value: f64,
    pub unit: String,
    pub description: String,
    pub source: ClauseReference,
    pub interpretation_note_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextValue {
    Number { value: f64, unit: String },
    Text { value: String },
    Boolean { value: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PredicateOperator {
    Exists,
    Equals,
    NotEquals,
    In,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApplicabilityPredicate {
    pub context_key: String,
    pub operator: PredicateOperator,
    /// Ignored for `exists`; one value for equality; one or more values for `in`.
    pub expected: Vec<ContextValue>,
    pub reason: String,
    pub clause_refs: Vec<ClauseReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Applicability {
    /// Every predicate must match.
    pub all: Vec<ApplicabilityPredicate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOperator {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum LimitSource {
    Parameter { key: String },
    Literal { value: f64, unit: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyRequirement {
    Pass,
    PassOrNotApplicable,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleDependency {
    pub rule_id: String,
    pub requirement: DependencyRequirement,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LimitCheckRule {
    pub id: String,
    pub title: String,
    pub applicability: Applicability,
    pub actual_input_key: String,
    pub operator: ComparisonOperator,
    pub limit: LimitSource,
    pub use_absolute_actual: bool,
    pub clause_refs: Vec<ClauseReference>,
    pub interpretation_note_ids: Vec<String>,
    /// Direct entry points into the clause graph. Transitive clause dependencies
    /// are resolved and included in every check result.
    #[serde(default)]
    pub clause_node_ids: Vec<String>,
    /// Other rules that must reach an acceptable outcome before this rule runs.
    #[serde(default)]
    pub depends_on: Vec<RuleDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RulePackage {
    pub schema_version: String,
    pub metadata: RulePackageMetadata,
    pub applicability: Applicability,
    pub parameters: BTreeMap<String, RuleParameter>,
    pub interpretation_notes: Vec<InterpretationNote>,
    #[serde(default)]
    pub clause_graph: ClauseGraph,
    pub rules: Vec<LimitCheckRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputReference {
    pub artifact_sha256: String,
    pub json_pointer: String,
    pub entity_id: Option<Uuid>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NumericInput {
    pub value: f64,
    pub unit: String,
    pub references: Vec<InputReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleEvaluationInput {
    pub schema_version: String,
    pub evaluation_id: Uuid,
    pub model_revision_id: Option<Uuid>,
    pub context: BTreeMap<String, ContextValue>,
    pub numeric_inputs: BTreeMap<String, NumericInput>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplicabilityStatus {
    Applicable,
    NotApplicable,
    Indeterminate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PredicateResult {
    pub context_key: String,
    pub status: ApplicabilityStatus,
    pub reason: String,
    pub clause_refs: Vec<ClauseReference>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Fail,
    NotApplicable,
    Blocked,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedParameter {
    pub key: String,
    pub value: f64,
    pub unit: String,
    pub source: ClauseReference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedClause {
    pub id: String,
    pub kind: ClauseKind,
    pub title: String,
    pub references: Vec<ClauseReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedRuleDependency {
    pub rule_id: String,
    pub requirement: DependencyRequirement,
    pub actual_status: CheckStatus,
    pub satisfied: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckResult {
    pub rule_id: String,
    pub title: String,
    pub status: CheckStatus,
    pub message: String,
    pub actual: Option<f64>,
    pub limit: Option<f64>,
    pub unit: Option<String>,
    pub utilization: Option<f64>,
    pub applicability: Vec<PredicateResult>,
    pub input_references: Vec<InputReference>,
    pub resolved_parameters: Vec<ResolvedParameter>,
    pub clause_refs: Vec<ClauseReference>,
    pub interpretation_note_ids: Vec<String>,
    pub direct_clause_node_ids: Vec<String>,
    pub resolved_clauses: Vec<ResolvedClause>,
    pub dependencies: Vec<ResolvedRuleDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleEvaluationReport {
    pub schema_version: String,
    pub evaluation_id: Uuid,
    pub package_id: String,
    pub package_version: String,
    pub package_sha256: String,
    pub clause_graph_sha256: String,
    pub national_annex: Option<String>,
    pub package_applicability: ApplicabilityStatus,
    pub package_applicability_details: Vec<PredicateResult>,
    /// Stable topological order used by the engine.
    pub evaluation_order: Vec<String>,
    pub checks: Vec<CheckResult>,
    pub pass_count: usize,
    pub fail_count: usize,
    pub not_applicable_count: usize,
    pub blocked_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleEvaluationEvidence {
    pub evidence_schema_version: String,
    pub engine_id: String,
    pub engine_version: String,
    pub package_sha256: String,
    pub clause_graph_sha256: String,
    pub input_sha256: String,
    pub report_sha256: String,
    pub deterministic: bool,
}

impl RulePackage {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != RULE_PACKAGE_SCHEMA_VERSION {
            bail!("unsupported rule package schema '{}'", self.schema_version);
        }
        if self.metadata.package_id.trim().is_empty()
            || self.metadata.package_version.trim().is_empty()
        {
            bail!("package id and version are required");
        }

        let note_ids: BTreeSet<&str> = self
            .interpretation_notes
            .iter()
            .map(|n| n.id.as_str())
            .collect();
        if note_ids.len() != self.interpretation_notes.len() {
            bail!("duplicate interpretation-note id");
        }

        validate_clause_graph(&self.clause_graph, &note_ids)?;

        for (key, parameter) in &self.parameters {
            if key != &parameter.key {
                bail!(
                    "parameter map key '{}' differs from embedded key '{}'",
                    key,
                    parameter.key
                );
            }
            if !parameter.value.is_finite() {
                bail!("parameter '{}' is not finite", key);
            }
            ensure_notes_exist(&parameter.interpretation_note_ids, &note_ids)?;
        }

        let mut rule_ids = BTreeSet::new();
        for rule in &self.rules {
            if !rule_ids.insert(rule.id.as_str()) {
                bail!("duplicate rule id '{}'", rule.id);
            }
            if let LimitSource::Parameter { key } = &rule.limit {
                if !self.parameters.contains_key(key) {
                    bail!("rule '{}' references missing parameter '{}'", rule.id, key);
                }
            }
            ensure_notes_exist(&rule.interpretation_note_ids, &note_ids)?;
            for clause_id in &rule.clause_node_ids {
                if !self.clause_graph.nodes.contains_key(clause_id) {
                    bail!(
                        "rule '{}' references missing clause node '{}'",
                        rule.id,
                        clause_id
                    );
                }
            }
        }

        for rule in &self.rules {
            let mut seen = BTreeSet::new();
            for dependency in &rule.depends_on {
                if dependency.rule_id == rule.id {
                    bail!("rule '{}' cannot depend on itself", rule.id);
                }
                if !rule_ids.contains(dependency.rule_id.as_str()) {
                    bail!(
                        "rule '{}' references missing dependency '{}'",
                        rule.id,
                        dependency.rule_id
                    );
                }
                if !seen.insert(dependency.rule_id.as_str()) {
                    bail!(
                        "rule '{}' repeats dependency '{}'",
                        rule.id,
                        dependency.rule_id
                    );
                }
            }
        }

        let rule_dependencies: BTreeMap<String, Vec<String>> = self
            .rules
            .iter()
            .map(|rule| {
                (
                    rule.id.clone(),
                    rule.depends_on
                        .iter()
                        .map(|dependency| dependency.rule_id.clone())
                        .collect(),
                )
            })
            .collect();
        ensure_acyclic("rule", &rule_dependencies)?;
        Ok(())
    }
}

fn validate_clause_graph(graph: &ClauseGraph, note_ids: &BTreeSet<&str>) -> Result<()> {
    let mut dependencies = BTreeMap::new();
    for (key, node) in &graph.nodes {
        if key != &node.id {
            bail!(
                "clause-node map key '{}' differs from embedded id '{}'",
                key,
                node.id
            );
        }
        ensure_notes_exist(&node.interpretation_note_ids, note_ids)?;
        let mut seen = BTreeSet::new();
        for dependency in &node.dependencies {
            if dependency == &node.id {
                bail!("clause node '{}' cannot depend on itself", node.id);
            }
            if !graph.nodes.contains_key(dependency) {
                bail!(
                    "clause node '{}' references missing dependency '{}'",
                    node.id,
                    dependency
                );
            }
            if !seen.insert(dependency.as_str()) {
                bail!(
                    "clause node '{}' repeats dependency '{}'",
                    node.id,
                    dependency
                );
            }
        }
        dependencies.insert(node.id.clone(), node.dependencies.clone());
    }

    let mut links = BTreeSet::new();
    for link in &graph.links {
        if !graph.nodes.contains_key(&link.from) || !graph.nodes.contains_key(&link.to) {
            bail!(
                "clause link '{} -> {}' references a missing node",
                link.from,
                link.to
            );
        }
        let key = format!("{}\u{0}{}\u{0}{:?}", link.from, link.to, link.relation);
        if !links.insert(key) {
            bail!("duplicate clause link '{} -> {}'", link.from, link.to);
        }
    }

    ensure_acyclic("clause", &dependencies)
}

fn ensure_acyclic(kind: &str, dependencies: &BTreeMap<String, Vec<String>>) -> Result<()> {
    fn visit(
        id: &str,
        dependencies: &BTreeMap<String, Vec<String>>,
        temporary: &mut BTreeSet<String>,
        permanent: &mut BTreeSet<String>,
    ) -> Result<()> {
        if permanent.contains(id) {
            return Ok(());
        }
        if !temporary.insert(id.to_owned()) {
            bail!("dependency cycle detected at '{}'", id);
        }
        if let Some(required) = dependencies.get(id) {
            for dependency in required {
                visit(dependency, dependencies, temporary, permanent)?;
            }
        }
        temporary.remove(id);
        permanent.insert(id.to_owned());
        Ok(())
    }

    let mut temporary = BTreeSet::new();
    let mut permanent = BTreeSet::new();
    for id in dependencies.keys() {
        visit(id, dependencies, &mut temporary, &mut permanent)
            .map_err(|error| anyhow::anyhow!("{} graph {}", kind, error))?;
    }
    Ok(())
}

fn ensure_notes_exist(ids: &[String], available: &BTreeSet<&str>) -> Result<()> {
    for id in ids {
        if !available.contains(id.as_str()) {
            bail!("missing interpretation note '{}'", id);
        }
    }
    Ok(())
}
