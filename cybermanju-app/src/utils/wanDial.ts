// CyberManju OS — WAN dial seam (src/utils/wanDial.ts).
// Role: frontend mirror of the iroh seam in crates/sync/src/iroh.rs
// (ANALYSIS.md §2B.5) — parse `<device>@<addr>` endpoints, return a LIVE dial
// plan (direct HTTP probe + op-log pull/push; relay-envelope rendezvous when
// a relay URL is configured), and refuse honestly (`unsupported:` +
// LAN/bundle way out) only when a relay ticket arrives with no relay set.
// Only the QUIC *datagram* upgrade stays sequenced — the relay *capability*
// (outbound HTTPS drop/poll, no inbound port) is live on both seams.
// Exports: WanEndpoint, parseWanEndpoint, wanDialPlan, wanWayOut,
// wanHttpBase, wanDialSteps, wanHttpProbe, isRelayAddr, wanPeekUrls,
// wanFederatedPlan, buildRelayEnvelope, relayDialSteps, relayDropPath,
// relayPollPath, parseIrohTicket, buildIrohTicket, ticketRelay.
// Integrates: DevicesPanel WAN card renders plan + dial steps; `wan plan|dial`
// cybsh verbs reuse the builders; Rust twin is crates/sync/src/iroh.rs (same
// grammar, same plan strings, same `unsupported:` prefix, pinned by
// wan-dial.test.ts + p2p-live.test.ts).
// Contracts: pure builders never throw except `invalid:`/`unsupported:` with
// AGENT-1 prefixes; `wanHttpProbe` never hangs (AbortController 15 s default)
// and normalizes failures to `network:` — never a guessed address.
export interface WanEndpoint {
  deviceId: string
  addr: string
}

/** Parse `<device-id>@<addr>`; both halves required, whitespace refused. */
export function parseWanEndpoint(text: string): WanEndpoint {
  const at = text.indexOf('@')
  if (at < 0) throw new Error('invalid: endpoint must be <device-id>@<addr>')
  const deviceId = text.slice(0, at).trim()
  const addr = text.slice(at + 1).trim()
  if (!deviceId || !addr) throw new Error('invalid: endpoint needs both a device id and an address')
  if (/\s/.test(deviceId) || /\s/.test(addr)) {
    throw new Error('invalid: endpoint halves must not contain whitespace')
  }
  return { deviceId, addr }
}

/** Validate a dial timeout (1..=120s, mirroring IrohConfig validation). */
export function checkDialTimeout(secs: number): void {
  if (!Number.isFinite(secs) || Math.floor(secs) !== secs || secs < 1 || secs > 120) {
    throw new Error('invalid: dial_timeout_secs must be 1..=120')
  }
}

/**
 * Live dial plan: direct addrs return the HTTP plan (probe + op-log
 * pull/push); relay tickets return the relay-envelope plan when `relayUrl`
 * is set, else throw `unsupported:` with the LAN/bundle way out. Same
 * strings as Rust `iroh::dial_plan`.
 */
export function wanDialPlan(endpoint: WanEndpoint, dialTimeoutSecs = 15, relayUrl = ''): string {
  checkDialTimeout(dialTimeoutSecs)
  if (!endpoint.deviceId.trim()) throw new Error('invalid: dial needs a device id')
  if (isRelayAddr(endpoint.addr)) {
    // Canonical tickets carry their own relay; the configured URL is the fallback.
    const relay = ticketRelay(endpoint.addr) || relayUrl.trim().replace(/\/+$/, '')
    if (!relay) {
      throw new Error(
        `unsupported: relay dial to '${endpoint.deviceId}' needs a relay_url (any dashboard serves POST /api/relay/drop|poll — set one, or use HTTP dial (GET /api/readyz + op-log pull/push), direct LAN sync or a sneakernet bundle)`,
      )
    }
    return `relay: POST ${relay}/api/relay/drop { to: '${endpoint.deviceId}', kind: op-envelope } (${dialTimeoutSecs} s abort, outbound HTTPS — no inbound port) + POST ${relay}/api/relay/poll { for: '${endpoint.deviceId}' } for the reply`
  }
  const base = wanHttpBase(endpoint)
  return `http: probe GET ${base}/api/readyz (${dialTimeoutSecs} s abort, fail-closed) + op-log pull/push over ${base}`
}

/** Relay envelope `{ v: 1, to, kind, payload }` — identical JSON to Rust `iroh::relay_envelope`. */
export function buildRelayEnvelope(toDevice: string, kind: string, payload: unknown): string {
  if (!toDevice.trim()) throw new Error('invalid: relay envelope needs a destination device id')
  if (!kind.trim()) throw new Error('invalid: relay envelope needs a kind')
  return JSON.stringify({ v: 1, to: toDevice, kind, payload: payload ?? null })
}

/** Live relay dial steps (drop → poll → merge → push → render). Same order as Rust `relay_dial_steps`. */
export function relayDialSteps(endpoint: WanEndpoint, relayUrl: string, since = 0): string[] {
  if (!isRelayAddr(endpoint.addr)) {
    throw new Error('invalid: relay steps need a relay ticket addr (direct addrs use wanDialSteps)')
  }
  const relay = ticketRelay(endpoint.addr) || relayUrl.trim().replace(/\/+$/, '')
  if (!relay) {
    throw new Error(
      `unsupported: relay dial to '${endpoint.deviceId}' needs a relay_url (any dashboard serves POST /api/relay/drop|poll — set one, or use HTTP dial, direct LAN sync or a sneakernet bundle)`,
    )
  }
  const s = Number.isFinite(since) ? Math.max(0, Math.floor(since)) : 0
  return [
    `drop: POST ${relay}/api/relay/drop { to: '${endpoint.deviceId}', kind: op-envelope }`,
    `poll: POST ${relay}/api/relay/poll { for: '${endpoint.deviceId}' } (15 s abort)`,
    `pull: verify polled signed envelope since=${s} against registered device key, then append locally`,
    'push: POST /api/relay/drop { to, kind: op-envelope, payload: signed envelope }',
    'render: GET /api/oplog/conflicts',
  ]
}

/** `POST <relay>/api/relay/drop` for a relay base URL. */
export function relayDropPath(relayBase: string): string {
  const base = relayBase.trim().replace(/\/+$/, '')
  if (!base) throw new Error('invalid: relay address is required')
  return `${base}/api/relay/drop`
}

/** `POST <relay>/api/relay/poll` for a relay base URL. */
export function relayPollPath(relayBase: string): string {
  const base = relayBase.trim().replace(/\/+$/, '')
  if (!base) throw new Error('invalid: relay address is required')
  return `${base}/api/relay/poll`
}

/** True when the addr half rendezvouses via the relay (same grammar as Rust `is_relay_addr`). */
export function isRelayAddr(addr: string): boolean {
  const low = addr.trim().toLowerCase()
  if (low.startsWith('iroh:') || low.includes('/p2p/')) return true
  return /(^|[^a-z0-9])relay([^a-z0-9]|$)/.test(low)
}

/** iroh endpoint id shape (64 hex, or 52 base32 — curve checked natively). */
const ENDPOINT_ID_RE = /^([0-9a-fA-F]{64}|[A-Za-z2-7]{52})$/

/**
 * Parse a canonical `iroh:<endpoint-id>@<relay-url>` ticket (shape-checked;
 * the ed25519 curve itself is validated natively by `iroh::PublicKey`).
 * Returns `{ endpointId, relayUrl }` with the relay trailing-slash trimmed.
 */
export function parseIrohTicket(text: string): { endpointId: string; relayUrl: string } {
  const raw = text.trim()
  if (!raw.toLowerCase().startsWith('iroh:')) {
    throw new Error('invalid: iroh ticket starts with `iroh:`')
  }
  const rest = raw.slice('iroh:'.length)
  const at = rest.indexOf('@')
  if (at < 0) throw new Error('invalid: iroh ticket is `iroh:<endpoint-id>@<relay-url>`')
  const endpointId = rest.slice(0, at).trim()
  const relayUrl = rest.slice(at + 1).trim().replace(/\/+$/, '')
  if (!ENDPOINT_ID_RE.test(endpointId)) {
    throw new Error('invalid: iroh ticket endpoint id is 64 hex or 52 base32 chars')
  }
  if (!relayUrl || /\s/.test(relayUrl)) {
    throw new Error('invalid: iroh ticket needs a relay URL')
  }
  return { endpointId, relayUrl }
}

/** Build a canonical ticket string (halves shape-checked, relay normalized). */
export function buildIrohTicket(endpointId: string, relayUrl: string): string {
  const id = endpointId.trim()
  const relay = relayUrl.trim().replace(/\/+$/, '')
  if (!ENDPOINT_ID_RE.test(id)) {
    throw new Error('invalid: iroh ticket endpoint id is 64 hex or 52 base32 chars')
  }
  if (!relay || /\s/.test(relay)) throw new Error('invalid: iroh ticket needs a relay URL')
  return `iroh:${id}@${relay}`
}

/** Relay carried by a ticket addr, or `''` when the addr holds none. */
export function ticketRelay(addr: string): string {
  try {
    return parseIrohTicket(addr).relayUrl
  } catch {
    return ''
  }
}

/** The three URL shapes one WAN endpoint fans out to (probe/search/op-log). */
export function wanPeekUrls(endpoint: WanEndpoint): { readyz: string; search: (q: string, limit?: number) => string; oplogSince: (since: number) => string } {
  const base = wanHttpBase(endpoint)
  return {
    readyz: `${base.replace(/\/+$/, '')}/api/readyz`,
    search: (q: string, limit = 20) => {
      const capped = Math.max(1, Math.min(Math.floor(limit) || 20, 200))
      return `${base.replace(/\/+$/, '')}/api/search?q=${encodeURIComponent(q)}&limit=${capped}`
    },
    oplogSince: (since: number) =>
      `${base.replace(/\/+$/, '')}/api/oplog/since?since=${Math.max(0, Math.floor(since) || 0)}&limit=500`,
  }
}

/**
 * Federated-search plan for a WAN endpoint (local BM25 + one bounded peer
 * fetch). Relay endpoints refuse with `unsupported:` — same gate as Rust
 * `federated_search_plan`.
 */
export function wanFederatedPlan(endpoint: WanEndpoint, query: string, limit = 20): string[] {
  if (!query.trim()) throw new Error('invalid: federated search needs a query')
  if (isRelayAddr(endpoint.addr)) {
    throw new Error(
      `unsupported: federated search to '${endpoint.deviceId}' is a relay ticket — configure a relay_url and search via the relay envelope, or use direct host:port peers / POST /api/search/federated with reachable base URLs`,
    )
  }
  const capped = Math.max(1, Math.min(Math.floor(limit) || 20, 200))
  const urls = wanPeekUrls(endpoint)
  return [
    `local: BM25 \`${query}\` (limit ${capped}, tagged \`local\`)`,
    `peer: GET ${urls.search(query, capped)} (timeout 8 s, dead peer → zero hits)`,
  ]
}

/** The way out offered alongside the `unsupported:` plan (never guessed). */
export function wanWayOut(endpoint: WanEndpoint): string[] {
  return [
    `lan: POST /api/peers/${endpoint.deviceId}/sync (same Wi-Fi, Devices → LAN peers → SYNC)`,
    'bundle: sneakernet op-log delta via GET/POST /api/sync/bundle/export|import (untrusted sticks stay tamper-evident)',
    `op-log: pull GET /api/oplog/since on next contact — no rescan, payloads ride unchanged when the endpoint arrives (${endpoint.addr})`,
  ]
}

/** `http(s)://` base for a WAN endpoint (bare http hosts default to :3456, like the Rust probe). */
export function wanHttpBase(endpoint: WanEndpoint): string {
  const addr = endpoint.addr.trim().replace(/\/+$/, '')
  if (/^https?:\/\//i.test(addr)) return addr
  // Bare `host` without a port means the dashboard — same default as
  // `iroh::http_probe` (`host:port` stays verbatim, IPv6 keeps brackets).
  if (!addr.includes(':')) return `http://${addr}:3456`
  return `http://${addr}`
}

/** Real HTTP dial steps: op-log pull → signed push → verify/merge → render. */
export function wanDialSteps(endpoint: WanEndpoint, since = 0): string[] {
  const base = wanHttpBase(endpoint)
  const s = Math.max(0, Math.floor(since))
  return [
    `probe: GET ${base}/api/readyz (15 s abort, fail-closed)`,
    `pull: GET ${base}/api/oplog/since?since=${s}`,
    `push: POST ${base}/api/oplog/push { deviceId, ops, signature, algorithm } (ML-DSA required)`,
    'merge: verify peer envelope against its registered key, then append locally',
    'render: GET /api/oplog/conflicts',
  ]
}

/**
 * Execute the WAN op-log exchange against a peer base URL: pull → push →
 * merge → render via `syncExchangeWithPeer`. Relay tickets refuse with
 * `unsupported:` (they rendezvous via `POST /api/relay/drop|poll` — pass a
 * relay URL through `relayDialSteps` instead); direct HTTP peers sync today
 * on all six targets wherever `/api` is reachable.
 */
export async function wanSyncExchange(
  endpoint: WanEndpoint,
  since: number,
  localOps: unknown[],
  deps: {
    localDeviceId: string
    /** Explicit credential for this target peer; never fall back to caller auth. */
    peerToken: string
    fetchJson: (method: string, url: string, body: unknown, peerToken: string) => Promise<unknown>
    pushOps: (ops: unknown[]) => Promise<{ appended: number }>
    fetchConflicts?: (peerToken: string) => Promise<unknown>
    signer: (deviceId: string, normalizedOps: Record<string, unknown>[]) => Promise<{
      signature: string
      algorithm: string
    }>
  },
): Promise<{ pulled: number; pushed: number; steps: string[] }> {
  if (isRelayAddr(endpoint.addr)) {
    throw new Error(
      `unsupported: relay dial to '${endpoint.deviceId}' needs a relay_url (any dashboard serves POST /api/relay/drop|poll — set one, or use HTTP dial (GET /api/readyz + op-log pull/push), direct LAN sync or a sneakernet bundle)`,
    )
  }
  const { syncExchangeWithPeer } = await import('./peerSearch')
  return syncExchangeWithPeer(
    { deviceId: endpoint.deviceId, baseUrl: wanHttpBase(endpoint), transport: 'wan-http' },
    since,
    localOps,
    deps,
  )
}
/**
 * Probe a peer's `/api/readyz` with a 15 s abort. Resolves `true` on HTTP 2xx,
 * otherwise throws an AGENT-1 `network:` error (never a hang, never a guess).
 * Node-env safe: without a global `fetch` it answers `unsupported:`.
 */
export async function wanHttpProbe(baseUrl: string, timeoutMs = 15000): Promise<true> {
  const fetchFn = (globalThis as { fetch?: typeof fetch }).fetch
  if (typeof fetchFn !== 'function') {
    throw new Error('unsupported: WAN HTTP probe needs fetch (browser or Node 18+)')
  }
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), timeoutMs)
  try {
    const res = await fetchFn(`${baseUrl.replace(/\/+$/, '')}/api/readyz`, {
      signal: ctrl.signal,
    })
    if (!res.ok) throw new Error(`network: WAN probe ${baseUrl} answered HTTP ${res.status}`)
    return true
  } catch (err) {
    if (err instanceof Error && /WAN probe|unsupported:/.test(err.message)) throw err
    const reason = err instanceof Error ? err.message : String(err)
    const aborted = err instanceof Error && (err.name === 'AbortError' || /abort/i.test(reason))
    throw new Error(
      aborted
        ? `network: WAN probe timed out after ${timeoutMs}ms calling ${baseUrl}: ${reason}`
        : `network: WAN probe failed for ${baseUrl}: ${reason}`,
    )
  } finally {
    clearTimeout(timer)
  }
}
