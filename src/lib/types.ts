export type Category = 'Critical' | 'SystemService' | 'Background' | 'UserApp';

export type OptimizeKind = 'Kill' | 'Suspend' | 'LowerPriority' | 'TrimRam';

export type PriorityLevel = 'Idle' | 'BelowNormal' | 'Normal' | 'AboveNormal' | 'High';

export const PRIORITY_LEVELS: PriorityLevel[] = [
  'Idle',
  'BelowNormal',
  'Normal',
  'AboveNormal',
  'High',
];

export const PRIORITY_LABELS: Record<PriorityLevel, string> = {
  Idle: 'Idle',
  BelowNormal: 'Below normal',
  Normal: 'Normal',
  AboveNormal: 'Above normal',
  High: 'High',
};

export interface NetInfo {
  rxBps: number;
  txBps: number;
  totalRx: number;
  totalTx: number;
}

export interface TempInfo {
  label: string;
  tempC: number;
  maxC: number | null;
}

export interface ProcessDetails {
  pid: number;
  cmd: string[];
  cwd: string | null;
  priority: string | null;
  efficiencyMode: boolean | null;
  affinityMask: number | null;
  coreCount: number;
  environCount: number;
}

export interface PriorityRule {
  exe: string;
  priority: PriorityLevel;
  efficiencyMode: boolean;
}

export interface CpuInfo {
  overall: number;
  perCore: number[];
  coreCount: number;
  brand: string;
}

export interface MemInfo {
  total: number;
  used: number;
  swapTotal: number;
  swapUsed: number;
}

export interface DiskInfo {
  name: string;
  mount: string;
  fs: string;
  total: number;
  available: number;
  readBps: number;
  writeBps: number;
}

export interface GpuInfo {
  available: boolean;
  reason: string | null;
  name: string | null;
  utilization: number | null;
  memUsed: number | null;
  memTotal: number | null;
  temperatureC: number | null;
}

export interface ProcInfo {
  pid: number;
  parentPid: number | null;
  name: string;
  exe: string | null;
  user: string | null;
  cpu: number;
  cpuRaw: number;
  memBytes: number;
  diskReadBps: number;
  diskWriteBps: number;
  gpuUtil: number | null;
  gpuMemBytes: number | null;
  category: Category;
  safeToKill: boolean;
  suspended: boolean;
  runTime: number;
  startTime: number;
  status: string;
}

export interface Snapshot {
  ts: number;
  cpu: CpuInfo;
  mem: MemInfo;
  disks: DiskInfo[];
  net: NetInfo;
  temps: TempInfo[];
  gpu: GpuInfo;
  processes: ProcInfo[];
  elevated: boolean;
}

export interface PlannedAction {
  pid: number;
  name: string;
  category: Category;
  kind: OptimizeKind;
  reason: string;
}

export interface ActionResult {
  pid: number;
  name: string;
  kind: OptimizeKind;
  ok: boolean;
  error: string | null;
}

export const CATEGORY_LABELS: Record<Category, string> = {
  UserApp: 'Apps',
  Background: 'Background processes',
  SystemService: 'OS services',
  Critical: 'Critical system',
};

export const CATEGORY_ORDER: Category[] = ['UserApp', 'Background', 'SystemService', 'Critical'];
