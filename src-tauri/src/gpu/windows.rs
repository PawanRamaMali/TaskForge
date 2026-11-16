//! Windows GPU telemetry for every adapter, not just NVIDIA ones.
//!
//! Identity and VRAM size come from DXGI; live utilization and memory usage
//! come from the `GPU Engine`, `GPU Adapter Memory` and `GPU Process Memory`
//! performance-counter sets. Those counters are keyed by adapter LUID and are
//! the same source Task Manager reads, so an Intel or AMD integrated GPU shows
//! up here even though NVML can never see it.

use std::collections::HashMap;

use windows::core::{w, PCWSTR};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
};

use super::{Adapter, ProcGpu};

const ERROR_SUCCESS: u32 = 0;

/// Vendor IDs, used to label an adapter when DXGI's description is empty.
const VENDOR_NVIDIA: u32 = 0x10DE;
const VENDOR_AMD: u32 = 0x1002;
const VENDOR_INTEL: u32 = 0x8086;
const VENDOR_MICROSOFT: u32 = 0x1414;

/// Below this much dedicated video memory an adapter is treated as integrated.
const DISCRETE_VRAM_THRESHOLD: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
struct DxgiAdapter {
    luid_hi: i32,
    luid_lo: u32,
    name: String,
    vendor_id: u32,
    vram_total: u64,
    shared_total: u64,
    discrete: bool,
}

impl DxgiAdapter {
    fn key(&self) -> (i32, u32) {
        (self.luid_hi, self.luid_lo)
    }

    /// An integrated GPU has essentially no dedicated VRAM - it borrows system
    /// memory - so reporting its dedicated pool would show a useless
    /// "0 B / 128 MB". Report the shared pool for those, as Task Manager does.
    fn mem_total(&self) -> Option<u64> {
        let total = if self.discrete {
            self.vram_total
        } else {
            self.shared_total
        };
        (total > 0).then_some(total)
    }

    fn to_adapter(&self) -> Adapter {
        Adapter {
            id: format!("luid-{:08x}-{:08x}", self.luid_hi, self.luid_lo),
            name: self.name.clone(),
            vendor_id: Some(self.vendor_id),
            discrete: Some(self.discrete),
            utilization: None,
            mem_used: None,
            mem_total: self.mem_total(),
            temperature_c: None,
        }
    }
}

/// One PDH query holding the three counter sets read each tick.
struct PdhQuery {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    adapter_mem: PDH_HCOUNTER,
    adapter_shared: PDH_HCOUNTER,
    process_mem: PDH_HCOUNTER,
    process_shared: PDH_HCOUNTER,
}

impl Drop for PdhQuery {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

impl PdhQuery {
    fn open() -> Option<PdhQuery> {
        unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != ERROR_SUCCESS {
                return None;
            }

            let mut engine = PDH_HCOUNTER::default();
            let mut adapter_mem = PDH_HCOUNTER::default();
            let mut adapter_shared = PDH_HCOUNTER::default();
            let mut process_mem = PDH_HCOUNTER::default();
            let mut process_shared = PDH_HCOUNTER::default();

            // The engine counter is the one that matters. The memory counters
            // are a bonus and the query stays usable without them.
            if PdhAddEnglishCounterW(
                query,
                w!("\\GPU Engine(*)\\Utilization Percentage"),
                0,
                &mut engine,
            ) != ERROR_SUCCESS
            {
                PdhCloseQuery(query);
                return None;
            }
            PdhAddEnglishCounterW(
                query,
                w!("\\GPU Adapter Memory(*)\\Dedicated Usage"),
                0,
                &mut adapter_mem,
            );
            PdhAddEnglishCounterW(
                query,
                w!("\\GPU Adapter Memory(*)\\Shared Usage"),
                0,
                &mut adapter_shared,
            );
            PdhAddEnglishCounterW(
                query,
                w!("\\GPU Process Memory(*)\\Dedicated Usage"),
                0,
                &mut process_mem,
            );
            PdhAddEnglishCounterW(
                query,
                w!("\\GPU Process Memory(*)\\Shared Usage"),
                0,
                &mut process_shared,
            );

            // Rate counters need a baseline sample before they read anything.
            PdhCollectQueryData(query);

            Some(PdhQuery {
                query,
                engine,
                adapter_mem,
                adapter_shared,
                process_mem,
                process_shared,
            })
        }
    }
}

/// One formatted sample: the counter instance name and its value.
struct Sample {
    instance: String,
    value: f64,
}

/// Reads a wildcard counter into (instance, value) pairs. Returns an empty
/// vector while the counter is still warming up or has no instances.
fn read_counter(counter: PDH_HCOUNTER) -> Vec<Sample> {
    if counter.is_invalid() {
        return Vec::new();
    }
    unsafe {
        let mut size: u32 = 0;
        let mut count: u32 = 0;
        let status =
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
        if status != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }

        // `size` is a byte count, but the instance strings live in the same
        // block after the array, so allocate that many bytes worth of
        // correctly aligned items.
        let item_size = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let items = (size as usize).div_ceil(item_size) + 1;
        let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> =
            vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); items];
        let status = PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut size,
            &mut count,
            Some(buf.as_mut_ptr()),
        );
        if status != ERROR_SUCCESS {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(count as usize);
        for item in buf.iter().take(count as usize) {
            if item.FmtValue.CStatus != PDH_CSTATUS_VALID_DATA
                && item.FmtValue.CStatus != PDH_CSTATUS_NEW_DATA
            {
                continue;
            }
            let Ok(instance) = item.szName.to_string() else {
                continue;
            };
            out.push(Sample {
                instance,
                value: item.FmtValue.Anonymous.doubleValue,
            });
        }
        out
    }
}

/// Pulls `0x<hi>_0x<lo>` out of an instance name such as
/// `pid_8420_luid_0x00000000_0x00012B78_phys_0_eng_1_engtype_3D`.
fn parse_luid(instance: &str) -> Option<(i32, u32)> {
    let rest = instance.split("luid_").nth(1)?;
    let mut parts = rest.split('_');
    let hi = parts.next()?.strip_prefix("0x")?;
    let lo = parts.next()?.strip_prefix("0x")?;
    let hi = u32::from_str_radix(hi, 16).ok()? as i32;
    let lo = u32::from_str_radix(lo, 16).ok()?;
    Some((hi, lo))
}

/// Pulls the pid out of an instance name that starts with `pid_<n>_`.
fn parse_pid(instance: &str) -> Option<u32> {
    instance
        .strip_prefix("pid_")?
        .split('_')
        .next()?
        .parse()
        .ok()
}

/// Pulls the engine type (`3D`, `Copy`, `VideoDecode`, ...) off the tail.
fn parse_engtype(instance: &str) -> &str {
    instance.split("engtype_").nth(1).unwrap_or("")
}

fn vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        VENDOR_NVIDIA => "NVIDIA GPU",
        VENDOR_AMD => "AMD GPU",
        VENDOR_INTEL => "Intel GPU",
        _ => "GPU",
    }
}

fn enumerate_adapters() -> Vec<DxgiAdapter> {
    let mut out = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return out;
        };
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else {
                break;
            };
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            // Skip WARP and the Basic Render Driver: not real hardware, and
            // they would show up as a permanently idle GPU.
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0
                || desc.VendorId == VENDOR_MICROSOFT
            {
                continue;
            }
            let len = desc
                .Description
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..len])
                .trim()
                .to_string();
            let vram_total = desc.DedicatedVideoMemory as u64;
            let shared_total = desc.SharedSystemMemory as u64;
            out.push(DxgiAdapter {
                luid_hi: desc.AdapterLuid.HighPart,
                luid_lo: desc.AdapterLuid.LowPart,
                name: if name.is_empty() {
                    vendor_name(desc.VendorId).to_string()
                } else {
                    name
                },
                vendor_id: desc.VendorId,
                vram_total,
                shared_total,
                // Integrated parts carve their VRAM out of system memory and
                // report little or no dedicated video memory.
                discrete: vram_total > DISCRETE_VRAM_THRESHOLD,
            });
        }
    }
    out
}

pub struct Platform {
    pdh: Option<PdhQuery>,
    adapters: Vec<DxgiAdapter>,
    error: Option<String>,
}

impl Platform {
    pub fn new() -> Self {
        let adapters = enumerate_adapters();
        let pdh = PdhQuery::open();
        let error = if adapters.is_empty() {
            Some("No display adapters reported by DXGI".to_string())
        } else if pdh.is_none() {
            Some("GPU performance counters unavailable".to_string())
        } else {
            None
        };
        Platform {
            pdh,
            adapters,
            error,
        }
    }

    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }

    pub fn sample(&mut self) -> (Vec<Adapter>, HashMap<u32, ProcGpu>) {
        let mut per_proc: HashMap<u32, ProcGpu> = HashMap::new();

        // DXGI saw adapters but the counters are unavailable: still list the
        // hardware so the user knows it exists, just without live numbers.
        let Some(pdh) = &self.pdh else {
            return (
                self.adapters.iter().map(DxgiAdapter::to_adapter).collect(),
                per_proc,
            );
        };

        unsafe {
            PdhCollectQueryData(pdh.query);
        }

        let engine = read_counter(pdh.engine);

        // Utilization per adapter: sum each engine type, then take the busiest
        // type. Summing every instance instead would double-count a frame that
        // runs 3D and Copy work at the same time, which is how a mostly idle
        // GPU ends up claiming 100%.
        let mut by_engtype: HashMap<(i32, u32), HashMap<String, f64>> = HashMap::new();
        let mut proc_util: HashMap<u32, f64> = HashMap::new();
        let mut seen_luids: Vec<(i32, u32)> = Vec::new();

        for s in &engine {
            let Some(luid) = parse_luid(&s.instance) else {
                continue;
            };
            if !seen_luids.contains(&luid) {
                seen_luids.push(luid);
            }
            *by_engtype
                .entry(luid)
                .or_default()
                .entry(parse_engtype(&s.instance).to_string())
                .or_insert(0.0) += s.value;
            if let Some(pid) = parse_pid(&s.instance) {
                *proc_util.entry(pid).or_insert(0.0) += s.value;
            }
        }

        // A LUID the counters know about but DXGI never reported means the
        // adapter list is stale: an eGPU was plugged in, or the driver reset.
        if seen_luids
            .iter()
            .any(|l| !self.adapters.iter().any(|a| a.key() == *l))
        {
            let refreshed = enumerate_adapters();
            if !refreshed.is_empty() {
                self.adapters = refreshed;
            }
        }

        let mut dedicated_mem: HashMap<(i32, u32), u64> = HashMap::new();
        for s in read_counter(pdh.adapter_mem) {
            if let Some(luid) = parse_luid(&s.instance) {
                *dedicated_mem.entry(luid).or_insert(0) += s.value.max(0.0) as u64;
            }
        }
        let mut shared_mem: HashMap<(i32, u32), u64> = HashMap::new();
        for s in read_counter(pdh.adapter_shared) {
            if let Some(luid) = parse_luid(&s.instance) {
                *shared_mem.entry(luid).or_insert(0) += s.value.max(0.0) as u64;
            }
        }

        // A process's GPU memory is both pools together, which is the number
        // Task Manager shows and the only non-zero one for an iGPU.
        for counter in [pdh.process_mem, pdh.process_shared] {
            for s in read_counter(counter) {
                let Some(pid) = parse_pid(&s.instance) else {
                    continue;
                };
                let entry = per_proc.entry(pid).or_default();
                entry.mem_bytes = Some(entry.mem_bytes.unwrap_or(0) + s.value.max(0.0) as u64);
            }
        }

        for (pid, util) in proc_util {
            per_proc.entry(pid).or_default().util = Some(util.clamp(0.0, 100.0) as u32);
        }

        let out = self
            .adapters
            .iter()
            .map(|a| {
                let mut adapter = a.to_adapter();
                adapter.utilization = by_engtype.get(&a.key()).map(|types| {
                    types
                        .values()
                        .fold(0.0_f64, |acc, v| acc.max(*v))
                        .clamp(0.0, 100.0) as u32
                });
                adapter.mem_used = if a.discrete {
                    dedicated_mem.get(&a.key()).copied()
                } else {
                    shared_mem.get(&a.key()).copied()
                };
                adapter
            })
            .collect();

        (out, per_proc)
    }
}
