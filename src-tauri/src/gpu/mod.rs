//! GPU telemetry across every adapter in the machine.
//!
//! Two sources are combined. The platform layer (`windows.rs` / `linux.rs`)
//! enumerates *all* adapters regardless of vendor, which is what makes an
//! Intel or AMD integrated GPU visible. NVML then layers richer data -
//! temperature, accurate utilization, per-process VRAM - onto the NVIDIA
//! cards it recognises. A machine with an NVIDIA dGPU and an Intel iGPU, the
//! common laptop setup, ends up with one entry each.

#[cfg(windows)]
#[path = "windows.rs"]
mod imp;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod imp;

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

/// One adapter as reported by the platform layer, before NVML is merged in.
#[derive(Clone, Debug)]
pub struct Adapter {
    pub id: String,
    pub name: String,
    pub vendor_id: Option<u32>,
    pub discrete: Option<bool>,
    pub utilization: Option<u32>,
    pub mem_used: Option<u64>,
    pub mem_total: Option<u64>,
    pub temperature_c: Option<u32>,
}

const VENDOR_NVIDIA: u32 = 0x10DE;

pub struct GpuMonitor {
    nvml: Option<Nvml>,
    init_error: Option<String>,
    platform: imp::Platform,
    /// Last per-process sample timestamp, per NVML device index. Shared state
    /// would make one busy GPU starve another's process list.
    last_sample_ts: HashMap<u32, u64>,
}

impl GpuMonitor {
    pub fn new() -> Self {
        let (nvml, init_error) = match Nvml::init() {
            Ok(nvml) => (Some(nvml), None),
            Err(e) => (None, Some(format!("NVML unavailable: {e}"))),
        };
        GpuMonitor {
            nvml,
            init_error,
            platform: imp::Platform::new(),
            last_sample_ts: HashMap::new(),
        }
    }

    /// Returns every visible adapter plus a pid -> (util, vram) map for this
    /// tick. The adapter order is stable across ticks so the UI can keep each
    /// GPU's sparkline attached to the same card.
    pub fn sample(&mut self) -> (Vec<GpuInfo>, HashMap<u32, ProcGpu>) {
        let (mut adapters, mut per_proc) = self.platform.sample();
        let nvidia = self.sample_nvml(&mut per_proc);

        // Merge each NVML device into the matching platform adapter, or append
        // it when the platform layer never saw it (NVIDIA on Linux, or a
        // headless compute card with no DXGI presence).
        // Two identical cards in one box share a marketing name, so remember
        // which adapters are already claimed and never match one twice.
        let mut claimed: Vec<usize> = Vec::new();
        for dev in nvidia {
            match match_adapter(&adapters, &claimed, &dev) {
                Some(idx) => {
                    claimed.push(idx);
                    let a = &mut adapters[idx];
                    // NVML is the better source where it has an answer.
                    a.utilization = dev.utilization.or(a.utilization);
                    a.mem_used = dev.mem_used.or(a.mem_used);
                    a.mem_total = dev.mem_total.or(a.mem_total);
                    a.temperature_c = dev.temperature_c.or(a.temperature_c);
                    if let Some(name) = dev.name.clone() {
                        a.name = name;
                    }
                }
                None => adapters.push(Adapter {
                    id: dev.id.clone(),
                    name: dev.name.clone().unwrap_or_else(|| "NVIDIA GPU".to_string()),
                    vendor_id: Some(VENDOR_NVIDIA),
                    discrete: Some(true),
                    utilization: dev.utilization,
                    mem_used: dev.mem_used,
                    mem_total: dev.mem_total,
                    temperature_c: dev.temperature_c,
                }),
            }
        }

        if adapters.is_empty() {
            let reason = self
                .platform
                .error()
                .or_else(|| self.init_error.clone())
                .unwrap_or_else(|| "No GPU telemetry available".to_string());
            return (vec![GpuInfo::unavailable(reason)], per_proc);
        }

        // Stable order: discrete cards first, then by id. Sorting by load
        // instead would make the cards swap places on every tick.
        adapters.sort_by(|a, b| {
            b.discrete
                .unwrap_or(false)
                .cmp(&a.discrete.unwrap_or(false))
                .then_with(|| a.id.cmp(&b.id))
        });

        let out = adapters
            .into_iter()
            .map(|a| GpuInfo {
                id: a.id,
                available: true,
                reason: None,
                name: Some(a.name),
                discrete: a.discrete,
                utilization: a.utilization,
                mem_used: a.mem_used,
                mem_total: a.mem_total,
                temperature_c: a.temperature_c,
            })
            .collect();

        (out, per_proc)
    }

    /// Per-device NVML readings, merged into `per_proc` as a side effect.
    fn sample_nvml(&mut self, per_proc: &mut HashMap<u32, ProcGpu>) -> Vec<NvmlDevice> {
        let Some(nvml) = &self.nvml else {
            return Vec::new();
        };
        let Ok(count) = nvml.device_count() else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for index in 0..count {
            let Ok(device) = nvml.device_by_index(index) else {
                continue;
            };

            out.push(NvmlDevice {
                id: format!("nvml-{index}"),
                name: device.name().ok(),
                utilization: device.utilization_rates().ok().map(|u| u.gpu),
                mem_used: device.memory_info().ok().map(|m| m.used),
                mem_total: device.memory_info().ok().map(|m| m.total),
                temperature_c: device.temperature(TemperatureSensor::Gpu).ok(),
            });

            let last = self.last_sample_ts.get(&index).copied();

            // Per-process utilization since the last poll on *this* device.
            // NotFound just means the GPU was idle.
            match device.process_utilization_stats(last) {
                Ok(samples) => {
                    let mut counts: HashMap<u32, (u64, u32)> = HashMap::new();
                    for s in &samples {
                        if s.timestamp > last.unwrap_or(0) {
                            self.last_sample_ts.insert(index, s.timestamp);
                        }
                        let entry = counts.entry(s.pid).or_insert((0, 0));
                        entry.0 += s.sm_util as u64;
                        entry.1 += 1;
                    }
                    for (pid, (sum, n)) in counts {
                        let util = (sum / n.max(1) as u64) as u32;
                        let entry = per_proc.entry(pid).or_default();
                        // A process can be busy on more than one adapter.
                        entry.util = Some(entry.util.unwrap_or(0).max(util));
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
        }
        out
    }
}

struct NvmlDevice {
    id: String,
    name: Option<String>,
    utilization: Option<u32>,
    mem_used: Option<u64>,
    mem_total: Option<u64>,
    temperature_c: Option<u32>,
}

/// Pairs an NVML device with the platform adapter describing the same card.
/// DXGI and NVML normally agree on the marketing name, so match on that
/// first; fall back to the vendor id when exactly one NVIDIA adapter is left,
/// which covers the rare case where the two strings differ.
fn match_adapter(adapters: &[Adapter], claimed: &[usize], dev: &NvmlDevice) -> Option<usize> {
    let free = |i: &usize| !claimed.contains(i);

    if let Some(name) = dev.name.as_deref() {
        let name = name.trim().to_ascii_lowercase();
        if let Some(idx) = adapters
            .iter()
            .enumerate()
            .filter(|(i, _)| free(i))
            .find(|(_, a)| a.name.trim().to_ascii_lowercase() == name)
            .map(|(i, _)| i)
        {
            return Some(idx);
        }
    }
    let nvidia: Vec<usize> = adapters
        .iter()
        .enumerate()
        .filter(|(i, a)| free(i) && a.vendor_id == Some(VENDOR_NVIDIA))
        .map(|(i, _)| i)
        .collect();
    if nvidia.len() == 1 {
        return Some(nvidia[0]);
    }
    None
}
