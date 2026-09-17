#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::process::{Command, Output};

use serde_json::{json, Value};

use super::{
    valid_active_hours, Backups, ChangeResult, SettingChange, SettingState, SettingValue,
    HELPER_FLAG,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// "The operation was canceled by the user" (UAC prompt declined).
const ERROR_CANCELLED: i32 = 1223;

const CRASH: &str = r"HKLM\SYSTEM\CurrentControlSet\Control\CrashControl";
const AU: &str = r"HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU";
const UX: &str = r"HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "TaskForge";

/// Every registry value TaskForge may change. The elevated helper only accepts
/// indexes into this table, so it can't be pointed at any other value.
const SLOTS: &[(&str, &str)] = &[
    (r"HKLM\SYSTEM\CurrentControlSet\Services\kbdhid\Parameters", "CrashOnCtrlScroll"),
    (r"HKLM\SYSTEM\CurrentControlSet\Services\i8042prt\Parameters", "CrashOnCtrlScroll"),
    (CRASH, "CrashDumpEnabled"),
    (CRASH, "AlwaysKeepMemoryDump"),
    (CRASH, "MinidumpsCount"),
    (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches\System error memory dump files",
        "Autorun",
    ),
    (
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches\System error minidump files",
        "Autorun",
    ),
    (r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Power", "HiberbootEnabled"),
    (AU, "NoAutoRebootWithLoggedOnUsers"),
    (AU, "AlwaysAutoRebootAtScheduledTime"),
    (UX, "RestartNotificationsAllowed2"),
    (UX, "ActiveHoursStart"),
    (UX, "ActiveHoursEnd"),
];

const KBDHID: usize = 0;
const I8042: usize = 1;
const DUMP_TYPE: usize = 2;
const KEEP_DUMP: usize = 3;
const MINIDUMPS: usize = 4;
const CLEAN_DUMP: usize = 5;
const CLEAN_MINI: usize = 6;
const HIBERBOOT: usize = 7;
const NO_REBOOT: usize = 8;
const SCHED_REBOOT: usize = 9;
const RESTART_NOTIFY: usize = 10;
const HOURS_START: usize = 11;
const HOURS_END: usize = 12;

struct Def {
    id: &'static str,
    group: &'static str,
    label: &'static str,
    description: &'static str,
    kind: &'static str,
    slots: &'static [usize],
    recommended: Option<SettingValue>,
    needs_admin: bool,
    needs_restart: bool,
    note: Option<&'static str>,
}

const ON: Option<SettingValue> = Some(SettingValue::Toggle { on: true });
const OFF: Option<SettingValue> = Some(SettingValue::Toggle { on: false });

const DEFS: &[Def] = &[
    Def {
        id: "crash_key",
        group: "Crash capture",
        label: "Ctrl+Scroll crash key",
        description: "When the PC freezes, hold Right Ctrl and press Scroll Lock twice to save a crash dump that shows what was stuck.",
        kind: "toggle",
        slots: &[KBDHID, I8042],
        recommended: ON,
        needs_admin: true,
        needs_restart: true,
        note: None,
    },
    Def {
        id: "dump_automatic",
        group: "Crash capture",
        label: "Automatic memory dump",
        description: "Save a kernel memory dump on a blue screen instead of only a small minidump.",
        kind: "toggle",
        slots: &[DUMP_TYPE],
        recommended: ON,
        needs_admin: true,
        needs_restart: true,
        note: None,
    },
    Def {
        id: "keep_dumps",
        group: "Crash capture",
        label: "Keep dump files",
        description: "Keep the memory dump even when disk space is low, and keep up to 50 minidumps.",
        kind: "toggle",
        slots: &[KEEP_DUMP, MINIDUMPS],
        recommended: ON,
        needs_admin: true,
        needs_restart: false,
        note: None,
    },
    Def {
        id: "dumps_not_cleaned",
        group: "Crash capture",
        label: "Keep dumps out of disk cleanup",
        description: "Stop automatic disk cleanup from deleting crash dump files.",
        kind: "toggle",
        slots: &[CLEAN_DUMP, CLEAN_MINI],
        recommended: ON,
        needs_admin: true,
        needs_restart: false,
        note: None,
    },
    Def {
        id: "fast_startup",
        group: "Startup",
        label: "Fast Startup",
        description: "Shut down saves the kernel and drivers to disk. With it off, every shutdown gives a clean start.",
        kind: "toggle",
        slots: &[HIBERBOOT],
        recommended: OFF,
        needs_admin: true,
        needs_restart: false,
        note: Some("Takes effect at the next shutdown."),
    },
    Def {
        id: "no_auto_restart",
        group: "Updates",
        label: "No auto-restart while signed in",
        description: "Windows Update installs updates but waits for you to restart.",
        kind: "toggle",
        slots: &[NO_REBOOT, SCHED_REBOOT],
        recommended: ON,
        needs_admin: true,
        needs_restart: false,
        note: Some("Windows Home may not always follow this policy."),
    },
    Def {
        id: "restart_notify",
        group: "Updates",
        label: "Restart notifications",
        description: "Show a notification when an update needs a restart.",
        kind: "toggle",
        slots: &[RESTART_NOTIFY],
        recommended: ON,
        needs_admin: true,
        needs_restart: false,
        note: None,
    },
    Def {
        id: "active_hours",
        group: "Updates",
        label: "Active hours",
        description: "Windows won't restart for updates during these hours. 18 hours at most.",
        kind: "hours",
        slots: &[HOURS_START, HOURS_END],
        recommended: None,
        needs_admin: true,
        needs_restart: false,
        note: None,
    },
    Def {
        id: "autostart",
        group: "App",
        label: "Start TaskForge at sign-in",
        description: "Open TaskForge when you sign in to Windows.",
        kind: "toggle",
        slots: &[],
        recommended: None,
        needs_admin: false,
        needs_restart: false,
        note: None,
    },
];

pub fn list(backups: &Backups) -> Vec<SettingState> {
    DEFS.iter()
        .map(|d| SettingState {
            id: d.id,
            group: d.group,
            label: d.label,
            description: d.description,
            kind: d.kind,
            value: read_value(d.id),
            recommended: d.recommended,
            supported: true,
            note: d.note.map(str::to_string),
            needs_admin: d.needs_admin,
            needs_restart: d.needs_restart,
            can_restore: backups.has(d.id),
        })
        .collect()
}

pub fn apply(backups: &mut Backups, changes: Vec<SettingChange>) -> Vec<ChangeResult> {
    let mut results = Vec::new();
    let mut writes: Vec<(usize, Option<u32>)> = Vec::new();
    // Settings written through the registry, with the value that was asked for.
    let mut pending: Vec<(&'static str, Option<SettingValue>)> = Vec::new();

    for change in changes {
        let Some(def) = DEFS.iter().find(|d| d.id == change.id) else {
            results.push(ChangeResult::failure(&change.id, "unknown setting"));
            continue;
        };
        if def.id == "autostart" {
            results.push(apply_autostart(backups, change.value));
            continue;
        }
        let planned = match change.value {
            Some(value) => plan(def.id, value),
            None => restore_plan(def, backups.get(def.id)),
        };
        match planned {
            Ok(p) => {
                if change.value.is_some() {
                    backups.remember(def.id, snapshot(def));
                }
                writes.extend(p);
                pending.push((def.id, change.value));
            }
            Err(e) => results.push(ChangeResult::failure(def.id, e)),
        }
    }

    if !writes.is_empty() {
        let outcome = if crate::actions::is_elevated() {
            write_all(&writes)
        } else {
            write_elevated(&writes)
        };
        for (id, requested) in pending {
            let result = match (&outcome, requested) {
                (Err(e), _) => ChangeResult::failure(id, e.clone()),
                (Ok(()), Some(value)) if read_value(id) != Some(value) => {
                    ChangeResult::failure(id, "the change did not take effect")
                }
                (Ok(()), Some(_)) => ChangeResult::success(id),
                (Ok(()), None) => {
                    backups.forget(id);
                    ChangeResult::success(id)
                }
            };
            results.push(result);
        }
    }
    results
}

/// Elevated helper: `task-manager --apply-settings s<slot>=<value|-> ...`.
pub fn run_helper(args: &[String]) -> i32 {
    let mut writes = Vec::new();
    for arg in args {
        match parse_write(arg) {
            Some(w) => writes.push(w),
            None => return 2,
        }
    }
    match write_all(&writes) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn parse_write(arg: &str) -> Option<(usize, Option<u32>)> {
    let (slot, value) = arg.strip_prefix('s')?.split_once('=')?;
    let slot: usize = slot.parse().ok()?;
    if slot >= SLOTS.len() {
        return None;
    }
    let value = if value == "-" {
        None
    } else {
        Some(value.parse::<u32>().ok()?)
    };
    Some((slot, value))
}

fn read_value(id: &str) -> Option<SettingValue> {
    use SettingValue::{Hours, Toggle};
    let s = read_slot;
    let value = match id {
        "crash_key" => Toggle { on: s(KBDHID) == Some(1) && s(I8042) == Some(1) },
        // 7 = automatic memory dump.
        "dump_automatic" => Toggle { on: s(DUMP_TYPE)? == 7 },
        // A missing MinidumpsCount means the default of 50.
        "keep_dumps" => Toggle {
            on: s(KEEP_DUMP) == Some(1) && s(MINIDUMPS).map_or(true, |n| n >= 50),
        },
        "dumps_not_cleaned" => Toggle { on: s(CLEAN_DUMP) == Some(0) && s(CLEAN_MINI) == Some(0) },
        // Windows treats a missing value as enabled.
        "fast_startup" => Toggle { on: s(HIBERBOOT).map_or(true, |v| v != 0) },
        // A scheduled-reboot policy of 1 still forces restarts, so it must not be set.
        "no_auto_restart" => Toggle {
            on: s(NO_REBOOT) == Some(1) && s(SCHED_REBOOT) != Some(1),
        },
        "restart_notify" => Toggle { on: s(RESTART_NOTIFY) == Some(1) },
        "active_hours" => Hours {
            start: s(HOURS_START)?.min(23) as u8,
            end: s(HOURS_END)?.min(23) as u8,
        },
        "autostart" => Toggle { on: autostart_enabled() },
        _ => return None,
    };
    Some(value)
}

fn plan(id: &str, value: SettingValue) -> Result<Vec<(usize, Option<u32>)>, String> {
    use SettingValue::{Hours, Toggle};
    let plan = match (id, value) {
        ("crash_key", Toggle { on: true }) => vec![(KBDHID, Some(1)), (I8042, Some(1))],
        ("crash_key", Toggle { on: false }) => vec![(KBDHID, None), (I8042, None)],
        // 7 = automatic memory dump, 3 = small memory dump.
        ("dump_automatic", Toggle { on }) => vec![(DUMP_TYPE, Some(if on { 7 } else { 3 }))],
        ("keep_dumps", Toggle { on: true }) => vec![(KEEP_DUMP, Some(1)), (MINIDUMPS, Some(50))],
        ("keep_dumps", Toggle { on: false }) => vec![(KEEP_DUMP, None)],
        // Autorun 0 keeps the cleanup handler out of automatic (low disk) cleanup.
        ("dumps_not_cleaned", Toggle { on }) => {
            let v = Some(if on { 0 } else { 1 });
            vec![(CLEAN_DUMP, v), (CLEAN_MINI, v)]
        }
        ("fast_startup", Toggle { on }) => vec![(HIBERBOOT, Some(u32::from(on)))],
        ("no_auto_restart", Toggle { on: true }) => {
            vec![(NO_REBOOT, Some(1)), (SCHED_REBOOT, Some(0))]
        }
        ("no_auto_restart", Toggle { on: false }) => vec![(NO_REBOOT, None), (SCHED_REBOOT, None)],
        ("restart_notify", Toggle { on }) => vec![(RESTART_NOTIFY, Some(u32::from(on)))],
        ("active_hours", Hours { start, end }) => {
            if !valid_active_hours(start, end) {
                return Err("active hours must differ and span at most 18 hours".into());
            }
            vec![
                (HOURS_START, Some(u32::from(start))),
                (HOURS_END, Some(u32::from(end))),
            ]
        }
        _ => return Err("this value doesn't fit the setting".into()),
    };
    Ok(plan)
}

/// Raw values of a setting's registry entries: `[[slot, value|null], ...]`.
fn snapshot(def: &Def) -> Value {
    Value::Array(
        def.slots
            .iter()
            .map(|&slot| json!([slot, read_slot(slot)]))
            .collect(),
    )
}

fn restore_plan(def: &Def, saved: Option<&Value>) -> Result<Vec<(usize, Option<u32>)>, String> {
    let entries = saved
        .and_then(Value::as_array)
        .ok_or("no saved value to restore")?;
    let mut plan = Vec::new();
    for entry in entries {
        let slot = entry.get(0).and_then(Value::as_u64).map(|n| n as usize);
        let value = entry.get(1).map(|v| v.as_u64().map(|n| n as u32));
        match (slot, value) {
            // Only this setting's own slots, so an edited backup file can't reach others.
            (Some(slot), Some(value)) if def.slots.contains(&slot) => plan.push((slot, value)),
            _ => return Err("the saved value is invalid".into()),
        }
    }
    Ok(plan)
}

fn apply_autostart(backups: &mut Backups, value: Option<SettingValue>) -> ChangeResult {
    let outcome = match value {
        Some(SettingValue::Toggle { on }) => {
            backups.remember("autostart", json!({ "autostart": autostart_snapshot() }));
            set_autostart(on)
        }
        Some(_) => Err("this value doesn't fit the setting".to_string()),
        None => {
            let saved = backups.get("autostart").and_then(|v| v.get("autostart")).cloned();
            match saved {
                Some(saved) => restore_autostart(&saved).map(|()| backups.forget("autostart")),
                None => Err("no saved value to restore".to_string()),
            }
        }
    };
    match outcome {
        Ok(()) => ChangeResult::success("autostart"),
        Err(e) => ChangeResult::failure("autostart", e),
    }
}

fn autostart_enabled() -> bool {
    query(RUN_KEY, RUN_VALUE).is_some()
}

/// The Run entry exactly as it was (type and command), or null if there was none.
fn autostart_snapshot() -> Value {
    match query(RUN_KEY, RUN_VALUE) {
        Some((kind, data)) => json!({ "kind": kind, "data": data }),
        None => Value::Null,
    }
}

fn restore_autostart(saved: &Value) -> Result<(), String> {
    if saved.is_null() {
        return set_autostart(false);
    }
    let kind = saved.get("kind").and_then(Value::as_str).unwrap_or("REG_SZ");
    if !matches!(kind, "REG_SZ" | "REG_EXPAND_SZ") {
        return Err("the saved value is invalid".into());
    }
    let data = saved
        .get("data")
        .and_then(Value::as_str)
        .ok_or("the saved value is invalid")?;
    check(
        reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", kind, "/d", data, "/f"]),
        RUN_VALUE,
    )
}

fn set_autostart(on: bool) -> Result<(), String> {
    let out = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\"", exe.display());
        reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &command, "/f"])
    } else {
        if !autostart_enabled() {
            return Ok(());
        }
        reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
    };
    check(out, RUN_VALUE)
}

fn reg(args: &[&str]) -> Option<Output> {
    Command::new("reg.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
}

fn check(out: Option<Output>, what: &str) -> Result<(), String> {
    match out {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => Err(format!("{what}: {}", String::from_utf8_lossy(&o.stderr).trim())),
        None => Err("could not run reg.exe".into()),
    }
}

/// `reg query` prints `    <name>    <TYPE>    <data>`; returns (type, data).
fn query(key: &str, name: &str) -> Option<(String, String)> {
    let out = reg(&["query", key, "/v", name])?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines().find_map(|line| {
        let line = line.trim_start();
        let mut parts = line.split_whitespace();
        if !parts.next()?.eq_ignore_ascii_case(name) {
            return None;
        }
        let kind = parts.next()?.to_string();
        // Keep the data exactly (inner spaces included): it follows the type after
        // reg's four-space column separator.
        let rest = line.get(name.len()..)?.trim_start().get(kind.len()..)?;
        let data = rest.strip_prefix("    ").unwrap_or_else(|| rest.trim_start());
        Some((kind, data.to_string()))
    })
}

fn read_slot(slot: usize) -> Option<u32> {
    let (key, name) = SLOTS[slot];
    let (kind, data) = query(key, name)?;
    if kind != "REG_DWORD" {
        return None;
    }
    u32::from_str_radix(data.trim_start_matches("0x"), 16).ok()
}

fn write_slot(slot: usize, value: Option<u32>) -> Result<(), String> {
    let (key, name) = SLOTS[slot];
    let out = match value {
        Some(v) => {
            let data = v.to_string();
            reg(&["add", key, "/v", name, "/t", "REG_DWORD", "/d", &data, "/f"])
        }
        None => {
            if read_slot(slot).is_none() {
                return Ok(());
            }
            reg(&["delete", key, "/v", name, "/f"])
        }
    };
    check(out, name)
}

fn write_all(writes: &[(usize, Option<u32>)]) -> Result<(), String> {
    let errors: Vec<String> = writes
        .iter()
        .filter_map(|&(slot, value)| write_slot(slot, value).err())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Run this executable as administrator in helper mode and wait for it. Only
/// slot indexes and numbers are passed, so nothing needs quoting beyond the path.
fn write_elevated(writes: &[(usize, Option<u32>)]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut args = vec![format!("'{HELPER_FLAG}'")];
    for &(slot, value) in writes {
        let v = value.map_or_else(|| "-".to_string(), |n| n.to_string());
        args.push(format!("'s{slot}={v}'"));
    }
    let script = format!(
        "try {{ $p = Start-Process -FilePath '{}' -ArgumentList {} -Verb RunAs -Wait -PassThru -ErrorAction Stop; exit $p.ExitCode }} catch {{ exit {ERROR_CANCELLED} }}",
        exe.display().to_string().replace('\'', "''"),
        args.join(",")
    );
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"])
        .arg(encode_command(&script))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("could not start PowerShell: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(ERROR_CANCELLED) => Err("administrator permission was declined".into()),
        Some(code) => Err(format!("the settings helper failed (exit code {code})")),
        None => Err("the settings helper was stopped".into()),
    }
}

/// PowerShell's -EncodedCommand takes base64 of the UTF-16LE script.
fn encode_command(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64(&bytes)
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(chunk[0]) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(encode_command("hi"), "aABpAA==");
    }

    #[test]
    fn helper_rejects_unknown_slots() {
        assert_eq!(parse_write("s2=7"), Some((2, Some(7))));
        assert_eq!(parse_write("s8=-"), Some((8, None)));
        assert_eq!(parse_write("s99=1"), None);
        assert_eq!(parse_write("HKLM=1"), None);
        assert_eq!(parse_write("s1=abc"), None);
    }

    #[test]
    fn active_hours_limits() {
        assert!(valid_active_hours(8, 2));
        assert!(!valid_active_hours(8, 8));
        assert!(!valid_active_hours(0, 23));
        assert!(valid_active_hours(11, 0));
    }
}
