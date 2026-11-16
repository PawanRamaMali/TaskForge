use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Critical,
    SystemService,
    Background,
    UserApp,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SafeKillAction {
    Kill,
    SuspendOnly,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    pub overall: f32,
    pub per_core: Vec<f32>,
    pub core_count: usize,
    pub brand: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MemInfo {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub fs: String,
    pub total: u64,
    pub available: u64,
    pub read_bps: u64,
    pub write_bps: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    /// Stable per-adapter key (LUID on Windows, card node on Linux). The UI
    /// uses it to keep each GPU's sparkline attached to the same adapter
    /// across ticks, so it must not change while the app runs.
    pub id: String,
    pub available: bool,
    pub reason: Option<String>,
    pub name: Option<String>,
    /// True for a discrete adapter, false for an integrated one, None when
    /// the platform layer cannot tell.
    pub discrete: Option<bool>,
    pub utilization: Option<u32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub temperature_c: Option<u32>,
}

impl GpuInfo {
    pub fn unavailable(reason: String) -> Self {
        GpuInfo {
            id: "none".to_string(),
            available: false,
            reason: Some(reason),
            name: None,
            discrete: None,
            utilization: None,
            mem_used: None,
            mem_total: None,
            temperature_c: None,
        }
    }
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetInfo {
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub total_rx: u64,
    pub total_tx: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TempInfo {
    pub label: String,
    pub temp_c: f32,
    pub max_c: Option<f32>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub exe: Option<String>,
    pub user: Option<String>,
    /// Normalized CPU % (0-100 across all cores).
    pub cpu: f32,
    /// Raw CPU % (% of a single core, can exceed 100).
    pub cpu_raw: f32,
    pub mem_bytes: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub gpu_util: Option<u32>,
    pub gpu_mem_bytes: Option<u64>,
    pub category: Category,
    pub safe_to_kill: bool,
    pub suspended: bool,
    /// Seconds the process has been running.
    pub run_time: u64,
    /// Unix epoch seconds when the process started.
    pub start_time: u64,
    pub status: String,
}

/// One tick of the values the dashboard sparklines plot.
///
/// The sampler keeps the last `HISTORY_LEN` of these so a window opened later -
/// the mini view, or the main window restored from the tray - starts with a
/// filled graph instead of 90 seconds of empty space. Each webview is its own
/// JavaScript context, so in-page history cannot be shared between them.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistorySample {
    pub cpu: f64,
    pub mem_pct: f64,
    /// Busiest adapter, matching `Snapshot::gpu`.
    pub gpu: u32,
    /// Per-adapter utilization, keyed by `GpuInfo::id`.
    pub gpus: HashMap<String, u32>,
    pub disk_mbps: f64,
    pub net_kbps: f64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub ts: u64,
    pub cpu: CpuInfo,
    pub mem: MemInfo,
    pub disks: Vec<DiskInfo>,
    pub net: NetInfo,
    pub temps: Vec<TempInfo>,
    /// The busiest adapter, kept for the mini view and the tray tooltip.
    /// `gpus` keeps a stable order instead, so this is not always `gpus[0]`.
    pub gpu: GpuInfo,
    /// Every adapter the platform can see, busiest first.
    pub gpus: Vec<GpuInfo>,
    pub processes: Vec<ProcInfo>,
    pub elevated: bool,
}

/// Windows priority classes / Linux nice buckets, mapped in the actions layer.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PriorityLevel {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
}

/// On-demand details for the process inspector panel (extra syscalls, not in the tick).
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProcessDetails {
    pub pid: u32,
    pub cmd: Vec<String>,
    pub cwd: Option<String>,
    pub priority: Option<String>,
    pub efficiency_mode: Option<bool>,
    /// Bitmask of cores the process may run on.
    pub affinity_mask: Option<u64>,
    pub core_count: u32,
    pub environ_count: usize,
}

/// A persistent "always run this executable at priority X" rule (ProBalance-lite).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PriorityRule {
    /// Executable base name, lowercased (e.g. "chrome.exe").
    pub exe: String,
    pub priority: PriorityLevel,
    #[serde(default)]
    pub efficiency_mode: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OptimizeKind {
    Kill,
    Suspend,
    LowerPriority,
    TrimRam,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlannedAction {
    pub pid: u32,
    pub name: String,
    pub category: Category,
    pub kind: OptimizeKind,
    pub reason: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub pid: u32,
    pub name: String,
    pub kind: OptimizeKind,
    pub ok: bool,
    pub error: Option<String>,
}
