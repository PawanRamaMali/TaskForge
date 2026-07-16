use crate::model::{Category, SafeKillAction};

/// Everything the classifier needs to know about a process, OS-agnostic.
/// Some fields only feed one OS's rules (uid on Linux, session on Windows).
#[cfg_attr(windows, allow(dead_code))]
pub struct ProcFacts<'a> {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: &'a str,
    pub exe: Option<&'a str>,
    pub user: Option<&'a str>,
    pub uid_under_1000: bool,
    pub cmd_empty: bool,
    pub session_id: Option<u32>,
    pub current_session_id: Option<u32>,
}

/// Substring patterns (lowercase) marking processes that "Optimize All" may act on.
/// SuspendOnly entries are paused rather than killed (e.g. indexers that resume fine).
const SAFE_TO_KILL: &[(&str, SafeKillAction)] = &[
    ("update", SafeKillAction::Kill),
    ("telemetry", SafeKillAction::Kill),
    ("compattelrunner", SafeKillAction::Kill),
    ("crashpad_handler", SafeKillAction::Kill),
    ("crashreporter", SafeKillAction::Kill),
    ("phoneexperiencehost", SafeKillAction::Kill),
    ("gamebar", SafeKillAction::Kill),
    ("yourphone", SafeKillAction::Kill),
    ("adobearm", SafeKillAction::Kill),
    ("tracker-miner", SafeKillAction::Kill),
    ("searchindexer", SafeKillAction::SuspendOnly),
    ("searchprotocolhost", SafeKillAction::SuspendOnly),
    ("searchfilterhost", SafeKillAction::SuspendOnly),
];

#[cfg(windows)]
const CRITICAL_NAMES: &[&str] = &[
    "system",
    "secure system",
    "registry",
    "memory compression",
    "idle",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "svchost.exe",
    "dwm.exe",
    "fontdrvhost.exe",
    "sihost.exe",
    "ctfmon.exe",
    "audiodg.exe",
    "wmiprvse.exe",
    "lsaiso.exe",
    "vmmem",
];

#[cfg(not(windows))]
const CRITICAL_NAMES: &[&str] = &[
    "systemd",
    "init",
    "dbus-daemon",
    "dbus-broker",
    "systemd-journald",
    "systemd-logind",
    "systemd-udevd",
    "systemd-resolved",
    "networkmanager",
    "polkitd",
    "xorg",
    "xwayland",
    "gnome-shell",
    "gnome-session-binary",
    "mutter",
    "kwin_wayland",
    "kwin_x11",
    "plasmashell",
    "sway",
    "weston",
    "login",
];

const SYSTEM_USERS: &[&str] = &[
    "system",
    "local service",
    "network service",
    "root",
    "dwm-1",
    "umfd-0",
    "umfd-1",
];

pub fn classify(f: &ProcFacts) -> (Category, bool) {
    let category = category_of(f);
    let safe = match category {
        Category::Critical | Category::SystemService => false,
        _ => safe_kill_action(f.name).is_some(),
    };
    (category, safe)
}

pub fn safe_kill_action(name: &str) -> Option<SafeKillAction> {
    let lower = name.to_lowercase();
    SAFE_TO_KILL
        .iter()
        .find(|(pat, _)| lower.contains(pat))
        .map(|(_, action)| *action)
}

#[cfg(windows)]
fn category_of(f: &ProcFacts) -> Category {
    let lower = f.name.to_lowercase();
    let exe_lower = f.exe.map(|e| e.to_lowercase());
    let windir = std::env::var("SystemRoot")
        .unwrap_or_else(|_| "C:\\Windows".into())
        .to_lowercase();

    if f.pid == 0 || f.pid == 4 {
        return Category::Critical;
    }
    if CRITICAL_NAMES.contains(&lower.as_str()) {
        return Category::Critical;
    }
    // conhost hosting session-0 services is critical plumbing; user-session conhost is not.
    if lower == "conhost.exe" && f.session_id == Some(0) {
        return Category::Critical;
    }
    if f.session_id == Some(0) {
        if let Some(exe) = &exe_lower {
            if exe.starts_with(&format!("{}\\system32", windir)) {
                return Category::Critical;
            }
        }
    }
    if lower == "explorer.exe" {
        return Category::SystemService;
    }
    if let Some(user) = f.user {
        if SYSTEM_USERS.contains(&user.to_lowercase().as_str()) {
            return Category::SystemService;
        }
    }
    if let Some(exe) = &exe_lower {
        if exe.starts_with(&windir) {
            return Category::SystemService;
        }
        if f.session_id.is_some() && f.session_id == f.current_session_id {
            return Category::UserApp;
        }
    }
    Category::Background
}

#[cfg(not(windows))]
fn category_of(f: &ProcFacts) -> Category {
    let lower = f.name.to_lowercase();

    if f.pid == 1 || f.pid == 2 {
        return Category::Critical;
    }
    // Kernel threads: children of kthreadd (pid 2) or no cmdline at all.
    if f.parent_pid == Some(2) || f.cmd_empty {
        return Category::Critical;
    }
    if CRITICAL_NAMES.contains(&lower.as_str()) {
        return Category::Critical;
    }
    // sshd daemon (direct child of init) is critical; per-connection children are not.
    if lower == "sshd" && f.parent_pid == Some(1) {
        return Category::Critical;
    }
    if f.uid_under_1000 {
        return Category::SystemService;
    }
    if let Some(exe) = f.exe {
        if exe.starts_with("/usr/lib/systemd")
            || exe.starts_with("/usr/libexec")
            || exe.starts_with("/usr/sbin")
        {
            return Category::SystemService;
        }
        if exe.starts_with("/usr/bin")
            || exe.starts_with("/usr/local")
            || exe.starts_with("/opt")
            || exe.starts_with("/snap")
            || exe.starts_with("/home")
        {
            return Category::UserApp;
        }
    }
    Category::Background
}

/// Actions permitted per category. Enforced by the backend before any OS call.
pub fn action_allowed(category: Category, kind: crate::model::OptimizeKind) -> Result<(), String> {
    use crate::model::OptimizeKind::*;
    match category {
        Category::Critical => Err("blocked: critical system process".into()),
        Category::SystemService => match kind {
            LowerPriority | TrimRam => Ok(()),
            Kill | Suspend => Err("blocked: OS service (only priority/RAM trim allowed)".into()),
        },
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OptimizeKind;

    fn facts<'a>(
        pid: u32,
        name: &'a str,
        exe: Option<&'a str>,
        user: Option<&'a str>,
        session_id: Option<u32>,
    ) -> ProcFacts<'a> {
        ProcFacts {
            pid,
            parent_pid: Some(100),
            name,
            exe,
            user,
            uid_under_1000: false,
            cmd_empty: false,
            session_id,
            current_session_id: Some(1),
        }
    }

    #[test]
    #[cfg(windows)]
    fn windows_critical_processes_are_protected() {
        for name in ["csrss.exe", "lsass.exe", "svchost.exe", "dwm.exe", "System"] {
            let (cat, safe) = classify(&facts(500, name, None, Some("SYSTEM"), Some(0)));
            assert_eq!(cat, Category::Critical, "{name} must be Critical");
            assert!(!safe, "{name} must never be safe_to_kill");
        }
        // pid 4 is the System process regardless of name
        let (cat, _) = classify(&facts(4, "whatever", None, None, Some(0)));
        assert_eq!(cat, Category::Critical);
    }

    #[test]
    #[cfg(windows)]
    fn windows_explorer_is_service_user_apps_are_apps() {
        let (cat, _) = classify(&facts(900, "explorer.exe", None, Some("alpine"), Some(1)));
        assert_eq!(cat, Category::SystemService);

        let (cat, _) = classify(&facts(
            901,
            "chrome.exe",
            Some("C:\\Program Files\\Google\\Chrome\\chrome.exe"),
            Some("alpine"),
            Some(1),
        ));
        assert_eq!(cat, Category::UserApp);
    }

    #[test]
    #[cfg(windows)]
    fn updaters_are_safe_to_kill_but_critical_names_never() {
        let (cat, safe) = classify(&facts(
            902,
            "GoogleUpdate.exe",
            Some("C:\\Program Files (x86)\\Google\\Update\\GoogleUpdate.exe"),
            Some("alpine"),
            Some(1),
        ));
        assert_ne!(cat, Category::Critical);
        assert!(safe);
        assert_eq!(safe_kill_action("SearchIndexer.exe"), Some(SafeKillAction::SuspendOnly));
    }

    #[test]
    #[cfg(not(windows))]
    fn linux_kernel_threads_and_init_are_critical() {
        let (cat, _) = classify(&facts(1, "systemd", Some("/usr/lib/systemd/systemd"), Some("root"), None));
        assert_eq!(cat, Category::Critical);
        let mut kthread = facts(321, "kworker/0:1", None, Some("root"), None);
        kthread.parent_pid = Some(2);
        kthread.cmd_empty = true;
        let (cat, _) = classify(&kthread);
        assert_eq!(cat, Category::Critical);
    }

    #[test]
    fn guard_blocks_kill_on_protected_categories() {
        assert!(action_allowed(Category::Critical, OptimizeKind::Kill).is_err());
        assert!(action_allowed(Category::Critical, OptimizeKind::LowerPriority).is_err());
        assert!(action_allowed(Category::SystemService, OptimizeKind::Kill).is_err());
        assert!(action_allowed(Category::SystemService, OptimizeKind::Suspend).is_err());
        assert!(action_allowed(Category::SystemService, OptimizeKind::LowerPriority).is_ok());
        assert!(action_allowed(Category::UserApp, OptimizeKind::Kill).is_ok());
        assert!(action_allowed(Category::Background, OptimizeKind::TrimRam).is_ok());
    }
}
