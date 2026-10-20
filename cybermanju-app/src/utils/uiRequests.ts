// CyberManju — one-shot UI requests from the agent (`ui_open_panel` /
// `ui_notify`). The Rust side stamps `JobState.ui_request` with a
// monotonic `ui_seq`; the SSE/poll snapshot carries it. This module
// applies a request exactly once per seq (dedupe by the caller) and
// validates the panel id against the same `MODULE_METADATA` the window
// manager renders from — an unknown id never opens a ghost window.

import { resolvePanel } from './panels'
import { MODULE_METADATA, type PanelType } from '@/types'

/** The wire shape the Rust `JobState.ui_request` carries. */
export interface UiRequest {
  op: 'open' | 'notify'
  panel?: string
  tab?: string
  path?: string
  level?: string
  msg?: string
  seq: number
}

export interface UiRequestHandlers {
  /** Open a panel — the window manager's `open(panel, props)`. */
  open: (panel: PanelType, props?: Record<string, unknown>) => void
  /** Fire a toast — the store's `notify(level, msg)`. */
  notify: (level: 'info' | 'success' | 'warning' | 'error', msg: string) => void
}

/** Panel ids the window manager can actually render — `landing` is a shell
 *  state, not a window (it has no component in the panel map). */
const NON_WINDOW_PANELS = new Set(['landing'])

/** True when `panel` names a window the shell can actually open. Canonical
 *  ids live in `MODULE_METADATA`; aliases (`cron`, `storage`, …) resolve via
 *  `resolvePanel` onto one of those. */
export function isKnownPanel(panel: string): boolean {
  const p = panel.trim()
  if (!p || NON_WINDOW_PANELS.has(p)) return false
  if (p in MODULE_METADATA) return true
  // An alias resolves to a canonical id that must itself be openable.
  const target = resolvePanel(p as PanelType)
  return target !== p && target in MODULE_METADATA && !NON_WINDOW_PANELS.has(target)
}

/**
 * Apply one `ui_request`. Returns a human string for the tool result /
 * log, or `null` when the request was malformed (never throws — a bad
 * request must not kill the agent run).
 */
export function applyUiRequest(req: UiRequest | null | undefined, handlers: UiRequestHandlers): string | null {
  if (!req || typeof req !== 'object') return null
  if (typeof req.seq !== 'number' || !Number.isFinite(req.seq)) return null
  if (req.op === 'open') {
    const panel = String(req.panel ?? '').trim()
    if (!panel || !isKnownPanel(panel)) return null
    const props: Record<string, unknown> = {}
    if (req.tab) props.tab = req.tab
    if (req.path) props.path = req.path
    handlers.open(resolvePanel(panel as PanelType), props)
    return `opened ${panel}`
  }
  if (req.op === 'notify') {
    const levelRaw = String(req.level ?? 'info')
    const level = (['info', 'success', 'warning', 'error'] as const).includes(levelRaw as never)
      ? (levelRaw as 'info' | 'success' | 'warning' | 'error')
      : 'info'
    const msg = String(req.msg ?? '').trim()
    if (!msg) return null
    handlers.notify(level, msg)
    return `notified (${level})`
  }
  return null
}

/** Track the highest seq applied so a replayed snapshot never re-opens a
 *  panel or re-fires a toast. One deduper per page session. */
export function createUiRequestDedupe() {
  let lastSeq = 0
  return {
    /** Apply once; returns the applied-note, or null when new/dup/malformed. */
    apply(req: UiRequest | null | undefined, handlers: UiRequestHandlers): string | null {
      if (!req || typeof req.seq !== 'number' || req.seq <= lastSeq) return null
      const note = applyUiRequest(req, handlers)
      lastSeq = req.seq
      return note
    },
    get seq(): number {
      return lastSeq
    },
  }
}
