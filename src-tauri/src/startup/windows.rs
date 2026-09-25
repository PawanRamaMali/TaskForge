#![cfg(windows)]

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const SCRIPT: &str = include_str!("collect-startup.ps1");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_CANCELLED: i32 = 1223;
/// First argument that turns the process into the elevated startup helper.
pub const HELPER_FLAG: &str = "--apply-startup";

/// Feed the embedded collector to PowerShell over stdin. `apply_b64` is Some when
/// changing entries (base64 JSON of `[{id,enabled}]`), None when only listing.
fn run_script(apply_b64: Option<&str>) -> Result<Value, String> {
    let bootstrap = match apply_b64 {
        Some(b64) => format!(
            "$s = [Console]::In.ReadToEnd(); & ([scriptblock]::Create($s)) -Apply -ChangesB64 '{b64}'"
        ),
        None => "$s = [Console]::In.ReadToEnd(); & ([scriptblock]::Create($s))".to_string(),
    };
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

pub fn list() -> Result<Value, String> {
    run_script(None)
}

/// Apply changes. User-scope entries are applied directly as the signed-in user;
/// machine-scope entries are batched into one elevated run of TaskForge's own
/// trusted binary (so the elevated code is the embedded script, never a
/// user-writable file). Separating them also keeps user entries correct when the
/// admin supplies different credentials at the UAC prompt.
pub fn apply(changes: Value) -> Result<Value, String> {
    let items: Vec<Value> = changes.as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return Ok(json!({ "results": [] }));
    }
    let (machine, user): (Vec<Value>, Vec<Value>) = items.into_iter().partition(|c| {
        c.get("id").and_then(Value::as_str).map(machine_scope).unwrap_or(false)
    });

    let mut results = Vec::new();
    if !user.is_empty() {
        merge_results(run_script(Some(&b64(&Value::Array(user))))?, &mut results);
    }
    if !machine.is_empty() {
        merge_results(run_elevated(&b64(&Value::Array(machine)))?, &mut results);
    }
    Ok(json!({ "results": results }))
}

/// Elevated helper entry point: `task-manager --apply-startup <b64> <out>`.
/// Runs the embedded script and writes the result JSON to the out file (the
/// UAC boundary prevents returning it on stdout).
pub fn run_helper(args: &[String]) -> i32 {
    let (Some(b64), Some(out)) = (args.first(), args.get(1)) else {
        return 2;
    };
    let value = run_script(Some(b64)).unwrap_or_else(|e| json!({ "results": [], "error": e }));
    let _ = std::fs::write(out, serde_json::to_vec(&value).unwrap_or_default());
    0
}

/// Machine-scope kinds (HKLM Run, Wow64 Run, common Startup folder, scheduled task).
fn machine_scope(id: &str) -> bool {
    matches!(id.split('|').next(), Some("rm" | "r32" | "sfm" | "st"))
}

fn run_elevated(changes_b64: &str) -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let out_file = std::env::temp_dir().join(format!("taskforge-startup-{stamp}.out.json"));
    let _cleanup = TempFile(out_file.clone());

    let quote = |s: &str| s.replace('\'', "''");
    // Only the trusted signed exe is elevated; args carry base64 data and an output path.
    let launcher = format!(
        "try {{ $p = Start-Process -FilePath '{}' -ArgumentList '{}','{}','{}' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ErrorAction Stop; exit $p.ExitCode }} catch {{ exit {ERROR_CANCELLED} }}",
        quote(&exe.display().to_string()),
        HELPER_FLAG,
        quote(changes_b64),
        quote(&out_file.display().to_string()),
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(&launcher)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;
    match out.status.code() {
        Some(0) => {
            let bytes = std::fs::read(&out_file)
                .map_err(|e| format!("could not read the startup result: {e}"))?;
            parse_json(&bytes).ok_or_else(|| "the startup helper returned no result".to_string())
        }
        Some(ERROR_CANCELLED) => Err("administrator permission was declined".into()),
        Some(code) => Err(format!("the startup helper failed (exit code {code})")),
        None => Err("the startup helper was stopped".into()),
    }
}

fn merge_results(value: Value, out: &mut Vec<Value>) {
    if let Some(arr) = value.get("results").and_then(Value::as_array) {
        out.extend(arr.iter().cloned());
    }
}

fn parse_json(bytes: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.find('{')?;
    serde_json::from_str(text[start..].trim()).ok()
}

/// Base64 (standard alphabet) of a compact JSON value; safe inside single quotes.
fn b64(value: &Value) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = serde_json::to_vec(value).unwrap_or_default();
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let b1 = c.first().copied().unwrap_or(0);
        let b2 = c.get(1).copied().unwrap_or(0);
        let b3 = c.get(2).copied().unwrap_or(0);
        let n = (u32::from(b1) << 16) | (u32::from(b2) << 8) | u32::from(b3);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Deletes the result temp file when the elevated call returns.
struct TempFile(std::path::PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
