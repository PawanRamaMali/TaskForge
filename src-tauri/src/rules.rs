use std::path::PathBuf;

use parking_lot::Mutex;

use crate::model::PriorityRule;

/// Persistent per-executable priority rules, ProBalance-style: when a matching
/// process appears, its priority (and optionally efficiency mode) is applied once.
#[derive(Default)]
pub struct RuleStore {
    rules: Mutex<Vec<PriorityRule>>,
    path: Mutex<Option<PathBuf>>,
}

impl RuleStore {
    pub fn load_from(&self, path: PathBuf) {
        let rules = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<PriorityRule>>(&s).ok())
            .unwrap_or_default();
        *self.rules.lock() = rules;
        *self.path.lock() = Some(path);
    }

    pub fn list(&self) -> Vec<PriorityRule> {
        self.rules.lock().clone()
    }

    /// Returns the rule matching an executable base name (case-insensitive), if any.
    pub fn match_exe(&self, exe_name: &str) -> Option<PriorityRule> {
        let lower = exe_name.to_lowercase();
        self.rules.lock().iter().find(|r| r.exe == lower).cloned()
    }

    pub fn upsert(&self, mut rule: PriorityRule) {
        rule.exe = rule.exe.to_lowercase();
        let mut rules = self.rules.lock();
        if let Some(existing) = rules.iter_mut().find(|r| r.exe == rule.exe) {
            *existing = rule;
        } else {
            rules.push(rule);
        }
        self.persist(&rules);
    }

    pub fn remove(&self, exe: &str) {
        let lower = exe.to_lowercase();
        let mut rules = self.rules.lock();
        rules.retain(|r| r.exe != lower);
        self.persist(&rules);
    }

    fn persist(&self, rules: &[PriorityRule]) {
        if let Some(path) = self.path.lock().as_ref() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(rules) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}
