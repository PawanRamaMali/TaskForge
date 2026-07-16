use crate::actions::{self, ActionError};
use crate::classify::{self, action_allowed};
use crate::model::*;
use crate::sampler::SharedState;

// CPU thresholds are raw per-core percentages (100 = one full core),
// so they behave the same on 4-core and 32-core machines.
const HOG_CPU_EMA: f32 = 50.0; // sustained half a core → lower priority
const ACTIVE_CPU_RAW: f32 = 5.0;
const ACTIVE_MEM: u64 = 100 * 1024 * 1024;
const BIG_MEM: u64 = 500 * 1024 * 1024;

pub fn plan(state: &SharedState) -> Vec<PlannedAction> {
    let Some(snapshot) = state.latest.lock().clone() else {
        return Vec::new();
    };
    let ema = state.cpu_ema.lock().clone();
    let own_pid = std::process::id();
    let own_parent = snapshot
        .processes
        .iter()
        .find(|p| p.pid == own_pid)
        .and_then(|p| p.parent_pid);

    let mut plan = Vec::new();
    for p in &snapshot.processes {
        if matches!(p.category, Category::Critical | Category::SystemService) {
            continue;
        }
        if p.pid == own_pid || Some(p.pid) == own_parent || p.suspended {
            continue;
        }

        let cpu_smoothed = ema.get(&p.pid).copied().unwrap_or(p.cpu_raw);
        if p.safe_to_kill && (cpu_smoothed > ACTIVE_CPU_RAW || p.mem_bytes > ACTIVE_MEM) {
            let (kind, verb) = match classify::safe_kill_action(&p.name) {
                Some(SafeKillAction::SuspendOnly) => (OptimizeKind::Suspend, "suspend"),
                _ => (OptimizeKind::Kill, "end"),
            };
            plan.push(PlannedAction {
                pid: p.pid,
                name: p.name.clone(),
                category: p.category,
                kind,
                reason: format!(
                    "non-essential helper using {:.0}% of a core / {} MB — safe to {verb}",
                    cpu_smoothed,
                    p.mem_bytes / (1024 * 1024)
                ),
            });
        } else if cpu_smoothed > HOG_CPU_EMA {
            plan.push(PlannedAction {
                pid: p.pid,
                name: p.name.clone(),
                category: p.category,
                kind: OptimizeKind::LowerPriority,
                reason: format!(
                    "sustained {:.0}% of a CPU core — lower priority to keep the system responsive",
                    cpu_smoothed
                ),
            });
        } else if p.category == Category::Background && p.mem_bytes > BIG_MEM {
            plan.push(PlannedAction {
                pid: p.pid,
                name: p.name.clone(),
                category: p.category,
                kind: OptimizeKind::TrimRam,
                reason: format!(
                    "background process holding {} MB — reclaim unused memory",
                    p.mem_bytes / (1024 * 1024)
                ),
            });
        }
    }
    plan.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    plan
}

pub fn apply(state: &SharedState, planned: Vec<PlannedAction>) -> Vec<ActionResult> {
    let snapshot = state.latest.lock().clone();
    let own_pid = std::process::id();

    planned
        .into_iter()
        .map(|a| {
            let outcome = execute_guarded(state, snapshot.as_ref(), own_pid, &a);
            ActionResult {
                pid: a.pid,
                name: a.name,
                kind: a.kind,
                ok: outcome.is_ok(),
                error: outcome.err().map(|e| e.to_string()),
            }
        })
        .collect()
}

fn execute_guarded(
    state: &SharedState,
    snapshot: Option<&Snapshot>,
    own_pid: u32,
    a: &PlannedAction,
) -> Result<(), ActionError> {
    // Re-validate against the live snapshot — never trust the frontend payload.
    let Some(current) = snapshot.and_then(|s| s.processes.iter().find(|p| p.pid == a.pid)) else {
        return Err(ActionError::NotFound);
    };
    if a.pid == own_pid {
        return Err(ActionError::Blocked("blocked: cannot act on TaskForge itself".into()));
    }
    action_allowed(current.category, a.kind).map_err(ActionError::Blocked)?;

    match a.kind {
        OptimizeKind::Kill => {
            actions::kill(a.pid, false)?;
            state.suspended.lock().remove(&a.pid);
            Ok(())
        }
        OptimizeKind::Suspend => {
            actions::suspend(a.pid)?;
            state.suspended.lock().insert(a.pid);
            Ok(())
        }
        OptimizeKind::LowerPriority => actions::set_priority_low(a.pid),
        OptimizeKind::TrimRam => actions::trim_ram(a.pid),
    }
}
