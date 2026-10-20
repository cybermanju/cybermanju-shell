// CyberManju OS — Format (src/utils/format.ts).
// Role: shared byte/percentage formatting — one implementation for every panel.
// Exports: humanBytes, diskPct.
// Integrates: standalone (no imports); used by DiskManagerPage + panels.
// Contracts: none special (pure; zero/NaN-safe, never throws).

/** `1536` → `"1.5 KB"`, `0`/null → `"0 B"`. Binary (1024) units. */
export function humanBytes(bytes: number | null | undefined): string {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value >= 10 || unit === 0 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`
}

/** Finite 0–100 percentage; missing/invalid capacity or usage falls back to 0. */
export function diskPct(
  used: number | null | undefined,
  capacity: number | null | undefined,
): number {
  if (typeof capacity !== 'number' || !Number.isFinite(capacity) || capacity <= 0) return 0
  const safeUsed = typeof used === 'number' && Number.isFinite(used) ? used : 0
  return Math.max(0, Math.min(100, (safeUsed / capacity) * 100))
}
