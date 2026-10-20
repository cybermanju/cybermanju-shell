// CyberManju OS — presence + relay + signed envelopes (src/utils/p2pPresence.ts).
// Role: frontend mirror of the server-assisted P2P layer (`peers::announce_
// presence/known_presence/relay_drop/poll/ack`, `auth::SignedSyncBody`, and
// the `POST /api/peers/*` + `POST /api/relay/*` routes) — the answer for
// android/ios/docker/wasm where mDNS multicast cannot go, plus the WebRTC
// datachannel handshake that rendezvouses through the same relay mailbox.
// Exports: PresenceEntry, RelayMessage, SignedSyncBody, ENVELOPE_DOMAIN,
// stableStringify, normalizeOpsForEnvelope, canonicalOpsEnvelope,
// buildSignedSyncBody, buildAnnounceBody, knownPeersUrl, relayDropBody,
// webrtcAvailable, openDataChannelViaRelay, dropRelayMessage, pollRelayMailbox.
// Integrates: DevicesPanel presence card + signed SYNC button; `pex`/`relay`
// cybsh verbs; `syncExchangeWithPeer` signer hook; Rust twins are
// crates/sync/src/peers.rs + auth.rs (same JSON shapes, same caps, pinned by
// p2p-live.test.ts). Signing itself stays platform-injected (desktop: the
// `sign_peer_ops_for_device` Tauri command (sealed key; no seed IPC); Pages:
// wasm `ml_dsa65_sign`) so this module imports no composable and creates no cycle.
// Contracts: pure builders throw only `invalid:` (AGENT-1); network failures
// normalize to `network:`; nothing hangs (every fetch races an
// AbortController timeout); canonical bytes are byte-identical to Rust
// `auth::canonical_envelope_bytes` (stable-sorted keys — the pinned vector
// in the test proves it).

export interface PresenceEntry {
  deviceId: string
  host: string
  port: number
  seenAtSecs: number
}

export interface RelayMessage {
  id: string
  from: string
  to: string
  kind: string
  payload: unknown
  createdAtSecs: number
}

/** Wire body for `POST /api/peers/{id}/sync`; signature is mandatory. */
export interface SignedSyncBody {
  ops: Record<string, unknown>[]
  signature: string
  algorithm: string
}

/** Domain separation — identical to Rust `auth::ENVELOPE_DOMAIN`. */
export const ENVELOPE_DOMAIN = 'cybermanju-peer-sync/v1:'

/** Relay kinds the mailbox routes (op envelopes + WebRTC handshake). */
export const RELAY_KINDS = ['op-envelope', 'offer', 'answer', 'ice'] as const

/** Max relay message on the wire (mirrors Rust `RELAY_MAX_BYTES`). */
export const RELAY_MAX_BYTES = 256 * 1024

/**
 * Canonical JSON: objects with deeply sorted keys, arrays in order.
 * Matches Rust `serde_json::to_value` + `to_string` (BTreeMap maps) exactly
 * for the JSON value shapes op-log rows use — the pinned vector test pins
 * the byte equality, exotic lone-surrogate strings aside.
 */
export function stableStringify(value: unknown): string {
  if (value === null || value === undefined) return 'null'
  if (Array.isArray(value)) return `[${value.map((v) => stableStringify(v)).join(',')}]`
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
      .filter(([, v]) => v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    return `{${entries.map(([k, v]) => `${JSON.stringify(k)}:${stableStringify(v)}`).join(',')}}`
  }
  return JSON.stringify(value) ?? 'null'
}

/**
 * Normalize ops to the exact shapes Rust `OpRow` serializes (explicit
 * `fileId: null` instead of a missing key, `payload: null` instead of
 * missing, `bases: {}` instead of missing). ALWAYS transport this form —
 * the signature covers these bytes, so signing the unnormalized array and
 * sending the normalized one (or vice versa) fails verification.
 */
export function normalizeOpsForEnvelope(ops: unknown[]): Record<string, unknown>[] {
  return ops.map((raw) => {
    const o = (raw ?? {}) as Record<string, unknown>
    const bases = (o.bases ?? o.Bases ?? {}) as Record<string, unknown>
    return {
      seq: typeof o.seq === 'number' ? Math.max(0, Math.floor(o.seq)) : 0,
      opId: String(o.opId ?? o.op_id ?? o.opID ?? ''),
      deviceId: String(o.deviceId ?? o.device_id ?? ''),
      createdAt: String(o.createdAt ?? o.created_at ?? ''),
      kind: String(o.kind ?? ''),
      fileId: (o.fileId ?? o.file_id ?? null) as unknown,
      payload: (o.payload ?? null) as unknown,
      bases: bases && typeof bases === 'object' ? bases : {},
    }
  })
}

/** Canonical signed bytes — byte-identical to Rust `canonical_envelope_bytes`. */
export function canonicalOpsEnvelope(ops: unknown[]): string {
  return ENVELOPE_DOMAIN + stableStringify(normalizeOpsForEnvelope(ops))
}

/** Build the signed `POST /api/peers/{id}/sync` body. */
export function buildSignedSyncBody(
  ops: unknown[],
  signature: string,
  algorithm: string,
): SignedSyncBody {
  if (ops.length > 10_000) {
    throw new Error(`too_large: sync holds ${ops.length} ops (cap 10000 per call — page it)`)
  }
  if (!signature.trim() || !algorithm.trim()) {
    throw new Error('auth: signed peer sync requires a signature and algorithm')
  }
  return { ops: normalizeOpsForEnvelope(ops), signature, algorithm }
}

/** Build the `POST /api/peers/announce` body (single source of truth). */
export function buildAnnounceBody(deviceId: string, host: string, port: number): {
  deviceId: string
  host: string
  port: number
} {
  if (!deviceId.trim()) throw new Error('invalid: announce needs a device id')
  if (/\s/.test(deviceId)) throw new Error('invalid: device id must not contain whitespace')
  if (!host.trim()) throw new Error('invalid: announce needs a host')
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error('invalid: announce needs a port (1..=65535)')
  }
  return { deviceId: deviceId.trim(), host: host.trim(), port }
}

/** `GET <base>/api/peers/known` — presence reads for every transport. */
export function knownPeersUrl(base: string): string {
  return `${base.replace(/\/+$/, '')}/api/peers/known`
}

/** Build the `POST /api/relay/drop` body (size-capped like the server). */
export function relayDropBody(from: string, to: string, kind: string, payload: unknown): {
  from: string
  to: string
  kind: string
  payload: unknown
} {
  if (!from.trim() || !to.trim()) throw new Error('invalid: relay drop needs from and to device ids')
  if (!kind.trim()) throw new Error('invalid: relay drop needs a kind (op-envelope|offer|answer|ice)')
  const wire = JSON.stringify({ v: 1, from, to, kind, payload: payload ?? null })
  if (wire.length > RELAY_MAX_BYTES) {
    throw new Error(
      `too_large: relay message is ${wire.length} bytes (cap ${RELAY_MAX_BYTES} — ship bulk bytes as bundle payloads over direct HTTP)`,
    )
  }
  return { from, to, kind, payload: payload ?? null }
}

export interface RelayFetchDeps {
  fetchJson: (method: string, url: string, body?: unknown) => Promise<unknown>
}

/** Drop one envelope into a recipient's relay mailbox (outbound HTTPS only). */
export async function dropRelayMessage(
  relayBase: string,
  from: string,
  to: string,
  kind: string,
  payload: unknown,
  deps: RelayFetchDeps,
): Promise<{ accepted: boolean; id: string }> {
  const body = relayDropBody(from, to, kind, payload)
  const res = (await deps.fetchJson(
    'POST',
    `${relayBase.replace(/\/+$/, '')}/api/relay/drop`,
    body,
  )) as { accepted?: boolean; id?: string }
  return { accepted: res.accepted === true, id: String(res.id ?? '') }
}

/** Poll a recipient's relay mailbox (non-destructive — ack clears). */
export async function pollRelayMailbox(
  relayBase: string,
  forDevice: string,
  deps: RelayFetchDeps,
): Promise<RelayMessage[]> {
  if (!forDevice.trim()) throw new Error('invalid: relay poll needs a device id')
  const res = (await deps.fetchJson('POST', `${relayBase.replace(/\/+$/, '')}/api/relay/poll`, {
    for: forDevice,
  })) as { messages?: RelayMessage[] }
  return Array.isArray(res.messages) ? res.messages : []
}

/** Ack delivered relay messages (returns the cleared count). */
export async function ackRelayMessages(
  relayBase: string,
  forDevice: string,
  ids: string[],
  deps: RelayFetchDeps,
): Promise<number> {
  if (!forDevice.trim()) throw new Error('invalid: relay ack needs a device id')
  const res = (await deps.fetchJson('POST', `${relayBase.replace(/\/+$/, '')}/api/relay/ack`, {
    for: forDevice,
    ids,
  })) as { cleared?: number }
  return Number(res.cleared ?? 0)
}

/** True when the browser can open a WebRTC datachannel (false in node/WKWebView-less shells). */
export function webrtcAvailable(): boolean {
  return typeof (globalThis as { RTCPeerConnection?: unknown }).RTCPeerConnection === 'function'
}

export interface DataChannelDeps extends RelayFetchDeps {
  /** Milliseconds to wait for the answer + channel open (default 30 s). */
  timeoutMs?: number
  /** Channel label (default `cybermanju-p2p`). */
  label?: string
}

/**
 * Open a WebRTC datachannel to a peer, rendezvousing the offer/answer/ICE
 * handshake through the relay mailbox (both sides outbound HTTPS — the same
 * NAT traversal as op envelopes, browser-native, no new deps). Resolves with
 * the open channel; throws `unsupported:` without WebRTC, `network:` on
 * timeout. The caller closes the channel when done.
 */
export async function openDataChannelViaRelay(
  relayBase: string,
  localId: string,
  remoteId: string,
  deps: DataChannelDeps,
  // oxlint-disable-next-line no-explicit-any
): Promise<any> {
  const RTC = (globalThis as { RTCPeerConnection?: new (...args: never[]) => any })
    .RTCPeerConnection
  if (typeof RTC !== 'function') {
    throw new Error(
      'unsupported: WebRTC datachannels need a browser RTCPeerConnection — use relay op-envelopes (POST /api/relay/drop|poll) or direct HTTP sync instead',
    )
  }
  if (!localId.trim() || !remoteId.trim()) {
    throw new Error('invalid: datachannel needs local and remote device ids')
  }
  const timeoutMs = Math.max(2000, Math.min(deps.timeoutMs ?? 30000, 120000))
  const label = deps.label?.trim() || 'cybermanju-p2p'
  const pc = new RTC()
  const channel = pc.createDataChannel(label)
  try {
    const offer = await pc.createOffer()
    await pc.setLocalDescription(offer)
    await dropRelayMessage(relayBase, localId, remoteId, 'offer', { sdp: offer.sdp }, deps)
    const deadline = Date.now() + timeoutMs
    let answer: RelayMessage | null = null
    while (Date.now() < deadline) {
      const messages = await pollRelayMailbox(relayBase, localId, deps).catch(() => [])
      for (const m of messages) {
        if (m.from !== remoteId) continue
        if (m.kind === 'answer' && !answer) {
          answer = m
          const sdp = (m.payload as { sdp?: string } | null)?.sdp
          if (typeof sdp === 'string' && sdp) {
            await pc.setRemoteDescription({ type: 'answer', sdp })
          }
        } else if (m.kind === 'ice') {
          const candidate = (m.payload as { candidate?: unknown } | null)?.candidate
          if (candidate) await pc.addIceCandidate(candidate).catch(() => undefined)
        }
      }
      if (answer && channel.readyState === 'open') break
      await new Promise((r) => setTimeout(r, 750))
    }
    // Trickle our own candidates while waiting (best-effort, never fatal).
    pc.onicecandidate = (event: { candidate?: unknown }) => {
      if (event.candidate) {
        dropRelayMessage(relayBase, localId, remoteId, 'ice', { candidate: event.candidate }, deps).catch(
          () => undefined,
        )
      }
    }
    if (channel.readyState !== 'open') {
      const opened = await new Promise<boolean>((resolve) => {
        const left = Math.max(1000, deadline - Date.now())
        const timer = setTimeout(() => resolve(false), left)
        channel.onopen = () => {
          clearTimeout(timer)
          resolve(true)
        }
      })
      if (!opened) throw new Error(`network: datachannel to ${remoteId} did not open within ${timeoutMs}ms`)
    }
    return channel
  } catch (err) {
    try {
      channel.close()
    } catch {
      // Close is best-effort on the failure path.
    }
    try {
      pc.close()
    } catch {
      // Close is best-effort on the failure path.
    }
    throw err
  }
}
