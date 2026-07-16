pub mod actions;
pub mod classify;
mod gpu;
pub mod model;
mod optimize;
mod sampler;

use std::sync::Arc;

use actions::ActionError;
use classify::action_allowed;
use model::{ActionResult, OptimizeKind, PlannedAction};
use sampler::SharedState;
use tauri::{Manager, State};

#[tauri::command]
fn process_action(
    pid: u32,
    action: String,
    state: State<'_, Arc<SharedState>>,
) -> Result<(), ActionError> {
    let own_pid = std::process::id();
    if pid == own_pid {
        return Err(ActionError::Blocked("blocked: cannot act on TaskForge itself".into()));
    }

    let snapshot = state.latest.lock().clone();
    let Some(proc_info) = snapshot
        .as_ref()
        .and_then(|s| s.processes.iter().find(|p| p.pid == pid))
    else {
        return Err(ActionError::NotFound);
    };
    if snapshot
        .as_ref()
        .and_then(|s| s.processes.iter().find(|p| p.pid == own_pid))
        .and_then(|p| p.parent_pid)
        == Some(pid)
    {
        return Err(ActionError::Blocked("blocked: cannot act on TaskForge's parent".into()));
    }

    let kind = match action.as_str() {
        "lower_priority" => OptimizeKind::LowerPriority,
        "trim_ram" => OptimizeKind::TrimRam,
        "suspend" => OptimizeKind::Suspend,
        "resume" => {
            // Resume is the undo of suspend — same permission class, always safe-ward.
            action_allowed(proc_info.category, OptimizeKind::Suspend).map_err(ActionError::Blocked)?;
            actions::resume(pid)?;
            state.suspended.lock().remove(&pid);
            return Ok(());
        }
        "kill" | "force_kill" => {
            action_allowed(proc_info.category, OptimizeKind::Kill).map_err(ActionError::Blocked)?;
            actions::kill(pid, action == "force_kill")?;
            state.suspended.lock().remove(&pid);
            return Ok(());
        }
        other => return Err(ActionError::Unsupported(format!("unknown action '{other}'"))),
    };

    action_allowed(proc_info.category, kind).map_err(ActionError::Blocked)?;
    match kind {
        OptimizeKind::LowerPriority => actions::set_priority_low(pid),
        OptimizeKind::TrimRam => actions::trim_ram(pid),
        OptimizeKind::Suspend => {
            actions::suspend(pid)?;
            state.suspended.lock().insert(pid);
            Ok(())
        }
        OptimizeKind::Kill => unreachable!(),
    }
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(SharedState::default()))
        .setup(|app| {
            actions::init_privileges();
            let state = app.state::<Arc<SharedState>>().inner().clone();
            sampler::spawn(app.handle().clone(), state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            process_action,
            plan_optimize,
            apply_optimize
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
