// Stability analyzer. Turns the raw document produced by the platform collectors
// (src-tauri/src/diagnostics) into ranked findings with evidence and next steps.
// Pure and dependency-free so it can also be run under Node against a saved report.

export type Severity = 'critical' | 'warning' | 'info' | 'ok';

export interface RawEvent {
  ts: number;
  source: string;
  id: number;
  level: number;
  category: string;
  message: string;
}

export interface RawShutdown {
  ts: number;
  lastAlive: number | null;
  bugcheck: number;
  params: string;
  sleepInProgress: boolean;
  resumedFromSleep: boolean;
  powerButton: boolean;
  longPowerPress: boolean;
}

export interface RawBugcheck {
  ts: number;
  code: string | null;
  params: string | null;
  dump: string | null;
}

export interface RawKernelReport {
  kind: string;
  code: string;
  param1: string;
  submissions: number;
  first: number;
  last: number;
}

export interface RawAppFault {
  app: string;
  kind: 'crash' | 'hang';
  count: number;
  first: number;
  last: number;
  times: number[];
}

export interface RawDisk {
  name: string;
  media: string;
  bus: string;
  health: string;
  operational: string;
  sizeGb: number;
  wearPct: number | null;
  tempC: number | null;
  tempMaxC: number | null;
  readErrors: number | null;
  writeErrors: number | null;
  powerOnHours: number | null;
}

export interface RawVolume {
  drive: string;
  fs: string;
  health: string;
  sizeGb: number;
  freeGb: number;
}

export interface RawDriver {
  device: string;
  class: string;
  provider: string;
  version: string;
  date: number | null;
  ageDays: number | null;
}

export interface RawConfig {
  crashDumpMode: number | null;
  autoReboot: boolean | null;
  minidumpCount: number | null;
  memoryDumpPresent: boolean;
  liveKernelDumps: string[] | null;
  fastStartup: boolean | null;
  hypervisorPresent: boolean | null;
  vbsRunning: boolean | null;
  hvciRunning: boolean | null;
  pagefileAuto: boolean | null;
  commitUsedGb: number | null;
  commitLimitGb: number | null;
  memoryDiagnostic: { ts: number; text: string } | null;
  powerPlan: string | null;
  bootsInWindow: number | null;
  cleanShutdowns: number | null;
  topMemory: { name: string; pid: number; privateGb: number }[];
}

export interface RawDiagnostics {
  schema: number;
  platform: 'windows' | 'linux';
  collectedAt: number;
  windowDays: number;
  elevated: boolean;
  system: {
    os: string;
    model: string;
    bios: string | null;
    biosDate: number | null;
    biosAgeDays: number | null;
    cpu: string | null;
    microcode: string | null;
    ramGb: number | null;
    lastBoot: number | null;
  };
  shutdowns: RawShutdown[];
  bugchecks: RawBugcheck[];
  kernelReports: RawKernelReport[];
  events: RawEvent[];
  appFaults: RawAppFault[];
  disks: RawDisk[];
  volumes: RawVolume[];
  drivers: RawDriver[];
  config: RawConfig;
  notes: string[];
}

export interface Finding {
  id: string;
  severity: Severity;
  area: string;
  title: string;
  detail: string;
  evidence: string[];
  actions: string[];
}

export interface TimelineEntry {
  ts: number;
  severity: Severity;
  label: string;
}

export interface StatChip {
  label: string;
  value: number;
  severity: Severity;
}

export interface StabilityReport {
  generatedAt: number;
  platform: string;
  windowDays: number;
  elevated: boolean;
  verdict: Severity;
  headline: string;
  stats: StatChip[];
  findings: Finding[];
  timeline: TimelineEntry[];
  system: [string, string][];
  notes: string[];
}

export const SEVERITY_RANK: Record<Severity, number> = { critical: 0, warning: 1, info: 2, ok: 3 };

type Area = 'gpu' | 'hypervisor' | 'hardware' | 'memory' | 'storage' | 'driver' | 'power' | 'network' | 'system' | 'manual';

interface BugcheckInfo {
  name: string;
  area: Area;
}

// Stop codes seen as blue screens or live kernel dumps, grouped by the subsystem they implicate.
const BUGCHECKS: Record<number, BugcheckInfo> = {
  0x0a: { name: 'IRQL_NOT_LESS_OR_EQUAL', area: 'driver' },
  0x19: { name: 'BAD_POOL_HEADER', area: 'driver' },
  0x1a: { name: 'MEMORY_MANAGEMENT', area: 'memory' },
  0x1e: { name: 'KMODE_EXCEPTION_NOT_HANDLED', area: 'driver' },
  0x3b: { name: 'SYSTEM_SERVICE_EXCEPTION', area: 'driver' },
  0x4e: { name: 'PFN_LIST_CORRUPT', area: 'memory' },
  0x50: { name: 'PAGE_FAULT_IN_NONPAGED_AREA', area: 'memory' },
  0x7a: { name: 'KERNEL_DATA_INPAGE_ERROR', area: 'storage' },
  0x7e: { name: 'SYSTEM_THREAD_EXCEPTION_NOT_HANDLED', area: 'driver' },
  0x9f: { name: 'DRIVER_POWER_STATE_FAILURE', area: 'power' },
  0xa0: { name: 'INTERNAL_POWER_ERROR', area: 'power' },
  0xc2: { name: 'BAD_POOL_CALLER', area: 'driver' },
  0xd1: { name: 'DRIVER_IRQL_NOT_LESS_OR_EQUAL', area: 'driver' },
  0xe2: { name: 'MANUALLY_INITIATED_CRASH', area: 'manual' },
  0xef: { name: 'CRITICAL_PROCESS_DIED', area: 'system' },
  0xfc: { name: 'ATTEMPTED_EXECUTE_OF_NOEXECUTE_MEMORY', area: 'driver' },
  0x101: { name: 'CLOCK_WATCHDOG_TIMEOUT', area: 'hardware' },
  0x109: { name: 'CRITICAL_STRUCTURE_CORRUPTION', area: 'memory' },
  0x10e: { name: 'VIDEO_MEMORY_MANAGEMENT_INTERNAL', area: 'gpu' },
  0x113: { name: 'VIDEO_DXGKRNL_FATAL_ERROR', area: 'gpu' },
  0x116: { name: 'VIDEO_TDR_FAILURE', area: 'gpu' },
  0x117: { name: 'VIDEO_TDR_TIMEOUT_DETECTED', area: 'gpu' },
  0x119: { name: 'VIDEO_SCHEDULER_INTERNAL_ERROR', area: 'gpu' },
  0x124: { name: 'WHEA_UNCORRECTABLE_ERROR', area: 'hardware' },
  0x133: { name: 'DPC_WATCHDOG_VIOLATION', area: 'driver' },
  0x139: { name: 'KERNEL_SECURITY_CHECK_FAILURE', area: 'driver' },
  0x13a: { name: 'KERNEL_MODE_HEAP_CORRUPTION', area: 'driver' },
  0x141: { name: 'VIDEO_ENGINE_TIMEOUT_DETECTED', area: 'gpu' },
  0x144: { name: 'BUGCODE_USB3_DRIVER', area: 'driver' },
  0x154: { name: 'UNEXPECTED_STORE_EXCEPTION', area: 'storage' },
  0x15f: { name: 'CONNECTED_STANDBY_WATCHDOG_TIMEOUT_LIVEDUMP', area: 'power' },
  0x18b: { name: 'SECURE_KERNEL_ERROR', area: 'hypervisor' },
  0x1a8: { name: 'BUGCODE_NDIS_DRIVER_LIVE_DUMP', area: 'network' },
  0x1c7: { name: 'STORE_DATA_STRUCTURE_CORRUPTION', area: 'storage' },
  0x20001: { name: 'HYPERVISOR_ERROR', area: 'hypervisor' },
};

const AREA_LABELS: Record<Area, string> = {
  gpu: 'graphics driver / GPU',
  hypervisor: 'hypervisor / firmware',
  hardware: 'hardware',
  memory: 'memory',
  storage: 'storage',
  driver: 'a kernel driver',
  power: 'power management',
  network: 'network driver',
  system: 'Windows system process',
  manual: 'manually triggered',
};

function arr<T>(v: T[] | T | null | undefined): T[] {
  if (Array.isArray(v)) return v;
  return v == null ? [] : [v];
}

function codeNum(code: string | number | null | undefined): number {
  if (typeof code === 'number') return code;
  if (!code) return 0;
  const n = parseInt(code.replace(/^0x/i, ''), 16);
  return Number.isFinite(n) ? n : 0;
}

export function hex(n: number): string {
  return `0x${n.toString(16).toUpperCase()}`;
}

function describeCode(n: number): string {
  const info = BUGCHECKS[n];
  return info ? `${hex(n)} ${info.name}` : `stop code ${hex(n)}`;
}

export function fmtTs(ts: number | null | undefined): string {
  if (!ts) return '-';
  return new Date(ts * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

function plural(n: number, word: string, many = `${word}s`): string {
  if (word === 'time') return n === 1 ? 'once' : n === 2 ? 'twice' : `${n} times`;
  return `${n} ${n === 1 ? word : many}`;
}

function fmtDate(ts: number | null | undefined): string {
  if (!ts) return '-';
  return new Date(ts * 1000).toLocaleDateString(undefined, { dateStyle: 'medium' });
}

interface Episode {
  start: number;
  end: number;
  count: number;
}

/** Group timestamps into bursts separated by more than `gap` seconds. */
function episodes(times: number[], gap = 300): Episode[] {
  const sorted = [...times].sort((a, b) => a - b);
  const out: Episode[] = [];
  for (const t of sorted) {
    const last = out[out.length - 1];
    if (last && t - last.end <= gap) {
      last.end = t;
      last.count++;
    } else {
      out.push({ start: t, end: t, count: 1 });
    }
  }
  return out;
}

/** Collapse near-identical messages (device paths, ids, addresses) and count them. */
function tally(messages: string[], limit = 4): string[] {
  const counts = new Map<string, number>();
  for (const m of messages) {
    const key = m
      .replace(/\\Device\\\S+/g, '')
      .replace(/\((GPC|TPC|SM)[^)]*\)/g, '')
      .replace(/0x[0-9a-f]+/gi, '#')
      .replace(/\b\d+\b/g, '#')
      .replace(/\s+/g, ' ')
      .trim()
      .slice(0, 160);
    // Skip payloads that are only hex/ids once normalized (e.g. raw Wi-Fi driver dumps).
    if (key && /[a-z]{3,}.*[a-z]{3,}/i.test(key.replace(/^\S+ #: /, ''))) counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([k, n]) => `${n}x "${k}"`);
}

interface Crash {
  ts: number;
  lastAlive: number | null;
  code: number;
  resumed: boolean;
}

export function analyze(raw: RawDiagnostics): StabilityReport {
  const days = raw.windowDays;
  const isWindows = raw.platform === 'windows';
  const events = arr(raw.events);
  const byCat = (c: string) => events.filter((e) => e.category === c);
  const cfg: Partial<RawConfig> = raw.config ?? {};
  const drivers = arr(raw.drivers);
  const faults = arr(raw.appFaults);
  const reports = arr(raw.kernelReports);
  const sys = raw.system ?? ({} as RawDiagnostics['system']);
  const findings: Finding[] = [];
  const timeline: TimelineEntry[] = [];

  // ---- Unexpected shutdowns, merged with the blue screen that caused each one ----
  const bugchecks = arr(raw.bugchecks).map((b) => ({ ts: b.ts, code: codeNum(b.code) }));
  const usedBugchecks = new Set<number>();
  const crashes: Crash[] = arr(raw.shutdowns).map((s) => {
    let code = s.bugcheck || 0;
    const idx = bugchecks.findIndex((b, i) => !usedBugchecks.has(i) && Math.abs(b.ts - s.ts) < 900);
    if (idx >= 0) {
      usedBugchecks.add(idx);
      code = code || bugchecks[idx].code;
    }
    return { ts: s.ts, lastAlive: s.lastAlive, code, resumed: !!(s.resumedFromSleep || s.sleepInProgress) };
  });
  bugchecks.forEach((b, i) => {
    if (!usedBugchecks.has(i)) crashes.push({ ts: b.ts, lastAlive: null, code: b.code, resumed: false });
  });
  crashes.sort((a, b) => b.ts - a.ts);

  const freezes = crashes.filter((c) => c.code === 0);
  const blueScreens = crashes.filter((c) => c.code !== 0);
  const crashAreas = (area: Area) => blueScreens.filter((c) => BUGCHECKS[c.code]?.area === area);
  const reportAreas = (area: Area) => reports.filter((r) => BUGCHECKS[codeNum(r.code)]?.area === area);

  for (const c of crashes) {
    timeline.push({
      ts: c.lastAlive ?? c.ts,
      severity: 'critical',
      label: c.code ? `Blue screen ${describeCode(c.code)}` : 'Hard freeze / forced power-off',
    });
  }

  if (crashes.length) {
    const parts = [];
    if (freezes.length) parts.push(plural(freezes.length, 'hard freeze', 'hard freezes'));
    if (blueScreens.length) parts.push(plural(blueScreens.length, 'blue screen'));
    const implicated = [...new Set(blueScreens.map((c) => BUGCHECKS[c.code]?.area).filter(Boolean))] as Area[];
    const actions: string[] = [];
    if (freezes.length && isWindows) {
      actions.push(
        'Make the next freeze leave evidence: add the DWORD CrashOnCtrlScroll=1 under HKLM\\SYSTEM\\CurrentControlSet\\Services\\kbdhid\\Parameters and ...\\Services\\i8042prt\\Parameters, reboot, and when the system hangs hold Right Ctrl and press Scroll Lock twice. Windows then writes a MANUALLY_INITIATED_CRASH (0xE2) dump showing what was stuck, instead of a silent power-off.'
      );
    }
    if (blueScreens.length && isWindows) {
      actions.push('Open the crash dumps in WinDbg (Microsoft Store) and run "!analyze -v" to name the exact faulting driver.');
    }
    if (freezes.length && !isWindows) {
      actions.push('Enable persistent journaling and kdump (or pstore) so the next lockup leaves a kernel log or crash dump behind.');
    }
    findings.push({
      id: 'crashes',
      severity: 'critical',
      area: 'Crashes',
      title: `${plural(crashes.length, 'unexpected shutdown')} in the last ${days} days (${parts.join(', ')})`,
      detail:
        (freezes.length
          ? 'A hard freeze records no stop code: the system stopped responding and was powered off by hand or lost power. '
          : '') +
        (implicated.length
          ? `The blue screens implicate ${implicated.map((a) => AREA_LABELS[a]).join(' and ')}; see the related findings below.`
          : 'See the related findings below for the subsystems showing errors before these shutdowns.'),
      evidence: crashes.map((c) => {
        const what = c.code ? `blue screen ${describeCode(c.code)}` : 'hard freeze or forced power-off (no stop code)';
        const alive = c.lastAlive ? `, last alive ${fmtTs(c.lastAlive)}` : '';
        const sleep = c.resumed ? ', right after resuming from sleep' : '';
        return `${fmtTs(c.ts)}: ${what}${alive}${sleep}`;
      }),
      actions,
    });
  }

  // ---- Graphics driver timeouts and resets ----
  const gpuEvents = byCat('gpu');
  const gpuCrashes = crashAreas('gpu');
  const gpuReports = reportAreas('gpu');
  const gpuEpisodes = episodes(gpuEvents.map((e) => e.ts));
  for (const ep of gpuEpisodes) {
    timeline.push({ ts: ep.start, severity: 'warning', label: `Graphics driver error / reset (${plural(ep.count, 'event')})` });
  }
  if (gpuEvents.length || gpuCrashes.length || gpuReports.length) {
    const displayDrivers = drivers.filter((d) => d.class === 'DISPLAY');
    const sources = new Set(gpuEvents.map((e) => e.source.toLowerCase()));
    const vendor = sources.has('nvlddmkm')
      ? 'NVIDIA'
      : [...sources].some((s) => s.startsWith('amd'))
        ? 'AMD'
        : [...sources].some((s) => s.startsWith('igfx'))
          ? 'Intel'
          : 'GPU vendor';
    const vendorDriver = displayDrivers.find((d) => d.device.toUpperCase().includes(vendor.toUpperCase())) ?? displayDrivers[0];

    // Apps that crashed within two minutes of a GPU error burst.
    const correlated = new Map<string, number>();
    for (const ep of gpuEpisodes) {
      for (const f of faults) {
        for (const t of arr(f.times)) {
          if (t >= ep.start - 120 && t <= ep.end + 120) {
            const key = `${f.app} (${f.kind})`;
            if (!correlated.has(key)) correlated.set(key, t);
          }
        }
      }
    }

    const evidence: string[] = [];
    if (gpuEvents.length) {
      const times = gpuEvents.map((e) => e.ts);
      evidence.push(
        `${plural(gpuEpisodes.length, 'error burst')} (${plural(gpuEvents.length, 'driver event')}) between ${fmtTs(Math.min(...times))} and ${fmtTs(Math.max(...times))}`
      );
      evidence.push(...tally(gpuEvents.map((e) => `${e.source} ${e.id}: ${e.message}`)));
    }
    for (const c of gpuCrashes) evidence.push(`${fmtTs(c.ts)}: system crashed with ${describeCode(c.code)}`);
    if (gpuReports.length) {
      const codes = [...new Set(gpuReports.map((r) => describeCode(codeNum(r.code))))];
      const first = Math.min(...gpuReports.map((r) => r.first));
      evidence.push(`Windows captured ${plural(gpuReports.length, 'distinct GPU timeout report')} (${codes.join(', ')}), earliest ${fmtTs(first)}`);
    }
    for (const [app, t] of correlated) evidence.push(`${app} failed at ${fmtTs(t)}, within two minutes of a GPU error`);
    if (vendorDriver) {
      evidence.push(
        `${vendorDriver.device}: driver ${vendorDriver.version}${vendorDriver.ageDays != null ? `, released ${plural(vendorDriver.ageDays, 'day')} ago` : ''}`
      );
    }

    const actions = [
      `Install the newest ${vendor} driver as a clean install (the installer's "clean installation" option, or DDU in Safe Mode if resets continue)${vendorDriver?.ageDays != null && vendorDriver.ageDays > 90 ? `; the current one is ${vendorDriver.ageDays} days old` : ''}.`,
      'Remove any GPU overclock or undervolt (MSI Afterburner, OEM performance modes) and retest at stock settings.',
    ];
    if (correlated.size) {
      const apps = [...new Set([...correlated.keys()].map((k) => k.replace(/ \(.*\)$/, '')))];
      actions.push(
        `GPU errors coincide with ${apps.join(', ')}. Heavy compute/VRAM workloads expose unstable drivers or memory: update the app and its CUDA/GPU runtime, reduce its GPU memory use, and run a VRAM stress test (e.g. OCCT) to rule out faulty video memory.`
      );
    }
    actions.push('Watch GPU temperature under load on the TaskForge GPU card; sustained readings near the thermal limit on a laptop call for cleaning the vents and a firmer surface or cooling pad.');
    actions.push('If resets continue on a clean driver at stock settings, the GPU itself may be failing: contact the manufacturer for warranty service.');

    findings.push({
      id: 'gpu',
      severity: gpuCrashes.length ? 'critical' : gpuEpisodes.length >= 3 || gpuReports.length ? 'warning' : 'info',
      area: 'Graphics',
      title: gpuCrashes.length
        ? `Graphics driver failures crashed the system ${plural(gpuCrashes.length, 'time')}`
        : `Graphics driver stopped responding ${plural(Math.max(gpuEpisodes.length, gpuReports.length), 'time')}`,
      detail:
        'The GPU stopped responding and the OS tried to reset its driver. When a reset fails the result is a VIDEO_TDR_FAILURE blue screen or a frozen screen. Frequent resets point to the graphics driver, an unstable GPU overclock/undervolt, overheating, or, if they persist on a clean stock setup, faulty graphics hardware.',
      evidence,
      actions,
    });
  }

  // ---- Hypervisor / secure kernel ----
  const hvCrashes = crashAreas('hypervisor');
  const hvReports = reportAreas('hypervisor');
  if (hvCrashes.length || hvReports.length) {
    const evidence = hvCrashes.map((c) => `${fmtTs(c.ts)}: system crashed with ${describeCode(c.code)}`);
    if (!hvCrashes.length) {
      for (const r of hvReports) evidence.push(`${describeCode(codeNum(r.code))} reported, first ${fmtTs(r.first)}, last ${fmtTs(r.last)}`);
    }
    if (sys.bios) {
      evidence.push(`Firmware ${sys.bios}${sys.biosDate ? ` dated ${fmtDate(sys.biosDate)}` : ''}${sys.biosAgeDays != null ? ` (${sys.biosAgeDays} days old)` : ''}`);
    }
    if (sys.microcode) evidence.push(`CPU microcode revision ${sys.microcode}`);
    if (cfg.vbsRunning != null) {
      evidence.push(`Virtualization-based security: ${cfg.vbsRunning ? 'running' : 'off'}; Memory integrity (HVCI): ${cfg.hvciRunning ? 'on' : 'off'}`);
    }
    const actions = [
      'Update the BIOS/UEFI firmware plus the chipset and Intel Management Engine (or AMD chipset) drivers from the laptop or motherboard vendor\'s support page. Firmware updates carry new CPU microcode, which is the usual fix for hypervisor errors.',
      'Reset any CPU or memory overclock, undervolt or XMP/EXPO profile (BIOS, Intel XTU, OEM "overclock" modes) to defaults.',
    ];
    if (cfg.hvciRunning) {
      actions.push(
        'As an isolation test, temporarily turn off Windows Security > Device security > Core isolation > Memory integrity. If the crashes stop, a driver or the firmware is incompatible with it: update them, then turn it back on.'
      );
    }
    actions.push('Run a full memory test (Windows Memory Diagnostic "Extended", or MemTest86 overnight); unstable RAM also trips the hypervisor.');
    findings.push({
      id: 'hypervisor',
      severity: hvCrashes.length ? 'critical' : 'warning',
      area: 'Firmware',
      title: hvCrashes.length
        ? `Hypervisor errors crashed the system ${plural(hvCrashes.length, 'time')}`
        : 'Hypervisor errors were reported earlier',
      detail:
        'The Windows hypervisor runs underneath Windows whenever Virtualization-based Security, Memory integrity, WSL2 or Hyper-V is on. A fatal hypervisor error almost always means firmware/CPU microcode problems, an unstable CPU or RAM configuration, or a low-level driver that misbehaves under virtualization.',
      evidence,
      actions,
    });
  }

  // ---- Hardware errors (WHEA / MCE) ----
  const hwEvents = byCat('hardware');
  const hwCrashes = crashAreas('hardware');
  if (hwEvents.length || hwCrashes.length) {
    for (const e of hwEvents.slice(0, 20)) timeline.push({ ts: e.ts, severity: 'critical', label: `Hardware error: ${e.message.slice(0, 80)}` });
    findings.push({
      id: 'hardware',
      severity: 'critical',
      area: 'Hardware',
      title: `CPU/PCIe hardware errors reported (${plural(hwEvents.length + hwCrashes.length, 'record')})`,
      detail:
        'The processor or a PCIe device reported a machine-check / WHEA error. Corrected errors are early warnings; uncorrected ones crash the system. Causes are overclocking or undervolting, overheating, outdated firmware, or failing hardware.',
      evidence: [
        ...hwCrashes.map((c) => `${fmtTs(c.ts)}: system crashed with ${describeCode(c.code)}`),
        ...tally(hwEvents.map((e) => e.message), 5),
      ],
      actions: [
        'Return CPU, memory and GPU to stock settings (no overclock, undervolt or XMP) and update the BIOS.',
        'Check temperatures under load; clean dust from fans and heatsinks.',
        'Run a memory test and, if errors continue at stock settings, arrange hardware service.',
      ],
    });
  }

  // ---- Memory pressure ----
  const memEvents = byCat('memory');
  for (const e of memEvents) timeline.push({ ts: e.ts, severity: 'warning', label: isWindows ? 'Ran out of virtual memory' : 'Out-of-memory kill' });
  const commitRatio = cfg.commitUsedGb != null && cfg.commitLimitGb ? cfg.commitUsedGb / cfg.commitLimitGb : null;
  if (memEvents.length || (commitRatio != null && commitRatio > 0.85)) {
    const evidence = memEvents.slice(0, 6).map((e) => `${fmtTs(e.ts)}: ${e.message}`);
    if (commitRatio != null) evidence.push(`Committed memory now: ${cfg.commitUsedGb} GB of ${cfg.commitLimitGb} GB (${Math.round(commitRatio * 100)}%)`);
    // Resource-Exhaustion-Detector lists "name.exe (pid) consumed N bytes"; keep the heavy hitters.
    const culprits = [
      ...new Set(
        memEvents.flatMap((e) =>
          [...e.message.matchAll(/([\w .-]+\.exe) \(\d+\) consumed (\d+) bytes/g)]
            .filter((m) => Number(m[2]) >= 2 * 2 ** 30)
            .map((m) => `${m[1].trim().replace(/^and /, '')} (${(Number(m[2]) / 2 ** 30).toFixed(0)} GB)`)
        )
      ),
    ];
    const actions = [];
    if (culprits.length) {
      actions.push(`Rein in the programs that exhausted memory: ${culprits.slice(0, 4).join(', ')}. Cap model/batch sizes or memory limits for scripts and AI tools before they reach the system limit.`);
    }
    if (isWindows && cfg.pagefileAuto === false) actions.push('Set the page file back to "Automatically manage paging file size" so Windows can grow commit space instead of stalling.');
    actions.push('Keep an eye on the TaskForge Memory card; when committed memory nears the limit, end or trim the largest process before the system stops responding.');
    findings.push({
      id: 'memory',
      severity: memEvents.length ? 'warning' : 'info',
      area: 'Memory',
      title: memEvents.length
        ? isWindows
          ? `System ran out of virtual memory ${plural(memEvents.length, 'time')}`
          : `Kernel killed processes for lack of memory ${plural(memEvents.length, 'time')}`
        : 'Committed memory is close to the limit',
      detail:
        'When committed memory reaches the limit (RAM plus page file), allocations fail, apps freeze and the whole desktop can appear hung for minutes. A runaway process is a common cause of "random" freezes.',
      evidence,
      actions,
    });
  }

  if (isWindows && crashes.length) {
    const md = cfg.memoryDiagnostic;
    if (md && /error|problem/i.test(md.text) && !/no errors|not detect/i.test(md.text)) {
      findings.push({
        id: 'memtest',
        severity: 'critical',
        area: 'Memory',
        title: 'Windows Memory Diagnostic found RAM errors',
        detail: 'Faulty RAM causes random crashes of every kind.',
        evidence: [`${fmtTs(md.ts)}: ${md.text}`],
        actions: ['Reseat or replace the memory modules (test one module at a time with MemTest86) and disable XMP/EXPO.'],
      });
    } else if (!md) {
      findings.push({
        id: 'memtest',
        severity: 'info',
        area: 'Memory',
        title: 'RAM has not been tested',
        detail: 'No Windows Memory Diagnostic result exists on this system. With unexplained crashes, ruling out RAM is a cheap step.',
        evidence: sys.ramGb ? [`${sys.ramGb} GB installed`] : [],
        actions: ['Run "mdsched.exe" and choose "Restart now" (Extended test via F1), or run MemTest86 from USB for a few passes overnight.'],
      });
    }
  }

  // ---- Storage ----
  const storageEvents = byCat('storage');
  const dumpFailures = storageEvents.filter((e) => e.source.toLowerCase() === 'volmgr' && e.id === 161);
  const shadowDrops = storageEvents.filter((e) => e.source.toLowerCase() === 'volsnap');
  const realStorage = storageEvents.filter((e) => !['volmgr', 'volsnap'].includes(e.source.toLowerCase()));
  const seriousIds = new Set([7, 11, 51, 55, 98, 129, 140, 153, 157]);
  const disks = arr(raw.disks);
  const badDisks = disks.filter(
    (d) => (d.health && d.health !== 'Healthy') || (d.readErrors ?? 0) > 0 || (d.writeErrors ?? 0) > 0
  );
  const worn = disks.filter((d) => (d.wearPct ?? 0) >= 80);
  const lowSpace = arr(raw.volumes).filter((v) => v.sizeGb > 0 && v.freeGb / v.sizeGb < 0.1);
  const diskLine = (d: RawDisk) =>
    `${d.name} (${d.bus} ${d.media}, ${d.sizeGb} GB): ${d.health}${d.wearPct != null ? `, wear ${d.wearPct}%` : ''}${d.tempMaxC ? `, max ${d.tempMaxC} C` : ''}${d.readErrors ? `, ${d.readErrors} uncorrected read errors` : ''}`;
  if (realStorage.length || badDisks.length || worn.length || lowSpace.length) {
    const serious = badDisks.length || realStorage.some((e) => seriousIds.has(e.id));
    findings.push({
      id: 'storage',
      severity: serious ? 'critical' : 'warning',
      area: 'Storage',
      title: badDisks.length ? 'A drive reports health problems' : realStorage.length ? `Storage errors logged (${plural(realStorage.length, 'event')})` : 'Storage needs attention',
      detail:
        'Disk timeouts and read errors stall everything waiting on I/O, which looks like a system freeze, and can crash Windows when the page file or system files are affected.',
      evidence: [
        ...tally(realStorage.map((e) => `${e.source} ${e.id}: ${e.message}`), 5),
        ...disks.map(diskLine),
        ...lowSpace.map((v) => `${v.drive} has only ${v.freeGb} GB free of ${v.sizeGb} GB`),
      ],
      actions: [
        'Back up important data now.',
        'Update the SSD firmware (vendor tool, e.g. Samsung Magician / Crucial Storage Executive) and the storage controller driver.',
        'Run "chkdsk C: /scan" and check SMART data with CrystalDiskInfo; replace any drive reporting uncorrected errors.',
      ],
    });
  } else if (disks.length) {
    findings.push({
      id: 'storage',
      severity: 'ok',
      area: 'Storage',
      title: 'Drives look healthy',
      detail: 'No disk, controller or file system errors were logged in this period.',
      evidence: [
        ...disks.map(diskLine),
        ...(shadowDrops.length ? [`${plural(shadowDrops.length, 'restore-point storage warning')} under heavy disk load (harmless for stability)`] : []),
      ],
      actions: [],
    });
  }

  // ---- Network drivers ----
  // A cable being unplugged is not a driver fault.
  const netEvents = byCat('network').filter((e) => !/link (is|has been) disconnected|link is down/i.test(e.message));
  const netReports = reportAreas('network');
  if (netEvents.length || netReports.length) {
    const netDrivers = drivers.filter((d) => d.class === 'NET' && !/microsoft/i.test(d.provider));
    for (const ep of episodes(netEvents.map((e) => e.ts))) {
      timeline.push({ ts: ep.start, severity: 'info', label: 'Network adapter driver failure' });
    }
    findings.push({
      id: 'network',
      severity: 'warning',
      area: 'Network',
      title: 'Network adapter driver failed or was reset',
      detail:
        'The Wi-Fi/Ethernet driver reported internal errors or a stall. Besides dropped connections, a wedged network driver can block sleep, shutdown and anything waiting on the network.',
      evidence: [
        ...tally(netEvents.map((e) => `${e.source} ${e.id}: ${e.message}`), 3),
        ...netReports.map((r) => `${describeCode(codeNum(r.code))} live dump, first ${fmtTs(r.first)}, last ${fmtTs(r.last)}`),
        ...netDrivers.map((d) => `${d.device}: driver ${d.version}${d.ageDays != null ? ` (${d.ageDays} days old)` : ''}`),
      ],
      actions: [
        'Install the latest Wi-Fi and Bluetooth driver package from the chip vendor (e.g. Intel Driver & Support Assistant) or the laptop maker.',
        'In Device Manager > network adapter > Power Management, untick "Allow the computer to turn off this device to save power" if failures cluster around sleep.',
      ],
    });
  }

  // ---- Sleep / fast startup ----
  const resumeCrashes = crashes.filter((c) => c.resumed);
  if (resumeCrashes.length) {
    findings.push({
      id: 'sleep',
      severity: 'warning',
      area: 'Power',
      title: `${plural(resumeCrashes.length, 'crash', 'crashes')} happened around sleep/resume`,
      detail: 'Drivers that mishandle power transitions (graphics, Wi-Fi, chipset) typically fail right after the system wakes.',
      evidence: resumeCrashes.map((c) => `${fmtTs(c.ts)}: ${c.code ? describeCode(c.code) : 'hard freeze'} after waking`),
      actions: [
        'Update graphics, chipset and Wi-Fi drivers first; they own most resume bugs.',
        ...(cfg.fastStartup ? ['Turn off Fast Startup (Control Panel > Power Options > Choose what the power buttons do) so a shutdown fully resets drivers.'] : []),
        'While troubleshooting, prefer shutdown or hibernate over sleep and see whether the crashes stop.',
      ],
    });
  } else if (cfg.fastStartup && crashes.length) {
    findings.push({
      id: 'faststartup',
      severity: 'info',
      area: 'Power',
      title: 'Fast Startup is on',
      detail:
        'With Fast Startup, "Shut down" hibernates the kernel and drivers instead of restarting them, so a driver stuck in a bad state survives a shutdown. Only Restart gives a truly fresh start.',
      evidence: [],
      actions: ['Turn off Fast Startup while troubleshooting (Control Panel > Power Options > Choose what the power buttons do), and use Restart rather than Shut down after a driver update.'],
    });
  }

  // ---- Crash dump retention ----
  if (isWindows) {
    const evidence: string[] = [];
    let severity: Severity | null = null;
    let title = '';
    if (cfg.crashDumpMode === 0) {
      severity = 'warning';
      title = 'Crash dumps are turned off';
      evidence.push('Startup and Recovery > Write debugging information is set to (none)');
    } else if (blueScreens.length && cfg.minidumpCount === 0 && !cfg.memoryDumpPresent) {
      severity = 'warning';
      title = 'The crash dumps from these blue screens are gone';
      evidence.push(`${plural(blueScreens.length, 'blue screen')} recorded, but no dump files remain in the Minidump folder`);
    }
    if (dumpFailures.length) {
      severity = severity ?? 'info';
      title = title || 'Windows failed to write a crash dump';
      evidence.push(...dumpFailures.map((e) => `${fmtTs(e.ts)}: ${e.message}`));
    }
    if (severity) {
      findings.push({
        id: 'dumps',
        severity,
        area: 'Crash data',
        title,
        detail: 'Without dump files the exact driver behind each blue screen cannot be identified.',
        evidence,
        actions: [
          'In Disk Cleanup and Storage Sense, stop deleting "System error memory dump files".',
          'Set System Properties > Advanced > Startup and Recovery > Write debugging information to "Automatic memory dump", and keep a page file on C:, which Windows needs to write the dump.',
        ],
      });
    }
  }

  // ---- Thermal throttling ----
  const thermal = byCat('thermal');
  if (thermal.length) {
    findings.push({
      id: 'thermal',
      severity: 'warning',
      area: 'Thermal',
      title: `CPU speed was limited by firmware ${plural(thermal.length, 'time')}`,
      detail: 'The firmware throttled the processor, usually because of heat or a power limit. Sustained overheating also destabilizes GPUs and VRMs.',
      evidence: tally(thermal.map((e) => e.message), 3),
      actions: ['Clean the vents and fans, avoid soft surfaces, and use the performance mode that matches the workload.'],
    });
  }

  // ---- Linux kernel hangs and oopses ----
  const hangs = byCat('hang');
  const oopses = byCat('kernel');
  if (hangs.length || oopses.length) {
    for (const e of [...hangs, ...oopses].slice(0, 20)) timeline.push({ ts: e.ts, severity: 'critical', label: e.message.slice(0, 90) });
    findings.push({
      id: 'kernel',
      severity: 'critical',
      area: 'Kernel',
      title: `Kernel lockups or oopses logged (${plural(hangs.length + oopses.length, 'event')})`,
      detail: 'Soft/hard lockups and oopses mean a kernel thread stalled or crashed; the driver named in the trace is the prime suspect.',
      evidence: tally([...hangs, ...oopses].map((e) => e.message), 6),
      actions: ['Update the kernel and the implicated driver (often GPU, Wi-Fi or storage), and test with the previous kernel from the boot menu.'],
    });
  }

  // ---- Application crashes and hangs ----
  if (faults.length) {
    const sorted = [...faults].sort((a, b) => b.count - a.count);
    const total = faults.reduce((n, f) => n + f.count, 0);
    findings.push({
      id: 'apps',
      severity: sorted[0].count >= 5 ? 'warning' : 'info',
      area: 'Apps',
      title: `${plural(total, 'app crash or hang', 'app crashes and hangs')} across ${plural(faults.length, 'program')}`,
      detail: 'Application faults do not take the system down on their own, but repeat offenders are worth updating, and a crash in a GPU or hardware utility often travels with driver trouble.',
      evidence: sorted
        .slice(0, 8)
        .map((f) => `${f.app}: ${plural(f.count, f.kind, f.kind === 'crash' ? 'crashes' : 'hangs')}, last ${fmtTs(f.last)}`),
      actions: ['Update or reinstall programs that fail repeatedly; remove vendor utilities you do not use.'],
    });
  }

  // ---- Failing updates and crashing services ----
  // Store app and Defender signature failures are routine noise; driver, firmware and OS updates matter.
  const updateTitles = byCat('update')
    .filter((e) => e.id === 20)
    .map((e) => (e.message.match(/error 0x[0-9a-f]+: (.+?)\.?$/i)?.[1] ?? e.message).slice(0, 140))
    .filter((t) => !/^[0-9A-Z]{12}-/.test(t) && !/Security Intelligence Update|\.NET Framework/i.test(t));
  if (updateTitles.length) {
    const titles = new Map<string, number>();
    for (const t of updateTitles) titles.set(t, (titles.get(t) ?? 0) + 1);
    findings.push({
      id: 'updates',
      severity: 'info',
      area: 'Updates',
      title: `Driver and system updates keep failing to install (${plural(updateTitles.length, 'attempt')})`,
      detail: 'Driver and firmware fixes often arrive through Windows Update; a stuck update can leave a known-buggy driver in place.',
      evidence: [...titles.entries()].map(([t, n]) => `${n}x ${t}`),
      actions: ['Install failing driver/firmware updates manually from the manufacturer\'s support site, then re-run Windows Update.'],
    });
  }
  const serviceCrashes = byCat('service');
  if (serviceCrashes.length) {
    findings.push({
      id: 'services',
      severity: 'info',
      area: 'Services',
      title: `Services terminated unexpectedly (${plural(serviceCrashes.length, 'event')})`,
      detail: 'Background services that crash repeatedly can stall logon, shutdown or the apps that depend on them.',
      evidence: tally(serviceCrashes.map((e) => e.message), 5),
      actions: ['Update or uninstall the software that owns the failing service.'],
    });
  }

  findings.sort((a, b) => SEVERITY_RANK[a.severity] - SEVERITY_RANK[b.severity]);
  timeline.sort((a, b) => b.ts - a.ts);

  const verdict: Severity = findings.length ? findings[0].severity : 'ok';
  const causes = findings.filter((f) => f.id !== 'crashes' && (f.severity === 'critical' || f.severity === 'warning') && f.id !== 'apps');
  let headline: string;
  if (crashes.length) {
    headline = `This system crashed or froze ${plural(crashes.length, 'time')} in ${days} days.`;
    if (causes.length) headline += ` Main suspects: ${causes.slice(0, 3).map((f) => f.area.toLowerCase()).join(', ')}.`;
  } else if (verdict === 'critical' || verdict === 'warning') {
    headline = `No crashes recorded, but ${plural(causes.length, 'issue')} need attention.`;
  } else {
    headline = `No stability problems found in the last ${days} days.`;
  }

  const stats: StatChip[] = [
    { label: 'Unexpected shutdowns', value: crashes.length, severity: crashes.length ? 'critical' : 'ok' },
    { label: 'Hard freezes', value: freezes.length, severity: freezes.length ? 'critical' : 'ok' },
    { label: 'Blue screens', value: blueScreens.length, severity: blueScreens.length ? 'critical' : 'ok' },
    { label: 'GPU driver resets', value: gpuEpisodes.length, severity: gpuEpisodes.length ? 'warning' : 'ok' },
    { label: 'Out of memory', value: memEvents.length, severity: memEvents.length ? 'warning' : 'ok' },
    { label: 'App crashes / hangs', value: faults.reduce((n, f) => n + f.count, 0), severity: faults.length ? 'info' : 'ok' },
  ];

  const system: [string, string][] = [];
  const push = (k: string, v: string | null | undefined) => {
    if (v) system.push([k, v]);
  };
  push('OS', sys.os);
  push('Model', sys.model);
  push('Firmware', sys.bios ? `${sys.bios}${sys.biosDate ? ` (${fmtDate(sys.biosDate)})` : ''}` : null);
  push('CPU', sys.cpu ? `${sys.cpu}${sys.microcode ? `, microcode ${sys.microcode}` : ''}` : null);
  push('RAM', sys.ramGb ? `${sys.ramGb} GB` : null);
  for (const d of drivers.filter((d) => d.class === 'DISPLAY')) push(d.device, `driver ${d.version}${d.ageDays != null ? ` (${d.ageDays} days old)` : ''}`);
  push('Last boot', sys.lastBoot ? fmtTs(sys.lastBoot) : null);
  push('Power plan', cfg.powerPlan ?? null);
  if (cfg.vbsRunning != null) push('Virtualization security', `${cfg.vbsRunning ? 'on' : 'off'}, Memory integrity ${cfg.hvciRunning ? 'on' : 'off'}`);
  if (cfg.fastStartup != null) push('Fast Startup', cfg.fastStartup ? 'on' : 'off');
  if (cfg.commitLimitGb) push('Committed memory', `${cfg.commitUsedGb} / ${cfg.commitLimitGb} GB`);

  return {
    generatedAt: raw.collectedAt,
    platform: raw.platform,
    windowDays: days,
    elevated: raw.elevated,
    verdict,
    headline,
    stats,
    findings,
    timeline: timeline.slice(0, 60),
    system,
    notes: arr(raw.notes),
  };
}

const SEVERITY_WORD: Record<Severity, string> = { critical: 'CRITICAL', warning: 'WARNING', info: 'INFO', ok: 'OK' };

export function toMarkdown(r: StabilityReport): string {
  const lines: string[] = [
    '# TaskForge stability report',
    '',
    `Generated ${fmtTs(r.generatedAt)} · last ${r.windowDays} days · ${r.platform}${r.elevated ? '' : ' · not elevated'}`,
    '',
    `**${r.headline}**`,
    '',
    r.stats.map((s) => `${s.label}: ${s.value}`).join(' · '),
    '',
  ];
  for (const f of r.findings) {
    lines.push(`## [${SEVERITY_WORD[f.severity]}] ${f.title}`, '', f.detail, '');
    if (f.evidence.length) lines.push('Evidence:', ...f.evidence.map((e) => `- ${e}`), '');
    if (f.actions.length) lines.push('What to do:', ...f.actions.map((a, i) => `${i + 1}. ${a}`), '');
  }
  if (r.timeline.length) {
    lines.push('## Timeline', '', ...r.timeline.map((t) => `- ${fmtTs(t.ts)} [${SEVERITY_WORD[t.severity]}] ${t.label}`), '');
  }
  lines.push('## System', '', ...r.system.map(([k, v]) => `- ${k}: ${v}`), '');
  if (r.notes.length) lines.push('## Notes', '', ...r.notes.map((n) => `- ${n}`), '');
  return lines.join('\n');
}
