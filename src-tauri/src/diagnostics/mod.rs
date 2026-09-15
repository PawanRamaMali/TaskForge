//! Stability diagnostics: gathers crash, hang and hardware-error history from the OS
//! as raw JSON. Collection is read-only; the analysis (findings, likely causes,
//! recommendations) runs in the frontend so both platforms share one rule set.

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

/// Collect the raw diagnostics document covering the last `window_days` days.
/// Blocking and slow (event log queries take seconds); call off the main thread.
pub fn collect(window_days: u32) -> Result<serde_json::Value, String> {
    imp::collect(window_days)
}
