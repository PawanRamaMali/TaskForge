use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use sysinfo::{Disks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};
use tauri::{AppHandle, Emitter};

use crate::actions;
use crate::classify::{self, ProcFacts};
use crate::gpu::GpuMonitor;
use crate::model::*;

const TICK: Duration = Duration::from_millis(1500);
/// EMA smoothing factor for sustained-CPU detection used by Optimize All.
const EMA_ALPHA: f32 = 0.4;

#[derive(Default)]
pub struct SharedState {
    pub latest: Mutex<Option<Snapshot>>,
    pub suspended: Mutex<HashSet<u32>>,
    pub cpu_ema: Mutex<HashMap<u32, f32>>,
}

pub fn spawn(app: AppHandle, state: Arc<SharedState>) {
    std::thread::Builder::new()
        .name("sampler".into())
        .spawn(move || run(app, state))
        .expect("failed to spawn sampler thread");
}

fn proc_refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_cmd(UpdateKind::OnlyIfNotSet)
}

fn run(app: AppHandle, state: Arc<SharedState>) {
    let mut sys = System::new();
    let mut disks = Disks::new_with_refreshed_list();
    let users = Users::new_with_refreshed_list();
    let mut gpu = GpuMonitor::new();
    let elevated = actions::is_elevated();
    let own_pid = std::process::id();

    // First refresh yields meaningless CPU deltas — warm up and discard.
    sys.refresh_cpu_usage();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_refresh_kind());
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250)));

    let mut last_tick = Instant::now();
    loop {
        let t0 = Instant::now();
        let interval_secs = last_tick.elapsed().as_secs_f64().max(0.001);
        last_tick = t0;

        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_refresh_kind());
        disks.refresh(true);

        let (gpu_info, gpu_procs) = gpu.sample();

        let core_count = sys.cpus().len().max(1);
        let current_session_id = sys
            .process(sysinfo::Pid::from_u32(own_pid))
            .and_then(|p| p.session_id())
            .map(|s| s.as_u32());

        let mut suspended = state.suspended.lock();
        suspended.retain(|pid| sys.process(sysinfo::Pid::from_u32(*pid)).is_some());
        let suspended_now: HashSet<u32> = suspended.clone();
        drop(suspended);

        let mut processes: Vec<ProcInfo> = Vec::with_capacity(sys.processes().len());
        let mut ema = state.cpu_ema.lock();
        let mut live_pids: HashSet<u32> = HashSet::with_capacity(sys.processes().len());

        for (pid, p) in sys.processes() {
            let pid_u32 = pid.as_u32();
            live_pids.insert(pid_u32);

            let name = p.name().to_string_lossy().to_string();
            let exe = p.exe().map(|e| e.to_string_lossy().to_string());
            let user = p
                .user_id()
                .and_then(|uid| users.get_user_by_id(uid))
                .map(|u| u.name().to_string());

            #[cfg(target_os = "linux")]
            let uid_under_1000 = p.user_id().map(|u| **u < 1000).unwrap_or(false);
            #[cfg(not(target_os = "linux"))]
            let uid_under_1000 = false;

            let facts = ProcFacts {
                pid: pid_u32,
                parent_pid: p.parent().map(|pp| pp.as_u32()),
                name: &name,
                exe: exe.as_deref(),
                user: user.as_deref(),
                uid_under_1000,
                cmd_empty: p.cmd().is_empty(),
                session_id: p.session_id().map(|s| s.as_u32()),
                current_session_id,
            };
            let (category, safe_to_kill) = classify::classify(&facts);

            let cpu_raw = p.cpu_usage();
            let cpu_norm = cpu_raw / core_count as f32;
            // EMA tracks raw (per-core) CPU so a process pinning one core registers
            // as a hog regardless of how many cores the machine has.
            let e = ema.entry(pid_u32).or_insert(cpu_raw);
            *e = *e * (1.0 - EMA_ALPHA) + cpu_raw * EMA_ALPHA;

            let du = p.disk_usage();
            let gpu_stats = gpu_procs.get(&pid_u32);

            #[cfg(target_os = "linux")]
            let os_stopped = matches!(p.status(), sysinfo::ProcessStatus::Stop);
            #[cfg(not(target_os = "linux"))]
            let os_stopped = false;

            processes.push(ProcInfo {
                pid: pid_u32,
                parent_pid: facts.parent_pid,
                name,
                exe,
                user,
                cpu: cpu_norm,
                cpu_raw,
                mem_bytes: p.memory(),
                disk_read_bps: (du.read_bytes as f64 / interval_secs) as u64,
                disk_write_bps: (du.written_bytes as f64 / interval_secs) as u64,
                gpu_util: gpu_stats.and_then(|g| g.util),
                gpu_mem_bytes: gpu_stats.and_then(|g| g.mem_bytes),
                category,
                safe_to_kill,
                suspended: suspended_now.contains(&pid_u32) || os_stopped,
            });
        }
        ema.retain(|pid, _| live_pids.contains(pid));
        drop(ema);

        let disks_info: Vec<DiskInfo> = disks
            .iter()
            .map(|d| {
                let du = d.usage();
                DiskInfo {
                    name: d.name().to_string_lossy().to_string(),
                    mount: d.mount_point().to_string_lossy().to_string(),
                    fs: d.file_system().to_string_lossy().to_string(),
                    total: d.total_space(),
                    available: d.available_space(),
                    read_bps: (du.read_bytes as f64 / interval_secs) as u64,
                    write_bps: (du.written_bytes as f64 / interval_secs) as u64,
                }
            })
            .collect();

        let snapshot = Snapshot {
            ts: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            cpu: CpuInfo {
                overall: sys.global_cpu_usage(),
                per_core: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
                core_count,
                brand: sys
                    .cpus()
                    .first()
                    .map(|c| c.brand().trim().to_string())
                    .unwrap_or_default(),
            },
            mem: MemInfo {
                total: sys.total_memory(),
                used: sys.used_memory(),
                swap_total: sys.total_swap(),
                swap_used: sys.used_swap(),
            },
            disks: disks_info,
            gpu: gpu_info,
            processes,
            elevated,
        };

        let _ = app.emit("snapshot", &snapshot);
        *state.latest.lock() = Some(snapshot);

        let elapsed = t0.elapsed();
        if elapsed < TICK {
            std::thread::sleep(TICK - elapsed);
        }
    }
}
