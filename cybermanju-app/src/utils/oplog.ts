// CyberManju OS — Op-log conflicts (src/utils/oplog.ts).
// Role: conflict-resolver data layer (ANALYSIS.md §2C) — shape fork pairs from
// `GET /api/oplog/conflicts`, build keep-a / keep-b / merge resolution payloads
// for `POST /api/oplog/resolve`, and plan queue-and-replay pulls.
// Exports: OpEntry, OpConflict, conflictTitle, conflictSummary, resolutionPayload,
// resolveLabel, syncPlanSteps.
// Integrates: SyncPanel CONFLICTS card renders pairs; `oplog` cybsh verb reuses
// the payload builders; server half is `crates/web/src/api/oplog.rs`.
// Contracts: pure, never throws; unknown op kinds merge as data (open
// vocabulary per `OpRow`); resolution never deletes history — it appends a
// `resolve` op so both forks stay readable.
export interface OpEntry {
  seq: number
  opId: string
  deviceId: string
  createdAt: string
  kind: string
  fileId?: string | null
  payload?: unknown
  bases?: Record<string, number>
}

export interface OpConflict {
  a: OpEntry
  b: OpEntry
}

export type ResolutionChoice = 'keep-a' | 'keep-b' | 'merge'

/** `rename × rename on f9` — the one-line title the resolver list shows. */
export function conflictTitle(pair: OpConflict): string {
  const kinds = [pair.a.kind || '?', pair.b.kind || '?'].join(' × ')
  const file = pair.a.fileId ?? pair.b.fileId ?? null
  return file ? `${kinds} on ${file}` : kinds
}

/** Human sentence for the pair (devices + kinds, no payload sniffing). */
export function conflictSummary(pair: OpConflict): string {
  const left = `${pair.a.deviceId || '?'} ${pair.a.kind || '?'} (seq ${pair.a.seq})`
  const right = `${pair.b.deviceId || '?'} ${pair.b.kind || '?'} (seq ${pair.b.seq})`
  return `${left} conflicts with ${right}`
}

/** `true` when both halves name the same file (resolvable), else cross-file. */
export function isSameFile(pair: OpConflict): boolean {
  const fa = pair.a.fileId ?? null
  const fb = pair.b.fileId ?? null
  return !!fa && fa === fb
}

/** Payload for `POST /api/oplog/resolve`. Never throws (unknown choice → merge). */
export function resolutionPayload(
  pair: OpConflict,
  choice: ResolutionChoice,
  deviceId: string,
): { opA: string; opB: string; choice: ResolutionChoice; deviceId: string; fileId: string | null } {
  const safe: ResolutionChoice = choice === 'keep-a' || choice === 'keep-b' ? choice : 'merge'
  return {
    opA: pair.a.opId,
    opB: pair.b.opId,
    choice: safe,
    deviceId,
    fileId: pair.a.fileId ?? pair.b.fileId ?? null,
  }
}

/** Button label for a resolution choice. */
export function resolveLabel(choice: ResolutionChoice): string {
  if (choice === 'keep-a') return 'Keep first'
  if (choice === 'keep-b') return 'Keep second'
  return 'Merge (keep both)'
}

/** Queue-and-replay steps for offline: pull `since`, then push buffered rows. */
export function syncPlanSteps(since: number): string[] {
  return [
    `pull: GET /api/oplog/since?since=${Math.max(0, since)}`,
    'push: POST /api/oplog/push { deviceId, ops, signature, algorithm } (ML-DSA required)',
    'render: GET /api/oplog/conflicts',
  ]
}

/** Normalize a raw conflicts response into pairs (tolerates `{a,b}` + `[a,b]`). */
export function normalizeConflicts(raw: unknown): OpConflict[] {
  const list = (raw as { conflicts?: unknown })?.conflicts
  if (!Array.isArray(list)) return []
  const out: OpConflict[] = []
  for (const item of list) {
    const pair = Array.isArray(item) ? { a: item[0], b: item[1] } : (item as { a?: OpEntry; b?: OpEntry })
    if (pair && pair.a && pair.b && typeof pair.a.opId === 'string' && typeof pair.b.opId === 'string') {
      out.push({ a: pair.a, b: pair.b })
    }
  }
  return out
}
