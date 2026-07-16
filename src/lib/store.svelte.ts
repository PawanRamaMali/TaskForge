import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type { ActionResult, PlannedAction, Snapshot } from './types';

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

class AppStore {
  snapshot = $state<Snapshot | null>(null);
  cpuHistory = $state<number[]>([]);
  memHistory = $state<number[]>([]);
  gpuHistory = $state<number[]>([]);
  diskHistory = $state<number[]>([]); // aggregate MB/s across drives
  toasts = $state<Toast[]>([]);
  private started = false;

  async start() {
    if (this.started) return;
    this.started = true;
    await listen<Snapshot>('snapshot', (event) => {
      const s = event.payload;
      this.snapshot = s;
      this.cpuHistory = pushHistory(this.cpuHistory, s.cpu.overall);
      this.memHistory = pushHistory(this.memHistory, (s.mem.used / s.mem.total) * 100);
      this.gpuHistory = pushHistory(this.gpuHistory, s.gpu.utilization ?? 0);
      const diskMBps =
        s.disks.reduce((acc, d) => acc + d.readBps + d.writeBps, 0) / (1024 * 1024);
      this.diskHistory = pushHistory(this.diskHistory, diskMBps);
    });
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
      this.toast('ok', `${action.replace('_', ' ')} — ${name} (${pid})`);
      return true;
    } catch (e) {
      this.toast('error', `${name} (${pid}): ${e}`);
      return false;
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
