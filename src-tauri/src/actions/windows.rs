#![cfg(windows)]

use super::ActionError;
use crate::model::{PriorityLevel, ProcessDetails};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, HANDLE, LUID,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation,
    LUID_AND_ATTRIBUTES, SE_DEBUG_NAME, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::ProcessStatus::K32EmptyWorkingSet;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetPriorityClass, GetProcessAffinityMask, OpenProcess, OpenProcessToken,
    SetPriorityClass, SetProcessAffinityMask, SetProcessInformation, TerminateProcess,
    ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
    IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_ACCESS_RIGHTS,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_SUSPEND_RESUME, PROCESS_TERMINATE, ProcessPowerThrottling,
    REALTIME_PRIORITY_CLASS,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_NORMAL;

#[link(name = "ntdll")]
extern "system" {
    fn NtSuspendProcess(handle: HANDLE) -> i32;
    fn NtResumeProcess(handle: HANDLE) -> i32;
}

struct ProcHandle(HANDLE);

impl ProcHandle {
    fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<Self, ActionError> {
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
            "access denied - try running TaskForge as administrator (protected processes stay off-limits)".into(),
        )
    } else if e.code() == ERROR_INVALID_PARAMETER.to_hresult() {
        ActionError::NotFound
    } else {
        ActionError::Os(e.message().to_string())
    }
}

fn priority_class(level: PriorityLevel) -> windows::Win32::System::Threading::PROCESS_CREATION_FLAGS {
    match level {
        PriorityLevel::Idle => IDLE_PRIORITY_CLASS,
        PriorityLevel::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
        PriorityLevel::Normal => NORMAL_PRIORITY_CLASS,
        PriorityLevel::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        PriorityLevel::High => HIGH_PRIORITY_CLASS,
    }
}

fn priority_label(class: u32) -> String {
    match windows::Win32::System::Threading::PROCESS_CREATION_FLAGS(class) {
        IDLE_PRIORITY_CLASS => "Idle",
        BELOW_NORMAL_PRIORITY_CLASS => "Below normal",
        NORMAL_PRIORITY_CLASS => "Normal",
        ABOVE_NORMAL_PRIORITY_CLASS => "Above normal",
        HIGH_PRIORITY_CLASS => "High",
        REALTIME_PRIORITY_CLASS => "Realtime",
        _ => "Unknown",
    }
    .to_string()
}

pub fn set_priority(pid: u32, level: PriorityLevel) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_SET_INFORMATION)?;
    unsafe { SetPriorityClass(h.0, priority_class(level)).map_err(|e| map_win_err(e, pid)) }
}

/// Toggle Windows EcoQoS (the "Efficiency mode" from Task Manager) via power throttling.
pub fn set_efficiency_mode(pid: u32, on: bool) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_SET_INFORMATION)?;
    // EcoQoS also wants Idle priority; Task Manager pairs the two.
    if on {
        let _ = unsafe { SetPriorityClass(h.0, IDLE_PRIORITY_CLASS) };
    }
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: if on {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        },
    };
    unsafe {
        SetProcessInformation(
            h.0,
            ProcessPowerThrottling,
            &state as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
        .map_err(|e| map_win_err(e, pid))
    }
}

pub fn set_affinity(pid: u32, mask: u64) -> Result<(), ActionError> {
    if mask == 0 {
        return Err(ActionError::Unsupported("affinity mask must select at least one core".into()));
    }
    let h = ProcHandle::open(pid, PROCESS_SET_INFORMATION)?;
    unsafe { SetProcessAffinityMask(h.0, mask as usize).map_err(|e| map_win_err(e, pid)) }
}

pub fn get_details(pid: u32) -> Result<ProcessDetails, ActionError> {
    let h = ProcHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut details = ProcessDetails {
        pid,
        core_count: num_cpus(),
        ..Default::default()
    };
    unsafe {
        let class = GetPriorityClass(h.0);
        if class != 0 {
            details.priority = Some(priority_label(class));
        }
        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        if GetProcessAffinityMask(h.0, &mut process_mask, &mut system_mask).is_ok() {
            details.affinity_mask = Some(process_mask as u64);
        }
    }
    Ok(details)
}

pub fn restart_as_admin() -> Result<(), ActionError> {
    let exe = std::env::current_exe()
        .map_err(|e| ActionError::Os(e.to_string()))?;
    let file = HSTRING::from(exe.as_os_str());
    let verb = HSTRING::from("runas");
    // Marks the elevated copy as our own relaunch so the single-instance guard lets it
    // take over instead of forwarding to the instance that is about to exit.
    let params = HSTRING::from(crate::ui::RELAUNCH_FLAG);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR::null(),
            SW_NORMAL,
        )
    };
    // ShellExecuteW returns > 32 on success.
    if result.0 as isize > 32 {
        std::process::exit(0);
    } else {
        Err(ActionError::PermissionDenied(
            "elevation was declined".into(),
        ))
    }
}

pub fn trim_ram(pid: u32) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA)?;
    unsafe { K32EmptyWorkingSet(h.0).ok().map_err(|e| map_win_err(e, pid)) }
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
        Err(ActionError::PermissionDenied("access denied".into()))
    } else {
        Err(ActionError::Os(format!("NTSTATUS 0x{status:08X}")))
    }
}

pub fn kill(pid: u32, _force: bool) -> Result<(), ActionError> {
    let h = ProcHandle::open(pid, PROCESS_TERMINATE)?;
    unsafe { TerminateProcess(h.0, 1).map_err(|e| map_win_err(e, pid)) }
}

fn num_cpus() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
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
