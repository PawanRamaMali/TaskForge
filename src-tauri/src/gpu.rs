use std::collections::HashMap;

use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::Nvml;

use crate::model::GpuInfo;

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcGpu {
    pub util: Option<u32>,
    pub mem_bytes: Option<u64>,
}

pub struct GpuMonitor {
    nvml: Option<Nvml>,
    init_error: Option<String>,
    last_sample_ts: Option<u64>,
}

impl GpuMonitor {
    pub fn new() -> Self {
        match Nvml::init() {
            Ok(nvml) => GpuMonitor {
                nvml: Some(nvml),
                init_error: None,
                last_sample_ts: None,
            },
            Err(e) => GpuMonitor {
                nvml: None,
                init_error: Some(format!("NVML unavailable: {e}")),
                last_sample_ts: None,
            },
        }
    }

    /// Returns overall GPU info plus a pid -> (util, vram) map for this tick.
    pub fn sample(&mut self) -> (GpuInfo, HashMap<u32, ProcGpu>) {
        let Some(nvml) = &self.nvml else {
            return (
                self.fallback_overall()
                    .unwrap_or_else(|| GpuInfo::unavailable(self.init_error.clone().unwrap_or_default())),
                HashMap::new(),
            );
        };

        let device = match nvml.device_by_index(0) {
            Ok(d) => d,
            Err(e) => {
                return (
                    GpuInfo::unavailable(format!("NVML device error: {e}")),
                    HashMap::new(),
                )
            }
        };

        let name = device.name().ok();
        let util = device.utilization_rates().ok().map(|u| u.gpu);
        let mem = device.memory_info().ok();
        let temp = device.temperature(TemperatureSensor::Gpu).ok();

        let mut per_proc: HashMap<u32, ProcGpu> = HashMap::new();

        // Per-process utilization since the last poll. NotFound just means the GPU was idle.
        match device.process_utilization_stats(self.last_sample_ts) {
            Ok(samples) => {
                let mut counts: HashMap<u32, (u64, u32)> = HashMap::new();
                for s in &samples {
                    if s.timestamp > self.last_sample_ts.unwrap_or(0) {
                        self.last_sample_ts = Some(s.timestamp);
                    }
                    let entry = counts.entry(s.pid).or_insert((0, 0));
                    entry.0 += s.sm_util as u64;
                    entry.1 += 1;
                }
                for (pid, (sum, n)) in counts {
                    per_proc.entry(pid).or_default().util = Some((sum / n.max(1) as u64) as u32);
                }
            }
            Err(NvmlError::NotFound) => {}
            Err(_) => {}
        }

        let mut procs = Vec::new();
        if let Ok(g) = device.running_graphics_processes() {
            procs.extend(g);
        }
        if let Ok(c) = device.running_compute_processes() {
            procs.extend(c);
        }
        for p in procs {
            let entry = per_proc.entry(p.pid).or_default();
            if let UsedGpuMemory::Used(bytes) = p.used_gpu_memory {
                entry.mem_bytes = Some(entry.mem_bytes.unwrap_or(0).max(bytes));
            }
            entry.util.get_or_insert(0);
        }

        (
            GpuInfo {
                available: true,
                reason: None,
                name,
                utilization: util,
                mem_used: mem.as_ref().map(|m| m.used),
                mem_total: mem.as_ref().map(|m| m.total),
                temperature_c: temp,
            },
            per_proc,
        )
    }

    /// Linux non-NVIDIA: some drivers (amdgpu, i915) expose an overall busy percentage.
    #[cfg(target_os = "linux")]
    fn fallback_overall(&self) -> Option<GpuInfo> {
        for i in 0..4 {
            let path = format!("/sys/class/drm/card{i}/device/gpu_busy_percent");
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Ok(pct) = s.trim().parse::<u32>() {
                    return Some(GpuInfo {
                        available: true,
                        reason: None,
                        name: Some(format!("GPU (card{i})")),
                        utilization: Some(pct),
                        mem_used: None,
                        mem_total: None,
                        temperature_c: None,
                    });
                }
            }
        }
        None
    }

    #[cfg(not(target_os = "linux"))]
    fn fallback_overall(&self) -> Option<GpuInfo> {
        None
    }
}
