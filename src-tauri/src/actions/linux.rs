#![cfg(target_os = "linux")]

use super::ActionError;

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

pub fn set_priority_low(pid: u32) -> Result<(), ActionError> {
    // Raising nice lowers priority; allowed unprivileged on own processes.
    // Note: cannot be lowered back without root (documented in the UI).
    unsafe {
        // setpriority returns -1 on error, but -1 is also a valid prior return; clear errno first.
        *libc::__errno_location() = 0;
        let ret = libc::setpriority(libc::PRIO_PROCESS, pid, 10);
        if ret == -1 && *libc::__errno_location() != 0 {
            return Err(last_errno());
        }
    }
    Ok(())
}

/// Best-effort RAM trim via process_madvise(MADV_PAGEOUT).
/// Requires CAP_SYS_NICE (root or `setcap cap_sys_nice+ep`), kernel >= 5.10.
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
        // Only private writable mappings (heap/anon memory) are worth paging out.
        if !perms.starts_with("rw") || !perms.ends_with('p') {
            continue;
        }
        let Some((start, end)) = range.split_once('-') else {
            continue;
        };
        let (Ok(start), Ok(end)) = (
            u64::from_str_radix(start, 16),
            u64::from_str_radix(end, 16),
        ) else {
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
        // IOV_MAX per call.
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
