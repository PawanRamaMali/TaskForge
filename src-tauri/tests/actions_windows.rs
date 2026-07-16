//! Windows integration tests for process actions.
//! Spawns a real child process and exercises priority/suspend/resume/kill.
#![cfg(windows)]

use std::process::Command;
use std::time::Duration;

use task_manager_lib::actions;

fn spawn_victim() -> std::process::Child {
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", "Start-Sleep 300"])
        .spawn()
        .expect("failed to spawn test process")
}

#[test]
fn priority_suspend_resume_trim_kill_roundtrip() {
    let mut child = spawn_victim();
    let pid = child.id();
    std::thread::sleep(Duration::from_millis(500));

    actions::set_priority_low(pid).expect("set_priority_low failed");
    // Confirmed externally: (Get-Process -Id pid).PriorityClass -eq BelowNormal
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("(Get-Process -Id {pid}).PriorityClass"),
        ])
        .output()
        .expect("failed to query priority");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("BelowNormal"),
        "priority was not lowered: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    actions::suspend(pid).expect("suspend failed");
    actions::resume(pid).expect("resume failed");
    actions::trim_ram(pid).expect("trim_ram failed");

    actions::kill(pid, false).expect("kill failed");
    std::thread::sleep(Duration::from_millis(500));
    match child.try_wait() {
        Ok(Some(_)) => {} // exited as expected
        other => panic!("process still alive after kill: {other:?}"),
    }
}

#[test]
fn kill_nonexistent_pid_reports_not_found() {
    // Pid 4_000_000 is far beyond any realistic pid space.
    let err = actions::kill(4_000_000, false).expect_err("kill of bogus pid should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("denied"),
        "unexpected error for bogus pid: {msg}"
    );
}
