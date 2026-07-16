#![cfg(target_os = "linux")]

use super::ActionError;
use crate::model::{PriorityLevel, ProcessDetails};

use nix::sys::signal::{kill as nix_kill, Signal};
use nix::unistd::Pid;

fn last_errno() -> ActionError {
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EPERM) | Some(libc::EACCES) => ActionError::PermissionDenied(err.to_string()),
        Some(libc::ESRCH) => ActionError::NotFound,
        _ => ActionError::Os(err.to_string()),
    }
}

fn map_nix(e: nix::errno::Errno) -> ActionError {
    match e {
        nix::errno::Errno::EPERM | nix::errno::Errno::EACCES => {
            ActionError::PermissionDenied("not permitted — run with sudo for other users' processes".into())
        }
        nix::errno::Errno::ESRCH => ActionError::NotFound,
        other => ActionError::Os(other.desc().into()),
    }
}

fn nice_for(level: PriorityLevel) -> i32 {
    match level {
        PriorityLevel::Idle => 19,
        PriorityLevel::BelowNormal => 10,
        PriorityLevel::Normal => 0,
        PriorityLevel::AboveNormal => -5,
        PriorityLevel::High => -10,
    }
}

pub fn set_priority(pid: u32, level: PriorityLevel) -> Result<(), ActionError> {
    // Lowering priority (raising nice) is unprivileged for own processes; raising
    // priority (negative nice) needs CAP_SYS_NICE. Errors surface to the UI.
    unsafe {
        *libc::__errno_location() = 0;
        let ret = libc::setpriority(libc::PRIO_PROCESS, pid, nice_for(level));
        if ret == -1 && *libc::__errno_location() != 0 {
            return Err(last_errno());
        }
    }
    Ok(())
}

/// Linux has no exact EcoQoS analog; approximate with SCHED_IDLE + max nice on, restore on off.
pub fn set_efficiency_mode(pid: u32, on: bool) -> Result<(), ActionError> {
    let policy = if on { libc::SCHED_IDLE } else { libc::SCHED_OTHER };
    let param = libc::sched_param { sched_priority: 0 };
    let ret = unsafe { libc::sched_setscheduler(pid as libc::pid_t, policy, &param) };
    if ret == -1 {
        return Err(last_errno());
    }
    set_priority(pid, if on { PriorityLevel::Idle } else { PriorityLevel::Normal })
}

pub fn set_affinity(pid: u32, mask: u64) -> Result<(), ActionError> {
    if mask == 0 {
        return Err(ActionError::Unsupported("affinity mask must select at least one core".into()));
    }
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for i in 0..64 {
            if mask & (1u64 << i) != 0 {
                libc::CPU_SET(i, &mut set);
            }
        }
        let ret = libc::sched_setaffinity(
            pid as libc::pid_t,
            std::mem::size_of::<libc::cpu_set_t>(),
            &set,
        );
        if ret == -1 {
            return Err(last_errno());
        }
    }
    Ok(())
}

pub fn get_details(pid: u32) -> Result<ProcessDetails, ActionError> {
    if !std::path::Path::new(&format!("/proc/{pid}")).exists() {
        return Err(ActionError::NotFound);
    }
    let core_count = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);

    let cmd = std::fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| {
            b.split(|&c| c == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    let environ_count = std::fs::read(format!("/proc/{pid}/environ"))
        .map(|b| b.split(|&c| c == 0).filter(|s| !s.is_empty()).count())
        .unwrap_or(0);

    // nice value is field 19 (0-indexed 18) of /proc/pid/stat, after the comm paren.
    let priority = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            let after = stat.rsplit_once(')').map(|(_, r)| r.to_string())?;
            let fields: Vec<&str> = after.split_whitespace().collect();
            // after the ')' the next field is state (index 0 here) = stat field 3.
            // nice is stat field 19 → index 19-3 = 16.
            fields.get(16).and_then(|n| n.parse::<i32>().ok())
        })
        .map(|nice| format!("nice {nice}"));

    let affinity_mask = unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(
            pid as libc::pid_t,
            std::mem::size_of::<libc::cpu_set_t>(),
            &mut set,
        ) == 0
        {
            let mut mask = 0u64;
            for i in 0..64 {
                if libc::CPU_ISSET(i, &set) {
                    mask |= 1u64 << i;
                }
            }
            Some(mask)
        } else {
            None
        }
    };

    Ok(ProcessDetails {
        pid,
        cmd,
        cwd,
        priority,
        efficiency_mode: None,
        affinity_mask,
        core_count,
        environ_count,
    })
}

pub fn restart_as_admin() -> Result<(), ActionError> {
    let exe = std::env::current_exe().map_err(|e| ActionError::Os(e.to_string()))?;
    // pkexec provides a graphical sudo prompt on most desktops.
    match std::process::Command::new("pkexec").arg(&exe).spawn() {
        Ok(_) => {
            std::process::exit(0);
        }
        Err(e) => Err(ActionError::Unsupported(format!(
            "could not elevate via pkexec ({e}); relaunch with sudo manually"
        ))),
    }
}

pub fn trim_ram(pid: u32) -> Result<(), ActionError> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ActionError::NotFound
        } else {
            ActionError::PermissionDenied(e.to_string())
        }
    })?;

    let mut regions: Vec<libc::iovec> = Vec::new();
    for line in maps.lines() {
        let mut parts = line.split_whitespace();
        let (Some(range), Some(perms)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !perms.starts_with("rw") || !perms.ends_with('p') {
            continue;
        }
        let Some((start, end)) = range.split_once('-') else {
            continue;
        };
        let (Ok(start), Ok(end)) = (u64::from_str_radix(start, 16), u64::from_str_radix(end, 16))
        else {
            continue;
        };
        regions.push(libc::iovec {
            iov_base: start as *mut libc::c_void,
            iov_len: (end - start) as usize,
        });
    }

    if regions.is_empty() {
        return Ok(());
    }

    unsafe {
        let pidfd = libc::syscall(libc::SYS_pidfd_open, pid, 0u32);
        if pidfd < 0 {
            return Err(last_errno());
        }
        let pidfd = pidfd as libc::c_int;
        for chunk in regions.chunks(1024) {
            let ret = libc::syscall(
                libc::SYS_process_madvise,
                pidfd,
                chunk.as_ptr(),
                chunk.len(),
                libc::MADV_PAGEOUT,
                0u32,
            );
            if ret < 0 {
                let err = std::io::Error::last_os_error();
                libc::close(pidfd);
                return Err(match err.raw_os_error() {
                    Some(libc::EPERM) => ActionError::PermissionDenied(
                        "RAM trim needs CAP_SYS_NICE — run with sudo or: sudo setcap cap_sys_nice+ep <binary>".into(),
                    ),
                    Some(libc::ENOSYS) => {
                        ActionError::Unsupported("kernel too old for process_madvise (needs >= 5.10)".into())
                    }
                    Some(libc::ESRCH) => ActionError::NotFound,
                    _ => ActionError::Os(err.to_string()),
                });
            }
        }
        libc::close(pidfd);
    }
    Ok(())
}

pub fn suspend(pid: u32) -> Result<(), ActionError> {
    nix_kill(Pid::from_raw(pid as i32), Signal::SIGSTOP).map_err(map_nix)
}

pub fn resume(pid: u32) -> Result<(), ActionError> {
    nix_kill(Pid::from_raw(pid as i32), Signal::SIGCONT).map_err(map_nix)
}

pub fn kill(pid: u32, force: bool) -> Result<(), ActionError> {
    let sig = if force { Signal::SIGKILL } else { Signal::SIGTERM };
    nix_kill(Pid::from_raw(pid as i32), sig).map_err(map_nix)
}

pub fn is_elevated() -> bool {
    unsafe { libc::geteuid() == 0 }
}

pub fn init_privileges() {}
