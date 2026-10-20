// CyberManju OS — cross-device search port (src/utils/peerSearch.ts).
// Role: P2P transport seam for all six targets (android/ios/windows/linux/
// docker/wasm) — one peer grammar (`<device>@<addr>`), one merge order
// (dedup by file id, max score wins, desc sort), and one executable fan-out:
// local hits plus one bounded `GET /api/search` per explicit peer base URL.
// Explicit URLs are what make mobile/docker/wasm work where mDNS multicast
// cannot; a dead peer degrades to zero hits for that peer, never a failure.
// Exports: PeerTransportName, PeerEndpoint, FederatedHit, FederatedPeer,
// parsePeerEndpoint, normalizeBaseUrl, classifyAddr, isLanBase, mergeFederatedHits,
// peerCapabilityMatrix, loadCachedPeers, saveCachedPeers, federatedSearch,
// syncExchangeWithPeer, peerReadyzUrl, peerSearchUrl, peerOplogSinceUrl,
// peerOplogPushUrl, deduplicatePeers, wasmPeerWarning, buildFederatedBody,
// MAX_FEDERATED_PEERS, FEDERATED_MARKER_HEADER.
// Integrates: DevicesPanel P2P card + OmniSearch federated mode; `useTauri`
// `search_files_federated` static handler fans out through this; Rust twins
// are crates/sync/src/peers.rs (same grammar, same merge) and
// crates/web/src/api/search_api.rs `search_federated` (server-side fan-out).
// Contracts: pure builders throw only `invalid:`/`unsupported:` (AGENT-1);
// network failures normalize to `network:`/`auth:`; nothing here guesses an
// address and nothing hangs (AbortController on every fetch).

export type PeerTransportName = 'lan-http' | 'wan-http' | 'bundle' | 'relay-future'

export interface PeerEndpoint {
  deviceId: string
  baseUrl: string
  transport: PeerTransportName
}

export interface FederatedHit {
  fileId: string
  fileName: string
  score: number
  snippet: string
  source: string
}

export interface FederatedPeer {
  baseUrl: string
  deviceId?: string
  /** The peer's own bearer; required before a network request is issued. */
  token?: string
}

/** Max peers per fan-out (agrees with Rust `MAX_FEDERATED_PEERS`). */
export const MAX_FEDERATED_PEERS = 8

/** Marker header on server-side peer fetches (log triage; never re-fanned). */
export const FEDERATED_MARKER_HEADER = 'x-cybermanju-federated'

/** `GET <base>/api/readyz` — single source of truth for the probe URL. */
export function peerReadyzUrl(baseUrl: string): string {
  return `${normalizeBaseUrl(baseUrl).replace(/\/+$/, '')}/api/readyz`
}

/** `GET <base>/api/search?q=&limit=` — the cross-device search port URL. */
export function peerSearchUrl(baseUrl: string, query: string, limit = 20): string {
  const base = normalizeBaseUrl(baseUrl).replace(/\/+$/, '')
  const capped = Math.max(1, Math.min(Math.floor(limit) || 20, 200))
  return `${base}/api/search?q=${encodeURIComponent(query)}&limit=${capped}`
}

/** `GET <base>/api/oplog/since?since=&limit=` — pull half of the exchange. */
export function peerOplogSinceUrl(baseUrl: string, since: number, limit = 500): string {
  const base = normalizeBaseUrl(baseUrl).replace(/\/+$/, '')
  const s = Math.max(0, Math.floor(since) || 0)
  const capped = Math.max(1, Math.min(Math.floor(limit) || 500, 500))
  return `${base}/api/oplog/since?since=${s}&limit=${capped}`
}

/** `POST <base>/api/oplog/push` — only a device-signed envelope is accepted. */
export function peerOplogPushUrl(baseUrl: string): string {
  return `${normalizeBaseUrl(baseUrl).replace(/\/+$/, '')}/api/oplog/push`
}

/**
 * Dedup peers by normalized base URL (first wins), drop blanks, cap at
 * `MAX_FEDERATED_PEERS`. Same order as Rust `deduplicate_peers` (explicit
 * peers first, cached peers after).
 */
export function deduplicatePeers(peers: FederatedPeer[]): FederatedPeer[] {
  const seen = new Set<string>()
  const out: FederatedPeer[] = []
  for (const p of peers) {
    let base = ''
    try {
      base = normalizeBaseUrl(p.baseUrl ?? '')
    } catch {
      continue
    }
    // Relay tickets rendezvous via the relay mailbox — never fetched over
    // direct HTTP, so drop them here instead of burning a fan-out slot.
    if (classifyAddr(base) === 'relay-future') continue
    const key = base.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    out.push({ ...p, baseUrl: base })
    if (out.length >= MAX_FEDERATED_PEERS) break
  }
  return out
}

/**
 * Browser-side warning for a peer base URL (`null` = should just work).
 * Same strings as Rust `wasm_peer_warning`: mixed-content when an https://
 * page targets http://, loopback when a remote page targets localhost.
 */
export function wasmPeerWarning(baseUrl: string, pageIsHttps: boolean): string | null {
  const low = (baseUrl ?? '').trim().toLowerCase()
  if (!low) return 'invalid: peer address is required'
  if (pageIsHttps && low.startsWith('http://')) {
    return 'mixed-content: this page is https:// but the peer is http:// — the browser blocks the fetch; serve the peer over https or use the server-side POST /api/search/federated fan-out'
  }
  if (low.includes('localhost') || low.includes('127.') || low.includes('[::1]')) {
    return 'loopback: a remote page\u2019s localhost peer points at the viewer, not this device — use the peer\u2019s LAN address (e.g. 192.168.1.60:3456) instead'
  }
  return null
}

/**
 * Build the `POST /api/search/federated` body (single source of truth for
 * the REST shape): camelCase peer twins + `timeoutSecs`, blanks dropped,
 * peers deduped/capped. Per-peer `token` fields pass through untouched.
 */
export function buildFederatedBody(args: {
  query: string
  limit?: number
  peers?: Array<Record<string, unknown>>
  timeoutSecs?: number
  timeout_secs?: number
}): { query: string; limit: number; peers: FederatedPeer[]; timeoutSecs: number } {
  const peers = deduplicatePeers(
    ((args.peers ?? []) as Array<Record<string, unknown>>).map((p) => ({
      baseUrl: String(p.baseUrl ?? p.base_url ?? ''),
      deviceId: String(p.deviceId ?? p.device_id ?? ''),
      ...(typeof (p.token ?? (p as Record<string, unknown>).peerToken) === 'string' &&
      String(p.token ?? (p as Record<string, unknown>).peerToken).trim()
        ? { token: String(p.token ?? (p as Record<string, unknown>).peerToken) }
        : {}),
    })).filter((p) => p.baseUrl.trim()),
  )
  return {
    query: args.query ?? '',
    limit: Math.max(1, Math.min(Number(args.limit ?? 50) || 50, 200)),
    peers,
    timeoutSecs: Math.max(1, Math.min(Number(args.timeoutSecs ?? args.timeout_secs ?? 8) || 8, 30)),
  }
}

/** `http(s)://` base for a raw addr half (addr stays opaque, never guessed). */
export function normalizeBaseUrl(addr: string): string {
  const base = addr.trim().replace(/\/+$/, '')
  if (!base) throw new Error('invalid: peer address is required')
  if (/\s/.test(base)) throw new Error('invalid: peer address must not contain whitespace')
  if (/^https?:\/\//i.test(base)) return base
  return `http://${base}`
}

/** True when `relay` appears as a standalone token (not inside `relayx`). */
export function isRelayToken(lowerAddr: string): boolean {
  return /(^|[^a-z0-9])relay([^a-z0-9]|$)/.test(lowerAddr)
}

/** Host half of a base/addr (scheme + port + path stripped, lower-cased). */
export function extractPeerHost(lowerBaseOrAddr: string): string {
  let h = lowerBaseOrAddr.toLowerCase().trim()
  h = h.replace(/^https?:\/\//, '')
  const slash = h.indexOf('/')
  if (slash >= 0) h = h.slice(0, slash)
  // IPv6 literals keep brackets: [::1]:3456 → [::1].
  if (h.startsWith('[')) {
    const close = h.indexOf(']')
    if (close >= 0) return h.slice(0, close + 1)
    return h
  }
  const colon = h.lastIndexOf(':')
  // Single colon = host:port; strip the port (bare IPv6 without brackets
  // is not dialable here and stays opaque for the relay/HTTP gates).
  if (colon >= 0 && h.indexOf(':') === colon) h = h.slice(0, colon)
  return h
}

/** True when a base/addr looks like same-network HTTP (host-based, no `110.` false positives). */
export function isLanBase(lowerBaseOrAddr: string): boolean {
  const b = lowerBaseOrAddr.toLowerCase()
  if (b.includes('localhost') || b.includes('[::1]')) return true
  const host = extractPeerHost(b)
  if (!host) return false
  if (host === 'localhost' || host.endsWith('.localhost')) return true
  if (host === '0.0.0.0') return true
  if (host.endsWith('.local') || host === 'local') return true
  if (host === '10' || host.startsWith('10.')) return true
  if (host.startsWith('192.168.')) return true
  if (host.startsWith('127.')) return true
  if (host.startsWith('169.254.')) return true
  if (host.startsWith('172.')) {
    const second = Number(host.split('.')[1] ?? '')
    if (Number.isInteger(second) && second >= 16 && second <= 31) return true
  }
  return false
}

/** Classify a raw addr half without opening any socket (relay = relay mailbox). */
export function classifyAddr(addr: string): PeerTransportName {
  const low = addr.trim().toLowerCase()
  if (low.startsWith('iroh:') || low.includes('/p2p/') || isRelayToken(low)) {
    return 'relay-future'
  }
  return isLanBase(low) ? 'lan-http' : 'wan-http'
}

/** Parse `<device-id>@<addr>` into a dialable endpoint (whitespace refused). */
export function parsePeerEndpoint(text: string): PeerEndpoint {
  const at = text.indexOf('@')
  if (at < 0) throw new Error('invalid: endpoint must be <device-id>@<addr>')
  const deviceId = text.slice(0, at).trim()
  const addr = text.slice(at + 1).trim()
  if (!deviceId || !addr) {
    throw new Error('invalid: endpoint needs both a device id and an address')
  }
  if (/\s/.test(deviceId) || /\s/.test(addr)) {
    throw new Error('invalid: endpoint halves must not contain whitespace')
  }
  return { deviceId, baseUrl: normalizeBaseUrl(addr), transport: classifyAddr(addr) }
}

/**
 * Merge ranked hits across devices: dedup by `fileId` (max score wins; on a
 * tie the richer snippet wins), desc sort by score, truncate to `limit`.
 * Same order as `peers::merge_federated_hits` (pinned by peer-search.test.ts).
 */
export function mergeFederatedHits(hits: FederatedHit[], limit = 50): FederatedHit[] {
  const best = new Map<string, FederatedHit>()
  for (const hit of hits) {
    if (!hit.fileId || !hit.fileId.trim()) continue
    const cur = best.get(hit.fileId)
    if (
      !cur ||
      hit.score > cur.score ||
      (hit.score === cur.score && (hit.snippet ?? '').length > (cur.snippet ?? '').length)
    ) {
      best.set(hit.fileId, hit)
    }
  }
  return [...best.values()]
    .sort((a, b) => b.score - a.score)
    .slice(0, Math.max(1, Math.min(limit, 200)))
}

export interface PeerCapability {
  target: 'android' | 'ios' | 'windows' | 'linux' | 'docker' | 'wasm'
  discovery: boolean
  httpSync: boolean
  bundle: boolean
  federatedSearch: boolean
  note: string
}

/**
 * Per-target P2P reality check (mirrors `peers::platform_notes`). The UI
 * renders this as the Devices → P2P matrix; every row names the working
 * fallback for the thing that target cannot do.
 */
export function peerCapabilityMatrix(): PeerCapability[] {
  return [
    {
      target: 'android',
      discovery: false,
      httpSync: true,
      bundle: true,
      federatedSearch: true,
      note: 'mDNS multicast is often blocked (no multicast lock) — use manual device@host:port peers + the cached peer list',
    },
    {
      target: 'ios',
      discovery: false,
      httpSync: true,
      bundle: true,
      federatedSearch: true,
      note: 'multicast restricted, background sockets suspended — manual peers + cached list; HTTP fan-out only while foregrounded',
    },
    {
      target: 'windows',
      discovery: true,
      httpSync: true,
      bundle: true,
      federatedSearch: true,
      note: 'no system mDNS daemon by default — active scan only; allow UDP 5353 / TCP 3456 through the firewall prompt',
    },
    {
      target: 'linux',
      discovery: true,
      httpSync: true,
      bundle: true,
      federatedSearch: true,
      note: 'port 5353 is usually owned by avahi — passive answering degrades, active scan still finds peers',
    },
    {
      target: 'docker',
      discovery: false,
      httpSync: true,
      bundle: true,
      federatedSearch: true,
      note: 'bridge networks block mDNS — run --network host for discovery or set explicit peer base URLs',
    },
    {
      target: 'wasm',
      discovery: false,
      httpSync: true,
      bundle: false,
      federatedSearch: true,
      note: 'no UDP sockets / no mDNS in the sandbox — fetch() to explicit https:// peer URLs only (CORS + mixed-content apply)',
    },
  ]
}

const PEER_CACHE_KEY = 'cybermanju.cachedPeers'

/** Manual peers cached for targets without mDNS (mobile/docker/wasm). */
export function loadCachedPeers(): PeerEndpoint[] {
  try {
    if (typeof localStorage === 'undefined') return []
    const raw = localStorage.getItem(PEER_CACHE_KEY)
    if (!raw) return []
    const items = JSON.parse(raw) as PeerEndpoint[]
    if (!Array.isArray(items)) return []
    return items.filter((p) => p && typeof p.deviceId === 'string' && typeof p.baseUrl === 'string')
  } catch {
    return []
  }
}

/** Persist manual peers (cap 32, dedup by device id). */
export function saveCachedPeers(peers: PeerEndpoint[]): void {
  try {
    if (typeof localStorage === 'undefined') return
    const seen = new Map<string, PeerEndpoint>()
    for (const p of peers) {
      if (!p || !p.deviceId.trim()) continue
      seen.set(p.deviceId, p)
    }
    localStorage.setItem(PEER_CACHE_KEY, JSON.stringify([...seen.values()].slice(0, 32)))
  } catch {
    // Storage unavailable — cache simply does not persist.
  }
}

function hitFromValue(value: unknown, source: string): FederatedHit | null {
  if (!value || typeof value !== 'object') return null
  const obj = value as Record<string, unknown>
  const fileId = String(obj.fileId ?? obj.file_id ?? obj.id ?? '')
  if (!fileId.trim()) return null
  return {
    fileId,
    fileName: String(obj.fileName ?? obj.file_name ?? obj.name ?? fileId),
    score: typeof obj.score === 'number' ? obj.score : 1.0,
    snippet: typeof obj.snippet === 'string' ? obj.snippet : '',
    source: typeof obj.source === 'string' && obj.source ? obj.source : source,
  }
}

async function fetchWithTimeout(url: string, init: RequestInit, timeoutMs: number): Promise<Response> {
  const fetchFn = (globalThis as { fetch?: typeof fetch }).fetch
  if (typeof fetchFn !== 'function') {
    throw new Error('unsupported: federated search needs fetch (browser or Node 18+)')
  }
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), timeoutMs)
  try {
    return await fetchFn(url, { ...init, signal: ctrl.signal })
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err)
    const aborted = err instanceof Error && (err.name === 'AbortError' || /abort/i.test(reason))
    throw new Error(
      aborted ? `network: peer search timed out after ${timeoutMs}ms calling ${url}: ${reason}` : `network: peer search failed for ${url}: ${reason}`,
    )
  } finally {
    clearTimeout(timer)
  }
}

/**
 * Query one peer's `GET /api/search?q=&limit=` with a bounded timeout.
 * Returns source-tagged hits (empty when the peer has no match). Throws
 * `auth:` on 401/403, `network:` otherwise — never a hang.
 */
export async function fetchPeerHits(
  baseUrl: string,
  query: string,
  limit = 20,
  timeoutMs = 8000,
  token?: string,
  deviceId?: string,
): Promise<FederatedHit[]> {
  if (!query.trim()) return []
  const rawBase = baseUrl.trim()
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(rawBase) && !/^https?:\/\//i.test(rawBase)) {
    throw new Error('invalid: peer URL must use HTTP or HTTPS')
  }
  const base = normalizeBaseUrl(baseUrl)
  const parsed = new URL(base)
  if (!['http:', 'https:'].includes(parsed.protocol) || parsed.username || parsed.password || parsed.search || parsed.hash) {
    throw new Error('invalid: peer URL must be HTTP(S), without credentials, query or fragment')
  }
  const host = parsed.hostname.replace(/^\[|\]$/g, '').toLowerCase()
  const ipv4 = host.split('.').map((part) => Number(part))
  const isIpv4 = ipv4.length === 4 && ipv4.every((part) => Number.isInteger(part) && part >= 0 && part <= 255)
  const blocked =
    host === 'localhost' ||
    host.endsWith('.localhost') ||
    host.startsWith('::') ||
    (host.includes(':') && host.startsWith('ff')) ||
    host.startsWith('fe80:') ||
    (isIpv4 && (ipv4[0] === 0 || ipv4[0] === 127 || (ipv4[0] === 169 && ipv4[1] === 254) || ipv4[0] >= 224))
  if (blocked) throw new Error('invalid: loopback, link-local and metadata destinations are blocked')
  const isPrivateV6 = host.includes(':') && (host.startsWith('fc') || host.startsWith('fd'))
  if ((isLanBase(base) || isPrivateV6) && (!deviceId?.trim() || !token?.trim())) {
    throw new Error('auth: LAN peers require an explicit device id and per-peer token')
  }
  if (!token?.trim()) throw new Error('auth: an explicit per-peer token is required')
  const capped = Math.max(1, Math.min(Math.floor(limit) || 20, 200))
  const url = peerSearchUrl(base, query, capped)
  const headers: Record<string, string> = {
    Accept: 'application/json',
    [FEDERATED_MARKER_HEADER]: '1',
  }
  headers.Authorization = `Bearer ${token.trim()}`
  const res = await fetchWithTimeout(url, { method: 'GET', headers, redirect: 'error' }, timeoutMs)
  if (res.status === 401 || res.status === 403) {
    throw new Error(`auth: peer search ${base} refused (HTTP ${res.status})`)
  }
  if (!res.ok) throw new Error(`network: peer search ${base} answered HTTP ${res.status}`)
  let body: unknown
  try {
    body = await res.json()
  } catch (err) {
    throw new Error(`network: peer search ${base} returned bad JSON: ${err instanceof Error ? err.message : String(err)}`)
  }
  const items: unknown[] = Array.isArray(body)
    ? body
    : Array.isArray((body as Record<string, unknown>)?.data)
      ? ((body as Record<string, unknown>).data as unknown[])
      : Array.isArray((body as Record<string, unknown>)?.results)
        ? ((body as Record<string, unknown>).results as unknown[])
        : []
  return items
    .map((v) => hitFromValue(v, base))
    .filter((h): h is FederatedHit => h !== null)
    .slice(0, capped)
}

export interface FederatedSearchOptions {
  peers: FederatedPeer[]
  limit?: number
  timeoutMs?: number
  /** Local hits (already searched); tagged `local` when source is empty. */
  localHits?: FederatedHit[]
}

/**
 * Cross-device search port: local hits plus one bounded peer fetch each
 * (max `MAX_FEDERATED_PEERS`), merged with `mergeFederatedHits`. A dead
 * peer contributes zero hits — never a failed search. Only an explicit
 * per-peer `token` is required and sent; the caller bearer is never used.
 */
export async function federatedSearch(
  query: string,
  options: FederatedSearchOptions,
): Promise<FederatedHit[]> {
  const limit = Math.max(1, Math.min(Math.floor(options.limit ?? 50) || 50, 200))
  const timeoutMs = Math.max(1000, Math.min(options.timeoutMs ?? 8000, 30000))
  if (!query.trim()) return []
  const hits: FederatedHit[] = (options.localHits ?? []).map((h) => ({
    ...h,
    source: h.source || 'local',
  }))
  const peers = deduplicatePeers(
    options.peers.filter((p) => p.baseUrl && p.baseUrl.trim() && p.token?.trim()),
  ).slice(0, MAX_FEDERATED_PEERS)
  const settled = await Promise.allSettled(
    peers.map((p) =>
      fetchPeerHits(p.baseUrl, query, limit, timeoutMs, p.token!.trim(), p.deviceId),
    ),
  )
  settled.forEach((result, i) => {
    if (result.status !== 'fulfilled') return
    const peer = peers[i]
    for (const hit of result.value) {
      hits.push(
        peer.deviceId && hit.source === peer.baseUrl
          ? { ...hit, source: peer.deviceId }
          : hit,
      )
    }
  })
  return mergeFederatedHits(hits, limit)
}

export interface SyncExchangeResult {
  pulled: number
  pushed: number
  steps: string[]
}

/**
 * Execute the direct device↔device op-log exchange against a peer base URL:
 * pull their ops since `since`, push ours, merge their ops locally via
 * `pushOps`, render conflicts via `fetchConflicts`. Relay tickets refuse
 * with `unsupported:` (they rendezvous via the relay mailbox); HTTP sync is
 * fail-closed and requires a signature from the explicitly named local device.
 * `since` is an inclusive next-sequence cursor; start at 0 and continue at `head`.
 */
export async function syncExchangeWithPeer(
  peer: PeerEndpoint,
  since: number,
  localOps: unknown[],
  deps: {
    localDeviceId: string
    /** Token configured for this peer only; never reuse the caller's bearer. */
    peerToken: string
    fetchJson: (method: string, url: string, body: unknown, peerToken: string) => Promise<unknown>
    pushOps: (ops: unknown[]) => Promise<{ appended: number }>
    fetchConflicts?: (peerToken: string) => Promise<unknown>
    /** Signs normalized operations using the sealed key for this device. */
    signer: (deviceId: string, normalizedOps: Record<string, unknown>[]) => Promise<{
      signature: string
      algorithm: string
    }>
  },
): Promise<SyncExchangeResult> {
  if (peer.transport === 'relay-future' || classifyAddr(peer.baseUrl) === 'relay-future') {
    throw new Error(
      `unsupported: relay dial to '${peer.deviceId}' rendezvouses via POST /api/relay/drop|poll (configure a relay_url) — direct HTTP dial, LAN sync and sneakernet bundles stay live`,
    )
  }
  if (localOps.length > 10_000) {
    throw new Error(`too_large: sync holds ${localOps.length} ops (cap 10000 per call — page it)`)
  }
  const base = normalizeBaseUrl(peer.baseUrl)
  const localDeviceId = typeof deps.localDeviceId === 'string' ? deps.localDeviceId.trim() : ''
  if (!localDeviceId) throw new Error('auth: a registered local device id is required for signed sync')
  const peerToken = typeof deps.peerToken === 'string' ? deps.peerToken.trim() : ''
  if (!peerToken) throw new Error('auth: sync requires a token explicitly configured for this peer')
  const { buildSignedSyncBody, normalizeOpsForEnvelope } = await import('./p2pPresence')
  const normalized = normalizeOpsForEnvelope(localOps)
  if (normalized.some((op) => op.deviceId !== localDeviceId)) {
    throw new Error('auth: every outgoing op must belong to the local signing device')
  }
  const s = Math.max(0, Math.floor(since) || 0)
  const pulled = (await deps.fetchJson(
    'GET',
    `${base}/api/oplog/since?since=${s}&limit=500`,
    undefined,
    peerToken,
  )) as { ops?: unknown[] }
  const theirOps = Array.isArray(pulled?.ops) ? pulled.ops : []
  // Attested flow only: the receiving server binds this path id to its
  // registered public key and the authenticated account.
  const { signature, algorithm } = await deps.signer(localDeviceId, normalized)
  const body = buildSignedSyncBody(localOps, signature, algorithm)
  const res = (await deps.fetchJson(
    'POST',
    `${base}/api/peers/${encodeURIComponent(localDeviceId)}/sync`,
    body,
    peerToken,
  )) as { verified?: boolean }
  const pushLine = `push: POST ${base}/api/peers/${localDeviceId}/sync { ops: ${localOps.length}, verified: ${res.verified === true} }`
  let appended = 0
  if (theirOps.length > 0) {
    appended = (await deps.pushOps(theirOps)).appended
  }
  if (deps.fetchConflicts) await deps.fetchConflicts(peerToken).catch(() => undefined)
  return {
    pulled: theirOps.length,
    pushed: localOps.length,
    steps: [
      `probe: GET ${base}/api/readyz (15 s abort, fail-closed)`,
      `pull: GET ${base}/api/oplog/since?since=${s} → ${theirOps.length} ops`,
      pushLine,
      `merge: merged ${appended} new ops locally`,
      'render: GET /api/oplog/conflicts',
    ],
  }
}
