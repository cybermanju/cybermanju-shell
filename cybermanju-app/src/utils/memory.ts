// Cybermanju — Hermes semantic-memory pattern, TS side.
//
// Mirror of `crates/agent/src/memory.rs`: identical token rule (lowercase
// alphanumeric, length >= 3), identical keyword scoring (0.25 + 0.05 ×
// overlap, capped 0.69), identical budgets (2200 recall / 2000 note / 0.30
// floor / top-3). The browser loop has no embeddings API, so it recalls over
// keyword scores only and marks results `degraded:` — honest, same contract.
//
// `LocalMemoryStore` is the browser persistence half (localStorage adapter
// wired in `useAgent.ts`); tests drive it with an in-memory Map.

import type { AgentMemory, MemoryHit, MemoryOrigin } from '@/types'
import { redactText } from '@/utils/redact'

/** Recalled-context budget per prompt (Hermes `memory_char_limit` parity). */
export const MEMORY_RECALL_BUDGET_CHARS = 2200
/** One memory is a note, not a document. */
export const MEMORY_TEXT_CAP_CHARS = 2000
/** Below this cosine the hit is noise, not memory. */
export const MEMORY_MIN_SCORE = 0.3
/** Default fan-in per recall. */
export const MEMORY_TOP_K = 3
/** Keyword fallback needs at least this token overlap to fire. */
export const MEMORY_MIN_OVERLAP = 2
/** Runs this long without storing anything earn a remember nudge. */
export const MEMORY_NUDGE_TURNS = 8

/** Cosine similarity; mismatch/empty/zero vectors score 0. */
export function cosine(a: ArrayLike<number>, b: ArrayLike<number>): number {
  if (a.length !== b.length || a.length === 0) return 0
  let dot = 0
  let na = 0
  let nb = 0
  for (let i = 0; i < a.length; i++) {
    const x = a[i]
    const y = b[i]
    dot += x * y
    na += x * x
    nb += y * y
  }
  if (na <= 0 || nb <= 0) return 0
  const s = dot / (Math.sqrt(na) * Math.sqrt(nb))
  if (!Number.isFinite(s)) return 0
  return Math.max(-1, Math.min(1, s))
}

/** Lowercase alphanumeric tokens, length >= 3 — identical to Rust `tokens`. */
export function memoryTokens(text: string): string[] {
  return (text ?? '')
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter(w => w.length >= 3)
}

/** Distinct query tokens present in the text. */
export function keywordOverlap(query: string, text: string): number {
  const hay = new Set(memoryTokens(text))
  const seen = new Set<string>()
  let hits = 0
  for (const w of memoryTokens(query)) {
    if (!seen.has(w)) {
      seen.add(w)
      if (hay.has(w)) hits++
    }
  }
  return hits
}

/** Paragraph-aware chunks, greedily packed to `maxChars`. */
export function chunkMemoryText(text: string, maxChars: number): string[] {
  const max = Math.max(64, maxChars)
  const chunks: string[] = []
  let current = ''
  const flush = () => {
    const trimmed = current.trim()
    if (trimmed) chunks.push(trimmed)
    current = ''
  }
  for (const raw of (text ?? '').split('\n\n')) {
    const para = raw.trim()
    if (!para) continue
    if ([...para].length > max) {
      flush()
      const chars = [...para]
      for (let i = 0; i < chars.length; i += max) {
        const piece = chars.slice(i, i + max).join('').trim()
        if (piece) chunks.push(piece)
      }
      continue
    }
    const add = [...para].length + 2
    if (current && [...current].length + add > max) flush()
    if (current) current += '\n\n'
    current += para
  }
  flush()
  return chunks
}

function keywordScore(overlap: number): number {
  return Math.min(0.69, 0.25 + 0.05 * overlap)
}

export interface RankedMemory {
  memory: AgentMemory
  score: number
}

/**
 * Rank memories for a query — same formula as Rust `recall_rank`: vector
 * cosine wins when comparable, keyword overlap otherwise (or when no query
 * vector exists, e.g. the browser loop). Ties break by `uses`, then recency.
 */
export function rankMemories(
  queryVec: ArrayLike<number> | null,
  queryText: string,
  memories: AgentMemory[],
  topK = MEMORY_TOP_K,
): MemoryHit[] {
  const k = Math.max(1, Math.min(10, Math.floor(topK) || MEMORY_TOP_K))
  const scored: RankedMemory[] = []
  for (const m of memories) {
    let best = 0
    const emb = m.embedding ?? []
    if (queryVec && queryVec.length > 0 && queryVec.length === emb.length && emb.length > 0) {
      const s = cosine(queryVec, emb)
      if (s >= MEMORY_MIN_SCORE) best = s
    }
    const overlap = keywordOverlap(queryText, m.text)
    if (overlap >= MEMORY_MIN_OVERLAP) {
      const kw = keywordScore(overlap)
      if (kw > best) best = kw
    }
    if (best > 0) scored.push({ memory: m, score: best })
  }
  scored.sort(
    (a, b) =>
      b.score - a.score ||
      b.memory.uses - a.memory.uses ||
      (b.memory.updatedAt < a.memory.updatedAt ? -1 : b.memory.updatedAt > a.memory.updatedAt ? 1 : 0),
  )
  return scored.slice(0, k).map(({ memory: m, score }) => ({
    id: m.id,
    text: m.text,
    score,
    origin: m.origin,
    sessionId: m.sessionId,
    updatedAt: m.updatedAt,
  }))
}

/** Bounded `<recalled-memories>` prompt block (empty → no block at all). */
export function renderRecallBlock(hits: MemoryHit[], budget = MEMORY_RECALL_BUDGET_CHARS): string {
  if (!hits.length) return ''
  let out = '\n--- recalled memories (bounded; verify against the volume before acting) ---\n'
  for (const h of hits) out += `- ${h.text}\n`
  if ([...out].length > budget) {
    out = `${[...out].slice(0, budget).join('')}\n… truncated at ${budget} chars`
  }
  return out
}

/** Hermes-style nudge predicate (surfaced once at terminal states). */
export function shouldNudgeMemory(turnsUsed: number, rememberedThisRun: boolean): boolean {
  if (!Number.isFinite(turnsUsed)) return false
  return turnsUsed >= MEMORY_NUDGE_TURNS && !rememberedThisRun
}

// ─── browser persistence ──────────────────────────────────────────────────

export interface MemoryStorage {
  load(): AgentMemory[]
  save(all: AgentMemory[]): void
}

export function newMemoryId(prefix = 'mem'): string {
  try {
    return `${prefix}-${crypto.randomUUID().slice(0, 8)}`
  } catch {
    return `${prefix}-${Date.now().toString(36)}`
  }
}

/**
 * Local (browser) memory store: keyword-only recall over persisted rows,
 * capped at 500 newest. Vectors are accepted on the row shape (imported via
 * export envelopes) and participate in ranking when present — the browser
 * simply never mints new ones without an embeddings endpoint.
 */
export class LocalMemoryStore {
  constructor(private readonly storage: MemoryStorage) {}

  list(configId?: string): AgentMemory[] {
    const all = this.storage.load()
    const scoped = configId ? all.filter(m => m.configId === configId) : all
    return scoped.sort((a, b) => (b.updatedAt < a.updatedAt ? -1 : 1)).slice(0, 500)
  }

  remember(configId: string, text: string, origin: MemoryOrigin = 'remember'): AgentMemory | null {
    // Native `sanitize_text` parity: secret-redact before the row persists
    // (memories sync and outlive the run — a leaked key there is permanent).
    const cleaned = redactText(text ?? '').text.trim().slice(0, MEMORY_TEXT_CAP_CHARS)
    if (!cleaned || !configId) return null
    const now = new Date().toISOString()
    const row: AgentMemory = {
      id: newMemoryId(),
      configId,
      text: cleaned,
      embedding: [],
      dims: 0,
      origin,
      sessionId: null,
      uses: 0,
      createdAt: now,
      updatedAt: now,
    }
    const all = this.storage.load().filter(m => m.id !== row.id)
    all.push(row)
    this.storage.save(all.slice(-500))
    return row
  }

  remove(id: string): void {
    this.storage.save(this.storage.load().filter(m => m.id !== id))
  }

  recall(configId: string | undefined, query: string, topK = MEMORY_TOP_K): MemoryHit[] {
    return rankMemories(null, query, this.list(configId), topK)
  }
}
