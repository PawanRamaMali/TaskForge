#![cfg(windows)]

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const SCRIPT: &str = include_str!("collect-startup.ps1");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_CANCELLED: i32 = 1223;

/// Enumerate startup entries by feeding the collector over stdin (no admin needed).
pub fn list() -> Result<Value, String> {
    let bootstrap =
        "$s = [Console]::In.ReadToEnd(); & ([scriptblock]::Create($s))".to_string();
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(&bootstrap)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;
    let write_result = child.stdin.take().map(|mut s| s.write_all(SCRIPT.as_bytes()));
    let output = child
        .wait_with_output()
        .map_err(|e| format!("startup collector failed: {e}"))?;
    if let Some(Err(e)) = write_result {
        return Err(format!("could not send the collector to PowerShell: {e}"));
    }
    parse_json(&output.stdout).ok_or_else(|| {
        let err = String::from_utf8_lossy(&output.stderr);
        format!("startup collector produced no data: {}", err.trim().chars().take(400).collect::<String>())
    })
}

/// Apply enable/disable changes. Machine-scope entries need admin, so the whole
/// batch runs elevated when any change touches one; user-scope batches run directly.
pub fn apply(changes: Value) -> Result<Value, String> {
    let items = changes.as_array().ok_or("changes must be a list")?;
    if items.is_empty() {
        return Ok(json!({ "results": [] }));
    }
    let needs_admin = items.iter().any(|c| {
        c.get("id")
            .and_then(Value::as_str)
            .map(machine_scope)
            .unwrap_or(false)
    });

    // The collector and the requested changes go to temp files the elevated helper can read.
    let dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let script = dir.join(format!("taskforge-startup-{stamp}.ps1"));
    let changes_file = dir.join(format!("taskforge-startup-{stamp}.json"));
    let out_file = dir.join(format!("taskforge-startup-{stamp}.out.json"));
    std::fs::write(&script, SCRIPT).map_err(|e| e.to_string())?;
    std::fs::write(&changes_file, serde_json::to_vec(&changes).unwrap_or_default())
        .map_err(|e| e.to_string())?;

    let _cleanup = TempFiles(vec![script.clone(), changes_file.clone(), out_file.clone()]);

    let result = if needs_admin {
        run_elevated(&script, &changes_file, &out_file)?;
        let text = std::fs::read(&out_file).map_err(|e| format!("could not read the result: {e}"))?;
        parse_json(&text).ok_or_else(|| "the startup helper returned no result".to_string())?
    } else {
        run_direct(&script, &changes_file)?
    };
    Ok(result)
}

/// Machine-scope kinds (HKLM Run, Wow64 Run, common Startup folder, scheduled task).
fn machine_scope(id: &str) -> bool {
    matches!(id.split('|').next(), Some("rm" | "r32" | "sfm" | "st"))
}

fn run_direct(script: &std::path::Path, changes_file: &std::path::Path) -> Result<Value, String> {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .arg("-Apply")
        .arg("-ChangesFile")
        .arg(changes_file)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;
    parse_json(&output.stdout).ok_or_else(|| "the startup change produced no result".to_string())
}

/// Elevate a plain powershell (not TaskForge itself) to run the helper script.
fn run_elevated(
    script: &std::path::Path,
    changes_file: &std::path::Path,
    out_file: &std::path::Path,
) -> Result<(), String> {
    let quote = |p: &std::path::Path| format!("'{}'", p.display().to_string().replace('\'', "''"));
    let inner = format!(
        "-NoProfile -ExecutionPolicy Bypass -File {} -Apply -ChangesFile {} -OutFile {}",
        quote(script),
        quote(changes_file),
        quote(out_file)
    );
    let launcher = format!(
        "try {{ $p = Start-Process powershell.exe -ArgumentList '{}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ErrorAction Stop; exit $p.ExitCode }} catch {{ exit {ERROR_CANCELLED} }}",
        inner.replace('\'', "''")
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(&launcher)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(ERROR_CANCELLED) => Err("administrator permission was declined".into()),
        Some(code) => Err(format!("the startup helper failed (exit code {code})")),
        None => Err("the startup helper was stopped".into()),
    }
}

fn parse_json(bytes: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.find('{')?;
    serde_json::from_str(text[start..].trim()).ok()
}

/// Deletes the temp files when the apply call returns.
struct TempFiles(Vec<std::path::PathBuf>);
impl Drop for TempFiles {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}
