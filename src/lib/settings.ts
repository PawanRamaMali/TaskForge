// Stability settings shared types (see src-tauri/src/settings).

export type ToggleValue = { on: boolean };
export type HoursValue = { start: number; end: number };
export type SettingValue = ToggleValue | HoursValue;

export interface SettingState {
  id: string;
  group: string;
  label: string;
  description: string;
  kind: 'toggle' | 'hours';
  value: SettingValue | null;
  recommended: SettingValue | null;
  supported: boolean;
  note: string | null;
  needsAdmin: boolean;
  needsRestart: boolean;
  canRestore: boolean;
}

export interface SettingChange {
  id: string;
  /** null restores the value saved before TaskForge changed it. */
  value: SettingValue | null;
}

export interface ChangeResult {
  id: string;
  ok: boolean;
  error: string | null;
}

/** A setting change suggested by a Stability check finding. */
export interface SettingFix {
  id: string;
  value: SettingValue;
  label: string;
}

export const GROUP_ORDER = ['Crash capture', 'Startup', 'Updates', 'App'];

export function isToggle(v: SettingValue | null | undefined): v is ToggleValue {
  return !!v && 'on' in v;
}

export function sameValue(a: SettingValue | null | undefined, b: SettingValue | null | undefined): boolean {
  if (!a || !b) return a === b;
  if (isToggle(a) || isToggle(b)) return isToggle(a) && isToggle(b) && a.on === b.on;
  return a.start === b.start && a.end === b.end;
}

export function hourLabel(h: number): string {
  return `${String(h).padStart(2, '0')}:00`;
}

/** Hours covered by active hours that run from start to end (wrapping past midnight). */
export function hoursSpan(v: HoursValue): number {
  return (v.end - v.start + 24) % 24;
}

export function describeValue(v: SettingValue | null | undefined): string {
  if (!v) return 'unknown';
  if (isToggle(v)) return v.on ? 'On' : 'Off';
  return `${hourLabel(v.start)} to ${hourLabel(v.end)}`;
}
