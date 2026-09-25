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
