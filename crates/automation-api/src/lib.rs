//! Stable, transport-neutral automation contracts.
//!
//! The wire contract is JSON and deliberately does not expose Rust implementation
//! types. Minor API versions may add optional fields and operations. Breaking wire
//! changes require a new major version.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;

pub const AUTOMATION_API_VERSION: &str = "1.1";
pub const AUTOMATION_SCHEMA_VERSION: &str = "structural-automation/1.0";
pub const PYTHON_BINDING_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    InspectSystem,
    ReadArtifacts,
    ValidateModels,
    EvaluateRules,
    RunAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiRequest {
    pub schema_version: String,
    pub request_id: Uuid,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

impl ApiRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != AUTOMATION_SCHEMA_VERSION {
            bail!(
                "unsupported automation schema '{}'; expected '{}'",
                self.schema_version,
                AUTOMATION_SCHEMA_VERSION
            );
        }
        if self.method.trim().is_empty() {
            bail!("automation method is required");
        }
        let unique: BTreeSet<_> = self.capabilities.iter().collect();
        if unique.len() != self.capabilities.len() {
            bail!("duplicate capabilities are not allowed");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiStatus {
    Ok,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiEvidence {
    pub host_id: String,
    pub host_version: String,
    pub request_sha256: String,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiResponse {
    pub schema_version: String,
    pub request_id: Uuid,
    pub status: ApiStatus,
    pub result: Option<Value>,
    pub error: Option<ApiError>,
    pub evidence: ApiEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationDescriptor {
    pub method: String,
    pub since_api_version: String,
    pub required_capability: Capability,
    pub deterministic: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiDescription {
    pub schema_version: String,
    pub api_version: String,
    pub minimum_client_api_version: String,
    pub python_binding_version: String,
    pub operations: Vec<OperationDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptLimits {
    pub timeout_ms: u64,
    pub maximum_script_bytes: usize,
    pub maximum_output_bytes: usize,
    pub maximum_operations: usize,
}

impl Default for ScriptLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 5_000,
            maximum_script_bytes: 64 * 1024,
            maximum_output_bytes: 1024 * 1024,
            maximum_operations: 32,
        }
    }
}

impl ScriptLimits {
    pub fn validate(&self) -> Result<()> {
        if self.timeout_ms == 0 || self.timeout_ms > 300_000 {
            bail!("timeout_ms must be in 1..=300000");
        }
        if self.maximum_script_bytes == 0 || self.maximum_script_bytes > 1024 * 1024 {
            bail!("maximum_script_bytes must be in 1..=1048576");
        }
        if self.maximum_output_bytes == 0 || self.maximum_output_bytes > 16 * 1024 * 1024 {
            bail!("maximum_output_bytes must be in 1..=16777216");
        }
        if self.maximum_operations == 0 || self.maximum_operations > 1_000 {
            bail!("maximum_operations must be in 1..=1000");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PythonScriptRequest {
    pub schema_version: String,
    pub request_id: Uuid,
    pub python_executable: String,
    pub script_source: String,
    #[serde(default)]
    pub context: Value,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub limits: ScriptLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptOperation {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptPlan {
    #[serde(default)]
    pub output: Value,
    #[serde(default)]
    pub operations: Vec<ScriptOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScriptStatus {
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PythonScriptResponse {
    pub schema_version: String,
    pub request_id: Uuid,
    pub status: ScriptStatus,
    pub output: Option<Value>,
    pub operation_responses: Vec<ApiResponse>,
    pub error: Option<ApiError>,
    pub elapsed_ms: u128,
}
