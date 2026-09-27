//! Transport-neutral local/cloud execution envelopes and parity evidence.
//!
//! The crate deliberately contains no HTTP client. Cloud vendors implement the
//! `RemoteExecutor` boundary while jobs, artifacts, retry decisions, attestations
//! and comparison reports retain the same versioned representation.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use structural_audit::{canonical_json_bytes, sha256_hex};
use uuid::Uuid;

pub const EXECUTION_SCHEMA_VERSION: &str = "structural-execution/1.0";
pub const ATTESTATION_SCHEMA_VERSION: &str = "structural-cloud-attestation/1.0";
pub const COMPARISON_SCHEMA_VERSION: &str = "structural-result-comparison/1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContentArtifact {
    pub logical_name: String,
    pub media_type: String,
    pub sha256: String,
    pub byte_length: u64,
}

impl ContentArtifact {
    pub fn from_bytes(
        logical_name: impl Into<String>,
        media_type: impl Into<String>,
        bytes: &[u8],
    ) -> Self {
        Self {
            logical_name: logical_name.into(),
            media_type: media_type.into(),
            sha256: sha256_hex(bytes),
            byte_length: bytes.len() as u64,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.logical_name.trim().is_empty() || self.media_type.trim().is_empty() {
            bail!("artifact logical_name and media_type are required");
        }
        validate_sha256(&self.sha256)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolchainIdentity {
    pub application_id: String,
    pub application_version: String,
    pub source_commit: Option<String>,
    pub target_triple: String,
    pub compiler: String,
    pub dependency_lock_sha256: Option<String>,
    pub executable_sha256: Option<String>,
    pub container_digest: Option<String>,
}

impl ToolchainIdentity {
    pub fn validate(&self) -> Result<()> {
        for (label, value) in [
            ("application_id", self.application_id.as_str()),
            ("application_version", self.application_version.as_str()),
            ("target_triple", self.target_triple.as_str()),
            ("compiler", self.compiler.as_str()),
        ] {
            if value.trim().is_empty() {
                bail!("{label} is required");
            }
        }
        for digest in [
            self.dependency_lock_sha256.as_deref(),
            self.executable_sha256.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_sha256(digest)?;
        }
        if let Some(digest) = &self.container_digest {
            if !digest.starts_with("sha256:") || digest.len() != 71 {
                bail!("container_digest must use sha256:<64 lowercase hex>");
            }
            validate_sha256(&digest[7..])?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRequest {
    pub cpu_cores: u32,
    pub memory_bytes: u64,
    pub maximum_runtime_seconds: u64,
}

impl ResourceRequest {
    fn validate(&self) -> Result<()> {
        if self.cpu_cores == 0 || self.memory_bytes == 0 || self.maximum_runtime_seconds == 0 {
            bail!("execution resources must be greater than zero");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub maximum_attempts: u32,
    pub initial_backoff_ms: u64,
    pub maximum_backoff_ms: u64,
}

impl RetryPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.maximum_attempts == 0 || self.maximum_attempts > 20 {
            bail!("maximum_attempts must be in 1..=20");
        }
        if self.initial_backoff_ms > self.maximum_backoff_ms {
            bail!("initial_backoff_ms must not exceed maximum_backoff_ms");
        }
        Ok(())
    }

    pub fn delay_before_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::ZERO;
        }
        let shift = (attempt - 2).min(62);
        let factor = 1_u64 << shift;
        Duration::from_millis(
            self.initial_backoff_ms
                .saturating_mul(factor)
                .min(self.maximum_backoff_ms),
        )
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            maximum_attempts: 3,
            initial_backoff_ms: 250,
            maximum_backoff_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionEnvelope {
    pub schema_version: String,
    pub job_id: Uuid,
    pub created_at_utc: DateTime<Utc>,
    pub operation: String,
    pub parameters: Value,
    pub inputs: Vec<ContentArtifact>,
    pub expected_output_media_types: Vec<String>,
    pub toolchain: ToolchainIdentity,
    pub resources: ResourceRequest,
    pub retry: RetryPolicy,
    pub cancellation_id: Uuid,
    pub deterministic_profile: bool,
    pub random_seed: Option<u64>,
}

impl ExecutionEnvelope {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != EXECUTION_SCHEMA_VERSION {
            bail!("unsupported execution schema '{}'", self.schema_version);
        }
        if self.operation.trim().is_empty() {
            bail!("operation is required");
        }
        if self.inputs.is_empty() {
            bail!("at least one input artifact is required");
        }
        for artifact in &self.inputs {
            artifact.validate()?;
        }
        let unique: BTreeSet<_> = self.inputs.iter().map(|a| &a.logical_name).collect();
        if unique.len() != self.inputs.len() {
            bail!("input logical names must be unique");
        }
        if self.expected_output_media_types.iter().any(|v| v.trim().is_empty()) {
            bail!("expected output media types must not be empty");
        }
        self.toolchain.validate()?;
        self.resources.validate()?;
        self.retry.validate()
    }

    pub fn sha256(&self) -> Result<String> {
        self.validate()?;
        Ok(sha256_hex(&canonical_json_bytes(self)?))
    }
}

/// Filesystem content-addressed store used by both local runners and reference
/// cloud adapters. Objects are re-hashed on every read.
#[derive(Debug, Clone)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("sha256"))?;
        Ok(Self { root })
    }

    pub fn upload(&self, bytes: &[u8]) -> Result<String> {
        let digest = sha256_hex(bytes);
        let path = self.object_path(&digest)?;
        if path.exists() {
            let existing = fs::read(&path)?;
            if existing != bytes {
                bail!("content-address collision or corrupt object for {digest}");
            }
        } else {
            let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
            fs::write(&temporary, bytes)?;
            fs::rename(&temporary, &path)?;
        }
        Ok(digest)
    }

    pub fn upload_file(&self, path: &Path) -> Result<ContentArtifact> {
        let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        let digest = self.upload(&bytes)?;
        Ok(ContentArtifact {
            logical_name: path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("artifact")
                .to_owned(),
            media_type: "application/octet-stream".to_owned(),
            sha256: digest,
            byte_length: bytes.len() as u64,
        })
    }

    pub fn download(&self, digest: &str) -> Result<Vec<u8>> {
        let path = self.object_path(digest)?;
        let bytes = fs::read(&path).with_context(|| format!("artifact {digest} is unavailable"))?;
        if sha256_hex(&bytes) != digest {
            bail!("artifact {digest} failed integrity verification");
        }
        Ok(bytes)
    }

    pub fn download_to(&self, artifact: &ContentArtifact, destination: &Path) -> Result<()> {
        artifact.validate()?;
        let bytes = self.download(&artifact.sha256)?;
        if bytes.len() as u64 != artifact.byte_length {
            bail!("artifact byte length differs from descriptor");
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, bytes)?;
        Ok(())
    }

    fn object_path(&self, digest: &str) -> Result<PathBuf> {
        validate_sha256(digest)?;
        Ok(self.root.join("sha256").join(digest))
    }
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("SHA-256 must contain exactly 64 lowercase hexadecimal characters");
    }
    Ok(())
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttemptFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttemptRecord {
    pub attempt: u32,
    pub outcome: String,
    pub error: Option<AttemptFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteExecutionResult {
    pub outputs: Vec<ContentArtifact>,
    pub result_summary: Value,
}

pub trait RemoteExecutor {
    fn execute(
        &mut self,
        envelope: &ExecutionEnvelope,
        cancellation: &CancellationToken,
    ) -> std::result::Result<RemoteExecutionResult, AttemptFailure>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetryExecutionReport {
    pub completed: bool,
    pub cancelled: bool,
    pub attempts: Vec<AttemptRecord>,
    pub result: Option<RemoteExecutionResult>,
}

pub fn execute_with_retry<E: RemoteExecutor>(
    executor: &mut E,
    envelope: &ExecutionEnvelope,
    cancellation: &CancellationToken,
) -> Result<RetryExecutionReport> {
    envelope.validate()?;
    let mut attempts = Vec::new();
    for attempt in 1..=envelope.retry.maximum_attempts {
        if cancellation.is_cancelled() {
            return Ok(RetryExecutionReport {
                completed: false,
                cancelled: true,
                attempts,
                result: None,
            });
        }
        let delay = envelope.retry.delay_before_attempt(attempt);
        if !delay.is_zero() {
            // Sleep in small slices so cancellation remains responsive.
            let mut waited = Duration::ZERO;
            while waited < delay {
                if cancellation.is_cancelled() {
                    return Ok(RetryExecutionReport {
                        completed: false,
                        cancelled: true,
                        attempts,
                        result: None,
                    });
                }
                let slice = (delay - waited).min(Duration::from_millis(25));
                thread::sleep(slice);
                waited += slice;
            }
        }
        match executor.execute(envelope, cancellation) {
            Ok(result) => {
                attempts.push(AttemptRecord {
                    attempt,
                    outcome: "completed".to_owned(),
                    error: None,
                });
                return Ok(RetryExecutionReport {
                    completed: true,
                    cancelled: false,
                    attempts,
                    result: Some(result),
                });
            }
            Err(error) => {
                let retryable = error.retryable;
                attempts.push(AttemptRecord {
                    attempt,
                    outcome: if retryable { "retryable_error" } else { "fatal_error" }.to_owned(),
                    error: Some(error),
                });
                if !retryable {
                    break;
                }
            }
        }
    }
    Ok(RetryExecutionReport {
        completed: false,
        cancelled: cancellation.is_cancelled(),
        attempts,
        result: None,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttestationSignature {
    pub algorithm: String,
    pub key_id: String,
    pub signed_sha256: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CloudAttestation {
    pub schema_version: String,
    pub attestation_id: Uuid,
    pub job_id: Uuid,
    pub envelope_sha256: String,
    pub started_at_utc: DateTime<Utc>,
    pub completed_at_utc: DateTime<Utc>,
    pub executor_id: String,
    pub execution_region: String,
    pub toolchain: ToolchainIdentity,
    pub inputs: Vec<ContentArtifact>,
    pub outputs: Vec<ContentArtifact>,
    pub attempt_count: u32,
    pub signature: Option<AttestationSignature>,
}

impl CloudAttestation {
    pub fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        self.validate_unsigned()?;
        let mut unsigned = self.clone();
        unsigned.signature = None;
        canonical_json_bytes(&unsigned)
    }

    pub fn sign(&mut self, key_id: impl Into<String>, key: &SigningKey) -> Result<()> {
        self.signature = None;
        let digest = Sha256::digest(self.unsigned_bytes()?);
        let signature = key.sign(&digest);
        self.signature = Some(AttestationSignature {
            algorithm: "ed25519-sha256".to_owned(),
            key_id: key_id.into(),
            signed_sha256: hex_lower(&digest),
            signature_base64: STANDARD.encode(signature.to_bytes()),
        });
        Ok(())
    }

    fn validate_unsigned(&self) -> Result<()> {
        if self.schema_version != ATTESTATION_SCHEMA_VERSION {
            bail!("unsupported attestation schema '{}'", self.schema_version);
        }
        validate_sha256(&self.envelope_sha256)?;
        if self.completed_at_utc < self.started_at_utc {
            bail!("attestation completion precedes start");
        }
        if self.executor_id.trim().is_empty() || self.execution_region.trim().is_empty() {
            bail!("executor_id and execution_region are required");
        }
        if self.attempt_count == 0 {
            bail!("attempt_count must be greater than zero");
        }
        self.toolchain.validate()?;
        for artifact in self.inputs.iter().chain(&self.outputs) {
            artifact.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttestationTrustStore {
    pub ed25519_public_keys: BTreeMap<String, String>,
}

pub fn public_key_base64(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_bytes())
}

pub fn verify_attestation(
    attestation: &CloudAttestation,
    trust: &AttestationTrustStore,
) -> Result<String> {
    attestation.validate_unsigned()?;
    let signature = attestation.signature.as_ref().context("attestation is unsigned")?;
    if signature.algorithm != "ed25519-sha256" {
        bail!("unsupported attestation signature algorithm");
    }
    let public = trust
        .ed25519_public_keys
        .get(&signature.key_id)
        .with_context(|| format!("untrusted attestation key '{}'", signature.key_id))?;
    let key_bytes: [u8; 32] = STANDARD
        .decode(public)
        .context("invalid public key base64")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 public key must contain 32 bytes"))?;
    let key = VerifyingKey::from_bytes(&key_bytes)?;
    let signature_bytes: [u8; 64] = STANDARD
        .decode(&signature.signature_base64)
        .context("invalid signature base64")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 signature must contain 64 bytes"))?;
    let digest = Sha256::digest(attestation.unsigned_bytes()?);
    let digest_hex = hex_lower(&digest);
    if digest_hex != signature.signed_sha256 {
        bail!("attestation signed_sha256 does not match payload");
    }
    key.verify(&digest, &Signature::from_bytes(&signature_bytes))
        .context("attestation signature verification failed")?;
    Ok(digest_hex)
}

pub fn verify_attestation_for(
    attestation: &CloudAttestation,
    envelope: &ExecutionEnvelope,
    trust: &AttestationTrustStore,
) -> Result<String> {
    envelope.validate()?;
    if attestation.job_id != envelope.job_id {
        bail!("attestation job_id does not match execution envelope");
    }
    if attestation.envelope_sha256 != envelope.sha256()? {
        bail!("attestation envelope digest does not match execution envelope");
    }
    if attestation.inputs != envelope.inputs {
        bail!("attested inputs do not match execution envelope");
    }
    verify_attestation(attestation, trust)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparisonProfile {
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    #[serde(default)]
    pub ignored_json_pointers: Vec<String>,
}

impl ComparisonProfile {
    pub fn validate(&self) -> Result<()> {
        if !self.absolute_tolerance.is_finite() || self.absolute_tolerance < 0.0 {
            bail!("absolute_tolerance must be finite and non-negative");
        }
        if !self.relative_tolerance.is_finite() || self.relative_tolerance < 0.0 {
            bail!("relative_tolerance must be finite and non-negative");
        }
        if self
            .ignored_json_pointers
            .iter()
            .any(|p| !p.is_empty() && !p.starts_with('/'))
        {
            bail!("ignored paths must be JSON pointers");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValueDifference {
    pub json_pointer: String,
    pub kind: String,
    pub local: Option<Value>,
    pub remote: Option<Value>,
    pub absolute_error: Option<f64>,
    pub allowed_error: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultComparison {
    pub schema_version: String,
    pub equivalent: bool,
    pub local_sha256: String,
    pub remote_sha256: String,
    pub profile: ComparisonProfile,
    pub compared_numeric_values: u64,
    pub maximum_absolute_error: f64,
    pub differences: Vec<ValueDifference>,
}

pub fn compare_json(
    local: &Value,
    remote: &Value,
    profile: ComparisonProfile,
) -> Result<ResultComparison> {
    profile.validate()?;
    let mut differences = Vec::new();
    let mut numeric = 0;
    let mut maximum = 0.0_f64;
    compare_value(
        "",
        local,
        remote,
        &profile,
        &mut differences,
        &mut numeric,
        &mut maximum,
    );
    Ok(ResultComparison {
        schema_version: COMPARISON_SCHEMA_VERSION.to_owned(),
        equivalent: differences.is_empty(),
        local_sha256: sha256_hex(&canonical_json_bytes(local)?),
        remote_sha256: sha256_hex(&canonical_json_bytes(remote)?),
        profile,
        compared_numeric_values: numeric,
        maximum_absolute_error: maximum,
        differences,
    })
}

fn compare_value(
    pointer: &str,
    local: &Value,
    remote: &Value,
    profile: &ComparisonProfile,
    differences: &mut Vec<ValueDifference>,
    numeric: &mut u64,
    maximum: &mut f64,
) {
    if profile.ignored_json_pointers.iter().any(|p| p == pointer) {
        return;
    }
    match (local, remote) {
        (Value::Number(a), Value::Number(b)) => {
            *numeric += 1;
            match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) if a.is_finite() && b.is_finite() => {
                    let error = (a - b).abs();
                    *maximum = (*maximum).max(error);
                    let allowed =
                        profile.absolute_tolerance + profile.relative_tolerance * a.abs().max(b.abs());
                    if error > allowed {
                        differences.push(difference(
                            pointer, "numeric_tolerance", local, remote, Some(error), Some(allowed),
                        ));
                    }
                }
                _ if a == b => {}
                _ => differences.push(difference(
                    pointer, "number_representation", local, remote, None, None,
                )),
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            let length = a.len().max(b.len());
            for index in 0..length {
                let child = format!("{pointer}/{index}");
                match (a.get(index), b.get(index)) {
                    (Some(a), Some(b)) => compare_value(
                        &child, a, b, profile, differences, numeric, maximum,
                    ),
                    (a, b) => differences.push(ValueDifference {
                        json_pointer: child,
                        kind: "missing_value".to_owned(),
                        local: a.cloned(),
                        remote: b.cloned(),
                        absolute_error: None,
                        allowed_error: None,
                    }),
                }
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                let child = format!("{pointer}/{escaped}");
                match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => compare_value(
                        &child, a, b, profile, differences, numeric, maximum,
                    ),
                    (a, b) => differences.push(ValueDifference {
                        json_pointer: child,
                        kind: "missing_value".to_owned(),
                        local: a.cloned(),
                        remote: b.cloned(),
                        absolute_error: None,
                        allowed_error: None,
                    }),
                }
            }
        }
        _ if local == remote => {}
        _ => differences.push(difference(
            pointer, "value_mismatch", local, remote, None, None,
        )),
    }
}

fn difference(
    pointer: &str,
    kind: &str,
    local: &Value,
    remote: &Value,
    absolute_error: Option<f64>,
    allowed_error: Option<f64>,
) -> ValueDifference {
    ValueDifference {
        json_pointer: pointer.to_owned(),
        kind: kind.to_owned(),
        local: Some(local.clone()),
        remote: Some(remote.clone()),
        absolute_error,
        allowed_error,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    fn toolchain() -> ToolchainIdentity {
        ToolchainIdentity {
            application_id: "structural-cli".to_owned(),
            application_version: "0.17.0".to_owned(),
            source_commit: Some("abc123".to_owned()),
            target_triple: "x86_64-unknown-linux-gnu".to_owned(),
            compiler: "rustc 1.x".to_owned(),
            dependency_lock_sha256: Some("0".repeat(64)),
            executable_sha256: None,
            container_digest: Some(format!("sha256:{}", "1".repeat(64))),
        }
    }

    fn envelope() -> ExecutionEnvelope {
        ExecutionEnvelope {
            schema_version: EXECUTION_SCHEMA_VERSION.to_owned(),
            job_id: Uuid::nil(),
            created_at_utc: Utc::now(),
            operation: "analysis.modal".to_owned(),
            parameters: json!({"requested_modes": 2}),
            inputs: vec![ContentArtifact::from_bytes("input.json", "application/json", b"{}")],
            expected_output_media_types: vec!["application/json".to_owned()],
            toolchain: toolchain(),
            resources: ResourceRequest {
                cpu_cores: 2,
                memory_bytes: 1_000_000,
                maximum_runtime_seconds: 60,
            },
            retry: RetryPolicy {
                maximum_attempts: 3,
                initial_backoff_ms: 0,
                maximum_backoff_ms: 0,
            },
            cancellation_id: Uuid::new_v4(),
            deterministic_profile: true,
            random_seed: None,
        }
    }

    #[test]
    fn store_round_trip_verifies_content() {
        let root = std::env::temp_dir().join(format!("structural-store-{}", Uuid::new_v4()));
        let store = ArtifactStore::open(&root).unwrap();
        let digest = store.upload(b"evidence").unwrap();
        assert_eq!(store.download(&digest).unwrap(), b"evidence");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn store_rejects_corrupt_content() {
        let root = std::env::temp_dir().join(format!("structural-store-{}", Uuid::new_v4()));
        let store = ArtifactStore::open(&root).unwrap();
        let digest = store.upload(b"evidence").unwrap();
        fs::write(root.join("sha256").join(&digest), b"tampered").unwrap();
        assert!(store.download(&digest).is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn attestation_signature_detects_tampering() {
        let envelope = envelope();
        let key = SigningKey::from_bytes(&[7; 32]);
        let mut attestation = CloudAttestation {
            schema_version: ATTESTATION_SCHEMA_VERSION.to_owned(),
            attestation_id: Uuid::new_v4(),
            job_id: envelope.job_id,
            envelope_sha256: envelope.sha256().unwrap(),
            started_at_utc: Utc::now(),
            completed_at_utc: Utc::now(),
            executor_id: "reference-cloud".to_owned(),
            execution_region: "eu-test-1".to_owned(),
            toolchain: toolchain(),
            inputs: envelope.inputs.clone(),
            outputs: vec![ContentArtifact::from_bytes("result.json", "application/json", b"{}")],
            attempt_count: 1,
            signature: None,
        };
        attestation.sign("test-key", &key).unwrap();
        let mut trust = AttestationTrustStore::default();
        trust
            .ed25519_public_keys
            .insert("test-key".to_owned(), public_key_base64(&key));
        assert!(verify_attestation_for(&attestation, &envelope, &trust).is_ok());
        attestation.executor_id = "tampered".to_owned();
        assert!(verify_attestation(&attestation, &trust).is_err());
    }

    #[test]
    fn comparison_accepts_small_numeric_differences_and_ignored_metadata() {
        let local = json!({"value": 100.0, "run_id": "local", "nested": [1.0, 2.0]});
        let remote = json!({"value": 100.00005, "run_id": "cloud", "nested": [1.0, 2.0000001]});
        let report = compare_json(
            &local,
            &remote,
            ComparisonProfile {
                absolute_tolerance: 1e-6,
                relative_tolerance: 1e-6,
                ignored_json_pointers: vec!["/run_id".to_owned()],
            },
        )
        .unwrap();
        assert!(report.equivalent);
        assert_eq!(report.compared_numeric_values, 3);
    }

    struct FlakyExecutor {
        calls: Arc<AtomicU32>,
    }

    impl RemoteExecutor for FlakyExecutor {
        fn execute(
            &mut self,
            _envelope: &ExecutionEnvelope,
            _cancellation: &CancellationToken,
        ) -> std::result::Result<RemoteExecutionResult, AttemptFailure> {
            let call = self.calls.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            if call < 3 {
                Err(AttemptFailure {
                    code: "temporary_unavailable".to_owned(),
                    message: "retry".to_owned(),
                    retryable: true,
                })
            } else {
                Ok(RemoteExecutionResult {
                    outputs: vec![],
                    result_summary: json!({"ok": true}),
                })
            }
        }
    }

    #[test]
    fn retry_policy_reaches_success_and_records_attempts() {
        let calls = Arc::new(AtomicU32::new(0));
        let mut executor = FlakyExecutor { calls };
        let report = execute_with_retry(
            &mut executor,
            &envelope(),
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(report.completed);
        assert_eq!(report.attempts.len(), 3);
    }

    #[test]
    fn cancelled_execution_does_not_call_remote() {
        let calls = Arc::new(AtomicU32::new(0));
        let mut executor = FlakyExecutor {
            calls: calls.clone(),
        };
        let token = CancellationToken::default();
        token.cancel();
        let report = execute_with_retry(&mut executor, &envelope(), &token).unwrap();
        assert!(report.cancelled);
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }
}
