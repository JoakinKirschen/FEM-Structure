//! Permissively licensed, transport-neutral plugin contracts.
//!
//! Plugins are separate processes. A plugin receives exactly one JSON request on
//! stdin and must emit exactly one JSON response on stdout. The host remains the
//! authority for capability grants, signature trust and sandbox policy.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use structural_automation_api::Capability;
use uuid::Uuid;

pub const PLUGIN_SCHEMA_VERSION: &str = "structural-plugin/1.0";
pub const PLUGIN_PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiRequirement {
    pub minimum: String,
    pub maximum_exclusive: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStrength {
    Process,
    Os,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginLimits {
    pub timeout_ms: u64,
    pub maximum_stdout_bytes: usize,
    pub maximum_stderr_bytes: usize,
}

impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 10_000,
            maximum_stdout_bytes: 1024 * 1024,
            maximum_stderr_bytes: 256 * 1024,
        }
    }
}

impl PluginLimits {
    pub fn validate(&self) -> Result<()> {
        if self.timeout_ms == 0 || self.timeout_ms > 300_000 {
            bail!("timeout_ms must be in 1..=300000");
        }
        if self.maximum_stdout_bytes == 0 || self.maximum_stdout_bytes > 16 * 1024 * 1024 {
            bail!("maximum_stdout_bytes must be in 1..=16777216");
        }
        if self.maximum_stderr_bytes == 0 || self.maximum_stderr_bytes > 4 * 1024 * 1024 {
            bail!("maximum_stderr_bytes must be in 1..=4194304");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginEntrypoint {
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginSignature {
    pub algorithm: String,
    pub key_id: String,
    pub signed_sha256: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub schema_version: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub protocol_version: String,
    pub automation_api: ApiRequirement,
    pub entrypoint: PluginEntrypoint,
    #[serde(default)]
    pub requested_capabilities: Vec<Capability>,
    pub minimum_sandbox: SandboxStrength,
    #[serde(default)]
    pub limits: PluginLimits,
    pub signature: Option<PluginSignature>,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PLUGIN_SCHEMA_VERSION {
            bail!("unsupported plugin schema '{}'", self.schema_version);
        }
        if self.protocol_version != PLUGIN_PROTOCOL_VERSION {
            bail!("unsupported plugin protocol '{}'", self.protocol_version);
        }
        for (label, value) in [
            ("plugin_id", self.plugin_id.as_str()),
            ("name", self.name.as_str()),
            ("version", self.version.as_str()),
            ("publisher", self.publisher.as_str()),
            ("entrypoint.executable", self.entrypoint.executable.as_str()),
        ] {
            if value.trim().is_empty() {
                bail!("{label} must not be empty");
            }
        }
        if self.entrypoint.executable.contains("..") {
            bail!("entrypoint executable must not contain '..'");
        }
        let unique: BTreeSet<_> = self.requested_capabilities.iter().collect();
        if unique.len() != self.requested_capabilities.len() {
            bail!("duplicate requested capabilities are not allowed");
        }
        self.limits.validate()
    }

    pub fn unsigned_bytes(&self) -> Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        Ok(serde_json::to_vec(&unsigned)?)
    }

    pub fn unsigned_sha256(&self) -> Result<String> {
        Ok(hex_lower(&Sha256::digest(self.unsigned_bytes()?)))
    }

    pub fn sign(&mut self, key_id: impl Into<String>, key: &SigningKey) -> Result<()> {
        self.signature = None;
        let bytes = self.unsigned_bytes()?;
        let digest = Sha256::digest(&bytes);
        let signature = key.sign(&digest);
        self.signature = Some(PluginSignature {
            algorithm: "ed25519-sha256".to_owned(),
            key_id: key_id.into(),
            signed_sha256: hex_lower(&digest),
            signature_base64: STANDARD.encode(signature.to_bytes()),
        });
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustStore {
    /// Ed25519 public keys encoded as base64, indexed by stable key ID.
    #[serde(default)]
    pub ed25519_public_keys: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignatureVerification {
    pub verified: bool,
    pub key_id: Option<String>,
    pub manifest_sha256: String,
}

pub fn verify_manifest(manifest: &PluginManifest, trust: &TrustStore) -> Result<SignatureVerification> {
    manifest.validate()?;
    let signature = manifest.signature.as_ref().context("plugin manifest is unsigned")?;
    if signature.algorithm != "ed25519-sha256" {
        bail!("unsupported signature algorithm '{}'", signature.algorithm);
    }
    let encoded_key = trust
        .ed25519_public_keys
        .get(&signature.key_id)
        .with_context(|| format!("untrusted plugin signing key '{}'", signature.key_id))?;
    let key_bytes: [u8; 32] = STANDARD
        .decode(encoded_key)
        .context("invalid base64 public key")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 public key must contain 32 bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)?;
    let signature_bytes: [u8; 64] = STANDARD
        .decode(&signature.signature_base64)
        .context("invalid base64 signature")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 signature must contain 64 bytes"))?;
    let parsed_signature = Signature::from_bytes(&signature_bytes);
    let bytes = manifest.unsigned_bytes()?;
    let digest = Sha256::digest(&bytes);
    let digest_hex = hex_lower(&digest);
    if signature.signed_sha256 != digest_hex {
        bail!("manifest digest does not match signed_sha256");
    }
    verifying_key
        .verify(&digest, &parsed_signature)
        .context("plugin signature verification failed")?;
    Ok(SignatureVerification {
        verified: true,
        key_id: Some(signature.key_id.clone()),
        manifest_sha256: digest_hex,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginInvocation {
    pub schema_version: String,
    pub protocol_version: String,
    pub invocation_id: Uuid,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub granted_capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Ok,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginResponse {
    pub schema_version: String,
    pub protocol_version: String,
    pub invocation_id: Uuid,
    pub status: PluginStatus,
    pub result: Option<Value>,
    pub error: Option<PluginError>,
}

pub fn negotiate_api(requirement: &ApiRequirement, host_version: &str) -> Result<String> {
    let minimum = parse_version(&requirement.minimum)?;
    let maximum = parse_version(&requirement.maximum_exclusive)?;
    let host = parse_version(host_version)?;
    if minimum >= maximum {
        bail!("automation API range is empty");
    }
    if host < minimum || host >= maximum {
        bail!(
            "host API {} is outside plugin range [{}, {})",
            host_version, requirement.minimum, requirement.maximum_exclusive
        );
    }
    Ok(host_version.to_owned())
}

fn parse_version(value: &str) -> Result<(u64, u64)> {
    let mut parts = value.split('.');
    let major = parts.next().context("missing major API version")?.parse()?;
    let minor = parts.next().context("missing minor API version")?.parse()?;
    if parts.next().is_some() {
        bail!("API version must be major.minor");
    }
    Ok((major, minor))
}

pub fn public_key_base64(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_bytes())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> PluginManifest {
        PluginManifest {
            schema_version: PLUGIN_SCHEMA_VERSION.to_owned(),
            plugin_id: "example.echo".to_owned(),
            name: "Echo".to_owned(),
            version: "1.0.0".to_owned(),
            publisher: "Example".to_owned(),
            protocol_version: PLUGIN_PROTOCOL_VERSION.to_owned(),
            automation_api: ApiRequirement {
                minimum: "1.0".to_owned(),
                maximum_exclusive: "2.0".to_owned(),
            },
            entrypoint: PluginEntrypoint {
                executable: "echo-plugin".to_owned(),
                arguments: vec![],
            },
            requested_capabilities: vec![Capability::InspectSystem],
            minimum_sandbox: SandboxStrength::Process,
            limits: PluginLimits::default(),
            signature: None,
        }
    }

    #[test]
    fn signs_and_verifies_manifest() {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let mut value = manifest();
        value.sign("test", &key).unwrap();
        let trust = TrustStore {
            ed25519_public_keys: BTreeMap::from([(
                "test".to_owned(),
                public_key_base64(&key),
            )]),
        };
        assert!(verify_manifest(&value, &trust).unwrap().verified);
        value.name.push('!');
        assert!(verify_manifest(&value, &trust).is_err());
    }

    #[test]
    fn negotiates_bounded_api_range() {
        let requirement = ApiRequirement {
            minimum: "1.0".to_owned(),
            maximum_exclusive: "2.0".to_owned(),
        };
        assert_eq!(negotiate_api(&requirement, "1.0").unwrap(), "1.0");
        assert!(negotiate_api(&requirement, "2.0").is_err());
    }
}
