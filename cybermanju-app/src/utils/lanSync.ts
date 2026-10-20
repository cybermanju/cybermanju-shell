// CyberManju OS — LAN direct sync (src/utils/lanSync.ts).
// Role: device↔device sync protocol over the LAN (ANALYSIS.md §2B.2 follow-up)
// — peer-auth check against the device registry, peer base-URL builder, and an
// explicit pull→push→render step plan reusing the op-log exchange.
// Exports: PeerDevice, peerBaseUrl, isPeerSyncable, lanSyncSteps, describePeer.
// Integrates: DevicesPanel LAN peers card (SYNC button); `lan sync` cybsh verb;
// server half is `POST /api/peers/{id}/sync` (peer auth via devices registry).
// Contracts: pure, never throws; revoked or missing device rows never sync
// (fail-closed); loopback peers keep their port; hostnames are never guessed.
export interface PeerDevice {
  deviceId: string
  name: string
  host: string
  port: number
  revoked?: boolean
}

/** `http://<host>:<port>` for a discovered peer (port always explicit, IPv6 bracketed). */
export function peerBaseUrl(peer: PeerDevice): string {
  let host = (peer.host || '').trim() || '127.0.0.1'
  // Bare IPv6 literals (`::1`, `fe80::1`) need brackets in a URL; already
  // bracketed forms (`[::1]`) pass through untouched.
  if (host.includes(':') && !host.startsWith('[')) host = `[${host}]`
  const port = Number.isInteger(peer.port) && peer.port > 0 && peer.port <= 65535 ? peer.port : 3456
  return `http://${host}:${port}`
}

/** Fail-closed gate: a peer syncs only with a known, non-revoked device row. */
export function isPeerSyncable(peer: PeerDevice, knownDeviceIds: Set<string> | string[]): boolean {
  if (peer.revoked) return false
  const id = peer.deviceId.trim()
  if (!id) return false
  const known = Array.isArray(knownDeviceIds) ? new Set(knownDeviceIds.map((v) => v.trim())) : knownDeviceIds
  return known.has(id)
}

/** Why a peer cannot sync (empty = syncable). */
export function peerBlockReason(peer: PeerDevice, knownDeviceIds: Set<string> | string[]): string {
  if (peer.revoked) return 'device revoked — re-enroll it first'
  const id = peer.deviceId.trim()
  if (!id) return 'peer has no device id'
  const known = Array.isArray(knownDeviceIds) ? new Set(knownDeviceIds.map((v) => v.trim())) : knownDeviceIds
  if (!known.has(id)) return 'unknown device — enroll it via Devices → Identity first'
  return ''
}

/** Explicit direct-sync steps (pull peer ops, push ours, render conflicts). */
export function lanSyncSteps(peer: PeerDevice, since: number): string[] {
  const base = peerBaseUrl(peer)
  const s = Number.isFinite(since) ? Math.max(0, Math.floor(since)) : 0
  return [
    `pull: GET ${base}/api/oplog/since?since=${s}`,
    `push: POST ${base}/api/peers/{localDeviceId}/sync { signed ops }`,
    'merge: append verified peer ops locally',
    'render: GET /api/oplog/conflicts',
  ]
}

/** One-line peer label for buttons and the cybsh verb. */
export function describePeer(peer: PeerDevice): string {
  const name = peer.name.trim() || peer.deviceId
  return `${name} (${peer.host}:${peer.port})`
}

/**
 * Execute the direct LAN op-log exchange against a discovered peer: pull →
 * push → merge → render via `syncExchangeWithPeer`. Same exchange as WAN —
 * only the base URL comes from mDNS discovery instead of a manual endpoint.
 */
export async function lanSyncExchange(
  peer: PeerDevice,
  since: number,
  localOps: unknown[],
  deps: {
    localDeviceId: string
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
  const { syncExchangeWithPeer } = await import('./peerSearch')
  return syncExchangeWithPeer(
    { deviceId: peer.deviceId, baseUrl: peerBaseUrl(peer), transport: 'lan-http' },
    since,
    localOps,
    deps,
  )
}
