// CyberManju — Hermes gateway pattern, TS side (pure half).
//
// One client shape over three transports (Tauri IPC / REST / WASM volume).
// The impure halves live in `useTauri.ts` (`isTauri`, `isStaticHost`,
// `REST_ROUTES`) and `useStudioAgent.ts` (driver selection); this module pins
// the decision matrix, the retry schedule, and the error→action mapping as
// pure, unit-tested functions so the gateway cannot drift per call site.

/** The three transports the app can run the agent over. */
export type TransportKind = 'tauri' | 'rest' | 'wasm'

export interface TransportEnv {
  isTauri: boolean
  /** Configured dashboard URL (`cybermanju.serverUrl`); empty when unset. */
  serverUrl: string
  /** `window.location.port` of the serving origin; empty in non-DOM contexts. */
  port: string
}

/**
 * Pure mirror of the `useTauri` routing: Tauri IPC wins when present; a
 * configured server URL or a `:3456` origin means REST; anything else is a
 * static host served without a backend, so the WASM volume dispatcher owns it.
 */
export function resolveTransport(env: TransportEnv): TransportKind {
  if (env.isTauri) return 'tauri'
  if (env.serverUrl || env.port === '3456') return 'rest'
  return 'wasm'
}

/**
 * Deterministic exponential backoff schedule (`baseMs · 2^attempt`, capped).
 * No jitter here — the caller adds it — so the schedule is unit-pinnable.
 */
export function backoffMs(attempt: number, baseMs = 300, capMs = 5000): number {
  const a = Number.isFinite(attempt) ? Math.max(0, Math.floor(attempt)) : 0
  const base = Number.isFinite(baseMs) && baseMs > 0 ? baseMs : 300
  const cap = Number.isFinite(capMs) && capMs > 0 ? capMs : 5000
  return Math.min(cap, base * 2 ** Math.min(a, 20))
}

/** Gateway-level error families (subset of the house machine prefixes). */
export type GatewayErrorKind = 'auth' | 'rate_limited' | 'network' | 'context' | 'unknown'

/**
 * Map provider/transport failure text to a family, using the same signals as
 * the native `classify_provider_error`: HTTP markers + context-window phrases.
 * Ordering matters: `auth` and `rate_limited` are checked before the generic
 * `network` bucket so a 401/429 is never retried as a blip.
 */
export function classifyGatewayError(message: string): GatewayErrorKind {
  const t = (message ?? '').toLowerCase()
  if (
    t.includes('401') || t.includes('unauthorized') || t.includes('forbidden') ||
    t.includes('invalid api key') || t.includes('incorrect api key') || t.includes('auth:')
  ) {
    return 'auth'
  }
  if (
    t.includes('429') || t.includes('rate_limited') || t.includes('rate limit') ||
    t.includes('too many requests') || t.includes('throttled')
  ) {
    return 'rate_limited'
  }
  if (
    t.includes('context_length_exceeded') || t.includes('context window') ||
    t.includes('maximum context') || t.includes('prompt is too long') ||
    t.includes('too many tokens') || t.includes('context:')
  ) {
    return 'context'
  }
  if (
    t.includes('network:') || t.includes('network error') || t.includes('fetch failed') ||
    t.includes('err_connection') || t.includes('econnrefused') || t.includes('socket') ||
    t.includes('timeout') || t.includes('temporarily unavailable') || t.includes('502') ||
    t.includes('503') || t.includes('504')
  ) {
    return 'network'
  }
  return 'unknown'
}

/**
 * Retry policy: `network`/`rate_limited` are transient (bounded retries with
 * `backoffMs`); `auth` never retries (reseal the key); `context` redirects to
 * COMPACT (retrying an overflowed transcript can never work).
 */
export function shouldRetry(kind: GatewayErrorKind): boolean {
  return kind === 'network' || kind === 'rate_limited'
}
