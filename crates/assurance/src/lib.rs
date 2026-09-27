//! Open assurance catalog and static validation portal.
//!
//! This crate publishes evidence; it does not certify engineering fitness. It keeps
//! benchmark claims, limitations, traceability and third-party reports explicit,
//! hash-addressed and independently downloadable.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
use structural_audit::{canonical_json_bytes, sha256_hex};

pub const ASSURANCE_SCHEMA_VERSION: &str = "structural-assurance-catalog/1.0";
pub const EVIDENCE_BUNDLE_SCHEMA_VERSION: &str = "structural-assurance-bundle/1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceArtifact {
    pub path: String,
    pub media_type: String,
    pub sha256: String,
    pub byte_length: u64,
}

impl EvidenceArtifact {
    pub fn validate(&self) -> Result<()> {
        validate_relative_path(&self.path)?;
        if self.media_type.trim().is_empty() {
            bail!("artifact media_type is required");
        }
        validate_sha256(&self.sha256)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpectedMetric {
    pub metric_id: String,
    pub description: String,
    pub unit: String,
    pub minimum: f64,
    pub maximum: f64,
}

impl ExpectedMetric {
    fn validate(&self) -> Result<()> {
        require_id(&self.metric_id, "metric_id")?;
        if !self.minimum.is_finite() || !self.maximum.is_finite() || self.minimum > self.maximum {
            bail!("metric '{}' has an invalid expected range", self.metric_id);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkCase {
    pub benchmark_id: String,
    pub title: String,
    pub analysis_type: String,
    pub description: String,
    pub inputs: Vec<EvidenceArtifact>,
    #[serde(default)]
    pub reference_artifacts: Vec<EvidenceArtifact>,
    pub expected_metrics: Vec<ExpectedMetric>,
    pub reference_sources: Vec<String>,
    pub limitation_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Passed,
    Failed,
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SolverValidation {
    pub validation_id: String,
    pub benchmark_id: String,
    pub solver_id: String,
    pub solver_version: String,
    pub platform: String,
    pub executed_at_utc: Option<DateTime<Utc>>,
    pub status: ValidationStatus,
    pub observed_metrics: BTreeMap<String, f64>,
    pub result: Option<EvidenceArtifact>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnownLimitation {
    pub limitation_id: String,
    pub title: String,
    pub description: String,
    pub affected_components: Vec<String>,
    pub severity: String,
    pub workaround: Option<String>,
    pub open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceabilityEntry {
    pub requirement_id: String,
    pub source: String,
    pub claim: String,
    pub benchmark_ids: Vec<String>,
    pub evidence_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LabVerificationStatus {
    IllustrativeUnverified,
    PublisherVerified,
    SignatureVerified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndependentLabResult {
    pub result_id: String,
    pub organization: String,
    pub report_reference: String,
    pub issue_date: NaiveDate,
    pub scope: String,
    pub accreditation_statement: Option<String>,
    pub verification_status: LabVerificationStatus,
    pub report: EvidenceArtifact,
    pub benchmark_ids: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssuranceCatalog {
    pub schema_version: String,
    pub product_name: String,
    pub product_version: String,
    pub published_at_utc: DateTime<Utc>,
    pub scope_statement: String,
    pub benchmarks: Vec<BenchmarkCase>,
    pub validations: Vec<SolverValidation>,
    pub known_limitations: Vec<KnownLimitation>,
    pub traceability: Vec<TraceabilityEntry>,
    pub independent_lab_results: Vec<IndependentLabResult>,
}

impl AssuranceCatalog {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != ASSURANCE_SCHEMA_VERSION {
            bail!("unsupported assurance schema '{}'", self.schema_version);
        }
        for value in [&self.product_name, &self.product_version, &self.scope_statement] {
            if value.trim().is_empty() { bail!("catalog identity and scope fields are required"); }
        }
        let limitation_ids = unique_ids(
            self.known_limitations.iter().map(|v| v.limitation_id.as_str()),
            "limitation",
        )?;
        let benchmark_ids = unique_ids(
            self.benchmarks.iter().map(|v| v.benchmark_id.as_str()),
            "benchmark",
        )?;
        unique_ids(
            self.validations.iter().map(|v| v.validation_id.as_str()),
            "validation",
        )?;
        unique_ids(
            self.traceability.iter().map(|v| v.requirement_id.as_str()),
            "requirement",
        )?;
        unique_ids(
            self.independent_lab_results.iter().map(|v| v.result_id.as_str()),
            "lab result",
        )?;

        for benchmark in &self.benchmarks {
            require_id(&benchmark.benchmark_id, "benchmark_id")?;
            if benchmark.inputs.is_empty() || benchmark.expected_metrics.is_empty() {
                bail!("benchmark '{}' requires inputs and expected metrics", benchmark.benchmark_id);
            }
            for artifact in benchmark.inputs.iter().chain(benchmark.reference_artifacts.iter()) {
                artifact.validate()?;
            }
            let metric_ids = unique_ids(
                benchmark.expected_metrics.iter().map(|m| m.metric_id.as_str()),
                "metric",
            )?;
            if metric_ids.len() != benchmark.expected_metrics.len() { unreachable!(); }
            for metric in &benchmark.expected_metrics { metric.validate()?; }
            for id in &benchmark.limitation_ids {
                if !limitation_ids.contains(id.as_str()) {
                    bail!("benchmark '{}' references unknown limitation '{}'", benchmark.benchmark_id, id);
                }
            }
        }

        for validation in &self.validations {
            if !benchmark_ids.contains(validation.benchmark_id.as_str()) {
                bail!("validation '{}' references unknown benchmark", validation.validation_id);
            }
            if validation.status == ValidationStatus::NotRun {
                if validation.result.is_some() || !validation.observed_metrics.is_empty() {
                    bail!("not-run validation '{}' must not contain results", validation.validation_id);
                }
                continue;
            }
            let result = validation.result.as_ref()
                .with_context(|| format!("validation '{}' requires a result artifact", validation.validation_id))?;
            result.validate()?;
            let benchmark = self.benchmarks.iter()
                .find(|b| b.benchmark_id == validation.benchmark_id).unwrap();
            let mut within_all = true;
            for metric in &benchmark.expected_metrics {
                let observed = validation.observed_metrics.get(&metric.metric_id)
                    .with_context(|| format!("validation '{}' misses metric '{}'", validation.validation_id, metric.metric_id))?;
                if !observed.is_finite() { bail!("observed metric must be finite"); }
                within_all &= *observed >= metric.minimum && *observed <= metric.maximum;
            }
            if (validation.status == ValidationStatus::Passed) != within_all {
                bail!("validation '{}' status contradicts expected ranges", validation.validation_id);
            }
        }

        let mut artifact_paths = BTreeMap::<&str, (&str, u64)>::new();
        for artifact in self.artifacts() {
            if let Some((digest, length)) =
                artifact_paths.insert(&artifact.path, (&artifact.sha256, artifact.byte_length))
            {
                if digest != artifact.sha256.as_str() || length != artifact.byte_length {
                    bail!("artifact path '{}' has conflicting descriptors", artifact.path);
                }
            }
        }

        for entry in &self.traceability {
            require_id(&entry.requirement_id, "requirement_id")?;
            if entry.benchmark_ids.is_empty() || entry.evidence_paths.is_empty() {
                bail!("traceability '{}' requires benchmark and evidence links", entry.requirement_id);
            }
            for id in &entry.benchmark_ids {
                if !benchmark_ids.contains(id.as_str()) {
                    bail!("traceability '{}' references unknown benchmark '{}'", entry.requirement_id, id);
                }
            }
            for path in &entry.evidence_paths {
                validate_relative_path(path)?;
                if !artifact_paths.contains_key(path.as_str()) {
                    bail!("traceability '{}' references undeclared evidence '{}'", entry.requirement_id, path);
                }
            }
        }

        for result in &self.independent_lab_results {
            require_id(&result.result_id, "result_id")?;
            result.report.validate()?;
            for id in &result.benchmark_ids {
                if !benchmark_ids.contains(id.as_str()) {
                    bail!("lab result '{}' references unknown benchmark '{}'", result.result_id, id);
                }
            }
            if result.verification_status == LabVerificationStatus::SignatureVerified {
                bail!("signature_verified requires a future detached-signature profile; fail closed");
            }
        }
        Ok(())
    }

    pub fn artifacts(&self) -> Vec<&EvidenceArtifact> {
        let mut artifacts = Vec::new();
        for benchmark in &self.benchmarks {
            artifacts.extend(benchmark.inputs.iter());
            artifacts.extend(benchmark.reference_artifacts.iter());
        }
        for validation in &self.validations {
            if let Some(result) = &validation.result { artifacts.push(result); }
        }
        for lab in &self.independent_lab_results { artifacts.push(&lab.report); }
        artifacts
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundledArtifact {
    pub source_path: String,
    pub bundle_path: String,
    pub media_type: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceBundleManifest {
    pub schema_version: String,
    pub generated_at_utc: DateTime<Utc>,
    pub catalog_sha256: String,
    pub artifacts: Vec<BundledArtifact>,
    pub bundle_sha256: String,
}

#[derive(Serialize)]
struct BundleHashMaterial<'a> {
    schema_version: &'a str,
    catalog_sha256: &'a str,
    artifacts: &'a [BundledArtifact],
}

pub fn build_portal(
    catalog: &AssuranceCatalog,
    source_root: &Path,
    output_root: &Path,
    generated_at_utc: DateTime<Utc>,
) -> Result<EvidenceBundleManifest> {
    catalog.validate()?;
    if output_root.exists() { fs::remove_dir_all(output_root)?; }
    fs::create_dir_all(output_root.join("evidence/sha256"))?;

    let mut bundled = BTreeMap::<String, BundledArtifact>::new();
    for artifact in catalog.artifacts() {
        let source = source_root.join(&artifact.path);
        let bytes = fs::read(&source)
            .with_context(|| format!("cannot read evidence artifact {}", source.display()))?;
        let actual = sha256_hex(&bytes);
        if actual != artifact.sha256 || bytes.len() as u64 != artifact.byte_length {
            bail!("evidence artifact '{}' failed hash or length verification", artifact.path);
        }
        let bundle_path = format!("evidence/sha256/{}", artifact.sha256);
        let destination = output_root.join(&bundle_path);
        if !destination.exists() { fs::write(&destination, &bytes)?; }
        bundled.entry(artifact.sha256.clone()).or_insert(BundledArtifact {
            source_path: artifact.path.clone(),
            bundle_path,
            media_type: artifact.media_type.clone(),
            sha256: artifact.sha256.clone(),
            byte_length: artifact.byte_length,
        });
    }

    let catalog_bytes = serde_json::to_vec_pretty(catalog)?;
    fs::write(output_root.join("catalog.json"), &catalog_bytes)?;
    let catalog_sha256 = sha256_hex(&canonical_json_bytes(catalog)?);
    let artifacts: Vec<_> = bundled.into_values().collect();
    let material = BundleHashMaterial {
        schema_version: EVIDENCE_BUNDLE_SCHEMA_VERSION,
        catalog_sha256: &catalog_sha256,
        artifacts: &artifacts,
    };
    let bundle_sha256 = sha256_hex(&canonical_json_bytes(&material)?);
    let manifest = EvidenceBundleManifest {
        schema_version: EVIDENCE_BUNDLE_SCHEMA_VERSION.to_owned(),
        generated_at_utc,
        catalog_sha256,
        artifacts,
        bundle_sha256,
    };
    fs::write(output_root.join("bundle-manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
    fs::write(output_root.join("index.html"), render_html(catalog, &manifest))?;
    Ok(manifest)
}

fn render_html(catalog: &AssuranceCatalog, manifest: &EvidenceBundleManifest) -> String {
    let mut rows = String::new();
    for benchmark in &catalog.benchmarks {
        let passed = catalog.validations.iter().filter(|v| {
            v.benchmark_id == benchmark.benchmark_id && v.status == ValidationStatus::Passed
        }).count();
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            html(&benchmark.benchmark_id), html(&benchmark.title),
            html(&benchmark.analysis_type), passed
        ));
    }
    let limitations = catalog.known_limitations.iter().map(|v| format!(
        "<li><strong>{}</strong>: {}</li>", html(&v.title), html(&v.description)
    )).collect::<String>();
    format!(r#"<!doctype html><html lang="en"><meta charset="utf-8">
<title>{0} assurance portal</title>
<style>body{{font:16px system-ui;max-width:1100px;margin:2rem auto;padding:0 1rem}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #bbb;padding:.5rem;text-align:left}}code{{word-break:break-all}}</style>
<h1>{0} assurance portal</h1><p><strong>Version:</strong> {1}</p>
<p>{2}</p><p>This portal publishes reproducible evidence. It is not a product certification and does not replace review by a qualified engineer.</p>
<h2>Benchmark matrix</h2><table><thead><tr><th>ID</th><th>Title</th><th>Analysis</th><th>Passing runs</th></tr></thead><tbody>{3}</tbody></table>
<h2>Known limitations</h2><ul>{4}</ul>
<h2>Evidence</h2><p><a href="catalog.json">Catalog</a> · <a href="bundle-manifest.json">Bundle manifest</a></p>
<p>Bundle SHA-256: <code>{5}</code></p></html>"#,
        html(&catalog.product_name), html(&catalog.product_version),
        html(&catalog.scope_statement), rows, limitations, manifest.bundle_sha256)
}

fn html(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&#39;")
}

fn unique_ids<'a>(values: impl Iterator<Item=&'a str>, label: &str) -> Result<BTreeSet<&'a str>> {
    let mut ids = BTreeSet::new();
    for value in values {
        require_id(value, label)?;
        if !ids.insert(value) { bail!("duplicate {label} id '{value}'"); }
    }
    Ok(ids)
}

fn require_id(value: &str, label: &str) -> Result<()> {
    if value.is_empty() || value.len() > 128 ||
        !value.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_' | b':' | b'/')) {
        bail!("{label} must be a non-empty portable identifier");
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<()> {
    let path = PathBuf::from(value);
    if value.is_empty() || path.is_absolute() ||
        path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        bail!("evidence path must be a safe relative path");
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        bail!("sha256 must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}
