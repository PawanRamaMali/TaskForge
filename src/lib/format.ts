export function fmtBytes(n: number | null | undefined): string {
  if (n == null) return '—';
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function fmtBps(n: number | null | undefined): string {
  if (n == null || n === 0) return '—';
  return `${fmtBytes(n)}/s`;
}

export function fmtPct(n: number | null | undefined, digits = 1): string {
  if (n == null) return '—';
  return `${n.toFixed(digits)}%`;
}
