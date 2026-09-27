//! Transport-neutral identity federation boundary.
//!
//! Production deployments fetch and cache issuer metadata/JWKS outside this crate,
//! then expose only trusted verification keys through `FederationTrustStore`.
//! Tokens are never accepted merely because their claims can be decoded.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use structural_audit::sha256_hex;

pub const IDENTITY_SCHEMA_VERSION: &str = "structural-federated-identity/1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OidcHeader {
    pub alg: String,
    pub kid: String,
    #[serde(default)]
    pub typ: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OidcClaims {
    pub iss: String,
    pub sub: String,
    #[serde(default)]
    pub aud: Vec<String>,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub nbf: Option<i64>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub acr: Option<String>,
    #[serde(default)]
    pub amr: Vec<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssuerPolicy {
    pub issuer: String,
    pub audiences: BTreeSet<String>,
    #[serde(default)]
    pub allowed_algorithms: BTreeSet<String>,
    #[serde(default)]
    pub required_acr: Option<String>,
    #[serde(default)]
    pub required_amr: BTreeSet<String>,
    #[serde(default)]
    pub allowed_tenants: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationPolicy {
    pub issuers: BTreeMap<String, IssuerPolicy>,
    pub clock_skew_seconds: i64,
    pub maximum_token_age_seconds: i64,
}

impl FederationPolicy {
    pub fn validate(&self) -> Result<()> {
        if self.issuers.is_empty() {
            bail!("at least one trusted issuer is required");
        }
        if !(0..=600).contains(&self.clock_skew_seconds) {
            bail!("clock_skew_seconds must be in 0..=600");
        }
        if self.maximum_token_age_seconds <= 0 || self.maximum_token_age_seconds > 86_400 {
            bail!("maximum_token_age_seconds must be in 1..=86400");
        }
        for (key, issuer) in &self.issuers {
            if key != &issuer.issuer || issuer.issuer.trim().is_empty() {
                bail!("issuer policy key must exactly match a non-empty issuer");
            }
            if issuer.audiences.is_empty() {
                bail!("issuer '{}' requires at least one audience", issuer.issuer);
            }
            let algorithms = if issuer.allowed_algorithms.is_empty() {
                BTreeSet::from(["EdDSA".to_owned()])
            } else {
                issuer.allowed_algorithms.clone()
            };
            if algorithms.iter().any(|value| value != "EdDSA") {
                bail!("reference verifier supports only EdDSA; use a reviewed provider adapter for other algorithms");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederatedPrincipal {
    pub schema_version: String,
    /// Stable pseudonymous ID derived from the exact issuer and subject.
    pub principal_id: String,
    pub issuer: String,
    pub subject: String,
    pub tenant_id: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub groups: Vec<String>,
    pub roles: Vec<String>,
    pub authentication_context: Option<String>,
    pub authentication_methods: Vec<String>,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub token_sha256: String,
    pub key_id: String,
}

pub trait FederationTrustStore {
    /// Return a currently trusted Ed25519 verification key for the exact issuer/key ID.
    fn ed25519_key(&self, issuer: &str, key_id: &str) -> Result<VerifyingKey>;
}

#[derive(Debug, Clone, Default)]
pub struct StaticFederationTrustStore {
    keys: BTreeMap<(String, String), VerifyingKey>,
}

impl StaticFederationTrustStore {
    pub fn insert(&mut self, issuer: impl Into<String>, key_id: impl Into<String>, key: VerifyingKey) {
        self.keys.insert((issuer.into(), key_id.into()), key);
    }
}

impl FederationTrustStore for StaticFederationTrustStore {
    fn ed25519_key(&self, issuer: &str, key_id: &str) -> Result<VerifyingKey> {
        self.keys
            .get(&(issuer.to_owned(), key_id.to_owned()))
            .copied()
            .with_context(|| format!("untrusted federation key '{key_id}' for issuer '{issuer}'"))
    }
}

pub fn stable_principal_id(issuer: &str, subject: &str) -> Result<String> {
    if issuer.trim().is_empty() || subject.trim().is_empty() {
        bail!("issuer and subject are required");
    }
    Ok(format!(
        "fed:{}",
        sha256_hex(format!("{issuer}\0{subject}").as_bytes())
    ))
}

/// Verify a compact JWS carrying OIDC-compatible claims.
///
/// The reference implementation intentionally supports only EdDSA. Production
/// adapters may support RS256/ES256, but must retain the same issuer, audience,
/// lifetime, tenant, ACR and AMR checks.
pub fn verify_compact_token(
    compact: &str,
    policy: &FederationPolicy,
    trust: &dyn FederationTrustStore,
    now: DateTime<Utc>,
) -> Result<FederatedPrincipal> {
    policy.validate()?;
    let parts: Vec<_> = compact.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        bail!("token must be a three-part compact JWS");
    }
    let header: OidcHeader = serde_json::from_slice(
        &URL_SAFE_NO_PAD.decode(parts[0]).context("invalid JWS header encoding")?,
    )
    .context("invalid JWS header")?;
    if header.alg == "none" {
        bail!("unsigned tokens are forbidden");
    }
    let claims: OidcClaims = serde_json::from_slice(
        &URL_SAFE_NO_PAD.decode(parts[1]).context("invalid JWS claims encoding")?,
    )
    .context("invalid JWS claims")?;
    let issuer = policy
        .issuers
        .get(&claims.iss)
        .with_context(|| format!("untrusted issuer '{}'", claims.iss))?;
    let algorithms = if issuer.allowed_algorithms.is_empty() {
        BTreeSet::from(["EdDSA".to_owned()])
    } else {
        issuer.allowed_algorithms.clone()
    };
    if !algorithms.contains(&header.alg) || header.alg != "EdDSA" {
        bail!("token algorithm '{}' is not allowed", header.alg);
    }
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .context("invalid JWS signature encoding")?;
    let signature = Signature::from_slice(&signature_bytes).context("invalid Ed25519 signature")?;
    trust
        .ed25519_key(&claims.iss, &header.kid)?
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .context("token signature verification failed")?;

    if claims.sub.trim().is_empty() {
        bail!("subject is required");
    }
    if !claims.aud.iter().any(|aud| issuer.audiences.contains(aud)) {
        bail!("token audience is not accepted");
    }
    let skew = Duration::seconds(policy.clock_skew_seconds);
    let issued = DateTime::from_timestamp(claims.iat, 0).context("invalid iat")?;
    let expires = DateTime::from_timestamp(claims.exp, 0).context("invalid exp")?;
    if issued > now + skew {
        bail!("token was issued in the future");
    }
    if expires < now - skew {
        bail!("token has expired");
    }
    if now - issued > Duration::seconds(policy.maximum_token_age_seconds) + skew {
        bail!("token exceeds maximum age");
    }
    if let Some(nbf) = claims.nbf {
        let not_before = DateTime::from_timestamp(nbf, 0).context("invalid nbf")?;
        if not_before > now + skew {
            bail!("token is not yet valid");
        }
    }
    if let Some(required) = &issuer.required_acr {
        if claims.acr.as_ref() != Some(required) {
            bail!("required authentication context was not satisfied");
        }
    }
    let methods: BTreeSet<_> = claims.amr.iter().cloned().collect();
    if !issuer.required_amr.is_subset(&methods) {
        bail!("required authentication methods were not satisfied");
    }
    if !issuer.allowed_tenants.is_empty() {
        let tenant = claims.tenant_id.as_ref().context("tenant_id is required")?;
        if !issuer.allowed_tenants.contains(tenant) {
            bail!("tenant is not allowed");
        }
    }

    Ok(FederatedPrincipal {
        schema_version: IDENTITY_SCHEMA_VERSION.to_owned(),
        principal_id: stable_principal_id(&claims.iss, &claims.sub)?,
        issuer: claims.iss,
        subject: claims.sub,
        tenant_id: claims.tenant_id,
        display_name: claims.name,
        email: claims.email,
        groups: claims.groups,
        roles: claims.roles,
        authentication_context: claims.acr,
        authentication_methods: claims.amr,
        authenticated_at: issued,
        expires_at: expires,
        token_sha256: sha256_hex(compact.as_bytes()),
        key_id: header.kid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn verifies_signed_token_and_rejects_wrong_audience() {
        let signing = SigningKey::from_bytes(&[7_u8; 32]);
        let issuer = "https://identity.example.test";
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let header = OidcHeader { alg: "EdDSA".into(), kid: "federation-2026-01".into(), typ: Some("JWT".into()) };
        let claims = OidcClaims {
            iss: issuer.into(), sub: "engineer-42".into(), aud: vec!["structural-platform".into()],
            exp: now.timestamp() + 300, iat: now.timestamp(), nbf: None,
            email: Some("engineer@example.test".into()), name: Some("Engineer 42".into()),
            groups: vec!["project-a".into()], roles: vec!["engineer".into()],
            acr: Some("urn:mfa".into()), amr: vec!["pwd".into(), "otp".into()],
            tenant_id: Some("tenant-a".into()),
        };
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let c = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());
        let signing_input = format!("{h}.{c}");
        let token = format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signing.sign(signing_input.as_bytes()).to_bytes()));
        let mut trust = StaticFederationTrustStore::default();
        trust.insert(issuer, "federation-2026-01", signing.verifying_key());
        let policy = FederationPolicy {
            issuers: BTreeMap::from([(issuer.into(), IssuerPolicy {
                issuer: issuer.into(), audiences: BTreeSet::from(["structural-platform".into()]),
                allowed_algorithms: BTreeSet::from(["EdDSA".into()]), required_acr: Some("urn:mfa".into()),
                required_amr: BTreeSet::from(["otp".into()]), allowed_tenants: BTreeSet::from(["tenant-a".into()]),
            })]),
            clock_skew_seconds: 30, maximum_token_age_seconds: 3600,
        };
        let principal = verify_compact_token(&token, &policy, &trust, now).unwrap();
        assert!(principal.principal_id.starts_with("fed:"));
        let mut wrong = policy.clone();
        wrong.issuers.get_mut(issuer).unwrap().audiences = BTreeSet::from(["other".into()]);
        assert!(verify_compact_token(&token, &wrong, &trust, now).is_err());
    }
}
