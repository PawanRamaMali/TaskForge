pub mod actions;
pub mod classify;
mod diagnostics;
mod gpu;
pub mod model;
mod optimize;
mod rules;
mod sampler;

use std::sync::Arc;

use actions::ActionError;
use classify::action_allowed;
use model::{
    ActionResult, OptimizeKind, PlannedAction, PriorityLevel, PriorityRule, ProcessDetails,
};
use sampler::SharedState;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, State};

/// Shared guard: refuse acting on TaskForge itself or its parent, and require the
/// process to still exist. Returns the category for the caller's policy check.
fn guard_target(
    state: &SharedState,
    pid: u32,
) -> Result<model::Category, ActionError> {
    let own_pid = std::process::id();
    if pid == own_pid {
        return Err(ActionError::Blocked("blocked: cannot act on TaskForge itself".into()));
    }
    let snapshot = state.latest.lock().clone();
    let snapshot = snapshot.ok_or(ActionError::NotFound)?;
    let proc_info = snapshot
        .processes
        .iter()
        .find(|p| p.pid == pid)
        .ok_or(ActionError::NotFound)?;
    let own_parent = snapshot
        .processes
        .iter()
        .find(|p| p.pid == own_pid)
        .and_then(|p| p.parent_pid);
    if own_parent == Some(pid) {
        return Err(ActionError::Blocked("blocked: cannot act on TaskForge's parent".into()));
    }
    Ok(proc_info.category)
}

#[tauri::command]
fn process_action(
    pid: u32,
    action: String,
    state: State<'_, Arc<SharedState>>,
) -> Result<(), ActionError> {
    let category = guard_target(&state, pid)?;

    match action.as_str() {
        "trim_ram" => {
            action_allowed(category, OptimizeKind::TrimRam).map_err(ActionError::Blocked)?;
            actions::trim_ram(pid)
        }
        "suspend" => {
            action_allowed(category, OptimizeKind::Suspend).map_err(ActionError::Blocked)?;
            actions::suspend(pid)?;
            state.suspended.lock().insert(pid);
            Ok(())
        }
        "resume" => {
            action_allowed(category, OptimizeKind::Suspend).map_err(ActionError::Blocked)?;
            actions::resume(pid)?;
            state.suspended.lock().remove(&pid);
            Ok(())
        }
        "kill" | "force_kill" => {
            action_allowed(category, OptimizeKind::Kill).map_err(ActionError::Blocked)?;
            actions::kill(pid, action == "force_kill")?;
            state.suspended.lock().remove(&pid);
            Ok(())
        }
        other => Err(ActionError::Unsupported(format!("unknown action '{other}'"))),
    }
}

#[tauri::command]
fn set_priority(
    pid: u32,
    level: PriorityLevel,
    state: State<'_, Arc<SharedState>>,
) -> Result<(), ActionError> {
    let category = guard_target(&state, pid)?;
    // Priority changes ride the same policy slot as LowerPriority (allowed for services).
    action_allowed(category, OptimizeKind::LowerPriority).map_err(ActionError::Blocked)?;
    actions::set_priority(pid, level)
}

#[tauri::command]
fn set_efficiency_mode(
    pid: u32,
    on: bool,
    state: State<'_, Arc<SharedState>>,
) -> Result<(), ActionError> {
    let category = guard_target(&state, pid)?;
    action_allowed(category, OptimizeKind::LowerPriority).map_err(ActionError::Blocked)?;
    actions::set_efficiency_mode(pid, on)
}

#[tauri::command]
fn set_affinity(
    pid: u32,
    mask: u64,
    state: State<'_, Arc<SharedState>>,
) -> Result<(), ActionError> {
    let category = guard_target(&state, pid)?;
    action_allowed(category, OptimizeKind::LowerPriority).map_err(ActionError::Blocked)?;
    actions::set_affinity(pid, mask)
}

/// Kill a process together with all its descendants (children first).
#[tauri::command]
fn kill_tree(pid: u32, state: State<'_, Arc<SharedState>>) -> Result<Vec<ActionResult>, ActionError> {
    let category = guard_target(&state, pid)?;
    action_allowed(category, OptimizeKind::Kill).map_err(ActionError::Blocked)?;

    let snapshot = state.latest.lock().clone().ok_or(ActionError::NotFound)?;
    let own_pid = std::process::id();

    // Collect descendants breadth-first, then terminate leaves before parents.
    let mut targets: Vec<u32> = vec![pid];
    let mut i = 0;
    while i < targets.len() {
        let parent = targets[i];
        for p in &snapshot.processes {
            if p.parent_pid == Some(parent) && !targets.contains(&p.pid) {
                targets.push(p.pid);
            }
        }
        i += 1;
    }
    targets.reverse();

    let mut results = Vec::new();
    for tpid in targets {
        let proc = snapshot.processes.iter().find(|p| p.pid == tpid);
        let name = proc.map(|p| p.name.clone()).unwrap_or_default();
        let outcome = (|| {
            if tpid == own_pid {
                return Err(ActionError::Blocked("skipped TaskForge itself".into()));
            }
            let cat = proc.map(|p| p.category).unwrap_or(model::Category::Background);
            action_allowed(cat, OptimizeKind::Kill).map_err(ActionError::Blocked)?;
            actions::kill(tpid, false)
        })();
        results.push(ActionResult {
            pid: tpid,
            name,
            kind: OptimizeKind::Kill,
            ok: outcome.is_ok(),
            error: outcome.err().map(|e| e.to_string()),
        });
    }
    Ok(results)
}

#[tauri::command]
fn get_process_details(pid: u32) -> Result<ProcessDetails, ActionError> {
    let mut details = actions::get_details(pid)?;
    // Fill command line / cwd / environ count from a one-off targeted refresh.
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_cwd(UpdateKind::Always)
            .with_environ(UpdateKind::Always),
    );
    if let Some(p) = sys.process(Pid::from_u32(pid)) {
        if details.cmd.is_empty() {
            details.cmd = p
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().to_string())
                .collect();
        }
        if details.cwd.is_none() {
            details.cwd = p.cwd().map(|c| c.to_string_lossy().to_string());
        }
        if details.environ_count == 0 {
            details.environ_count = p.environ().len();
        }
    }
    Ok(details)
}

#[tauri::command]
fn restart_as_admin() -> Result<(), ActionError> {
    actions::restart_as_admin()
}

#[tauri::command]
fn list_rules(state: State<'_, Arc<SharedState>>) -> Vec<PriorityRule> {
    state.rules.list()
}

#[tauri::command]
fn set_rule(
    exe: String,
    priority: PriorityLevel,
    efficiency_mode: bool,
    state: State<'_, Arc<SharedState>>,
) {
    state.rules.upsert(PriorityRule {
        exe,
        priority,
        efficiency_mode,
    });
    // Clear the applied set so the rule re-applies to already-running matches next tick.
    state.rules_applied.lock().clear();
}

#[tauri::command]
fn remove_rule(exe: String, state: State<'_, Arc<SharedState>>) {
    state.rules.remove(&exe);
}

#[tauri::command]
fn plan_optimize(state: State<'_, Arc<SharedState>>) -> Vec<PlannedAction> {
    optimize::plan(&state)
}

#[tauri::command]
fn apply_optimize(
    actions: Vec<PlannedAction>,
    state: State<'_, Arc<SharedState>>,
) -> Vec<ActionResult> {
    optimize::apply(&state, actions)
}

/// Gather crash, hang and hardware-error history. Runs on a blocking worker because
/// event log queries take several seconds and would otherwise freeze the UI.
#[tauri::command]
async fn run_diagnostics(window_days: Option<u32>) -> Result<serde_json::Value, String> {
    let days = window_days.unwrap_or(60).clamp(1, 365);
    tauri::async_runtime::spawn_blocking(move || diagnostics::collect(days))
        .await
        .map_err(|e| format!("diagnostics task failed: {e}"))?
}

/// Write a rendered stability report to the user's Documents folder; returns the path.
#[tauri::command]
fn save_diagnostics_report(app: tauri::AppHandle, content: String) -> Result<String, String> {
    let dir = app
        .path()
        .document_dir()
        .or_else(|_| app.path().temp_dir())
        .map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("TaskForge-stability-report-{stamp}.md"));
    std::fs::write(&path, content).map_err(|e| format!("could not save report: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(SharedState::default()))
        .setup(|app| {
            actions::init_privileges();
            let state = app.state::<Arc<SharedState>>().inner().clone();

            // Load persistent priority rules from the app config dir.
            if let Ok(dir) = app.path().app_config_dir() {
                state.rules.load_from(dir.join("priority-rules.json"));
            }

            build_tray(app)?;

            sampler::spawn(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            process_action,
            set_priority,
            set_efficiency_mode,
            set_affinity,
            kill_tree,
            get_process_details,
            restart_as_admin,
            list_rules,
            set_rule,
            remove_rule,
            plan_optimize,
            apply_optimize,
            run_diagnostics,
            save_diagnostics_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show TaskForge", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("TaskForge")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(win) = tray.app_handle().get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
