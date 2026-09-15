#![cfg(windows)]

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use serde_json::Value;

/// Read-only PowerShell collector; see the script header for what it queries.
const SCRIPT: &str = include_str!("collect-windows.ps1");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn collect(window_days: u32) -> Result<Value, String> {
    // Feed the script over stdin rather than a temp file, so nothing on disk can be
    // swapped out between writing and executing it when TaskForge runs elevated.
    let bootstrap = format!(
        "$s = [Console]::In.ReadToEnd(); & ([scriptblock]::Create($s)) -WindowDays {window_days}"
    );
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(&bootstrap)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;

    // Dropping stdin at the end of the closure closes the pipe so ReadToEnd returns.
    let write_result = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(SCRIPT.as_bytes()));
    let output = child
        .wait_with_output()
        .map_err(|e| format!("diagnostics collector failed: {e}"))?;
    if let Some(Err(e)) = write_result {
        return Err(format!("could not send the collector script to PowerShell: {e}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    match stdout.find('{') {
        Some(start) => serde_json::from_str(stdout[start..].trim())
            .map_err(|e| format!("collector output was not valid JSON: {e}")),
        None => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr: String = stderr.trim().chars().take(600).collect();
            Err(format!(
                "diagnostics collector produced no report (exit {:?}): {stderr}",
                output.status.code()
            ))
        }
    }
}
