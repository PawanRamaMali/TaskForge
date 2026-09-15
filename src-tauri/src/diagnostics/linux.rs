#![cfg(target_os = "linux")]

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

/// Lines that only appear when a boot ended cleanly (poweroff / reboot / journal flush).
const SHUTDOWN_MARKERS: &[&str] = &[
    "Journal stopped",
    "systemd-shutdown",
    "reboot: Power down",
    "reboot: Restarting system",
    "Reached target System Power Off",
    "Reached target System Reboot",
    "Reached target Power-Off",
];

pub fn collect(window_days: u32) -> Result<Value, String> {
    let now = epoch(SystemTime::now());
    let cutoff = now - i64::from(window_days) * 86_400;
    let mut notes: Vec<String> = Vec::new();

    // ---- Kernel warnings/errors across all boots in the window ----
    let since = format!("-{window_days}d");
    let mut events: Vec<Value> = Vec::new();
    match run(
        "journalctl",
        &["-k", "--no-pager", "-q", "-o", "short-unix", "-p", "warning", "--since", &since],
    ) {
        Some(out) => {
            for line in out.lines() {
                let Some((ts, msg)) = parse_short_unix(line) else {
                    continue;
                };
                if let Some(category) = classify(msg) {
                    events.push(json!({
                        "ts": ts,
                        "source": "kernel",
                        "id": 0,
                        "level": 2,
                        "category": category,
                        "message": clip(msg, 400),
                    }));
                }
            }
        }
        None => notes.push(
            "journalctl could not be read; add your user to the systemd-journal or adm group for kernel history."
                .into(),
        ),
    }
    // Newest first, matching the Windows collector.
    events.reverse();
    events.truncate(1500);

    // ---- Previous boots that ended without a clean shutdown ----
    let mut shutdowns: Vec<Value> = Vec::new();
    for boot in 1..=20 {
        let arg = format!("-{boot}");
        let Some(out) = run(
            "journalctl",
            &["-b", &arg, "-n", "80", "--no-pager", "-q", "-o", "short-unix"],
        ) else {
            break;
        };
        let last_alive = out
            .lines()
            .rev()
            .find_map(|l| parse_short_unix(l).map(|(ts, _)| ts));
        let Some(last_alive) = last_alive else {
            break;
        };
        if last_alive < cutoff {
            break;
        }
        let clean = out
            .lines()
            .any(|l| SHUTDOWN_MARKERS.iter().any(|m| l.contains(m)));
        if !clean {
            shutdowns.push(json!({
                "ts": last_alive,
                "lastAlive": last_alive,
                "bugcheck": 0,
                "params": "",
                "sleepInProgress": false,
                "resumedFromSleep": false,
                "powerButton": false,
                "longPowerPress": false,
            }));
        }
    }
    if shutdowns.is_empty() && run("journalctl", &["-b", "-1", "-n", "1", "-q"]).is_none() {
        notes.push(
            "No previous boots in the journal; enable persistent journaling (Storage=persistent) to track crashes across reboots."
                .into(),
        );
    }

    // ---- Userspace crash reports (Apport) ----
    let mut app_faults: Vec<Value> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/var/crash") {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".crash") {
                continue;
            }
            let ts = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(epoch)
                .unwrap_or(0);
            if ts < cutoff {
                continue;
            }
            // "_usr_bin_foo.1000.crash" -> "foo"
            let app = name
                .split('.')
                .next()
                .unwrap_or("")
                .rsplit('_')
                .next()
                .unwrap_or("")
                .to_string();
            app_faults.push(json!({
                "app": app, "kind": "crash", "count": 1, "first": ts, "last": ts, "times": [ts],
            }));
        }
    }

    let cpuinfo = read("/proc/cpuinfo").unwrap_or_default();
    let meminfo = read("/proc/meminfo").unwrap_or_default();
    let uptime_secs = read("/proc/uptime")
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0);

    let system = json!({
        "os": read("/etc/os-release")
            .and_then(|s| key_value(&s, "PRETTY_NAME"))
            .unwrap_or_else(|| "Linux".into()),
        "model": format!(
            "{} {}",
            read("/sys/class/dmi/id/sys_vendor").unwrap_or_default(),
            read("/sys/class/dmi/id/product_name").unwrap_or_default()
        ).trim().to_string(),
        "bios": read("/sys/class/dmi/id/bios_version"),
        "biosDate": Value::Null,
        "biosAgeDays": Value::Null,
        "cpu": colon_field(&cpuinfo, "model name"),
        "microcode": colon_field(&cpuinfo, "microcode"),
        "ramGb": kb_field(&meminfo, "MemTotal").map(|kb| (kb / 1024.0 / 1024.0 * 10.0).round() / 10.0),
        "lastBoot": now - uptime_secs as i64,
    });

    let mut drivers: Vec<Value> = Vec::new();
    if let Some(v) = read("/proc/driver/nvidia/version") {
        drivers.push(json!({
            "device": "NVIDIA GPU", "class": "DISPLAY", "provider": "NVIDIA",
            "version": clip(v.lines().next().unwrap_or(""), 160), "date": Value::Null, "ageDays": Value::Null,
        }));
    }

    let pstore = std::fs::read_dir("/var/lib/systemd/pstore")
        .map(|d| d.flatten().count())
        .ok();
    let gb = |kb: Option<f64>| kb.map(|v| (v / 1024.0 / 1024.0 * 10.0).round() / 10.0);
    let config = json!({
        "crashDumpMode": Value::Null,
        "autoReboot": Value::Null,
        "minidumpCount": pstore,
        "memoryDumpPresent": false,
        "liveKernelDumps": Value::Null,
        "fastStartup": Value::Null,
        "hypervisorPresent": Value::Null,
        "vbsRunning": Value::Null,
        "hvciRunning": Value::Null,
        "pagefileAuto": Value::Null,
        "commitUsedGb": gb(kb_field(&meminfo, "Committed_AS")),
        "commitLimitGb": gb(kb_field(&meminfo, "CommitLimit")),
        "memoryDiagnostic": Value::Null,
        "powerPlan": read("/sys/firmware/acpi/platform_profile"),
        "bootsInWindow": Value::Null,
        "cleanShutdowns": Value::Null,
        "topMemory": [],
    });

    Ok(json!({
        "schema": 1,
        "platform": "linux",
        "collectedAt": now,
        "windowDays": window_days,
        "elevated": crate::actions::is_elevated(),
        "system": system,
        "shutdowns": shutdowns,
        "bugchecks": [],
        "kernelReports": [],
        "events": events,
        "appFaults": app_faults,
        "disks": [],
        "volumes": [],
        "drivers": drivers,
        "config": config,
        "notes": notes,
    }))
}

/// Map a kernel log line to an analyzer category, or None if it isn't stability-relevant.
fn classify(msg: &str) -> Option<&'static str> {
    let m = msg.to_ascii_lowercase();
    let has = |s: &str| m.contains(s);
    if has("nvrm: xid")
        || has("gpu has fallen off the bus")
        || has("gpu hang")
        || (has("amdgpu") && (has("timeout") || has("reset")))
        || (has("i915") && (has("reset") || has("hang")))
    {
        return Some("gpu");
    }
    if has("machine check") || has("mce:") || has("hardware error") || has("edac") {
        return Some("hardware");
    }
    if has("soft lockup") || has("hard lockup") || has("blocked for more than") || has("self-detected stall") {
        return Some("hang");
    }
    if has("out of memory") || has("oom-kill") {
        return Some("memory");
    }
    if has("i/o error")
        || (has("nvme") && (has("timeout") || has("reset")))
        || (has("ata") && has("error"))
        || has("ext4-fs error")
        || has("btrfs error")
    {
        return Some("storage");
    }
    if has("kernel panic") || has("oops") || has("bug:") || has("general protection fault") {
        return Some("kernel");
    }
    if (has("iwlwifi") && (has("error") || has("fail"))) || has("netdev watchdog") {
        return Some("network");
    }
    if (has("thermal") && (has("throttl") || has("critical"))) || has("temperature above threshold") {
        return Some("thermal");
    }
    None
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `short-unix` lines look like "1726373207.123456 host ident[pid]: message".
fn parse_short_unix(line: &str) -> Option<(i64, &str)> {
    let mut parts = line.splitn(3, ' ');
    let ts = parts.next()?.parse::<f64>().ok()? as i64;
    parts.next()?;
    Some((ts, parts.next().unwrap_or("")))
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn key_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(|v| v.trim_matches('"').to_string())
}

fn colon_field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
}

fn kb_field(text: &str, key: &str) -> Option<f64> {
    colon_field(text, key)?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push_str("...");
        t
    }
}

fn epoch(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
