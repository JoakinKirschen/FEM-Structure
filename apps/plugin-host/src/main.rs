use anyhow::{bail, Context, Result};
use ed25519_dalek::SigningKey;
use std::{env, fs, path::PathBuf};
use structural_plugin_host::{
    execute_audited, load_manifest, write_audit, CancellationToken, ExecutionStatus,
    SandboxBackend, SandboxPolicy,
};
use structural_plugin_sdk::{
    public_key_base64, verify_manifest, PluginInvocation, PluginManifest, TrustStore,
};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_, command, manifest, trust] if command == "verify" => {
            let manifest = load_manifest(&PathBuf::from(manifest))?;
            let trust: TrustStore = read_json(trust.into())?;
            println!("{}", serde_json::to_string_pretty(&verify_manifest(&manifest, &trust)?)?);
            Ok(())
        }
        [_, command, manifest, secret_hex, key_id, output] if command == "sign" => {
            let mut manifest: PluginManifest = read_json(manifest.into())?;
            let secret = decode_secret(secret_hex)?;
            manifest.sign(key_id, &SigningKey::from_bytes(&secret))?;
            write_json(output.into(), &manifest)
        }
        [_, command, secret_hex] if command == "public-key" => {
            let key = SigningKey::from_bytes(&decode_secret(secret_hex)?);
            println!("{}", public_key_base64(&key));
            Ok(())
        }
        [_, command, directory, manifest, trust, invocation, audit]
            if command == "run" =>
        {
            let manifest = load_manifest(&PathBuf::from(manifest))?;
            let trust: TrustStore = read_json(trust.into())?;
            let invocation: PluginInvocation = read_json(invocation.into())?;
            let policy = SandboxPolicy {
                backend: SandboxBackend::Auto,
                allow_unsafe_direct: false,
                network_access: false,
                granted_capabilities: invocation.granted_capabilities.clone(),
            };
            let execution = execute_audited(
                &PathBuf::from(directory),
                &manifest,
                &trust,
                &invocation,
                &policy,
                &CancellationToken::default(),
            );
            write_audit(&PathBuf::from(audit), &execution)?;
            println!("{}", serde_json::to_string_pretty(&execution.response)?);
            if execution.audit.status == ExecutionStatus::Rejected {
                bail!(
                    "plugin rejected: {}",
                    execution
                        .audit
                        .diagnostic
                        .as_deref()
                        .unwrap_or("unspecified policy failure")
                );
            }
            Ok(())
        }
        _ => {
            eprintln!(
                "Usage:\n  structural-plugin verify <manifest.json> <trust.json>\n  structural-plugin sign <manifest.json> <secret-key-hex> <key-id> <output.json>\n  structural-plugin public-key <secret-key-hex>\n  structural-plugin run <plugin-dir> <manifest.json> <trust.json> <invocation.json> <audit.json>"
            );
            bail!("invalid command line")
        }
    }
}

fn decode_secret(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("secret key must be 64 hexadecimal characters");
    }
    let mut output = [0_u8; 32];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .context("secret key contains invalid hexadecimal")?;
    }
    Ok(output)
}

fn read_json<T: serde::de::DeserializeOwned>(path: PathBuf) -> Result<T> {
    let bytes = fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn write_json<T: serde::Serialize>(path: PathBuf, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
