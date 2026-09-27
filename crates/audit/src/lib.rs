use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use structural_units::RoundingPolicy;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArtifactRef {
    pub media_type: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionEnvironment {
    pub operating_system: String,
    pub architecture: String,
    pub runtime: String,
    pub hardware_summary: Option<String>,
    pub container_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunManifest {
    pub manifest_schema_version: String,
    pub run_id: Uuid,
    pub parent_run_id: Option<Uuid>,
    pub created_at_utc: DateTime<Utc>,
    pub input: ArtifactRef,
    pub result: ArtifactRef,
    pub solver_id: String,
    pub solver_version: String,
    pub solver_commit: Option<String>,
    pub relative_tolerance: f64,
    pub deterministic_profile: bool,
    pub random_seed: Option<u64>,
    /// Coherent computational unit system used by the artifacts.
    pub unit_system: String,
    /// Optional display/export rounding; raw solver artifacts remain unrounded.
    pub presentation_rounding: Option<RoundingPolicy>,
    pub environment: ExecutionEnvironment,
    /// Signature support is added later; the unsigned payload remains hashable now.
    pub signature: Option<String>,
}

pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    // Struct field order is stable for these versioned envelopes. Future map-like
    // payloads must be recursively key-sorted before relying on this function.
    Ok(serde_json::to_vec(value)?)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn hash_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(sha256_hex(&canonical_json_bytes(value)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Sample {
        a: u32,
        b: &'static str,
    }

    #[test]
    fn same_payload_has_same_hash() {
        let value = Sample { a: 42, b: "test" };
        assert_eq!(hash_json(&value).unwrap(), hash_json(&value).unwrap());
    }
}
