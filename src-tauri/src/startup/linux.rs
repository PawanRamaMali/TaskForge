#![cfg(target_os = "linux")]

use std::path::PathBuf;

use serde_json::{json, Value};

/// XDG autostart entries (~/.config/autostart and system dirs). Disabling sets
/// `Hidden=true` in a user-level copy, the standard reversible mechanism.
pub fn list() -> Result<Value, String> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in autostart_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let user_scope = dir.starts_with(user_autostart_dir().unwrap_or_default());
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".desktop") || !seen.insert(name.clone()) {
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            let field = |key: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
                    .map(str::trim)
                    .map(str::to_string)
            };
            let hidden = field("Hidden").as_deref() == Some("true");
            let disabled_by_show = field("X-GNOME-Autostart-enabled").as_deref() == Some("false");
            items.push(json!({
                "id": format!("xdg|{name}"),
                "name": field("Name").unwrap_or_else(|| name.trim_end_matches(".desktop").to_string()),
                "command": field("Exec").unwrap_or_default(),
                "source": if user_scope { "Autostart (user)" } else { "Autostart (system)" },
                "scope": if user_scope { "user" } else { "machine" },
                "kind": "xdg",
                "microsoft": false,
                "enabled": !hidden && !disabled_by_show,
            }));
        }
    }
    Ok(json!({ "items": items }))
}

pub fn apply(changes: Value) -> Result<Value, String> {
    let mut results = Vec::new();
    for change in changes.as_array().cloned().unwrap_or_default() {
        let id = change.get("id").and_then(Value::as_str).unwrap_or("");
        let enabled = change.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let outcome = set_enabled(id, enabled);
        results.push(json!({
            "id": id,
            "ok": outcome.is_ok(),
            "error": outcome.err(),
        }));
    }
    Ok(json!({ "results": results }))
}

fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let file = id.strip_prefix("xdg|").ok_or("unknown entry")?;
    let user_dir = user_autostart_dir().ok_or("no config directory")?;
    let user_path = user_dir.join(file);
    // Start from the user copy, else the first system entry, so a system entry is
    // overridden by a hidden user copy rather than edited in place.
    let base = if user_path.exists() {
        std::fs::read_to_string(&user_path).unwrap_or_default()
    } else {
        autostart_dirs()
            .iter()
            .map(|d| d.join(file))
            .find(|p| p.exists())
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| "[Desktop Entry]\nType=Application\n".to_string())
    };
    let mut lines: Vec<String> = base
        .lines()
        .filter(|l| !l.starts_with("Hidden=") && !l.starts_with("X-GNOME-Autostart-enabled="))
        .map(str::to_string)
        .collect();
    lines.push(format!("Hidden={}", if enabled { "false" } else { "true" }));
    std::fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    std::fs::write(&user_path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

fn user_autostart_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("autostart"))
}

fn autostart_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(u) = user_autostart_dir() {
        dirs.push(u);
    }
    dirs.push(PathBuf::from("/etc/xdg/autostart"));
    dirs
}
