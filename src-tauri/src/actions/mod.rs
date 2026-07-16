#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

use serde::Serializer;

use crate::model::{PriorityLevel, ProcessDetails};

#[derive(Debug, Clone)]
pub enum ActionError {
    PermissionDenied(String),
    NotFound,
    Blocked(String),
    Unsupported(String),
    Os(String),
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionError::PermissionDenied(msg) => write!(f, "permission denied: {msg}"),
            ActionError::NotFound => write!(f, "process not found"),
            ActionError::Blocked(msg) => write!(f, "{msg}"),
            ActionError::Unsupported(msg) => write!(f, "unsupported: {msg}"),
            ActionError::Os(msg) => write!(f, "OS error: {msg}"),
        }
    }
}

impl serde::Serialize for ActionError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub fn set_priority_low(pid: u32) -> Result<(), ActionError> {
    imp::set_priority(pid, PriorityLevel::BelowNormal)
}

pub fn set_priority(pid: u32, level: PriorityLevel) -> Result<(), ActionError> {
    imp::set_priority(pid, level)
}

pub fn set_efficiency_mode(pid: u32, on: bool) -> Result<(), ActionError> {
    imp::set_efficiency_mode(pid, on)
}

pub fn set_affinity(pid: u32, mask: u64) -> Result<(), ActionError> {
    imp::set_affinity(pid, mask)
}

pub fn get_details(pid: u32) -> Result<ProcessDetails, ActionError> {
    imp::get_details(pid)
}

pub fn restart_as_admin() -> Result<(), ActionError> {
    imp::restart_as_admin()
}

pub fn trim_ram(pid: u32) -> Result<(), ActionError> {
    imp::trim_ram(pid)
}

pub fn suspend(pid: u32) -> Result<(), ActionError> {
    imp::suspend(pid)
}

pub fn resume(pid: u32) -> Result<(), ActionError> {
    imp::resume(pid)
}

pub fn kill(pid: u32, force: bool) -> Result<(), ActionError> {
    imp::kill(pid, force)
}

pub fn is_elevated() -> bool {
    imp::is_elevated()
}

/// Windows: enable SeDebugPrivilege when running elevated so services can be managed.
/// No-op elsewhere.
pub fn init_privileges() {
    imp::init_privileges()
}
