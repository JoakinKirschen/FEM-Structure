//! Reference local automation host and restricted Python planner.
//!
//! Capability checks are authoritative at every API operation. The Python runner
//! additionally removes normal imports and dangerous builtins, clears the process
//! environment, uses an isolated working directory, and enforces time/output/call
//! limits. It is defense in depth, not a substitute for an OS/container sandbox
//! when running hostile code.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use structural_audit::{hash_json, sha256_hex};
use structural_automation_api::*;
use structural_domain::AnalysisInput;
use structural_rules_api::{RuleEvaluationInput, RulePackage};
use structural_rules_engine::evaluate;
use structural_solver_api::{
    NonlinearAnalysisInput, NonlinearExecutionOptions, NonlinearRestartPoint,
    NonlinearStructuralSolver,
};
use structural_solver_nonlinear::ReferenceNonlinearSpringSolver;
use uuid::Uuid;

pub const HOST_ID: &str = "structural-local-automation";
pub const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");

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

pub fn describe() -> ApiDescription {
    ApiDescription {
        schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
        api_version: AUTOMATION_API_VERSION.to_owned(),
        minimum_client_api_version: "1.0".to_owned(),
        python_binding_version: PYTHON_BINDING_VERSION.to_owned(),
        operations: vec![
            descriptor(
                "system.describe",
                Capability::InspectSystem,
                true,
                "Describe API compatibility and available operations.",
            ),
            descriptor(
                "artifact.sha256",
                Capability::ReadArtifacts,
                true,
                "Hash UTF-8 text supplied in the request; no host path access.",
            ),
            descriptor(
                "model.validate",
                Capability::ValidateModels,
                true,
                "Validate a structural AnalysisInput document.",
            ),
            descriptor(
                "rules.evaluate",
                Capability::EvaluateRules,
                true,
                "Evaluate a versioned rules package and input.",
            ),
            descriptor_since(
                "analysis.solve_nonlinear",
                "1.1",
                Capability::RunAnalysis,
                true,
                "Run the deterministic reference nonlinear spring solver.",
            ),
        ],
    }
}

fn descriptor(
    method: &str,
    required_capability: Capability,
    deterministic: bool,
    summary: &str,
) -> OperationDescriptor {
    OperationDescriptor {
        method: method.to_owned(),
        since_api_version: "1.0".to_owned(),
        required_capability,
        deterministic,
        summary: summary.to_owned(),
    }
}

fn descriptor_since(
    method: &str,
    since_api_version: &str,
    required_capability: Capability,
    deterministic: bool,
    summary: &str,
) -> OperationDescriptor {
    OperationDescriptor {
        method: method.to_owned(),
        since_api_version: since_api_version.to_owned(),
        required_capability,
        deterministic,
        summary: summary.to_owned(),
    }
}

pub fn invoke(request: &ApiRequest, cancellation: &CancellationToken) -> ApiResponse {
    let request_hash = hash_json(request).unwrap_or_else(|_| "unavailable".to_owned());
    let evidence = || ApiEvidence {
        host_id: HOST_ID.to_owned(),
        host_version: HOST_VERSION.to_owned(),
        request_sha256: request_hash.clone(),
        deterministic: true,
    };

    if let Err(error) = request.validate() {
        return failure(request.request_id, "invalid_request", error.to_string(), evidence());
    }
    if cancellation.is_cancelled() {
        return ApiResponse {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: request.request_id,
            status: ApiStatus::Cancelled,
            result: None,
            error: Some(ApiError {
                code: "cancelled".to_owned(),
                message: "Request was cancelled before dispatch.".to_owned(),
                retryable: true,
            }),
            evidence: evidence(),
        };
    }

    let required = match required_capability(&request.method) {
        Some(value) => value,
        None => {
            return failure(
                request.request_id,
                "method_not_found",
                format!("Unknown automation method '{}'.", request.method),
                evidence(),
            )
        }
    };
    if !request.capabilities.contains(&required) {
        return failure(
            request.request_id,
            "capability_denied",
            format!(
                "Method '{}' requires capability '{:?}'.",
                request.method, required
            ),
            evidence(),
        );
    }

    let result = dispatch(request);
    match result {
        Ok(value) => ApiResponse {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: request.request_id,
            status: ApiStatus::Ok,
            result: Some(value),
            error: None,
            evidence: evidence(),
        },
        Err(error) => failure(
            request.request_id,
            "operation_failed",
            error.to_string(),
            evidence(),
        ),
    }
}

fn required_capability(method: &str) -> Option<Capability> {
    match method {
        "system.describe" => Some(Capability::InspectSystem),
        "artifact.sha256" => Some(Capability::ReadArtifacts),
        "model.validate" => Some(Capability::ValidateModels),
        "rules.evaluate" => Some(Capability::EvaluateRules),
        "analysis.solve_nonlinear" => Some(Capability::RunAnalysis),
        _ => None,
    }
}

fn dispatch(request: &ApiRequest) -> Result<Value> {
    match request.method.as_str() {
        "system.describe" => Ok(serde_json::to_value(describe())?),
        "artifact.sha256" => {
            #[derive(Deserialize)]
            struct Params {
                text: String,
            }
            let params: Params = serde_json::from_value(request.params.clone())
                .context("artifact.sha256 params must contain string field 'text'")?;
            Ok(json!({
                "sha256": sha256_hex(params.text.as_bytes()),
                "byte_length": params.text.len()
            }))
        }
        "model.validate" => {
            #[derive(Deserialize)]
            struct Params {
                input: AnalysisInput,
            }
            let params: Params = serde_json::from_value(request.params.clone())
                .context("model.validate params must contain an AnalysisInput field 'input'")?;
            let issues = params.input.model.validate();
            let validation_error = params
                .input
                .validate_or_error()
                .err()
                .map(|error| error.to_string());
            let valid = validation_error.is_none();
            Ok(json!({
                "valid": valid,
                "issues": issues,
                "validation_error": validation_error
            }))
        }
        "rules.evaluate" => {
            #[derive(Deserialize)]
            struct Params {
                package: RulePackage,
                input: RuleEvaluationInput,
            }
            let params: Params = serde_json::from_value(request.params.clone())
                .context("rules.evaluate params must contain 'package' and 'input'")?;
            let (report, evidence) = evaluate(&params.package, &params.input)?;
            Ok(json!({ "report": report, "evidence": evidence }))
        }
        "analysis.solve_nonlinear" => {
            #[derive(Deserialize)]
            struct Params {
                input: NonlinearAnalysisInput,
                options: NonlinearExecutionOptions,
                #[serde(default)]
                restart: Option<NonlinearRestartPoint>,
            }
            let params: Params = serde_json::from_value(request.params.clone()).context(
                "analysis.solve_nonlinear params must contain 'input' and 'options'",
            )?;
            let solver = ReferenceNonlinearSpringSolver;
            let result =
                solver.solve_nonlinear(&params.input, params.options, params.restart.as_ref())?;
            Ok(serde_json::to_value(result)?)
        }
        _ => bail!("unknown method"),
    }
}

fn failure(request_id: Uuid, code: &str, message: String, evidence: ApiEvidence) -> ApiResponse {
    ApiResponse {
        schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
        request_id,
        status: ApiStatus::Error,
        result: None,
        error: Some(ApiError {
            code: code.to_owned(),
            message,
            retryable: false,
        }),
        evidence,
    }
}

pub fn run_python(
    request: &PythonScriptRequest,
    cancellation: &CancellationToken,
) -> PythonScriptResponse {
    let started = Instant::now();
    match run_python_inner(request, cancellation) {
        Ok((status, plan, operation_responses)) => PythonScriptResponse {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: request.request_id,
            status,
            output: Some(plan.output),
            operation_responses,
            error: None,
            elapsed_ms: started.elapsed().as_millis(),
        },
        Err((status, code, message)) => PythonScriptResponse {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: request.request_id,
            status,
            output: None,
            operation_responses: Vec::new(),
            error: Some(ApiError {
                code,
                message,
                retryable: false,
            }),
            elapsed_ms: started.elapsed().as_millis(),
        },
    }
}

fn run_python_inner(
    request: &PythonScriptRequest,
    cancellation: &CancellationToken,
) -> std::result::Result<(ScriptStatus, ScriptPlan, Vec<ApiResponse>), (ScriptStatus, String, String)> {
    if request.schema_version != AUTOMATION_SCHEMA_VERSION {
        return Err((
            ScriptStatus::Failed,
            "invalid_request".to_owned(),
            format!("unsupported schema '{}'", request.schema_version),
        ));
    }
    if let Err(error) = request.limits.validate() {
        return Err((
            ScriptStatus::Failed,
            "invalid_limits".to_owned(),
            error.to_string(),
        ));
    }
    if request.script_source.len() > request.limits.maximum_script_bytes {
        return Err((
            ScriptStatus::Failed,
            "script_too_large".to_owned(),
            "script exceeds maximum_script_bytes".to_owned(),
        ));
    }
    if cancellation.is_cancelled() {
        return Err((
            ScriptStatus::Cancelled,
            "cancelled".to_owned(),
            "script cancelled before launch".to_owned(),
        ));
    }

    let work = isolated_work_directory(request.request_id).map_err(internal_error)?;
    let input_path = work.join("input.json");
    let output_path = work.join("output.json");
    let bootstrap_path = work.join("bootstrap.py");
    let input = json!({
        "script_source": request.script_source,
        "context": request.context,
        "maximum_operations": request.limits.maximum_operations,
        "output_path": output_path
    });
    fs::write(&input_path, serde_json::to_vec(&input).map_err(internal_error)?)
        .map_err(internal_error)?;
    fs::write(&bootstrap_path, PYTHON_BOOTSTRAP).map_err(internal_error)?;

    let mut child = Command::new(&request.python_executable)
        .arg("-I")
        .arg("-S")
        .arg("-B")
        .arg(&bootstrap_path)
        .arg(&input_path)
        .current_dir(&work)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            (
                ScriptStatus::Failed,
                "python_start_failed".to_owned(),
                error.to_string(),
            )
        })?;

    let deadline = Instant::now() + Duration::from_millis(request.limits.timeout_ms);
    loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            cleanup(&work);
            return Err((
                ScriptStatus::Cancelled,
                "cancelled".to_owned(),
                "script execution was cancelled".to_owned(),
            ));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            cleanup(&work);
            return Err((
                ScriptStatus::TimedOut,
                "timeout".to_owned(),
                "script exceeded timeout_ms".to_owned(),
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() && !output_path.exists() {
                    cleanup(&work);
                    return Err((
                        ScriptStatus::Failed,
                        "python_failed".to_owned(),
                        format!("restricted Python exited with {}", status),
                    ));
                }
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                cleanup(&work);
                return Err((
                    ScriptStatus::Failed,
                    "python_wait_failed".to_owned(),
                    error.to_string(),
                ));
            }
        }
    }

    let metadata = fs::metadata(&output_path).map_err(internal_error)?;
    if metadata.len() > request.limits.maximum_output_bytes as u64 {
        cleanup(&work);
        return Err((
            ScriptStatus::Failed,
            "output_too_large".to_owned(),
            "script output exceeds maximum_output_bytes".to_owned(),
        ));
    }
    let bytes = fs::read(&output_path).map_err(internal_error)?;
    let plan: ScriptPlan = serde_json::from_slice(&bytes).map_err(|error| {
        (
            ScriptStatus::Failed,
            "invalid_script_output".to_owned(),
            error.to_string(),
        )
    })?;
    cleanup(&work);

    if plan.operations.len() > request.limits.maximum_operations {
        return Err((
            ScriptStatus::Failed,
            "too_many_operations".to_owned(),
            "script requested too many operations".to_owned(),
        ));
    }

    let mut responses = Vec::with_capacity(plan.operations.len());
    for operation in &plan.operations {
        if cancellation.is_cancelled() {
            return Err((
                ScriptStatus::Cancelled,
                "cancelled".to_owned(),
                "script operation dispatch was cancelled".to_owned(),
            ));
        }
        responses.push(invoke(
            &ApiRequest {
                schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
                request_id: Uuid::new_v4(),
                method: operation.method.clone(),
                params: operation.params.clone(),
                capabilities: request.capabilities.clone(),
            },
            cancellation,
        ));
    }
    let status = if responses.iter().any(|response| response.status != ApiStatus::Ok) {
        ScriptStatus::CompletedWithErrors
    } else {
        ScriptStatus::Completed
    };
    Ok((status, plan, responses))
}

fn isolated_work_directory(request_id: Uuid) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("structural-automation-{}", request_id));
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }
    fs::create_dir(&path)?;
    Ok(path)
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn internal_error(error: impl std::fmt::Display) -> (ScriptStatus, String, String) {
    (
        ScriptStatus::Failed,
        "internal_error".to_owned(),
        error.to_string(),
    )
}

const PYTHON_BOOTSTRAP: &str = r#"
import json
import sys

class Api:
    def __init__(self, maximum):
        self._maximum = maximum
        self._operations = []

    def call(self, method, params=None):
        if not isinstance(method, str) or not method:
            raise ValueError("method must be a non-empty string")
        if len(self._operations) >= self._maximum:
            raise RuntimeError("maximum operation count exceeded")
        self._operations.append({"method": method, "params": {} if params is None else params})
        return len(self._operations) - 1

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    request = json.load(handle)

api = Api(request["maximum_operations"])
result_box = {"value": None}

def emit(value):
    result_box["value"] = value

safe_builtins = {
    "abs": abs, "all": all, "any": any, "bool": bool, "dict": dict,
    "enumerate": enumerate, "float": float, "int": int, "len": len,
    "list": list, "max": max, "min": min, "range": range, "round": round,
    "set": set, "sorted": sorted, "str": str, "sum": sum, "tuple": tuple,
    "zip": zip, "Exception": Exception, "RuntimeError": RuntimeError,
    "ValueError": ValueError,
}
scope = {
    "__builtins__": safe_builtins,
    "api": api,
    "context": request.get("context"),
    "emit": emit,
}
exec(compile(request["script_source"], "<automation-script>", "exec"), scope, scope)
plan = {"output": result_box["value"], "operations": api._operations}
encoded = json.dumps(plan, sort_keys=True, separators=(",", ":")).encode("utf-8")
with open(request["output_path"], "wb") as handle:
    handle.write(encoded)
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(method: &str, capabilities: Vec<Capability>) -> ApiRequest {
        ApiRequest {
            schema_version: AUTOMATION_SCHEMA_VERSION.to_owned(),
            request_id: Uuid::nil(),
            method: method.to_owned(),
            params: json!({}),
            capabilities,
        }
    }

    #[test]
    fn description_has_stable_sorted_operations() {
        let methods: Vec<_> = describe().operations.into_iter().map(|x| x.method).collect();
        assert_eq!(
            methods,
            vec![
                "system.describe",
                "artifact.sha256",
                "model.validate",
                "rules.evaluate",
                "analysis.solve_nonlinear",
            ]
        );
    }

    #[test]
    fn capability_is_enforced() {
        let response = invoke(&request("system.describe", vec![]), &CancellationToken::default());
        assert_eq!(response.status, ApiStatus::Error);
        assert_eq!(response.error.unwrap().code, "capability_denied");
    }

    #[test]
    fn cancelled_request_never_dispatches() {
        let token = CancellationToken::default();
        token.cancel();
        let response = invoke(
            &request("system.describe", vec![Capability::InspectSystem]),
            &token,
        );
        assert_eq!(response.status, ApiStatus::Cancelled);
    }

    #[test]
    fn hashing_is_deterministic() {
        let mut request = request("artifact.sha256", vec![Capability::ReadArtifacts]);
        request.params = json!({"text": "abc"});
        let left = invoke(&request, &CancellationToken::default());
        let right = invoke(&request, &CancellationToken::default());
        assert_eq!(left, right);
        assert_eq!(
            left.result.unwrap()["sha256"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
