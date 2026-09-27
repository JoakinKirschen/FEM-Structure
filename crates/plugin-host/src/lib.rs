//! Signed out-of-process plugin host with fail-closed sandbox selection.
//!
//! `Direct` provides process separation and crash containment, but is deliberately
//! disabled unless the caller opts in. `Bubblewrap` is the reference OS sandbox on
//! Linux. Production deployments should provide an equivalent reviewed backend on
//! every supported operating system.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
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
use structural_automation_api::{Capability, AUTOMATION_API_VERSION};
use structural_plugin_sdk::{
    negotiate_api, verify_manifest, PluginInvocation, PluginManifest, PluginResponse,
    PluginStatus, SandboxStrength, SignatureVerification, TrustStore, PLUGIN_PROTOCOL_VERSION,
    PLUGIN_SCHEMA_VERSION,
};
use uuid::Uuid;

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxBackend {
    Auto,
    Bubblewrap,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SandboxPolicy {
    pub backend: SandboxBackend,
    pub allow_unsafe_direct: bool,
    pub network_access: bool,
    #[serde(default)]
    pub granted_capabilities: Vec<Capability>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            backend: SandboxBackend::Auto,
            allow_unsafe_direct: false,
            network_access: false,
            granted_capabilities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Completed,
    Rejected,
    Cancelled,
    TimedOut,
    Crashed,
    InvalidResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginAuditRecord {
    pub schema_version: String,
    pub invocation_id: Uuid,
    pub plugin_id: String,
    pub plugin_version: String,
    pub publisher: String,
    pub signature: SignatureVerification,
    pub negotiated_api_version: String,
    pub protocol_version: String,
    pub requested_capabilities: Vec<Capability>,
    pub granted_capabilities: Vec<Capability>,
    pub sandbox_backend: SandboxBackend,
    pub network_access: bool,
    pub started_at: DateTime<Utc>,
    pub elapsed_ms: u128,
    pub status: ExecutionStatus,
    pub exit_code: Option<i32>,
    pub invocation_sha256: String,
    pub response_sha256: Option<String>,
    pub stderr_sha256: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginExecution {
    pub response: Option<PluginResponse>,
    pub audit: PluginAuditRecord,
}

pub fn load_manifest(path: &Path) -> Result<PluginManifest> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid manifest {}", path.display()))
}

pub fn verify_and_negotiate(
    manifest: &PluginManifest,
    trust: &TrustStore,
) -> Result<(SignatureVerification, String)> {
    let verification = verify_manifest(manifest, trust)?;
    let negotiated = negotiate_api(&manifest.automation_api, AUTOMATION_API_VERSION)?;
    Ok((verification, negotiated))
}


/// Execute a plugin and always return an auditable outcome.
///
/// Policy, signature, negotiation and launch rejections are represented as
/// `ExecutionStatus::Rejected` rather than disappearing into an unaudited error
/// path. Audit-file write failures remain ordinary I/O errors at the caller.
pub fn execute_audited(
    plugin_directory: &Path,
    manifest: &PluginManifest,
    trust: &TrustStore,
    invocation: &PluginInvocation,
    policy: &SandboxPolicy,
    cancellation: &CancellationToken,
) -> PluginExecution {
    let started_at = Utc::now();
    let started = Instant::now();
    match execute(
        plugin_directory,
        manifest,
        trust,
        invocation,
        policy,
        cancellation,
    ) {
        Ok(execution) => execution,
        Err(error) => {
            let manifest_sha256 = manifest
                .unsigned_sha256()
                .unwrap_or_else(|_| "unavailable".to_owned());
            PluginExecution {
                response: None,
                audit: PluginAuditRecord {
                    schema_version: "structural-plugin-audit/1.0".to_owned(),
                    invocation_id: invocation.invocation_id,
                    plugin_id: manifest.plugin_id.clone(),
                    plugin_version: manifest.version.clone(),
                    publisher: manifest.publisher.clone(),
                    signature: SignatureVerification {
                        verified: false,
                        key_id: manifest.signature.as_ref().map(|value| value.key_id.clone()),
                        manifest_sha256,
                    },
                    negotiated_api_version: "not_negotiated".to_owned(),
                    protocol_version: manifest.protocol_version.clone(),
                    requested_capabilities: manifest.requested_capabilities.clone(),
                    granted_capabilities: policy.granted_capabilities.clone(),
                    sandbox_backend: policy.backend,
                    network_access: policy.network_access,
                    started_at,
                    elapsed_ms: started.elapsed().as_millis(),
                    status: ExecutionStatus::Rejected,
                    exit_code: None,
                    invocation_sha256: hash_json(invocation)
                        .unwrap_or_else(|_| "unavailable".to_owned()),
                    response_sha256: None,
                    stderr_sha256: None,
                    diagnostic: Some(error.to_string()),
                },
            }
        }
    }
}

pub fn execute(
    plugin_directory: &Path,
    manifest: &PluginManifest,
    trust: &TrustStore,
    invocation: &PluginInvocation,
    policy: &SandboxPolicy,
    cancellation: &CancellationToken,
) -> Result<PluginExecution> {
    manifest.validate()?;
    validate_invocation(invocation)?;
    let (signature, negotiated_api_version) = verify_and_negotiate(manifest, trust)?;
    let granted = resolve_capabilities(
        &manifest.requested_capabilities,
        &policy.granted_capabilities,
    )?;
    if invocation.granted_capabilities != granted {
        bail!("invocation capabilities must exactly match resolved host grants");
    }

    let backend = select_backend(manifest.minimum_sandbox.clone(), policy)?;
    let plugin_directory = plugin_directory.canonicalize()?;
    let executable = resolve_entrypoint(&plugin_directory, &manifest.entrypoint.executable)?;
    let invocation_bytes = serde_json::to_vec(invocation)?;
    let invocation_sha256 = sha256_hex(&invocation_bytes);
    let started_at = Utc::now();
    let started = Instant::now();
    let work = isolated_work_directory(invocation.invocation_id)?;
    let stdin_path = work.join("stdin.json");
    let stdout_path = work.join("stdout.json");
    let stderr_path = work.join("stderr.log");
    fs::write(&stdin_path, &invocation_bytes)?;

    let mut command = build_command(
        backend,
        &plugin_directory,
        &executable,
        &manifest.entrypoint.arguments,
        policy.network_access,
    )?;
    command
        .env_clear()
        .env("STRUCTURAL_PLUGIN_PROTOCOL", PLUGIN_PROTOCOL_VERSION)
        .env("STRUCTURAL_AUTOMATION_API", &negotiated_api_version)
        .current_dir(&work)
        .stdin(Stdio::from(File::open(&stdin_path)?))
        .stdout(Stdio::from(File::create(&stdout_path)?))
        .stderr(Stdio::from(File::create(&stderr_path)?));

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            cleanup(&work);
            return Err(error).context("failed to launch plugin");
        }
    };
    let deadline = Instant::now() + Duration::from_millis(manifest.limits.timeout_ms);
    let (status, exit_code, diagnostic) = loop {
        if cancellation.is_cancelled() {
            terminate(&mut child);
            break (
                ExecutionStatus::Cancelled,
                None,
                Some("plugin invocation cancelled".to_owned()),
            );
        }
        if Instant::now() >= deadline {
            terminate(&mut child);
            break (
                ExecutionStatus::TimedOut,
                None,
                Some("plugin exceeded timeout_ms".to_owned()),
            );
        }
        match child.try_wait() {
            Ok(Some(exit)) if exit.success() => {
                break (ExecutionStatus::Completed, exit.code(), None)
            }
            Ok(Some(exit)) => {
                break (
                    ExecutionStatus::Crashed,
                    exit.code(),
                    Some(format!("plugin exited with {exit}")),
                )
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                terminate(&mut child);
                break (
                    ExecutionStatus::Crashed,
                    None,
                    Some(format!("failed while waiting for plugin: {error}")),
                );
            }
        }
    };

    let stderr = read_bounded(&stderr_path, manifest.limits.maximum_stderr_bytes)
        .unwrap_or_else(|error| format!("<stderr unavailable: {error}>").into_bytes());
    let stderr_sha256 = if stderr.is_empty() {
        None
    } else {
        Some(sha256_hex(&stderr))
    };

    let mut final_status = status;
    let mut final_diagnostic = diagnostic;
    let mut response = None;
    let mut response_sha256 = None;
    if final_status == ExecutionStatus::Completed {
        match read_bounded(&stdout_path, manifest.limits.maximum_stdout_bytes)
            .and_then(|bytes| {
                response_sha256 = Some(sha256_hex(&bytes));
                let parsed: PluginResponse =
                    serde_json::from_slice(&bytes).context("plugin stdout is not a valid response")?;
                validate_response(invocation, &parsed)?;
                Ok(parsed)
            }) {
            Ok(parsed) => response = Some(parsed),
            Err(error) => {
                final_status = ExecutionStatus::InvalidResponse;
                final_diagnostic = Some(error.to_string());
            }
        }
    }

    let audit = PluginAuditRecord {
        schema_version: "structural-plugin-audit/1.0".to_owned(),
        invocation_id: invocation.invocation_id,
        plugin_id: manifest.plugin_id.clone(),
        plugin_version: manifest.version.clone(),
        publisher: manifest.publisher.clone(),
        signature,
        negotiated_api_version,
        protocol_version: manifest.protocol_version.clone(),
        requested_capabilities: manifest.requested_capabilities.clone(),
        granted_capabilities: granted,
        sandbox_backend: backend,
        network_access: policy.network_access,
        started_at,
        elapsed_ms: started.elapsed().as_millis(),
        status: final_status,
        exit_code,
        invocation_sha256,
        response_sha256,
        stderr_sha256,
        diagnostic: final_diagnostic,
    };
    cleanup(&work);
    Ok(PluginExecution { response, audit })
}

pub fn write_audit(path: &Path, execution: &PluginExecution) -> Result<()> {
    let mut value = serde_json::to_value(execution)?;
    let digest = hash_json(&value)?;
    value["audit_bundle_sha256"] = serde_json::Value::String(digest);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

fn validate_invocation(invocation: &PluginInvocation) -> Result<()> {
    if invocation.schema_version != PLUGIN_SCHEMA_VERSION {
        bail!("unsupported invocation schema '{}'", invocation.schema_version);
    }
    if invocation.protocol_version != PLUGIN_PROTOCOL_VERSION {
        bail!("unsupported invocation protocol '{}'", invocation.protocol_version);
    }
    if invocation.method.trim().is_empty() {
        bail!("plugin method is required");
    }
    Ok(())
}

fn validate_response(invocation: &PluginInvocation, response: &PluginResponse) -> Result<()> {
    if response.schema_version != PLUGIN_SCHEMA_VERSION
        || response.protocol_version != PLUGIN_PROTOCOL_VERSION
    {
        bail!("plugin response used an incompatible protocol");
    }
    if response.invocation_id != invocation.invocation_id {
        bail!("plugin response invocation_id does not match request");
    }
    match response.status {
        PluginStatus::Ok if response.result.is_none() || response.error.is_some() => {
            bail!("successful response must contain result and no error")
        }
        PluginStatus::Error if response.error.is_none() || response.result.is_some() => {
            bail!("error response must contain error and no result")
        }
        _ => Ok(()),
    }
}

fn resolve_capabilities(
    requested: &[Capability],
    allowed: &[Capability],
) -> Result<Vec<Capability>> {
    let mut granted = Vec::new();
    for capability in requested {
        if !allowed.contains(capability) {
            bail!("plugin capability '{capability:?}' was not granted");
        }
        granted.push(capability.clone());
    }
    granted.sort();
    granted.dedup();
    Ok(granted)
}

fn select_backend(minimum: SandboxStrength, policy: &SandboxPolicy) -> Result<SandboxBackend> {
    let bubblewrap_available = command_exists("bwrap");
    match policy.backend {
        SandboxBackend::Bubblewrap if bubblewrap_available => Ok(SandboxBackend::Bubblewrap),
        SandboxBackend::Bubblewrap => bail!("bubblewrap sandbox requested but 'bwrap' is unavailable"),
        SandboxBackend::Direct
            if minimum == SandboxStrength::Process && policy.allow_unsafe_direct =>
        {
            Ok(SandboxBackend::Direct)
        }
        SandboxBackend::Direct => bail!("direct execution is denied by sandbox policy"),
        SandboxBackend::Auto if bubblewrap_available => Ok(SandboxBackend::Bubblewrap),
        SandboxBackend::Auto
            if minimum == SandboxStrength::Process && policy.allow_unsafe_direct =>
        {
            Ok(SandboxBackend::Direct)
        }
        SandboxBackend::Auto => {
            bail!("no acceptable sandbox backend is available; refusing to run plugin")
        }
    }
}

fn build_command(
    backend: SandboxBackend,
    plugin_directory: &Path,
    executable: &Path,
    arguments: &[String],
    network_access: bool,
) -> Result<Command> {
    match backend {
        SandboxBackend::Direct => {
            let mut command = Command::new(executable);
            command.args(arguments);
            Ok(command)
        }
        SandboxBackend::Bubblewrap => {
            let relative_executable = executable
                .strip_prefix(plugin_directory)
                .context("plugin executable is outside the package directory")?;
            let sandbox_executable = Path::new("/plugin").join(relative_executable);
            let mut command = Command::new("bwrap");
            command
                .arg("--die-with-parent")
                .arg("--new-session")
                .arg("--unshare-all");
            if network_access {
                command.arg("--share-net");
            }
            command
                .arg("--ro-bind")
                .arg(plugin_directory)
                .arg("/plugin")
                .arg("--tmpfs")
                .arg("/tmp")
                .arg("--proc")
                .arg("/proc")
                .arg("--dev")
                .arg("/dev");
            // Dynamically linked plugins need the host runtime, mounted read-only.
            for system_path in ["/usr", "/bin", "/lib", "/lib64"] {
                if Path::new(system_path).exists() {
                    command
                        .arg("--ro-bind")
                        .arg(system_path)
                        .arg(system_path);
                }
            }
            command
                .arg("--chdir")
                .arg("/tmp")
                .arg("--")
                .arg(sandbox_executable)
                .args(arguments);
            Ok(command)
        }
        SandboxBackend::Auto => bail!("automatic backend must be resolved before command creation"),
    }
}

fn resolve_entrypoint(plugin_directory: &Path, entrypoint: &str) -> Result<PathBuf> {
    let base = plugin_directory.canonicalize()?;
    let candidate = base.join(entrypoint).canonicalize().context("plugin entrypoint not found")?;
    if !candidate.starts_with(&base) {
        bail!("plugin entrypoint escapes plugin directory");
    }
    if !candidate.is_file() {
        bail!("plugin entrypoint is not a file");
    }
    Ok(candidate)
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > maximum as u64 {
        bail!("plugin output exceeded configured byte limit");
    }
    Ok(fs::read(path)?)
}

fn isolated_work_directory(invocation_id: Uuid) -> Result<PathBuf> {
    let path = std::env::temp_dir().join(format!("structural-plugin-{invocation_id}"));
    if path.exists() {
        fs::remove_dir_all(&path)?;
    }
    fs::create_dir(&path)?;
    Ok(path)
}

fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .any(|path| path.join(name).is_file())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use structural_plugin_sdk::{ApiRequirement, PluginEntrypoint, PluginLimits};

    fn manifest(minimum_sandbox: SandboxStrength) -> PluginManifest {
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
                executable: "echo".to_owned(),
                arguments: vec![],
            },
            requested_capabilities: vec![Capability::InspectSystem],
            minimum_sandbox,
            limits: PluginLimits::default(),
            signature: None,
        }
    }

    #[test]
    fn direct_execution_is_fail_closed() {
        let error = select_backend(
            SandboxStrength::Process,
            &SandboxPolicy {
                backend: SandboxBackend::Direct,
                allow_unsafe_direct: false,
                network_access: false,
                granted_capabilities: vec![],
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("denied"));
    }

    #[test]
    fn os_sandbox_cannot_fall_back_to_direct() {
        let error = select_backend(
            SandboxStrength::Os,
            &SandboxPolicy {
                backend: SandboxBackend::Direct,
                allow_unsafe_direct: true,
                network_access: false,
                granted_capabilities: vec![],
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("denied"));
    }

    #[test]
    fn capability_grants_must_cover_manifest() {
        assert!(resolve_capabilities(
            &manifest(SandboxStrength::Process).requested_capabilities,
            &[]
        )
        .is_err());
    }

    #[test]
    fn rejected_plugin_attempt_is_audited() {
        let manifest = manifest(SandboxStrength::Process);
        let invocation = PluginInvocation {
            schema_version: PLUGIN_SCHEMA_VERSION.to_owned(),
            protocol_version: PLUGIN_PROTOCOL_VERSION.to_owned(),
            invocation_id: Uuid::nil(),
            method: "echo".to_owned(),
            params: serde_json::json!({}),
            granted_capabilities: vec![],
        };
        let execution = execute_audited(
            Path::new("."),
            &manifest,
            &TrustStore::default(),
            &invocation,
            &SandboxPolicy::default(),
            &CancellationToken::default(),
        );
        assert_eq!(execution.audit.status, ExecutionStatus::Rejected);
        assert!(!execution.audit.signature.verified);
        assert!(execution.audit.diagnostic.is_some());
    }
}
