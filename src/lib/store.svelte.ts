import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type {
  ActionResult,
  PlannedAction,
  PriorityLevel,
  PriorityRule,
  ProcessDetails,
  Snapshot,
} from './types';

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
  gpuHistory = $state<number[]>([]);
  diskHistory = $state<number[]>([]); // aggregate MB/s across drives
  netHistory = $state<number[]>([]); // aggregate KB/s across interfaces
  toasts = $state<Toast[]>([]);
  theme = $state<'dark' | 'light'>(initialTheme());
  rules = $state<PriorityRule[]>([]);
  private started = false;

  async start() {
    if (this.started) return;
    this.started = true;
    this.applyTheme();
    this.refreshRules();
    await listen<Snapshot>('snapshot', (event) => {
      const s = event.payload;
      this.snapshot = s;
      this.cpuHistory = pushHistory(this.cpuHistory, s.cpu.overall);
      this.memHistory = pushHistory(this.memHistory, (s.mem.used / s.mem.total) * 100);
      this.gpuHistory = pushHistory(this.gpuHistory, s.gpu.utilization ?? 0);
      const diskMBps = s.disks.reduce((acc, d) => acc + d.readBps + d.writeBps, 0) / (1024 * 1024);
      this.diskHistory = pushHistory(this.diskHistory, diskMBps);
      const netKBps = (s.net.rxBps + s.net.txBps) / 1024;
      this.netHistory = pushHistory(this.netHistory, netKBps);
    });
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
      this.toast('ok', `${action.replace(/_/g, ' ')} — ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async setPriority(pid: number, level: PriorityLevel, name: string): Promise<boolean> {
    try {
      await invoke('set_priority', { pid, level });
      this.toast('ok', `priority → ${level} — ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async setEfficiencyMode(pid: number, on: boolean, name: string): Promise<boolean> {
    try {
      await invoke('set_efficiency_mode', { pid, on });
      this.toast('ok', `efficiency mode ${on ? 'on' : 'off'} — ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
    }
  }

  async setAffinity(pid: number, mask: number, name: string): Promise<boolean> {
    try {
      await invoke('set_affinity', { pid, mask });
      this.toast('ok', `affinity updated — ${name} (${pid})`);
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
      this.toast('ok', `killed ${ok}/${results.length} in tree — ${name} (${pid})`);
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
      this.toast('ok', `rule saved — ${exe} → ${priority}`);
    } catch (e) {
      this.toast('error', `rule: ${e}`);
    }
  }

  async removeRule(exe: string) {
    try {
      await invoke('remove_rule', { exe });
      await this.refreshRules();
      this.toast('ok', `rule removed — ${exe}`);
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
}

export const store = new AppStore();
