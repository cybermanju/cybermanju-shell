// CyberManju — Hermes memory/sessions pattern, TS side (pure half).
//
// Transcripts are append-only memory: the viewer keeps every byte while the
// wire gets a pruned copy (`prepareWireMessages` in `agentUi.ts`). This module
// adds the lifecycle primitives around that memory: a declarative
// auto-compact trigger, pure session forks (compact/revert flows), and a
// versioned export/import envelope with structural validation.

import type { AgentSession, ChatMessage } from '@/types'

/** Context share (0–100) at which the session should compact. */
export const AUTO_COMPACT_PCT = 85
/** Envelope version written by `exportSession` and required by `importSession`. */
export const SESSION_EXPORT_VERSION = 1

/**
 * Declarative auto-compact trigger for the future meter watcher.
 * Boundary-inclusive, NaN/negative-safe: garbage in → `false`, never a
 * surprise compaction.
 */
export function shouldAutoCompact(pct: number, threshold = AUTO_COMPACT_PCT): boolean {
  if (!Number.isFinite(pct) || !Number.isFinite(threshold)) return false
  return pct >= threshold
}

/** First-8-words title fallback (mirrors `sendLocal` session creation). */
export function sessionTitleFor(prompt: string): string {
  const title = (prompt ?? '').split(/\s+/).filter(Boolean).slice(0, 8).join(' ')
  return title || 'Untitled session'
}

export interface ForkOptions {
  /** Injected for tests; defaults to a time+random id (no crypto dependency). */
  idFactory?: () => string
  /** Keep only the trailing N messages (compact flow); default keeps all. */
  keepLast?: number
  /** Reset token counters on the fork (fresh-budget flow). */
  resetUsage?: boolean
  titleSuffix?: string
  now?: () => string
}

const defaultId = (): string =>
  `ses-${Date.now().toString(36)}-${Math.floor(Math.random() * 0xffffff).toString(36)}`

/**
 * Pure session fork: new id, optionally truncated transcript, optionally
 * reset usage. The source session is never mutated (messages are copied).
 */
export function forkSession(session: AgentSession, opts: ForkOptions = {}): AgentSession {
  const keep = opts.keepLast == null ? session.messages.length : Math.max(0, opts.keepLast)
  const messages: ChatMessage[] = session.messages.slice(-keep).map(m => ({ ...m }))
  const now = opts.now ? opts.now() : new Date().toISOString()
  return {
    ...session,
    id: opts.idFactory ? opts.idFactory() : defaultId(),
    title: `${session.title}${opts.titleSuffix ?? ' (fork)'}`,
    messages,
    usage: opts.resetUsage ? { inputTokens: 0, outputTokens: 0 } : { ...session.usage },
    createdAt: now,
    updatedAt: now,
  }
}

/** Versioned JSON envelope — the portable session format. */
export function exportSession(session: AgentSession): string {
  return JSON.stringify({ version: SESSION_EXPORT_VERSION, session })
}

export type ImportResult = { ok: true; session: AgentSession } | { ok: false; error: string }

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null
}

/**
 * Validate + import a session envelope. Never throws: corrupt payloads return
 * `{ ok: false, error: 'invalid: …' }` under the house machine-prefix contract.
 * Shape is validated; content length is not (long transcripts stay intact).
 */
export function importSession(payload: string): ImportResult {
  let root: unknown
  try {
    root = JSON.parse(payload)
  } catch {
    return { ok: false, error: 'invalid: session payload is not JSON' }
  }
  if (!isRecord(root)) return { ok: false, error: 'invalid: session envelope must be an object' }
  if (root.version !== SESSION_EXPORT_VERSION) {
    return { ok: false, error: `invalid: unsupported session version ${String(root.version)}` }
  }
  const s = root.session
  if (!isRecord(s)) return { ok: false, error: 'invalid: session envelope has no session' }
  if (typeof s.id !== 'string' || !s.id) {
    return { ok: false, error: 'invalid: session id must be a non-empty string' }
  }
  if (!Array.isArray(s.messages)) {
    return { ok: false, error: 'invalid: session messages must be an array' }
  }
  const messages: ChatMessage[] = []
  for (const m of s.messages) {
    if (!isRecord(m) || typeof m.role !== 'string' || typeof m.content !== 'string') {
      return { ok: false, error: 'invalid: every message needs a string role and content' }
    }
    messages.push({
      role: m.role,
      content: m.content,
      toolCallId: typeof m.toolCallId === 'string' ? m.toolCallId : undefined,
      toolName: typeof m.toolName === 'string' ? m.toolName : undefined,
      toolInput: m.toolInput,
    })
  }
  const usage = isRecord(s.usage) ? s.usage : {}
  const num = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : 0)
  const str = (v: unknown, fallback: string): string => (typeof v === 'string' ? v : fallback)
  const now = new Date().toISOString()
  return {
    ok: true,
    session: {
      id: s.id,
      title: str(s.title, 'Imported session'),
      configId: str(s.configId, ''),
      providerId: str(s.providerId, ''),
      model: str(s.model, ''),
      agentKind: s.agentKind === 'plan' ? 'plan' : 'build',
      workingDir: str(s.workingDir, '/'),
      messages,
      usage: { inputTokens: num(usage.inputTokens), outputTokens: num(usage.outputTokens) },
      createdAt: str(s.createdAt, now),
      updatedAt: str(s.updatedAt, now),
    },
  }
}
