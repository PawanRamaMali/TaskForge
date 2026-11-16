//! Linux GPU telemetry from sysfs.
//!
//! amdgpu and i915/xe expose a busy percentage and, on AMD, VRAM counters
//! under `/sys/class/drm/cardN/device/`. NVIDIA's proprietary driver exposes
//! none of this, which is fine: NVML covers those cards in the parent module.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{Adapter, ProcGpu};

/// Highest `cardN` index to probe. Well past any realistic multi-GPU box.
const MAX_CARDS: u32 = 16;

const VENDOR_NVIDIA: u32 = 0x10DE;
const VENDOR_AMD: u32 = 0x1002;
const VENDOR_INTEL: u32 = 0x8086;

fn read_u64(path: &Path) -> Option<u64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_hex(path: &Path) -> Option<u32> {
    let s = fs::read_to_string(path).ok()?;
    let s = s.trim();
    u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// The driver module name (`amdgpu`, `i915`, `xe`, ...) from the uevent file.
fn driver_name(device: &Path) -> Option<String> {
    let uevent = fs::read_to_string(device.join("uevent")).ok()?;
    uevent
        .lines()
        .find_map(|l| l.strip_prefix("DRIVER=").map(|s| s.to_string()))
}

/// Hottest hwmon reading for this card, in whole degrees Celsius.
fn card_temperature(device: &Path) -> Option<u32> {
    let hwmon = fs::read_dir(device.join("hwmon")).ok()?;
    let mut hottest: Option<u32> = None;
    for entry in hwmon.flatten() {
        for idx in 1..=4 {
            let path = entry.path().join(format!("temp{idx}_input"));
            // sysfs reports millidegrees.
            if let Some(milli) = read_u64(&path) {
                let c = (milli / 1000) as u32;
                hottest = Some(hottest.map_or(c, |h: u32| h.max(c)));
            }
        }
    }
    hottest
}

pub struct Platform {
    error: Option<String>,
}

impl Platform {
    pub fn new() -> Self {
        Platform { error: None }
    }

    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    pub fn sample(&mut self) -> (Vec<Adapter>, HashMap<u32, ProcGpu>) {
        let mut out = Vec::new();

        for i in 0..MAX_CARDS {
            let device = Path::new("/sys/class/drm")
                .join(format!("card{i}"))
                .join("device");
            if !device.exists() {
                continue;
            }
            let vendor_id = read_hex(&device.join("vendor"));
            // The NVIDIA blob publishes no busy/VRAM counters here, and NVML
            // already covers those cards - listing them twice would be worse
            // than not listing them at all.
            if vendor_id == Some(VENDOR_NVIDIA) {
                continue;
            }

            let utilization = read_u64(&device.join("gpu_busy_percent")).map(|v| v.min(100) as u32);
            let mem_used = read_u64(&device.join("mem_info_vram_used"));
            let mem_total = read_u64(&device.join("mem_info_vram_total"));
            if utilization.is_none() && mem_total.is_none() {
                continue;
            }

            let driver = driver_name(&device);
            let name = match (vendor_id, driver.as_deref()) {
                (Some(VENDOR_AMD), _) => format!("AMD GPU (card{i})"),
                (Some(VENDOR_INTEL), _) => format!("Intel GPU (card{i})"),
                (_, Some(d)) => format!("{d} (card{i})"),
                _ => format!("GPU (card{i})"),
            };

            out.push(Adapter {
                id: format!("card{i}"),
                name,
                vendor_id,
                // Integrated parts report little or no dedicated VRAM.
                discrete: mem_total.map(|t| t > 512 * 1024 * 1024),
                utilization,
                mem_used,
                mem_total,
                temperature_c: card_temperature(&device),
            });
        }

        if out.is_empty() {
            self.error = Some("No GPU counters under /sys/class/drm".to_string());
        } else {
            self.error = None;
        }

        // sysfs has no per-process GPU accounting.
        (out, HashMap::new())
    }
}
