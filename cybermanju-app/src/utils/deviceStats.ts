// CyberManju OS — Device stats formatting (src/utils/deviceStats.ts).
// Role: never-throw formatters for live telemetry shown in Devices/Process panels.
// Exports: TopSnapshot, isFiniteNum, formatCpuPercent, formatLoadTriplet,
//   formatTopLine, formatHeapLine, formatCoord, formatBytesDec.
// Integrates: standalone (no imports); DevicesPanel + ProcessPanel render
//   `store.osTop`, whose wasm twin answers tasks/counts/uptime ONLY —
//   `cpuPercent`/`mem`/`load` are undefined there, and a bare `.toFixed()`
//   on them crashed the Devices window on mount (every 4s poll).
// Contracts: every export is total — null/undefined/NaN/non-numeric input
//   yields an 'n/a'-style fallback, never throws, never fabricates readings.

/** Structural subset of `OsTop` — every field optional because the wasm
 *  `top` command answers tasks/counts/uptimeMs only (no host telemetry). */
export interface TopSnapshot {
  cpuPercent?: number | null
  uptimeMs?: number | null
  load?: {
    load1?: number | null
    load5?: number | null
    load15?: number | null
    source?: string | null
  } | null
  mem?: {
    rssBytes?: number | null
    totalBytes?: number | null
    availableBytes?: number | null
  } | null
}

/** Finite-number guard — the single check every formatter below funnels through. */
export function isFiniteNum(n: unknown): n is number {
  return typeof n === 'number' && Number.isFinite(n)
}

/** `'12.3%'`, or `'n/a'` when the runtime reports no CPU reading (wasm). */
export function formatCpuPercent(top: TopSnapshot | null | undefined): string {
  const cpu = (top as TopSnapshot | null | undefined)?.cpuPercent
  return isFiniteNum(cpu) ? `${cpu.toFixed(1)}%` : 'n/a'
}

/** `'0.42 0.37 0.31'`, or `'n/a'` unless all three load averages are numeric. */
export function formatLoadTriplet(
  load: TopSnapshot['load'] | null | undefined,
): string {
  if (!load || typeof load !== 'object') return 'n/a'
  const { load1, load5, load15 } = load
  return isFiniteNum(load1) && isFiniteNum(load5) && isFiniteNum(load15)
    ? `${load1} ${load5} ${load15}`
    : 'n/a'
}

/** Decimal bytes: `1.5 GB` / `12.3 MB` / `512 KB`. Non-numeric → `'n/a'`. */
export function formatBytesDec(n: unknown): string {
  if (!isFiniteNum(n) || n < 0) return 'n/a'
  if (n > 1e9) return `${(n / 1e9).toFixed(1)} GB`
  if (n > 1e6) return `${(n / 1e6).toFixed(1)} MB`
  return `${Math.round(n / 1e3)} KB`
}

/** The Devices SYSTEM line. Null → the unavailable sentence; partial
 *  (wasm) → real readings where present, `n/a` where the sandbox has none. */
export function formatTopLine(top: TopSnapshot | null | undefined): string {
  if (!top || typeof top !== 'object') {
    return 'Native CPU/RAM metrics unavailable in this runtime.'
  }
  const parts = [`app CPU ${formatCpuPercent(top)}`]
  parts.push(`app RSS ${formatBytesDec(top.mem?.rssBytes)}`)
  const total = top.mem?.totalBytes
  if (isFiniteNum(total) && total > 0) {
    const avail = top.mem?.availableBytes
    const used = Math.max(0, total - (isFiniteNum(avail) ? avail : 0))
    parts.push(`RAM ${formatBytesDec(used)} / ${formatBytesDec(total)}`)
  } else {
    parts.push('system RAM unavailable')
  }
  if (top.load?.source === 'proc' && isFiniteNum(top.load?.load1)) {
    parts.push(`load ${(top.load.load1 as number).toFixed(2)}`)
  }
  return parts.join(' · ')
}

/** WebView JS-heap line: reading / live value / not-exposed fallback. */
export function formatHeapLine(
  supported: boolean,
  memory: { usedJSHeapSize?: number | null; jsHeapSizeLimit?: number | null } | null | undefined,
): string {
  const used = memory?.usedJSHeapSize
  if (!isFiniteNum(used) || used <= 0) {
    return supported ? 'memory: reading…' : 'memory API not exposed here.'
  }
  const lim = memory?.jsHeapSizeLimit
  const limPart = isFiniteNum(lim) && lim > 0 ? ` / ${(lim / 1048576).toFixed(0)} MB` : ''
  return `JS heap ${(used / 1048576).toFixed(0)} MB${limPart}`
}

/** Safe coordinate: finite → fixed decimals, anything else → `'—'`. */
export function formatCoord(v: unknown, digits = 5): string {
  return isFiniteNum(v) ? (v as number).toFixed(digits) : '—'
}
