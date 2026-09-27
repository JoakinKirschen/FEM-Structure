use anyhow::{bail, Context, Result};
use std::{
    env,
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
};
use structural_automation_api::{ApiRequest, PythonScriptRequest};
use structural_automation_host::{describe, invoke, run_python, CancellationToken};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.as_slice() {
        [_, command, output] if command == "describe" => {
            write_json(output.into(), &describe())
        }
        [_, command, request, output] if command == "call" => {
            let request: ApiRequest = read_json(request.into())?;
            write_json(
                output.into(),
                &invoke(&request, &CancellationToken::default()),
            )
        }
        [_, command] if command == "serve" => serve(),
        [_, command, request, output] if command == "python" => {
            let request: PythonScriptRequest = read_json(request.into())?;
            write_json(
                output.into(),
                &run_python(&request, &CancellationToken::default()),
            )
        }
        _ => {
            eprintln!(
                "Usage:\n  structural-automation describe <output.json>\n  structural-automation call <request.json> <response.json>\n  structural-automation serve\n  structural-automation python <script-request.json> <response.json>"
            );
            bail!("invalid command line")
        }
    }
}

fn serve() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<ApiRequest>(&line) {
            Ok(request) => invoke(&request, &CancellationToken::default()),
            Err(error) => {
                eprintln!("invalid request: {error}");
                continue;
            }
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
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
