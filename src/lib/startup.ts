// Startup manager types (see src-tauri/src/startup).

export interface StartupItem {
  id: string;
  name: string;
  command: string;
  source: string;
  scope: 'user' | 'machine';
  kind: string;
  /** OS / desktop component (Microsoft-signed on Windows) — kept by "disable non-essential". */
  essential: boolean;
  enabled: boolean;
}

export interface StartupChangeResult {
  id: string;
  ok: boolean;
  error: string | null;
}
