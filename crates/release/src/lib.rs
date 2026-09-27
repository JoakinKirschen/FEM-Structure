//! Production-release policy and verification boundary.
//!
//! This crate verifies release evidence; it does not claim that a release is safe,
//! certified, or fit for structural design. Human approval and independent validation
//! remain mandatory.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path},
};
use structural_audit::{canonical_json_bytes, sha256_hex};

pub const RELEASE_SCHEMA_VERSION: &str = "structural-release/1.0";
pub const RELEASE_REPORT_SCHEMA_VERSION: &str = "structural-release-report/1.0";
pub const RELEASE_TRUST_SCHEMA_VERSION: &str = "structural-release-trust/1.0";

const REQUIRED_CONTROLS: &[&str] = &[
    "security.threat_model",
    "performance.budgets",
    "quality.fuzzing",
    "resilience.backup_restore",
    "compatibility.migrations",
    "supply_chain.sbom",
    "operations.incident_response",
    "operations.support",
    "marketplace.governance",
    "privacy.telemetry",
    "pilot.acceptance",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseChannel {
    Pilot,
    Beta,
    Stable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Binary,
    Installer,
    Container,
    Sbom,
    Provenance,
    Evidence,
    Documentation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseArtifact {
    pub path: String,
    pub kind: ArtifactKind,
    pub media_type: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlStatus {
    Passed,
    Failed,
    Waived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Waiver {
    pub approved_by: String,
    pub reason: String,
    pub expires_at_utc: DateTime<Utc>,
    pub issue_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseControl {
    pub id: String,
    pub title: String,
    pub status: ControlStatus,
    #[serde(default)]
    pub evidence_paths: Vec<String>,
    pub waiver: Option<Waiver>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryPolicy {
    pub policy_version: String,
    pub default_enabled: bool,
    pub explicit_consent_required: bool,
    pub model_content_collected: bool,
    pub precise_geometry_collected: bool,
    pub retention_days: u32,
    #[serde(default)]
    pub allowed_event_categories: Vec<String>,
    pub deletion_request_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketplacePolicy {
    pub signed_packages_required: bool,
    pub sandbox_required: bool,
    pub malware_scan_required: bool,
    pub independent_review_required: bool,
    pub revocation_supported: bool,
    pub emergency_kill_switch_supported: bool,
    pub compatibility_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseSignature {
    pub key_id: String,
    pub algorithm: String,
    pub signed_sha256: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseCandidate {
    pub schema_version: String,
    pub release_id: String,
    pub version: String,
    pub channel: ReleaseChannel,
    pub created_at_utc: DateTime<Utc>,
    pub source_commit: String,
    #[serde(default)]
    pub artifacts: Vec<ReleaseArtifact>,
    #[serde(default)]
    pub controls: Vec<ReleaseControl>,
    pub telemetry: TelemetryPolicy,
    pub marketplace: MarketplacePolicy,
    #[serde(default)]
    pub signatures: Vec<ReleaseSignature>,
}

impl ReleaseCandidate {
    pub fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signatures.clear();
        canonical_json_bytes(&unsigned)
    }

    pub fn unsigned_sha256(&self) -> Result<String> {
        Ok(sha256_hex(&self.unsigned_bytes()?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseTrustStore {
    pub schema_version: String,
    #[serde(default)]
    pub ed25519_public_keys: BTreeMap<String, String>,
    #[serde(default)]
    pub revoked_key_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateIssue {
    pub code: String,
    pub message: String,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedArtifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseVerificationReport {
    pub schema_version: String,
    pub release_id: String,
    pub version: String,
    pub candidate_sha256: String,
    pub accepted: bool,
    pub verified_artifacts: Vec<VerifiedArtifact>,
    pub verified_signature_key_ids: Vec<String>,
    pub issues: Vec<GateIssue>,
}

pub fn verify_release(
    candidate: &ReleaseCandidate,
    source_directory: &Path,
    trust: &ReleaseTrustStore,
    now: DateTime<Utc>,
) -> Result<ReleaseVerificationReport> {
    let mut issues = Vec::new();
    let mut verified_artifacts = Vec::new();
    let mut verified_signature_key_ids = Vec::new();

    validate_metadata(candidate, now, &mut issues);
    verify_artifacts(candidate, source_directory, &mut verified_artifacts, &mut issues);
    verify_controls(candidate, now, &mut issues);
    verify_privacy_and_marketplace(candidate, &mut issues);
    verify_signatures(candidate, trust, &mut verified_signature_key_ids, &mut issues)?;

    Ok(ReleaseVerificationReport {
        schema_version: RELEASE_REPORT_SCHEMA_VERSION.to_owned(),
        release_id: candidate.release_id.clone(),
        version: candidate.version.clone(),
        candidate_sha256: candidate.unsigned_sha256()?,
        accepted: issues.is_empty(),
        verified_artifacts,
        verified_signature_key_ids,
        issues,
    })
}

fn validate_metadata(candidate: &ReleaseCandidate, now: DateTime<Utc>, issues: &mut Vec<GateIssue>) {
    if candidate.schema_version != RELEASE_SCHEMA_VERSION {
        issue(issues, "release.schema.unsupported", "unsupported release schema", None);
    }
    if candidate.release_id.trim().is_empty() || candidate.source_commit.trim().is_empty() {
        issue(issues, "release.identity.missing", "release_id and source_commit are required", None);
    }
    if !is_semver(&candidate.version) {
        issue(issues, "release.version.invalid", "version must be numeric major.minor.patch", Some(&candidate.version));
    }
    if candidate.created_at_utc > now {
        issue(issues, "release.time.future", "created_at_utc is in the future", None);
    }
}

fn verify_artifacts(
    candidate: &ReleaseCandidate,
    source_directory: &Path,
    verified: &mut Vec<VerifiedArtifact>,
    issues: &mut Vec<GateIssue>,
) {
    let mut paths = BTreeSet::new();
    for artifact in &candidate.artifacts {
        if !paths.insert(artifact.path.clone()) {
            issue(issues, "artifact.path.duplicate", "artifact path is duplicated", Some(&artifact.path));
            continue;
        }
        if !safe_relative_path(&artifact.path) {
            issue(issues, "artifact.path.unsafe", "artifact path must be safe and relative", Some(&artifact.path));
            continue;
        }
        if artifact.sha256.len() != 64 || !artifact.sha256.bytes().all(|v| v.is_ascii_hexdigit()) {
            issue(issues, "artifact.digest.invalid", "artifact SHA-256 must contain 64 hexadecimal characters", Some(&artifact.path));
            continue;
        }
        let path = source_directory.join(&artifact.path);
        match fs::read(&path) {
            Ok(bytes) => {
                let digest = sha256_hex(&bytes);
                if digest != artifact.sha256 {
                    issue(issues, "artifact.digest.mismatch", "artifact digest does not match", Some(&artifact.path));
                } else if bytes.len() as u64 != artifact.bytes {
                    issue(issues, "artifact.length.mismatch", "artifact byte length does not match", Some(&artifact.path));
                } else {
                    verified.push(VerifiedArtifact {
                        path: artifact.path.clone(),
                        sha256: digest,
                        bytes: bytes.len() as u64,
                    });
                }
            }
            Err(_) => issue(issues, "artifact.missing", "declared artifact is missing", Some(&artifact.path)),
        }
    }
    if !candidate.artifacts.iter().any(|value| value.kind == ArtifactKind::Sbom) {
        issue(issues, "supply_chain.sbom.missing", "at least one SBOM artifact is required", None);
    }
    if !candidate.artifacts.iter().any(|value| value.kind == ArtifactKind::Provenance) {
        issue(issues, "supply_chain.provenance.missing", "at least one provenance artifact is required", None);
    }
}

fn verify_controls(candidate: &ReleaseCandidate, now: DateTime<Utc>, issues: &mut Vec<GateIssue>) {
    let artifacts: BTreeSet<_> = candidate.artifacts.iter().map(|value| value.path.as_str()).collect();
    let mut controls = BTreeMap::new();
    for control in &candidate.controls {
        if controls.insert(control.id.as_str(), control).is_some() {
            issue(issues, "control.duplicate", "control ID is duplicated", Some(&control.id));
        }
        if control.evidence_paths.is_empty() {
            issue(issues, "control.evidence.empty", "control requires evidence", Some(&control.id));
        }
        for evidence in &control.evidence_paths {
            if !artifacts.contains(evidence.as_str()) {
                issue(issues, "control.evidence.undeclared", "control references an undeclared artifact", Some(evidence));
            }
        }
        match control.status {
            ControlStatus::Passed if control.waiver.is_some() => {
                issue(issues, "control.waiver.unexpected", "passed control must not carry a waiver", Some(&control.id));
            }
            ControlStatus::Failed => {
                issue(issues, "control.failed", "release control failed", Some(&control.id));
            }
            ControlStatus::Waived => match &control.waiver {
                Some(waiver)
                    if !waiver.approved_by.trim().is_empty()
                        && !waiver.reason.trim().is_empty()
                        && !waiver.issue_reference.trim().is_empty()
                        && waiver.expires_at_utc > now => {}
                _ => issue(issues, "control.waiver.invalid", "waiver is missing, expired, or incomplete", Some(&control.id)),
            },
            _ => {}
        }
    }
    for required in REQUIRED_CONTROLS {
        if !controls.contains_key(required) {
            issue(issues, "control.required.missing", "required release control is missing", Some(required));
        }
    }
}

fn verify_privacy_and_marketplace(candidate: &ReleaseCandidate, issues: &mut Vec<GateIssue>) {
    let telemetry = &candidate.telemetry;
    if telemetry.policy_version.trim().is_empty()
        || telemetry.default_enabled
        || !telemetry.explicit_consent_required
        || telemetry.model_content_collected
        || telemetry.precise_geometry_collected
        || telemetry.retention_days == 0
        || telemetry.retention_days > 90
        || !telemetry.deletion_request_supported
    {
        issue(issues, "privacy.telemetry.policy", "telemetry must be opt-in, data-minimised, deletable, and retained for at most 90 days", None);
    }
    let market = &candidate.marketplace;
    if !market.signed_packages_required
        || !market.sandbox_required
        || !market.malware_scan_required
        || !market.independent_review_required
        || !market.revocation_supported
        || !market.emergency_kill_switch_supported
        || market.compatibility_policy.trim().is_empty()
    {
        issue(issues, "marketplace.policy.incomplete", "marketplace production safeguards are incomplete", None);
    }
}

fn verify_signatures(
    candidate: &ReleaseCandidate,
    trust: &ReleaseTrustStore,
    verified: &mut Vec<String>,
    issues: &mut Vec<GateIssue>,
) -> Result<()> {
    if trust.schema_version != RELEASE_TRUST_SCHEMA_VERSION {
        issue(issues, "signature.trust.unsupported", "unsupported release trust-store schema", None);
        return Ok(());
    }
    let required = candidate.channel == ReleaseChannel::Stable;
    if candidate.signatures.is_empty() {
        if required {
            issue(issues, "signature.required", "stable releases require a trusted signature", None);
        }
        return Ok(());
    }
    let payload = candidate.unsigned_bytes()?;
    let digest = sha256_hex(&payload);
    for value in &candidate.signatures {
        if value.algorithm != "ed25519-sha256" || value.signed_sha256 != digest {
            issue(issues, "signature.metadata.invalid", "signature metadata does not match candidate", Some(&value.key_id));
            continue;
        }
        if trust.revoked_key_ids.contains(&value.key_id) {
            issue(issues, "signature.key.revoked", "release signing key is revoked", Some(&value.key_id));
            continue;
        }
        let Some(encoded_key) = trust.ed25519_public_keys.get(&value.key_id) else {
            issue(issues, "signature.key.untrusted", "release signing key is not trusted", Some(&value.key_id));
            continue;
        };
        let key_bytes: [u8; 32] = match STANDARD.decode(encoded_key).ok().and_then(|v| v.try_into().ok()) {
            Some(value) => value,
            None => {
                issue(issues, "signature.key.invalid", "trusted Ed25519 key is invalid", Some(&value.key_id));
                continue;
            }
        };
        let signature_bytes: [u8; 64] = match STANDARD.decode(&value.signature_base64).ok().and_then(|v| v.try_into().ok()) {
            Some(value) => value,
            None => {
                issue(issues, "signature.encoding.invalid", "Ed25519 signature is invalid", Some(&value.key_id));
                continue;
            }
        };
        let key = VerifyingKey::from_bytes(&key_bytes).context("invalid Ed25519 release key")?;
        let signature = Signature::from_bytes(&signature_bytes);
        match key.verify(digest.as_bytes(), &signature) {
            Ok(()) => verified.push(value.key_id.clone()),
            Err(_) => issue(issues, "signature.verification.failed", "release signature verification failed", Some(&value.key_id)),
        }
    }
    if required && verified.is_empty() {
        issue(issues, "signature.required.unverified", "stable release has no verified trusted signature", None);
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && path.components().all(|part| matches!(part, Component::Normal(_)))
}

fn is_semver(value: &str) -> bool {
    let core = value.split_once('-').map(|v| v.0).unwrap_or(value);
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| !part.is_empty() && part.bytes().all(|v| v.is_ascii_digit()))
}

fn issue(issues: &mut Vec<GateIssue>, code: &str, message: &str, subject: Option<&str>) {
    issues.push(GateIssue {
        code: code.to_owned(),
        message: message.to_owned(),
        subject: subject.map(str::to_owned),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ed25519_dalek::{Signer, SigningKey};
    use tempfile::tempdir;

    fn candidate(path: &str, bytes: &[u8]) -> ReleaseCandidate {
        let evidence = ReleaseArtifact {
            path: path.into(), kind: ArtifactKind::Evidence, media_type: "application/json".into(),
            sha256: sha256_hex(bytes), bytes: bytes.len() as u64,
        };
        let mut artifacts = vec![evidence];
        artifacts.push(ReleaseArtifact {
            path: "sbom.spdx.json".into(), kind: ArtifactKind::Sbom, media_type: "application/spdx+json".into(),
            sha256: sha256_hex(b"{}"), bytes: 2,
        });
        artifacts.push(ReleaseArtifact {
            path: "provenance.json".into(), kind: ArtifactKind::Provenance, media_type: "application/json".into(),
            sha256: sha256_hex(b"{}"), bytes: 2,
        });
        ReleaseCandidate {
            schema_version: RELEASE_SCHEMA_VERSION.into(), release_id: "release-0.20.0".into(),
            version: "0.20.0".into(), channel: ReleaseChannel::Stable,
            created_at_utc: Utc.with_ymd_and_hms(2026, 9, 27, 8, 0, 0).unwrap(),
            source_commit: "0123456789abcdef".into(), artifacts,
            controls: REQUIRED_CONTROLS.iter().map(|id| ReleaseControl {
                id: (*id).into(), title: (*id).into(), status: ControlStatus::Passed,
                evidence_paths: vec![path.into()], waiver: None,
            }).collect(),
            telemetry: TelemetryPolicy {
                policy_version: "1".into(), default_enabled: false, explicit_consent_required: true,
                model_content_collected: false, precise_geometry_collected: false, retention_days: 30,
                allowed_event_categories: vec!["crash".into()], deletion_request_supported: true,
            },
            marketplace: MarketplacePolicy {
                signed_packages_required: true, sandbox_required: true, malware_scan_required: true,
                independent_review_required: true, revocation_supported: true,
                emergency_kill_switch_supported: true, compatibility_policy: "semver".into(),
            },
            signatures: vec![],
        }
    }

    #[test]
    fn accepts_complete_signed_candidate_and_rejects_tampering() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("evidence.json"), b"evidence").unwrap();
        fs::write(dir.path().join("sbom.spdx.json"), b"{}").unwrap();
        fs::write(dir.path().join("provenance.json"), b"{}").unwrap();
        let mut value = candidate("evidence.json", b"evidence");
        let signing = SigningKey::from_bytes(&[20_u8; 32]);
        let digest = value.unsigned_sha256().unwrap();
        value.signatures.push(ReleaseSignature {
            key_id: "release-2026".into(), algorithm: "ed25519-sha256".into(),
            signed_sha256: digest.clone(),
            signature_base64: STANDARD.encode(signing.sign(digest.as_bytes()).to_bytes()),
        });
        let trust = ReleaseTrustStore {
            schema_version: RELEASE_TRUST_SCHEMA_VERSION.into(),
            ed25519_public_keys: BTreeMap::from([("release-2026".into(), STANDARD.encode(signing.verifying_key().to_bytes()))]),
            revoked_key_ids: BTreeSet::new(),
        };
        let now = Utc.with_ymd_and_hms(2026, 9, 27, 12, 0, 0).unwrap();
        assert!(verify_release(&value, dir.path(), &trust, now).unwrap().accepted);
        fs::write(dir.path().join("evidence.json"), b"changed").unwrap();
        assert!(!verify_release(&value, dir.path(), &trust, now).unwrap().accepted);
    }

    #[test]
    fn rejects_opt_out_telemetry_and_expired_waiver() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("evidence.json"), b"evidence").unwrap();
        fs::write(dir.path().join("sbom.spdx.json"), b"{}").unwrap();
        fs::write(dir.path().join("provenance.json"), b"{}").unwrap();
        let mut value = candidate("evidence.json", b"evidence");
        value.channel = ReleaseChannel::Pilot;
        value.telemetry.default_enabled = true;
        value.controls[0].status = ControlStatus::Waived;
        value.controls[0].waiver = Some(Waiver {
            approved_by: "security".into(), reason: "temporary".into(),
            expires_at_utc: Utc.with_ymd_and_hms(2026, 9, 26, 0, 0, 0).unwrap(),
            issue_reference: "SEC-1".into(),
        });
        let trust = ReleaseTrustStore {
            schema_version: RELEASE_TRUST_SCHEMA_VERSION.into(),
            ed25519_public_keys: BTreeMap::new(), revoked_key_ids: BTreeSet::new(),
        };
        let report = verify_release(
            &value, dir.path(), &trust, Utc.with_ymd_and_hms(2026, 9, 27, 12, 0, 0).unwrap()
        ).unwrap();
        assert!(!report.accepted);
        assert!(report.issues.iter().any(|v| v.code == "privacy.telemetry.policy"));
        assert!(report.issues.iter().any(|v| v.code == "control.waiver.invalid"));
    }
}
