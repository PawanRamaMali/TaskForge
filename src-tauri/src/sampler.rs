use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use sysinfo::{
    Components, Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users,
};
use tauri::{AppHandle, Emitter};

use crate::actions;
use crate::classify::{self, ProcFacts};
use crate::gpu::GpuMonitor;
use crate::model::*;
use crate::rules::RuleStore;

const TICK: Duration = Duration::from_millis(1500);
/// EMA smoothing factor for sustained-CPU detection used by Optimize All.
const EMA_ALPHA: f32 = 0.4;

#[derive(Default)]
pub struct SharedState {
    pub latest: Mutex<Option<Snapshot>>,
    pub suspended: Mutex<HashSet<u32>>,
    pub cpu_ema: Mutex<HashMap<u32, f32>>,
    pub rules: RuleStore,
    /// Pids we've already applied a persistent rule to (so we don't reassert every tick).
    pub rules_applied: Mutex<HashSet<u32>>,
    /// Cached (name, category, safe_to_kill) per pid. Classification is stable for
    /// the life of a process, so we classify once instead of every tick; the name
    /// is stored so an exec that renames the process triggers a reclassification.
    pub class_cache: Mutex<HashMap<u32, (String, Category, bool)>>,
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
    let mut networks = Networks::new_with_refreshed_list();
    let mut components = Components::new_with_refreshed_list();
    let users = Users::new_with_refreshed_list();
    let mut gpu = GpuMonitor::new();
    let elevated = actions::is_elevated();
    let own_pid = std::process::id();

    // First refresh yields meaningless CPU deltas - warm up and discard.
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
        networks.refresh(true);
        components.refresh(true);

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
        let mut applied = state.rules_applied.lock();
        let mut class_cache = state.class_cache.lock();
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
            let (category, safe_to_kill) = match class_cache.get(&pid_u32) {
                Some((cached_name, cat, safe)) if *cached_name == name => (*cat, *safe),
                _ => {
                    let r = classify::classify(&facts);
                    class_cache.insert(pid_u32, (name.clone(), r.0, r.1));
                    r
                }
            };

            // Apply a persistent priority rule the first time we see this pid.
            if !applied.contains(&pid_u32) && category != Category::Critical {
                if let Some(rule) = state.rules.match_exe(&name) {
                    let _ = actions::set_priority(pid_u32, rule.priority);
                    if rule.efficiency_mode {
                        let _ = actions::set_efficiency_mode(pid_u32, true);
                    }
                    applied.insert(pid_u32);
                }
            }

            let cpu_raw = p.cpu_usage();
            let cpu_norm = cpu_raw / core_count as f32;
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
                // Protected processes report start_time 0 on Windows; avoid a bogus
                // "runs since 1970" uptime by zeroing run_time when start is unknown.
                run_time: if p.start_time() == 0 { 0 } else { p.run_time() },
                start_time: p.start_time(),
                status: p.status().to_string(),
            });
        }
        ema.retain(|pid, _| live_pids.contains(pid));
        applied.retain(|pid| live_pids.contains(pid));
        class_cache.retain(|pid, _| live_pids.contains(pid));
        drop(ema);
        drop(applied);
        drop(class_cache);

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

        let mut net = NetInfo::default();
        for (_name, data) in networks.iter() {
            net.rx_bps += (data.received() as f64 / interval_secs) as u64;
            net.tx_bps += (data.transmitted() as f64 / interval_secs) as u64;
            net.total_rx += data.total_received();
            net.total_tx += data.total_transmitted();
        }

        let temps: Vec<TempInfo> = components
            .iter()
            .filter_map(|c| {
                c.temperature().map(|t| TempInfo {
                    label: c.label().to_string(),
                    temp_c: t,
                    max_c: c.max(),
                })
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
            net,
            temps,
            gpu: gpu_info,
            processes,
            elevated,
        };

        // Keep the tray tooltip live.
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(format!(
                "TaskForge - CPU {:.0}%  ·  RAM {:.0}%",
                snapshot.cpu.overall,
                snapshot.mem.used as f64 / snapshot.mem.total.max(1) as f64 * 100.0
            )));
        }

        let _ = app.emit("snapshot", &snapshot);
        *state.latest.lock() = Some(snapshot);

        let elapsed = t0.elapsed();
        if elapsed < TICK {
            std::thread::sleep(TICK - elapsed);
        }
    }
}
