// CyberManju OS — Launcher (src/utils/launcher.ts).
// CyberManju launcher helpers — pure + unit-tested (no Tauri imports).
// Ordering/filtering for the Gaveta drawer and the home springboard share
// these so Rust (`sort_apps`) and TS never disagree on tie-breaks.

import type { AndroidApp, LauncherOverride } from '@/types'

export type OverrideMap = Record<string, LauncherOverride>

export function launcherKeyForApp(pkg: string): string {
  return `android:${pkg.trim()}`
}

export function launcherKeyForPanel(panel: string): string {
  return `os:${panel.trim()}`
}

export function displayLabel(fallback: string, override: LauncherOverride | undefined): string {
  const alias = (override?.alias ?? '').trim()
  return alias || fallback
}

export function displayIcon(
  systemIcon: string,
  fallbackLetter: string,
  override: LauncherOverride | undefined,
): { kind: 'image' | 'letter'; src: string } {
  const custom = (override?.customIcon ?? '').trim()
  if (custom) return { kind: 'image', src: custom }
  if (systemIcon) return { kind: 'image', src: systemIcon }
  return { kind: 'letter', src: fallbackLetter.slice(0, 1).toUpperCase() || '?' }
}

export interface OrderedApp {
  app: AndroidApp
  key: string
  name: string
  order: number
}

/** Merge apps + overrides: hidden dropped, ordered-first then label sort. */
export function orderAndroidApps(apps: AndroidApp[], overrides: OverrideMap): OrderedApp[] {
  const rows: OrderedApp[] = []
  for (const app of apps) {
    const key = launcherKeyForApp(app.packageName)
    const ov = overrides[key]
    if (ov?.hidden) continue
    rows.push({
      app,
      key,
      name: displayLabel(app.label, ov),
      order: typeof ov?.order === 'number' ? ov.order : -1,
    })
  }
  rows.sort((a, b) => {
    const ao = a.order >= 0 ? a.order : Number.MAX_SAFE_INTEGER
    const bo = b.order >= 0 ? b.order : Number.MAX_SAFE_INTEGER
    if (ao !== bo) return ao - bo
    const name = a.name.toLowerCase().localeCompare(b.name.toLowerCase())
    if (name !== 0) return name
    return a.app.packageName.localeCompare(b.app.packageName)
  })
  return rows
}

export function filterOrderedApps(rows: OrderedApp[], query: string): OrderedApp[] {
  const term = query.trim().toLowerCase()
  if (!term) return rows
  return rows.filter(
    (r) =>
      r.name.toLowerCase().includes(term) ||
      r.app.packageName.toLowerCase().includes(term),
  )
}

/** Next free order slot for "move to end" / drag append. */
export function nextOrder(overrides: OverrideMap): number {
  let max = -1
  for (const ov of Object.values(overrides)) {
    if (typeof ov.order === 'number' && ov.order > max) max = ov.order
  }
  return max + 1
}

/** Swap two rows' order slots (place change / switch others). */
export function swappedOrders(
  overrides: OverrideMap,
  keyA: string,
  orderA: number,
  keyB: string,
  orderB: number,
): OverrideMap {
  const normA = orderA >= 0 ? orderA : nextOrder(overrides)
  // When B is unordered it takes A's old slot and A appends after the max.
  const normB = orderB >= 0 ? orderB : normA
  const aOrd = orderA >= 0 ? orderB : nextOrder(overrides)
  return {
    ...overrides,
    [keyA]: { ...(overrides[keyA] ?? { alias: '', order: -1, customIcon: '', iconPack: '', hidden: false }), order: aOrd >= 0 ? aOrd : nextOrder(overrides) },
    [keyB]: { ...(overrides[keyB] ?? { alias: '', order: -1, customIcon: '', iconPack: '', hidden: false }), order: normB },
  }
}

export function isValidAlias(alias: string): boolean {
  const t = alias.trim()
  return t.length > 0 && t.length <= 48
}

/**
 * Gallery file → 96px PNG data URL (canvas resize keeps the override store
 * small: ~10-30 KB per custom icon instead of whole camera photos).
 * Runs in the browser only; throws `too_large:` over 8 MiB input.
 */
export async function fileToLauncherIcon(file: Blob): Promise<string> {
  const MAX_INPUT = 8 * 1024 * 1024
  if (file.size > MAX_INPUT) {
    throw new Error('too_large: icon file is over 8 MiB — pick a smaller image')
  }
  const bitmap = await createImageBitmap(file)
  try {
    const size = 96
    const canvas = document.createElement('canvas')
    canvas.width = size
    canvas.height = size
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('unsupported: canvas 2d is unavailable in this WebView')
    // Cover-fit + center so gallery photos fill the squircle.
    const scale = Math.max(size / bitmap.width, size / bitmap.height)
    const w = bitmap.width * scale
    const h = bitmap.height * scale
    ctx.clearRect(0, 0, size, size)
    ctx.drawImage(bitmap, (size - w) / 2, (size - h) / 2, w, h)
    return canvas.toDataURL('image/png')
  } finally {
    try { bitmap.close() } catch { /* older WebViews */ }
  }
}

/** Swipe-up detection shared by home → drawer (Gaveta) gesture. */
export function isSwipeUpGesture(startY: number, endY: number, minDistance = 64): boolean {
  return startY - endY >= minDistance
}

/** Short relative time for stored messages ("now", "5m", "3h", "Tue"). Pure. */
export function formatMessageTime(timestampMs: number, nowMs = Date.now()): string {
  if (!Number.isFinite(timestampMs) || timestampMs <= 0) return ''
  const diff = Math.max(0, nowMs - timestampMs)
  const min = Math.floor(diff / 60_000)
  if (min < 1) return 'now'
  if (min < 60) return `${min}m`
  const hours = Math.floor(min / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.floor(hours / 24)
  if (days < 7) {
    try {
      return new Date(timestampMs).toLocaleDateString([], { weekday: 'short' })
    } catch {
      return `${days}d`
    }
  }
  try {
    return new Date(timestampMs).toLocaleDateString([], { month: 'short', day: 'numeric' })
  } catch {
    return `${days}d`
  }
}

/** Unread = stored after the last seen mark. Pure. */
export function countUnread(timestamps: number[], seenAtMs: number): number {
  let n = 0
  for (const t of timestamps) {
    if (t > seenAtMs) n++
  }
  return n
}
