import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type {
  ActionResult,
  HistorySample,
  PlannedAction,
  PriorityLevel,
  PriorityRule,
  ProcessDetails,
  Snapshot,
} from './types';
import { analyze, toMarkdown, type RawDiagnostics, type StabilityReport } from './diagnostics';
import type { ChangeResult, SettingChange, SettingState } from './settings';
import type { StartupChangeResult, StartupItem } from './startup';

const HISTORY = 60;

export interface Toast {
  id: number;
  kind: 'ok' | 'error';
  text: string;
}

function pushHistory(arr: number[], v: number): number[] {
  const next = arr.length >= HISTORY ? arr.slice(arr.length - HISTORY + 1) : arr.slice();
  next.push(v);
  return next;
}

let toastId = 0;

function initialTheme(): 'dark' | 'light' {
  if (typeof localStorage !== 'undefined') {
    const saved = localStorage.getItem('taskforge-theme');
    if (saved === 'light' || saved === 'dark') return saved;
  }
  return 'dark';
}

class AppStore {
  snapshot = $state<Snapshot | null>(null);
  cpuHistory = $state<number[]>([]);
  memHistory = $state<number[]>([]);
  /** Sparkline for the busiest adapter, used by the mini view. */
  gpuHistory = $state<number[]>([]);
  /** One sparkline per adapter, keyed by GpuInfo.id. */
  gpuHistories = $state<Record<string, number[]>>({});
  diskHistory = $state<number[]>([]); // aggregate MB/s across drives
  netHistory = $state<number[]>([]); // aggregate KB/s across interfaces
  toasts = $state<Toast[]>([]);
  theme = $state<'dark' | 'light'>(initialTheme());
  rules = $state<PriorityRule[]>([]);
  settings = $state<SettingState[]>([]);
  private started = false;

  async start() {
    if (this.started) return;
    this.started = true;
    this.applyTheme();
    this.refreshRules();
    // Seed before subscribing: the backend's buffer already contains the tick
    // we would otherwise receive first, so seeding afterwards would duplicate
    // it at the end of every graph.
    await this.seedHistory();
    await listen<Snapshot>('snapshot', (event) => {
      const s = event.payload;
      this.snapshot = s;
      this.cpuHistory = pushHistory(this.cpuHistory, s.cpu.overall);
      this.memHistory = pushHistory(this.memHistory, (s.mem.used / s.mem.total) * 100);
      this.gpuHistory = pushHistory(this.gpuHistory, s.gpu.utilization ?? 0);
      // Rebuild rather than mutate so a GPU that disappears (eGPU unplugged,
      // driver reset) does not leave a stale sparkline behind.
      const gpuHistories: Record<string, number[]> = {};
      for (const g of s.gpus ?? []) {
        gpuHistories[g.id] = pushHistory(this.gpuHistories[g.id] ?? [], g.utilization ?? 0);
      }
      this.gpuHistories = gpuHistories;
      const diskMBps = s.disks.reduce((acc, d) => acc + d.readBps + d.writeBps, 0) / (1024 * 1024);
      this.diskHistory = pushHistory(this.diskHistory, diskMBps);
      const netKBps = (s.net.rxBps + s.net.txBps) / 1024;
      this.netHistory = pushHistory(this.netHistory, netKBps);
    });
  }

  /** Fills the graphs from the sampler's ring buffer, so a window that opens
   *  later - the mini view, or the main window reopened from the tray - starts
   *  with history instead of 90 seconds of blank sparkline. */
  private async seedHistory() {
    let samples: HistorySample[] | null = null;
    try {
      samples = await invoke<HistorySample[]>('get_history');
    } catch {
      // No backend yet (or demo mode): just start with empty graphs.
      return;
    }
    if (!samples?.length) return;

    const tail = samples.slice(-HISTORY);
    this.cpuHistory = tail.map((h) => h.cpu);
    this.memHistory = tail.map((h) => h.memPct);
    this.gpuHistory = tail.map((h) => h.gpu);
    this.diskHistory = tail.map((h) => h.diskMbps);
    this.netHistory = tail.map((h) => h.netKbps);

    // An adapter may appear part-way through the buffer, so collect every id
    // seen and read a missing tick as zero rather than dropping the series.
    const ids = new Set<string>();
    for (const h of tail) for (const id of Object.keys(h.gpus ?? {})) ids.add(id);
    const gpuHistories: Record<string, number[]> = {};
    for (const id of ids) gpuHistories[id] = tail.map((h) => h.gpus?.[id] ?? 0);
    this.gpuHistories = gpuHistories;
  }

  applyTheme() {
    if (typeof document !== 'undefined') {
      document.documentElement.dataset.theme = this.theme;
    }
  }

  toggleTheme() {
    this.theme = this.theme === 'dark' ? 'light' : 'dark';
    if (typeof localStorage !== 'undefined') localStorage.setItem('taskforge-theme', this.theme);
    this.applyTheme();
  }

  toast(kind: Toast['kind'], text: string) {
    const id = ++toastId;
    this.toasts = [...this.toasts, { id, kind, text }];
    setTimeout(() => {
      this.toasts = this.toasts.filter((t) => t.id !== id);
    }, 5000);
  }

  async processAction(pid: number, action: string, name: string): Promise<boolean> {
    try {
      await invoke('process_action', { pid, action });
      this.toast('ok', `${action.replace(/_/g, ' ')} - ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async setPriority(pid: number, level: PriorityLevel, name: string): Promise<boolean> {
    try {
      await invoke('set_priority', { pid, level });
      this.toast('ok', `priority → ${level} - ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async setEfficiencyMode(pid: number, on: boolean, name: string): Promise<boolean> {
    try {
      await invoke('set_efficiency_mode', { pid, on });
      this.toast('ok', `efficiency mode ${on ? 'on' : 'off'} - ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async setAffinity(pid: number, mask: number, name: string): Promise<boolean> {
    try {
      await invoke('set_affinity', { pid, mask });
      this.toast('ok', `affinity updated - ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async killTree(pid: number, name: string): Promise<ActionResult[]> {
    try {
      const results = await invoke<ActionResult[]>('kill_tree', { pid });
      const ok = results.filter((r) => r.ok).length;
      this.toast('ok', `killed ${ok}/${results.length} in tree - ${name} (${pid})`);
      return results;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return [];
    }
  }

  async getProcessDetails(pid: number): Promise<ProcessDetails | null> {
    try {
      return await invoke<ProcessDetails>('get_process_details', { pid });
    } catch (e) {
      this.toast('error', `details (${pid}): ${e}`);
      return null;
    }
  }

  async restartAsAdmin() {
    try {
      await invoke('restart_as_admin');
    } catch (e) {
      this.toast('error', `elevation: ${e}`);
    }
  }

  async refreshRules() {
    try {
      this.rules = await invoke<PriorityRule[]>('list_rules');
    } catch {
      this.rules = [];
    }
  }

  async setRule(exe: string, priority: PriorityLevel, efficiencyMode: boolean) {
    try {
      await invoke('set_rule', { exe, priority, efficiencyMode });
      await this.refreshRules();
      this.toast('ok', `rule saved - ${exe} → ${priority}`);
    } catch (e) {
      this.toast('error', `rule: ${e}`);
    }
  }

  async removeRule(exe: string) {
    try {
      await invoke('remove_rule', { exe });
      await this.refreshRules();
      this.toast('ok', `rule removed - ${exe}`);
    } catch (e) {
      this.toast('error', `rule: ${e}`);
    }
  }

  async planOptimize(): Promise<PlannedAction[]> {
    return invoke<PlannedAction[]>('plan_optimize');
  }

  async applyOptimize(actions: PlannedAction[]): Promise<ActionResult[]> {
    return invoke<ActionResult[]>('apply_optimize', { actions });
  }

  async runDiagnostics(windowDays: number): Promise<StabilityReport> {
    const raw = await invoke<RawDiagnostics>('run_diagnostics', { windowDays });
    return analyze(raw);
  }

  /** Switch between the full window and the compact always-on-top mini view. */
  async setMiniMode(on: boolean) {
    try {
      await invoke('set_mini_mode', { on });
    } catch (e) {
      this.toast('error', `mini view: ${e}`);
    }
  }

  async listStartup(): Promise<StartupItem[]> {
    const res = await invoke<{ items: StartupItem[] }>('list_startup');
    return res.items ?? [];
  }

  async applyStartup(changes: { id: string; enabled: boolean }[]): Promise<StartupChangeResult[]> {
    let results: StartupChangeResult[];
    try {
      const res = await invoke<{ results: StartupChangeResult[] }>('apply_startup', { changes });
      results = res.results ?? [];
    } catch (e) {
      this.toast('error', `startup: ${e}`);
      return [];
    }
    const failed = results.filter((r) => !r.ok);
    if (failed.length) this.toast('error', failed.map((r) => `${r.id}: ${r.error}`).join(' · '));
    else if (results.length) this.toast('ok', `${results.length} startup item${results.length === 1 ? '' : 's'} updated`);
    return results;
  }

  async loadSettings() {
    try {
      this.settings = await invoke<SettingState[]>('list_settings');
    } catch (e) {
      this.toast('error', `settings: ${e}`);
    }
  }

  async applySettings(changes: SettingChange[]): Promise<ChangeResult[]> {
    let results: ChangeResult[];
    try {
      results = await invoke<ChangeResult[]>('apply_settings', { changes });
    } catch (e) {
      this.toast('error', `settings: ${e}`);
      return [];
    }
    const label = (id: string) => this.settings.find((s) => s.id === id)?.label ?? id;
    const failed = results.filter((r) => !r.ok);
    if (failed.length) {
      this.toast('error', failed.map((r) => `${label(r.id)}: ${r.error}`).join(' · '));
    } else if (results.length) {
      this.toast('ok', `${results.length} setting${results.length === 1 ? '' : 's'} updated`);
    }
    await this.loadSettings();
    return results;
  }

  async saveDiagnosticsReport(report: StabilityReport) {
    try {
      const path = await invoke<string>('save_diagnostics_report', { content: toMarkdown(report) });
      this.toast('ok', `report saved - ${path}`);
    } catch (e) {
      this.toast('error', `save report: ${e}`);
    }
  }
}

export const store = new AppStore();
