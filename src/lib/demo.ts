// Development-only demo mode: a fake backend so the UI runs in a normal browser
// with realistic, made-up data (used for README screenshots). Enabled from
// +layout.ts with ?demo; ?view=mini shows the mini view.

import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { Category, PlannedAction, ProcInfo, ProcessDetails, Snapshot } from './types';
import type { RawDiagnostics } from './diagnostics';
import type { ChangeResult, SettingChange, SettingState, SettingValue } from './settings';

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const STARTED = Math.floor(Date.now() / 1000);

/** Smooth, repeatable wave in [0, 1]. */
function wave(t: number, period: number, phase = 0): number {
  return (Math.sin((t / period) * Math.PI * 2 + phase) + 1) / 2;
}

/** Repeatable pseudo-random value in [0, 1). */
function noise(t: number, seed: number): number {
  const x = Math.sin(t * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

type ProcSeed = [name: string, category: Category, cpu: number, memMb: number, gpu?: number];

const PROCS: ProcSeed[] = [
  ['chrome.exe', 'UserApp', 6, 1450],
  ['Code.exe', 'UserApp', 3.5, 980],
  ['python.exe', 'UserApp', 9, 3100, 38],
  ['ollama.exe', 'UserApp', 4, 5200, 22],
  ['Spotify.exe', 'UserApp', 0.8, 310],
  ['Discord.exe', 'UserApp', 1.1, 420],
  ['explorer.exe', 'UserApp', 0.6, 260],
  ['WindowsTerminal.exe', 'UserApp', 0.4, 140],
  ['OneDrive.exe', 'Background', 0.3, 120],
  ['steamwebhelper.exe', 'Background', 0.5, 380],
  ['GoogleUpdate.exe', 'Background', 0.1, 14],
  ['NVIDIA Web Helper.exe', 'Background', 0.2, 60],
  ['SearchHost.exe', 'Background', 0.2, 180],
  ['PhoneExperienceHost.exe', 'Background', 0.1, 90],
  ['MsMpEng.exe', 'SystemService', 1.4, 450],
  ['svchost.exe', 'SystemService', 0.3, 45],
  ['svchost.exe', 'SystemService', 0.2, 30],
  ['svchost.exe', 'SystemService', 0.1, 22],
  ['spoolsv.exe', 'SystemService', 0, 12],
  ['audiodg.exe', 'SystemService', 0.4, 28],
  ['dwm.exe', 'Critical', 1.2, 250, 4],
  ['csrss.exe', 'Critical', 0.1, 8],
  ['lsass.exe', 'Critical', 0.1, 22],
  ['services.exe', 'Critical', 0, 11],
  ['wininit.exe', 'Critical', 0, 6],
  ['System', 'Critical', 0.5, 4],
  ['Registry', 'Critical', 0, 90],
];

function makeSnapshot(t: number): Snapshot {
  const cores = 24;
  const load = 14 + 22 * wave(t, 40) + 8 * noise(t, 1);
  const processes: ProcInfo[] = PROCS.map(([name, category, cpu, memMb, gpu], i) => {
    const c = Math.max(0, cpu * (0.6 + 0.8 * noise(t, i + 3)) * (0.7 + load / 40));
    const userOwned = category === 'UserApp' || category === 'Background';
    return {
      pid: 1000 + i * 124,
      parentPid: category === 'Critical' ? 4 : 812,
      name,
      exe: category === 'Critical' ? null : `C:\\Program Files\\${name.replace('.exe', '')}\\${name}`,
      user: userOwned ? 'alex' : 'SYSTEM',
      cpu: c,
      cpuRaw: c * cores,
      memBytes: memMb * MB * (0.95 + 0.1 * wave(t, 30, i)),
      diskReadBps: i === 2 ? 2.4 * MB * wave(t, 12) : noise(t, i) * 40_000,
      diskWriteBps: i === 0 ? 0.6 * MB * wave(t, 9) : noise(t, i + 7) * 20_000,
      gpuUtil: gpu != null ? Math.round(gpu * (0.6 + 0.8 * wave(t, 20, i))) : null,
      gpuMemBytes: gpu != null ? (name === 'ollama.exe' ? 6.2 : 1.1) * GB : null,
      category,
      safeToKill: userOwned,
      suspended: false,
      runTime: Math.round(18_000 + i * 300 + t * 1.5),
      startTime: STARTED - 18_000 - i * 300,
      status: 'Run',
    };
  });

  return {
    ts: Date.now(),
    cpu: {
      overall: load,
      perCore: Array.from({ length: cores }, (_, i) => Math.min(100, load * (0.4 + 1.2 * noise(t, i + 50)))),
      coreCount: cores,
      brand: 'Intel(R) Core(TM) Ultra 9 275HX',
    },
    mem: { total: 64 * GB, used: (21 + 3 * wave(t, 60)) * GB, swapTotal: 16 * GB, swapUsed: 0.4 * GB },
    disks: [
      {
        name: 'Windows',
        mount: 'C:\\',
        fs: 'NTFS',
        total: 952 * GB,
        available: 191 * GB,
        readBps: 3.2 * MB * wave(t, 12),
        writeBps: 1.1 * MB * wave(t, 9, 1),
      },
      {
        name: 'Data',
        mount: 'D:\\',
        fs: 'NTFS',
        total: 954 * GB,
        available: 353 * GB,
        readBps: 0.4 * MB * noise(t, 90),
        writeBps: 0,
      },
    ],
    net: {
      rxBps: 180_000 + 900_000 * wave(t, 18) * noise(t, 91),
      txBps: 40_000 + 120_000 * noise(t, 92),
      totalRx: 12 * GB,
      totalTx: 2 * GB,
    },
    temps: [{ label: 'CPU Package', tempC: 56 + 12 * wave(t, 40), maxC: 100 }],
    gpu: {
      available: true,
      reason: null,
      name: 'NVIDIA GeForce RTX 5070 Ti Laptop GPU',
      utilization: Math.round(30 + 35 * wave(t, 25)),
      memUsed: 7.6 * GB,
      memTotal: 12 * GB,
      temperatureC: Math.round(52 + 14 * wave(t, 25)),
    },
    processes,
    elevated: true,
  };
}

function demoDiagnostics(): RawDiagnostics {
  const now = Math.floor(Date.now() / 1000);
  const at = (daysAgo: number, hours = 0) => now - daysAgo * 86_400 + hours * 3600;
  const gpu = (ts: number, message: string) => ({ ts, source: 'nvlddmkm', id: 153, level: 2, category: 'gpu', message });
  const tdr = [at(9), at(9) + 1, at(9) + 2, at(16, 3), at(21) - 40, at(27, 5), at(33, 2), at(40, 7)];
  return {
    schema: 1,
    platform: 'windows',
    collectedAt: now,
    windowDays: 60,
    elevated: true,
    system: {
      os: 'Microsoft Windows 11 Pro (build 26200)',
      model: 'Contoso Laptop 16',
      bios: 'C1CN45WW',
      biosDate: at(290),
      biosAgeDays: 290,
      cpu: 'Intel(R) Core(TM) Ultra 9 275HX',
      microcode: '0x11B',
      ramGb: 63.4,
      lastBoot: at(0, -3),
    },
    shutdowns: [
      {
        ts: at(2),
        lastAlive: at(2, -17),
        bugcheck: 0,
        params: '0x0, 0x0, 0x0, 0x0',
        sleepInProgress: false,
        resumedFromSleep: false,
        powerButton: false,
        longPowerPress: false,
      },
      {
        ts: at(21),
        lastAlive: at(21) - 600,
        bugcheck: 0x116,
        params: '0xffffbd88e6fd6010, 0xfffff803499f1050, 0xffffffffc000009a, 0x4',
        sleepInProgress: false,
        resumedFromSleep: true,
        powerButton: false,
        longPowerPress: false,
      },
    ],
    restarts: [
      {
        ts: at(5, 1),
        process: 'C:\\WINDOWS\\uus\\AMD64\\MoUsoCoreWorker.exe (DEMO-PC)',
        reason: 'Operating System: Service pack (Planned)',
        action: 'restart',
        user: 'NT AUTHORITY\\SYSTEM',
      },
      {
        ts: at(12, 2),
        process: 'C:\\WINDOWS\\servicing\\TrustedInstaller.exe (DEMO-PC)',
        reason: 'Operating System: Upgrade (Planned)',
        action: 'restart',
        user: 'NT AUTHORITY\\SYSTEM',
      },
    ],
    bugchecks: [{ ts: at(21), code: '0x00000116', params: null, dump: '' }],
    kernelReports: [
      { kind: 'LiveKernelEvent', code: '141', param1: 'ffffe207f479d010', submissions: 14, first: at(55), last: at(3) },
    ],
    events: [
      ...tdr.map((ts) => gpu(ts, '\\Device\\Video8 Reset TDR occurred on GPUID:100')),
      { ts: at(18), source: 'Netwaw18', id: 5005, level: 2, category: 'network', message: 'Has encountered an internal error and has failed.' },
    ],
    appFaults: [
      { app: 'llama-server.exe', kind: 'crash', count: 1, first: at(9), last: at(9), times: [at(9) + 1] },
      { app: 'VendorUpdater.exe', kind: 'crash', count: 4, first: at(30), last: at(6), times: [at(30), at(20), at(12), at(6)] },
    ],
    disks: [
      {
        name: 'NVMe SSD 1TB',
        media: 'SSD',
        bus: 'NVMe',
        health: 'Healthy',
        operational: 'OK',
        sizeGb: 954,
        wearPct: 3,
        tempC: 41,
        tempMaxC: 67,
        readErrors: 0,
        writeErrors: 0,
        powerOnHours: 1210,
      },
    ],
    volumes: [{ drive: 'C:', fs: 'NTFS', health: 'Healthy', sizeGb: 952, freeGb: 191 }],
    drivers: [
      {
        device: 'NVIDIA GeForce RTX 5070 Ti Laptop GPU',
        class: 'DISPLAY',
        provider: 'NVIDIA',
        version: '32.0.15.9201',
        date: at(212),
        ageDays: 212,
      },
      {
        device: 'Intel(R) Wi-Fi 7 BE200 320MHz',
        class: 'NET',
        provider: 'Intel',
        version: '24.20.2.1',
        date: at(215),
        ageDays: 215,
      },
    ],
    config: {
      crashDumpMode: 3,
      autoReboot: true,
      minidumpCount: 0,
      memoryDumpPresent: false,
      liveKernelDumps: null,
      fastStartup: true,
      hypervisorPresent: true,
      vbsRunning: true,
      hvciRunning: true,
      pagefileAuto: true,
      commitUsedGb: 24.1,
      commitLimitGb: 88.4,
      memoryDiagnostic: null,
      powerPlan: 'Balanced',
      bootsInWindow: 14,
      cleanShutdowns: 12,
      topMemory: [],
    },
    notes: [],
  };
}

function demoSettings(): SettingState[] {
  const on: SettingValue = { on: true };
  const off: SettingValue = { on: false };
  const item = (
    id: string,
    group: string,
    label: string,
    description: string,
    value: SettingValue,
    recommended: SettingValue | null,
    extra: Partial<SettingState> = {}
  ): SettingState => ({
    id,
    group,
    label,
    description,
    kind: 'on' in value ? 'toggle' : 'hours',
    value,
    recommended,
    supported: true,
    note: null,
    needsAdmin: true,
    needsRestart: false,
    canRestore: false,
    ...extra,
  });
  return [
    item('crash_key', 'Crash capture', 'Ctrl+Scroll crash key', 'When the PC freezes, hold Right Ctrl and press Scroll Lock twice to save a crash dump that shows what was stuck.', off, on, { needsRestart: true }),
    item('dump_automatic', 'Crash capture', 'Automatic memory dump', 'Save a kernel memory dump on a blue screen instead of only a small minidump.', off, on, { needsRestart: true }),
    item('keep_dumps', 'Crash capture', 'Keep dump files', 'Keep the memory dump even when disk space is low, and keep up to 50 minidumps.', on, on, { canRestore: true }),
    item('dumps_not_cleaned', 'Crash capture', 'Keep dumps out of disk cleanup', 'Stop automatic disk cleanup from deleting crash dump files.', off, on),
    item('fast_startup', 'Startup', 'Fast Startup', 'Shut down saves the kernel and drivers to disk. With it off, every shutdown gives a clean start.', on, off, { note: 'Takes effect at the next shutdown.' }),
    item('no_auto_restart', 'Updates', 'No auto-restart while signed in', 'Windows Update installs updates but waits for you to restart.', off, on, { note: 'Windows Home may not always follow this policy.' }),
    item('restart_notify', 'Updates', 'Restart notifications', 'Show a notification when an update needs a restart.', on, on),
    item('active_hours', 'Updates', 'Active hours', "Windows won't restart for updates during these hours. 18 hours at most.", { start: 8, end: 23 }, null),
    item('replace_taskmgr', 'System', 'Open TaskForge with Ctrl+Shift+Esc', 'Make Windows open TaskForge in place of Task Manager, including from Ctrl+Alt+Del > Task Manager and Ctrl+Shift+Esc.', off, null, { note: "Turn this off before uninstalling TaskForge, or those shortcuts won't open anything." }),
    item('autostart', 'App', 'Start TaskForge at sign-in', 'Open TaskForge when you sign in to Windows.', on, null, { needsAdmin: false }),
  ];
}

const PLAN: PlannedAction[] = [
  { pid: 1000 + 10 * 124, name: 'GoogleUpdate.exe', category: 'Background', kind: 'Kill', reason: 'Updater helper; not needed while you work' },
  { pid: 1000 + 9 * 124, name: 'steamwebhelper.exe', category: 'Background', kind: 'TrimRam', reason: 'Idle for 20 min, holding 380 MB' },
  { pid: 1000 + 2 * 124, name: 'python.exe', category: 'UserApp', kind: 'LowerPriority', reason: 'Sustained high CPU for 5 min' },
];

export function installDemo(view: string | null) {
  mockWindows(view === 'mini' ? 'mini' : 'main');
  const settings = demoSettings();

  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case 'list_rules':
          return [];
        case 'run_diagnostics':
          return demoDiagnostics();
        case 'list_settings':
          return settings.map((s) => ({ ...s }));
        case 'apply_settings': {
          const { changes } = args as unknown as { changes: SettingChange[] };
          return changes.map((c): ChangeResult => {
            const s = settings.find((x) => x.id === c.id);
            if (s && c.value) {
              s.value = c.value;
              s.canRestore = true;
            }
            return { id: c.id, ok: true, error: null };
          });
        }
        case 'plan_optimize':
          return PLAN;
        case 'apply_optimize':
          return PLAN.map((a) => ({ pid: a.pid, name: a.name, kind: a.kind, ok: true, error: null }));
        case 'get_process_details': {
          const { pid } = args as unknown as { pid: number };
          const details: ProcessDetails = {
            pid,
            cmd: ['C:\\Program Files\\Demo\\app.exe', '--profile', 'default'],
            cwd: 'C:\\Users\\alex',
            priority: 'Normal',
            efficiencyMode: false,
            affinityMask: 2 ** 24 - 1,
            coreCount: 24,
            environCount: 58,
          };
          return details;
        }
        case 'plugin:window|available_monitors':
          return [];
        case 'plugin:window|scale_factor':
          return 1;
        default:
          return null;
      }
    },
    { shouldMockEvents: true }
  );

  // Fill the 90-second graphs right away, then tick like the real sampler.
  let t = 0;
  const send = () => {
    void emit('snapshot', makeSnapshot(t));
    t += 1;
  };
  setTimeout(() => {
    for (let i = 0; i < 60; i++) setTimeout(send, i * 15);
    setInterval(send, 1500);
  }, 400);
}
