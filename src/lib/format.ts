export function formatHz(hz: number): string {
  if (hz >= 1_000_000) return `${trim(hz / 1_000_000)} MHz`;
  if (hz >= 1_000) return `${trim(hz / 1_000)} kHz`;
  return `${hz} Hz`;
}

function trim(n: number): string {
  return Number.parseFloat(n.toFixed(4)).toString();
}

export function formatBytes(n: number): string {
  if (n >= 1024 && n % 1024 === 0) return `${n / 1024} KiB`;
  if (n >= 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${n} B`;
}

export function percent(used: number, total: number): number {
  return total > 0 ? (used / total) * 100 : 0;
}

export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}

export function timeAgo(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86_400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(ms).toLocaleDateString();
}

export function clockTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}.${p(d.getMilliseconds(), 3)}`;
}

/** Last path component, for both `/` and `\` separators. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Joins with the separator already used in `base`. */
export function joinPath(base: string, ...parts: string[]): string {
  const sep = base.includes("\\") && !base.includes("/") ? "\\" : "/";
  return [base.replace(/[\\/]+$/, ""), ...parts].join(sep);
}
