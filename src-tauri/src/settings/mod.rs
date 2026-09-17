//! Stability settings: a fixed list of OS settings that help diagnose and avoid
//! unexpected restarts (crash capture, Fast Startup, update restarts, autostart).
//! Reading never needs admin rights. Changes that do are handed to an elevated
//! copy of TaskForge (`--apply-settings`), which only accepts entries from a
//! built-in table.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

/// First argument that turns the process into the elevated settings helper.
pub const HELPER_FLAG: &str = "--apply-settings";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(untagged)]
pub enum SettingValue {
    Toggle { on: bool },
    Hours { start: u8, end: u8 },
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SettingState {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// "toggle" or "hours".
    pub kind: &'static str,
    /// Current value; None when it couldn't be read.
    pub value: Option<SettingValue>,
    pub recommended: Option<SettingValue>,
    pub supported: bool,
    pub note: Option<String>,
    pub needs_admin: bool,
    pub needs_restart: bool,
    /// The value from before TaskForge first changed it was saved and can be put back.
    pub can_restore: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SettingChange {
    pub id: String,
    /// New value, or None to restore the value saved before TaskForge changed it.
    pub value: Option<SettingValue>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ChangeResult {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

impl ChangeResult {
    pub(crate) fn success(id: &str) -> Self {
        ChangeResult { id: id.to_string(), ok: true, error: None }
    }

    pub(crate) fn failure(id: &str, error: impl Into<String>) -> Self {
        ChangeResult { id: id.to_string(), ok: false, error: Some(error.into()) }
    }
}

/// Values as they were before TaskForge first changed each setting, kept in the
/// app config dir so they survive restarts and can be restored later.
pub(crate) struct Backups {
    path: PathBuf,
    map: BTreeMap<String, serde_json::Value>,
}

impl Backups {
    pub(crate) fn load(dir: &Path) -> Self {
        let path = dir.join("setting-backups.json");
        let map: BTreeMap<String, serde_json::Value> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Backups { path, map }
    }

    pub(crate) fn get(&self, id: &str) -> Option<&serde_json::Value> {
        self.map.get(id)
    }

    pub(crate) fn has(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    /// Only the first snapshot is kept: that one is the user's own setting.
    pub(crate) fn remember(&mut self, id: &str, value: serde_json::Value) {
        self.map.entry(id.to_string()).or_insert(value);
    }

    pub(crate) fn forget(&mut self, id: &str) {
        self.map.remove(id);
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.map) {
            let _ = std::fs::write(&self.path, text);
        }
    }
}

pub fn list(backup_dir: &Path) -> Vec<SettingState> {
    imp::list(&Backups::load(backup_dir))
}

pub fn apply(backup_dir: &Path, changes: Vec<SettingChange>) -> Vec<ChangeResult> {
    let mut backups = Backups::load(backup_dir);
    let results = imp::apply(&mut backups, changes);
    backups.save();
    results
}

/// When started as `task-manager --apply-settings ...`, apply the requested
/// entries and return the process exit code; otherwise None.
pub fn run_helper_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some(HELPER_FLAG) {
        return None;
    }
    Some(imp::run_helper(&args[1..]))
}

/// Windows allows at most 18 active hours; start and end are hours of the day.
#[cfg(windows)]
pub(crate) fn valid_active_hours(start: u8, end: u8) -> bool {
    start <= 23
        && end <= 23
        && start != end
        && (u16::from(end) + 24 - u16::from(start)) % 24 <= 18
}
