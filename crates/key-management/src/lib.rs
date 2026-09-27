//! Key-management boundary for HSM/KMS-backed signing and envelope encryption.
//!
//! Production code should implement `KeyManagementProvider` with non-exportable
//! keys held by an HSM, cloud KMS or approved on-premise vault. The in-memory
//! provider exists only for deterministic integration tests and local development.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use structural_audit::sha256_hex;

pub const KEY_ENVELOPE_SCHEMA_VERSION: &str = "structural-key-envelope/1.0";
pub const KEY_AUDIT_SCHEMA_VERSION: &str = "structural-key-audit/1.0";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyPurpose {
    Signing,
    KeyEncryption,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Active,
    DecryptOnly,
    Revoked,
    Destroyed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyMetadata {
    pub key_id: String,
    pub version: u32,
    pub purpose: KeyPurpose,
    pub algorithm: String,
    pub state: KeyState,
    pub created_at_utc: DateTime<Utc>,
    pub activates_at_utc: DateTime<Utc>,
    pub expires_at_utc: Option<DateTime<Utc>>,
    pub provider: String,
    pub hardware_protected: bool,
    pub exportable: bool,
}

impl KeyMetadata {
    pub fn validate_for_use(&self, purpose: KeyPurpose, now: DateTime<Utc>) -> Result<()> {
        if self.purpose != purpose {
            bail!("key purpose mismatch");
        }
        if self.state != KeyState::Active {
            bail!("key is not active");
        }
        if now < self.activates_at_utc || self.expires_at_utc.is_some_and(|expiry| now >= expiry) {
            bail!("key is outside its activation window");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetachedSignature {
    pub key_id: String,
    pub key_version: u32,
    pub algorithm: String,
    pub payload_sha256: String,
    pub signature_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WrappedDataKey {
    pub key_id: String,
    pub key_version: u32,
    pub algorithm: String,
    pub context_sha256: String,
    pub nonce_base64: String,
    pub wrapped_key_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedEnvelope {
    pub schema_version: String,
    pub algorithm: String,
    pub nonce_base64: String,
    pub ciphertext_base64: String,
    pub associated_data_sha256: String,
    pub plaintext_sha256: String,
    pub wrapped_data_key: WrappedDataKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyAuditEvent {
    pub schema_version: String,
    pub event_id: String,
    pub occurred_at_utc: DateTime<Utc>,
    pub actor_principal_id: String,
    pub operation: String,
    pub key_id: String,
    pub key_version: u32,
    pub purpose: KeyPurpose,
    pub request_sha256: String,
    pub outcome: String,
}

pub trait KeyManagementProvider {
    fn metadata(&self, key_id: &str, version: Option<u32>) -> Result<KeyMetadata>;
    fn sign(&self, key_id: &str, payload: &[u8], context: &[u8], now: DateTime<Utc>) -> Result<DetachedSignature>;
    fn verify(&self, signature: &DetachedSignature, payload: &[u8], context: &[u8]) -> Result<()>;
    fn wrap_data_key(&self, key_id: &str, data_key: &[u8; 32], context: &[u8], now: DateTime<Utc>) -> Result<WrappedDataKey>;
    fn unwrap_data_key(&self, wrapped: &WrappedDataKey, context: &[u8]) -> Result<[u8; 32]>;
}

pub fn encrypt_envelope(
    provider: &dyn KeyManagementProvider,
    wrapping_key_id: &str,
    plaintext: &[u8],
    associated_data: &[u8],
    now: DateTime<Utc>,
) -> Result<EncryptedEnvelope> {
    let mut data_key = [0_u8; 32];
    let mut nonce = [0_u8; 24];
    getrandom::getrandom(&mut data_key)
        .map_err(|error| anyhow::anyhow!("data-key generation failed: {error}"))?;
    getrandom::getrandom(&mut nonce)
        .map_err(|error| anyhow::anyhow!("nonce generation failed: {error}"))?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&data_key));
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad: associated_data })
        .map_err(|_| anyhow::anyhow!("envelope encryption failed"))?;
    let wrapped = provider.wrap_data_key(wrapping_key_id, &data_key, associated_data, now)?;
    data_key.fill(0);
    Ok(EncryptedEnvelope {
        schema_version: KEY_ENVELOPE_SCHEMA_VERSION.to_owned(),
        algorithm: "XChaCha20-Poly1305".to_owned(),
        nonce_base64: STANDARD.encode(nonce),
        ciphertext_base64: STANDARD.encode(ciphertext),
        associated_data_sha256: sha256_hex(associated_data),
        plaintext_sha256: sha256_hex(plaintext),
        wrapped_data_key: wrapped,
    })
}

pub fn decrypt_envelope(
    provider: &dyn KeyManagementProvider,
    envelope: &EncryptedEnvelope,
    associated_data: &[u8],
) -> Result<Vec<u8>> {
    if envelope.schema_version != KEY_ENVELOPE_SCHEMA_VERSION
        || envelope.algorithm != "XChaCha20-Poly1305"
        || envelope.associated_data_sha256 != sha256_hex(associated_data)
    {
        bail!("envelope metadata or associated data mismatch");
    }
    let mut data_key = provider.unwrap_data_key(&envelope.wrapped_data_key, associated_data)?;
    let nonce = STANDARD.decode(&envelope.nonce_base64).context("invalid nonce encoding")?;
    if nonce.len() != 24 {
        bail!("invalid XChaCha20 nonce length");
    }
    let ciphertext = STANDARD.decode(&envelope.ciphertext_base64).context("invalid ciphertext encoding")?;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&data_key));
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&nonce), Payload { msg: &ciphertext, aad: associated_data })
        .map_err(|_| anyhow::anyhow!("envelope authentication failed"))?;
    data_key.fill(0);
    if sha256_hex(&plaintext) != envelope.plaintext_sha256 {
        bail!("plaintext hash mismatch");
    }
    Ok(plaintext)
}

/// Development-only provider. It deliberately reports `hardware_protected=false`
/// and `exportable=true`, so deployment policy can reject it.
pub struct InMemoryKeyManager {
    signing: BTreeMap<(String, u32), (KeyMetadata, SigningKey)>,
    wrapping: BTreeMap<(String, u32), (KeyMetadata, [u8; 32])>,
    active_versions: BTreeMap<String, u32>,
}

impl InMemoryKeyManager {
    pub fn new() -> Self {
        Self { signing: BTreeMap::new(), wrapping: BTreeMap::new(), active_versions: BTreeMap::new() }
    }

    pub fn add_signing_key(&mut self, key_id: &str, version: u32, bytes: [u8; 32], now: DateTime<Utc>) {
        self.signing.insert((key_id.into(), version), (KeyMetadata {
            key_id: key_id.into(), version, purpose: KeyPurpose::Signing,
            algorithm: "Ed25519".into(), state: KeyState::Active,
            created_at_utc: now, activates_at_utc: now, expires_at_utc: None,
            provider: "in-memory-development-only".into(), hardware_protected: false, exportable: true,
        }, SigningKey::from_bytes(&bytes)));
        self.active_versions.insert(key_id.into(), version);
    }

    pub fn add_wrapping_key(&mut self, key_id: &str, version: u32, bytes: [u8; 32], now: DateTime<Utc>) {
        self.wrapping.insert((key_id.into(), version), (KeyMetadata {
            key_id: key_id.into(), version, purpose: KeyPurpose::KeyEncryption,
            algorithm: "XChaCha20-Poly1305-KW".into(), state: KeyState::Active,
            created_at_utc: now, activates_at_utc: now, expires_at_utc: None,
            provider: "in-memory-development-only".into(), hardware_protected: false, exportable: true,
        }, bytes));
        self.active_versions.insert(key_id.into(), version);
    }

    fn version(&self, key_id: &str, requested: Option<u32>) -> Result<u32> {
        requested.or_else(|| self.active_versions.get(key_id).copied())
            .with_context(|| format!("unknown key '{key_id}'"))
    }
}

impl Default for InMemoryKeyManager { fn default() -> Self { Self::new() } }

impl KeyManagementProvider for InMemoryKeyManager {
    fn metadata(&self, key_id: &str, version: Option<u32>) -> Result<KeyMetadata> {
        let version = self.version(key_id, version)?;
        self.signing.get(&(key_id.into(), version)).map(|v| v.0.clone())
            .or_else(|| self.wrapping.get(&(key_id.into(), version)).map(|v| v.0.clone()))
            .with_context(|| format!("unknown key '{key_id}' version {version}"))
    }

    fn sign(&self, key_id: &str, payload: &[u8], context: &[u8], now: DateTime<Utc>) -> Result<DetachedSignature> {
        let version = self.version(key_id, None)?;
        let (metadata, key) = self.signing.get(&(key_id.into(), version)).context("signing key not found")?;
        metadata.validate_for_use(KeyPurpose::Signing, now)?;
        let material = signature_material(payload, context);
        Ok(DetachedSignature {
            key_id: key_id.into(), key_version: version, algorithm: "Ed25519".into(),
            payload_sha256: sha256_hex(payload),
            signature_base64: STANDARD.encode(key.sign(&material).to_bytes()),
        })
    }

    fn verify(&self, signature: &DetachedSignature, payload: &[u8], context: &[u8]) -> Result<()> {
        if signature.algorithm != "Ed25519" || signature.payload_sha256 != sha256_hex(payload) {
            bail!("signature metadata mismatch");
        }
        let (_, key) = self.signing.get(&(signature.key_id.clone(), signature.key_version))
            .context("verification key not found")?;
        let bytes = STANDARD.decode(&signature.signature_base64).context("invalid signature encoding")?;
        let signature_value = Signature::from_slice(&bytes).context("invalid Ed25519 signature")?;
        key.verifying_key().verify(&signature_material(payload, context), &signature_value)
            .context("signature verification failed")
    }

    fn wrap_data_key(&self, key_id: &str, data_key: &[u8; 32], context: &[u8], now: DateTime<Utc>) -> Result<WrappedDataKey> {
        let version = self.version(key_id, None)?;
        let (metadata, key) = self.wrapping.get(&(key_id.into(), version)).context("wrapping key not found")?;
        metadata.validate_for_use(KeyPurpose::KeyEncryption, now)?;
        let mut nonce = [0_u8; 24];
        getrandom::getrandom(&mut nonce)
            .map_err(|error| anyhow::anyhow!("wrapping nonce generation failed: {error}"))?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
        let wrapped = cipher.encrypt(XNonce::from_slice(&nonce), Payload { msg: data_key, aad: context })
            .map_err(|_| anyhow::anyhow!("data-key wrapping failed"))?;
        Ok(WrappedDataKey {
            key_id: key_id.into(), key_version: version, algorithm: "XChaCha20-Poly1305-KW".into(),
            context_sha256: sha256_hex(context), nonce_base64: STANDARD.encode(nonce),
            wrapped_key_base64: STANDARD.encode(wrapped),
        })
    }

    fn unwrap_data_key(&self, wrapped: &WrappedDataKey, context: &[u8]) -> Result<[u8; 32]> {
        if wrapped.algorithm != "XChaCha20-Poly1305-KW" || wrapped.context_sha256 != sha256_hex(context) {
            bail!("wrapped-key metadata mismatch");
        }
        let (metadata, key) = self.wrapping.get(&(wrapped.key_id.clone(), wrapped.key_version))
            .context("wrapping key version not found")?;
        if matches!(metadata.state, KeyState::Revoked | KeyState::Destroyed) {
            bail!("key version is revoked or destroyed");
        }
        let nonce = STANDARD.decode(&wrapped.nonce_base64).context("invalid wrapping nonce encoding")?;
        if nonce.len() != 24 { bail!("invalid wrapping nonce length"); }
        let bytes = STANDARD.decode(&wrapped.wrapped_key_base64).context("invalid wrapped key encoding")?;
        let clear = XChaCha20Poly1305::new(Key::from_slice(key))
            .decrypt(XNonce::from_slice(&nonce), Payload { msg: &bytes, aad: context })
            .map_err(|_| anyhow::anyhow!("data-key unwrap failed"))?;
        clear.try_into().map_err(|_| anyhow::anyhow!("unwrapped data key has invalid length"))
    }
}

fn signature_material(payload: &[u8], context: &[u8]) -> Vec<u8> {
    let mut value = b"structural-signature-v1\0".to_vec();
    value.extend_from_slice(&(context.len() as u64).to_be_bytes());
    value.extend_from_slice(context);
    value.extend_from_slice(payload);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_envelope_encrypts_with_versioned_keys() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut keys = InMemoryKeyManager::new();
        keys.add_signing_key("assurance-signing", 3, [3; 32], now);
        keys.add_wrapping_key("evidence-kek", 5, [5; 32], now);
        let payload = b"evidence";
        let signature = keys.sign("assurance-signing", payload, b"portal", now).unwrap();
        keys.verify(&signature, payload, b"portal").unwrap();
        assert!(keys.verify(&signature, b"changed", b"portal").is_err());

        let envelope = encrypt_envelope(&keys, "evidence-kek", payload, b"tenant/project", now).unwrap();
        assert_eq!(decrypt_envelope(&keys, &envelope, b"tenant/project").unwrap(), payload);
        assert!(decrypt_envelope(&keys, &envelope, b"other").is_err());
    }
}
