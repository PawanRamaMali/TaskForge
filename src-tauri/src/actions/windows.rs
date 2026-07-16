#![cfg(windows)]

use super::ActionError;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation,
    LUID_AND_ATTRIBUTES, SE_DEBUG_NAME, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, SetPriorityClass, TerminateProcess,
    BELOW_NORMAL_PRIORITY_CLASS, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_SUSPEND_RESUME, PROCESS_TERMINATE,
};

// Undocumented-but-stable ntdll exports (used by Process Explorer and friends).
// Kept behind suspend()/resume() wrappers so they could be swapped for
// per-thread SuspendThread iteration if these ever break.
#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(handle: HANDLE) -> i32;
    fn NtResumeProcess(handle: HANDLE) -> i32;
}

struct ProcHandle(HANDLE);

impl ProcHandle {
    fn open(pid: u32, access: windows::Win32::System::Threading::PROCESS_ACCESS_RIGHTS) -> Result<Self, ActionError> {
        unsafe {
            OpenProcess(access, false, pid)
                .map(ProcHandle)
                .map_err(|e| map_win_err(e, pid))
        }
    }
}

impl Drop for ProcHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn map_win_err(e: windows::core::Error, _pid: u32) -> ActionError {
    if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
        ActionError::PermissionDenied(
            "access denied — try running TaskForge as administrator (protected processes stay off-limits)".into(),
        )
    } else if e.code() == ERROR_INVALID_PARAMETER.to_hresult() {
        // OpenProcess yields ERROR_INVALID_PARAMETER for exited pids.
        ActionError::NotFound
    } else {
        ActionError::Os(e.message().to_string())
    }
}

pub fn set_priority_low(pid: u32) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_SET_INFORMATION)?;
    unsafe { SetPriorityClass(h.0, BELOW_NORMAL_PRIORITY_CLASS).map_err(|e| map_win_err(e, pid)) }
}

pub fn trim_ram(pid: u32) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA)?;
    unsafe {
        K32EmptyWorkingSet(h.0)
            .ok()
            .map_err(|e| map_win_err(e, pid))
    }
}

pub fn suspend(pid: u32) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_SUSPEND_RESUME)?;
    let status = unsafe { NtSuspendProcess(h.0) };
    nt_result(status)
}

pub fn resume(pid: u32) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_SUSPEND_RESUME)?;
    let status = unsafe { NtResumeProcess(h.0) };
    nt_result(status)
}

fn nt_result(status: i32) -> Result<(), ActionError> {
    if status >= 0 {
        Ok(())
    } else if status == 0xC0000022u32 as i32 {
        // STATUS_ACCESS_DENIED
        Err(ActionError::PermissionDenied("access denied".into()))
    } else {
        Err(ActionError::Os(format!("NTSTATUS 0x{status:08X}")))
    }
}

pub fn kill(pid: u32, _force: bool) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_TERMINATE)?;
    unsafe { TerminateProcess(h.0, 1).map_err(|e| map_win_err(e, pid)) }
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

pub fn init_privileges() {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
        .is_err()
        {
            return;
        }
        let mut luid = LUID::default();
        if LookupPrivilegeValueW(PCWSTR::null(), SE_DEBUG_NAME, &mut luid).is_ok() {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let _ = AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None);
        }
        let _ = CloseHandle(token);
    }
}
