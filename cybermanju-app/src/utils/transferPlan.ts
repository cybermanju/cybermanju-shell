// CyberManju OS — visual transfer board plan model (pure, unit-tested).
//
// `TransferGraph.vue` renders file/folder nodes and provider nodes on a
// canvas; this module owns everything that does not need the DOM: edge
// identity, validation, destination math and plan summaries. Execution
// (read → write → optional delete through the VFS canal) lives in the
// component, which reports per-edge status back here only as data.

export type TransferOp = 'cp' | 'mv'

export interface TransferEndpoint {
  mountId: string
  /** Source file/folder path, or destination *folder* ('' = mount root). */
  remotePath: string
}

export interface TransferEdge {
  id: string
  op: TransferOp
  sourceName: string
  sourceIsDir: boolean
  from: TransferEndpoint
  to: TransferEndpoint
}

export const TRANSFER_LAYOUT_KEY = 'cybermanju.transferLayout.v1'

export interface NodePos {
  x: number
  y: number
}

function cleanSeg(s: string | undefined): string {
  return String(s ?? '').replace(/^\/+|\/+$/g, '')
}

/** Join remote segments without doubled or leading slashes (`''` = root). */
export function joinRemote(...segs: Array<string | undefined>): string {
  return segs.map(cleanSeg).filter(Boolean).join('/')
}

/** Full destination path for an edge's root node (dest folder + source name). */
export function destPathFor(edge: TransferEdge): string {
  return joinRemote(edge.to.remotePath, edge.sourceName)
}

/** Stable dedupe key: same file → same folder twice is one plan row. */
export function planKey(edge: TransferEdge): string {
  return `${edge.from.mountId}:${cleanSeg(edge.from.remotePath)}>${edge.to.mountId}:${cleanSeg(edge.to.remotePath)}`
}

export function makeEdge(
  id: string,
  init: Omit<TransferEdge, 'id' | 'op'> & { op?: TransferOp },
): TransferEdge {
  return {
    id,
    op: init.op ?? 'cp',
    sourceName: init.sourceName,
    sourceIsDir: init.sourceIsDir,
    from: { mountId: init.from.mountId, remotePath: cleanSeg(init.from.remotePath) },
    to: { mountId: init.to.mountId, remotePath: cleanSeg(init.to.remotePath) },
  }
}

export function toggleEdgeOp(edge: TransferEdge): TransferEdge {
  return { ...edge, op: edge.op === 'cp' ? 'mv' : 'cp' }
}

/**
 * Refuse plans that can never work. Returns the reason, or null when the
 * edge is runnable. Same-mount same-path is a no-op for `mv` and a
 * self-overwrite for `cp` — both refused with the same honest message.
 * A folder aimed into its own subtree is refused too: the recursive walk
 * would otherwise chase its own output.
 */
export function validateEdge(edge: TransferEdge): string | null {
  if (!cleanSeg(edge.from.remotePath)) return 'Pick a file or folder first — the mount root itself cannot move.'
  if (!edge.from.mountId || !edge.to.mountId) return 'Both ends need a mounted provider.'
  if (
    edge.from.mountId === edge.to.mountId &&
    destPathFor(edge) === cleanSeg(edge.from.remotePath)
  ) {
    return 'Source and destination are the same path — nothing would change.'
  }
  if (edge.sourceIsDir && edge.from.mountId === edge.to.mountId) {
    const src = cleanSeg(edge.from.remotePath)
    const dest = destPathFor(edge)
    if (dest.startsWith(`${src}/`)) {
      return 'Cannot copy a folder into itself — pick a destination outside it.'
    }
  }
  return null
}

export interface PlanSummary {
  edges: number
  cp: number
  mv: number
  files: number
  folders: number
}

export function summarizePlan(edges: TransferEdge[]): PlanSummary {
  return {
    edges: edges.length,
    cp: edges.filter(e => e.op === 'cp').length,
    mv: edges.filter(e => e.op === 'mv').length,
    files: edges.filter(e => !e.sourceIsDir).length,
    folders: edges.filter(e => e.sourceIsDir).length,
  }
}

export function planSummaryText(s: PlanSummary): string {
  if (!s.edges) return 'No transfers planned — click a file node, then a provider.'
  const bits = [`${s.edges} planned`, `${s.cp} copy`, `${s.mv} move`]
  if (s.folders) bits.push(`${s.folders} folder${s.folders === 1 ? '' : 's'} (recursive)`)
  return bits.join(' · ')
}

/** Clamp a dragged node inside the canvas. */
export function clampPos(p: NodePos, w: number, h: number): NodePos {
  return {
    x: Math.min(Math.max(0, Math.round(p.x)), Math.max(0, w - 40)),
    y: Math.min(Math.max(0, Math.round(p.y)), Math.max(0, h - 40)),
  }
}

/**
 * Largest single file the canal write path accepts (mirrors
 * `writeVfsFile`'s cap). Plans pre-check sizes against this so an
 * oversized file fails the row before a single byte is read — never after
 * megabytes are already in memory.
 */
export const TRANSFER_WRITE_CAP_BYTES = 5 * 1024 * 1024

/** True when a file can never be written through the canal. */
export function overWriteCap(sizeBytes: number): boolean {
  return Number.isFinite(sizeBytes) && sizeBytes > TRANSFER_WRITE_CAP_BYTES
}

/**
 * Byte equality for move verification: the destination is re-read after
 * every write and compared in full — only then may the source be deleted.
 */
export function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false
  let diff = 0
  for (let i = 0; i < a.length; i++) diff |= a[i] ^ b[i]
  return diff === 0
}

/** Node layout persistence (localStorage mirror, never throws). */
export function loadNodeLayout(): Record<string, NodePos> {
  try {
    if (typeof localStorage === 'undefined') return {}
    const raw = localStorage.getItem(TRANSFER_LAYOUT_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as Record<string, unknown>
    const out: Record<string, NodePos> = {}
    for (const [k, v] of Object.entries(parsed)) {
      const p = v as Partial<NodePos>
      if (Number.isFinite(p?.x) && Number.isFinite(p?.y)) out[k] = { x: Number(p.x), y: Number(p.y) }
    }
    return out
  } catch {
    return {}
  }
}

export function saveNodeLayout(layout: Record<string, NodePos>): void {
  try {
    if (typeof localStorage === 'undefined') return
    localStorage.setItem(TRANSFER_LAYOUT_KEY, JSON.stringify(layout))
  } catch {
    // Private mode — layout simply does not persist.
  }
}
