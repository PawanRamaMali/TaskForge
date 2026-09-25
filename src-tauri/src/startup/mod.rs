//! Startup manager: list the apps, scripts and scheduled tasks that run at sign-in,
//! and enable/disable them (reversibly). Windows uses the same StartupApproved flags
//! Task Manager uses, so nothing is deleted. Reading is unprivileged; changing a
//! machine-scope entry asks for administrator rights.

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

/// The startup entries and their current enabled state.
pub fn list() -> Result<serde_json::Value, String> {
    imp::list()
}

/// Apply `[{ id, enabled }]` changes. Returns `{ results: [{ id, ok, error }] }`.
pub fn apply(changes: serde_json::Value) -> Result<serde_json::Value, String> {
    imp::apply(changes)
}

/// When started as `task-manager --apply-startup <b64> <out>`, apply the machine-
/// scope changes elevated and return the process exit code; otherwise None.
#[cfg(windows)]
pub fn run_helper_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some(imp::HELPER_FLAG) {
        return None;
    }
    Some(imp::run_helper(&args[1..]))
}
