#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use super::{Backups, ChangeResult, SettingChange, SettingState, SettingValue, HELPER_FLAG};

const JOURNAL_DROPIN: &str = "/etc/systemd/journald.conf.d/60-taskforge.conf";
const KDUMP_DEFAULTS: &str = "/etc/default/kdump-tools";
const UU_CONFIG: &str = "/etc/apt/apt.conf.d/50unattended-upgrades";
const UU_NO_REBOOT: &str = "/etc/apt/apt.conf.d/99taskforge-no-reboot";
/// pkexec exit codes: dialog dismissed / not authorized.
const PKEXEC_DISMISSED: i32 = 126;
const PKEXEC_DENIED: i32 = 127;
/// Names the elevated helper accepts, as `<name>=<on|off|default>`.
const HELPER_NAMES: &[&str] = &["journal", "kdump", "uu_no_reboot"];

struct Def {
    id: &'static str,
    group: &'static str,
    label: &'static str,
    description: &'static str,
    recommended: Option<SettingValue>,
    needs_admin: bool,
    needs_restart: bool,
}

const ON: Option<SettingValue> = Some(SettingValue::Toggle { on: true });

const DEFS: &[Def] = &[
    Def {
        id: "persistent_journal",
        group: "Crash capture",
        label: "Persistent journal",
        description: "Keep system logs across reboots so crashes and freezes can be traced afterwards.",
        recommended: ON,
        needs_admin: true,
        needs_restart: false,
    },
    Def {
        id: "kdump",
        group: "Crash capture",
        label: "Kernel crash dumps (kdump)",
        description: "Save a memory dump when the kernel crashes.",
        recommended: ON,
        needs_admin: true,
        needs_restart: true,
    },
    Def {
        id: "uu_no_reboot",
        group: "Updates",
        label: "No automatic reboot after updates",
        description: "Stop unattended-upgrades from rebooting the PC on its own.",
        recommended: ON,
        needs_admin: true,
        needs_restart: false,
    },
    Def {
        id: "autostart",
        group: "App",
        label: "Start TaskForge at sign-in",
        description: "Open TaskForge when you sign in.",
        recommended: None,
        needs_admin: false,
        needs_restart: false,
    },
];

pub fn list(backups: &Backups) -> Vec<SettingState> {
    DEFS.iter()
        .map(|d| {
            let (supported, note) = support(d.id);
            SettingState {
                id: d.id,
                group: d.group,
                label: d.label,
                description: d.description,
                kind: "toggle",
                value: if supported { read_value(d.id) } else { None },
                recommended: d.recommended,
                supported,
                note,
                needs_admin: d.needs_admin,
                needs_restart: d.needs_restart,
                can_restore: backups.has(d.id),
            }
        })
        .collect()
}

pub fn apply(backups: &mut Backups, changes: Vec<SettingChange>) -> Vec<ChangeResult> {
    let mut results = Vec::new();
    let mut actions: Vec<(&'static str, String)> = Vec::new();
    let mut pending: Vec<(&'static str, Option<SettingValue>)> = Vec::new();

    for change in changes {
        let Some(def) = DEFS.iter().find(|d| d.id == change.id) else {
            results.push(ChangeResult::failure(&change.id, "unknown setting"));
            continue;
        };
        let (supported, note) = support(def.id);
        if !supported {
            let reason = note.unwrap_or_else(|| "not supported on this system".into());
            results.push(ChangeResult::failure(def.id, reason));
            continue;
        }
        if def.id == "autostart" {
            results.push(apply_autostart(backups, change.value));
            continue;
        }
        let Some(name) = helper_name(def.id) else {
            continue;
        };
        let state = match change.value {
            Some(SettingValue::Toggle { on }) => {
                backups.remember(def.id, json!(current_state(def.id)));
                let state = if on { "on" } else { "off" };
                state.to_string()
            }
            Some(_) => {
                results.push(ChangeResult::failure(def.id, "this value doesn't fit the setting"));
                continue;
            }
            None => match backups.get(def.id).and_then(Value::as_str) {
                Some(s) if matches!(s, "on" | "off" | "default") => s.to_string(),
                _ => {
                    results.push(ChangeResult::failure(def.id, "no saved value to restore"));
                    continue;
                }
            },
        };
        actions.push((name, state));
        pending.push((def.id, change.value));
    }

    if !actions.is_empty() {
        let outcome = if crate::actions::is_elevated() {
            run_actions(&actions)
        } else {
            run_elevated(&actions)
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

/// Elevated helper: `task-manager --apply-settings <name>=<state> ...`.
pub fn run_helper(args: &[String]) -> i32 {
    let mut actions = Vec::new();
    for arg in args {
        let Some((name, state)) = arg.split_once('=') else {
            return 2;
        };
        if !HELPER_NAMES.contains(&name) {
            return 2;
        }
        actions.push((name, state.to_string()));
    }
    match run_actions(&actions) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn helper_name(id: &str) -> Option<&'static str> {
    match id {
        "persistent_journal" => Some("journal"),
        "kdump" => Some("kdump"),
        "uu_no_reboot" => Some("uu_no_reboot"),
        _ => None,
    }
}

fn support(id: &str) -> (bool, Option<String>) {
    if std::env::var_os("SNAP").is_some() {
        return (
            false,
            Some("Not available in the Snap version: these files are outside its sandbox.".into()),
        );
    }
    match id {
        "kdump" if !Path::new(KDUMP_DEFAULTS).exists() => (
            false,
            Some("Install kdump-tools first (sudo apt install kdump-tools).".into()),
        ),
        "kdump" => (true, Some("Takes effect after a restart.".into())),
        "uu_no_reboot" if !Path::new(UU_CONFIG).exists() => {
            (false, Some("unattended-upgrades is not installed.".into()))
        }
        _ => (true, None),
    }
}

fn read_value(id: &str) -> Option<SettingValue> {
    let on = match id {
        "persistent_journal" => match journal_state() {
            "on" => true,
            "off" => false,
            // journald's default ("auto") is persistent once /var/log/journal exists.
            _ => Path::new("/var/log/journal").is_dir(),
        },
        "kdump" => kdump_enabled()?,
        "uu_no_reboot" => !apt_auto_reboot()?,
        "autostart" => autostart_path()?.exists(),
        _ => return None,
    };
    Some(SettingValue::Toggle { on })
}

/// "on" / "off" when our journald drop-in exists, otherwise "default".
fn journal_state() -> &'static str {
    match std::fs::read_to_string(JOURNAL_DROPIN) {
        Ok(text) if text.contains("Storage=persistent") => "on",
        Ok(_) => "off",
        Err(_) => "default",
    }
}

fn current_state(id: &str) -> &'static str {
    match id {
        "persistent_journal" => journal_state(),
        "kdump" => {
            if kdump_enabled().unwrap_or(false) {
                "on"
            } else {
                "off"
            }
        }
        "uu_no_reboot" => {
            if Path::new(UU_NO_REBOOT).exists() {
                "on"
            } else {
                "off"
            }
        }
        _ => "default",
    }
}

fn kdump_enabled() -> Option<bool> {
    let text = std::fs::read_to_string(KDUMP_DEFAULTS).ok()?;
    Some(text.lines().map(str::trim).any(|l| l == "USE_KDUMP=1"))
}

/// Effective `Unattended-Upgrade::Automatic-Reboot`; apt's default is false.
fn apt_auto_reboot() -> Option<bool> {
    if !Path::new(UU_CONFIG).exists() {
        return None;
    }
    let out = Command::new("apt-config")
        .args(["dump", "Unattended-Upgrade::Automatic-Reboot"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    Some(text.lines().any(|l| {
        l.trim().starts_with("Unattended-Upgrade::Automatic-Reboot ")
            && (l.contains("\"true\"") || l.contains("\"1\""))
    }))
}

fn autostart_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("autostart").join("taskforge.desktop"))
}

/// An AppImage runs from a temporary mount, so point at the AppImage file itself.
fn launch_path() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

fn apply_autostart(backups: &mut Backups, value: Option<SettingValue>) -> ChangeResult {
    let enabled = || autostart_path().map_or(false, |p| p.exists());
    let on = match value {
        Some(SettingValue::Toggle { on }) => {
            backups.remember("autostart", json!({ "autostart": enabled() }));
            on
        }
        Some(_) => return ChangeResult::failure("autostart", "this value doesn't fit the setting"),
        None => match backups
            .get("autostart")
            .and_then(|v| v.get("autostart"))
            .and_then(Value::as_bool)
        {
            Some(on) => on,
            None => return ChangeResult::failure("autostart", "no saved value to restore"),
        },
    };
    match set_autostart(on) {
        Ok(()) => {
            if value.is_none() {
                backups.forget("autostart");
            }
            ChangeResult::success("autostart")
        }
        Err(e) => ChangeResult::failure("autostart", e),
    }
}

fn set_autostart(on: bool) -> Result<(), String> {
    let path = autostart_path().ok_or("could not find the config directory")?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = launch_path().ok_or("could not find the TaskForge executable")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=TaskForge\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    std::fs::write(&path, entry).map_err(|e| e.to_string())
}

fn run_actions(actions: &[(&str, String)]) -> Result<(), String> {
    let errors: Vec<String> = actions
        .iter()
        .filter_map(|(name, state)| system_action(name, state).err())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn system_action(name: &str, state: &str) -> Result<(), String> {
    match (name, state) {
        ("journal", "on" | "off") => {
            let storage = if state == "on" { "persistent" } else { "volatile" };
            write_file(
                JOURNAL_DROPIN,
                &format!("# Written by TaskForge\n[Journal]\nStorage={storage}\n"),
            )?;
            restart_journald()
        }
        ("journal", "default") => {
            remove_file(JOURNAL_DROPIN)?;
            restart_journald()
        }
        ("kdump", "on" | "off") => {
            let on = state == "on";
            set_use_kdump(on)?;
            run("systemctl", &[if on { "enable" } else { "disable" }, "kdump-tools"])
        }
        ("uu_no_reboot", "on") => write_file(
            UU_NO_REBOOT,
            "// Written by TaskForge\nUnattended-Upgrade::Automatic-Reboot \"false\";\n",
        ),
        ("uu_no_reboot", "off") => remove_file(UU_NO_REBOOT),
        _ => Err(format!("unsupported change {name}={state}")),
    }
}

fn restart_journald() -> Result<(), String> {
    run("systemctl", &["restart", "systemd-journald"])?;
    // Move logs kept in memory so far onto disk; harmless when volatile.
    let _ = run("journalctl", &["--flush"]);
    Ok(())
}

fn set_use_kdump(on: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(KDUMP_DEFAULTS).map_err(|e| format!("{KDUMP_DEFAULTS}: {e}"))?;
    let wanted = if on { "USE_KDUMP=1" } else { "USE_KDUMP=0" };
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("USE_KDUMP=") {
                found = true;
                wanted.to_string()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(wanted.to_string());
    }
    write_file(KDUMP_DEFAULTS, &(lines.join("\n") + "\n"))
}

fn write_file(path: &str, contents: &str) -> Result<(), String> {
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{path}: {e}"))?;
    }
    std::fs::write(path, contents).map_err(|e| format!("{path}: {e}"))
}

fn remove_file(path: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{path}: {e}")),
        _ => Ok(()),
    }
}

fn run(cmd: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{cmd} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Run this executable through pkexec in helper mode and wait for it.
fn run_elevated(actions: &[(&str, String)]) -> Result<(), String> {
    let exe = launch_path().ok_or("could not find the TaskForge executable")?;
    let mut cmd = Command::new("pkexec");
    cmd.arg(exe).arg(HELPER_FLAG);
    for (name, state) in actions {
        cmd.arg(format!("{name}={state}"));
    }
    let status = cmd
        .status()
        .map_err(|e| format!("could not start pkexec: {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(PKEXEC_DISMISSED) | Some(PKEXEC_DENIED) => {
            Err("administrator permission was declined".into())
        }
        Some(code) => Err(format!("the settings helper failed (exit code {code})")),
        None => Err("the settings helper was stopped".into()),
    }
}
