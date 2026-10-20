// CyberManju OS — p2p-live.test (tests/frontend/p2p-live.test.ts).
// Role: pin the live P2P layer — relay-over-HTTPS envelopes, server-assisted
// presence, relay mailbox clients, ML-DSA signed sync envelopes (canonical
// bytes byte-identical to Rust `auth::canonical_envelope_bytes`), the
// WebRTC handshake helper, the attested op-log exchange, and the new REST
// routes + cybsh verbs.
// Exports: none (vitest suite).
// Integrates: src/utils/p2pPresence.ts; src/utils/wanDial.ts;
// src/utils/peerSearch.ts (`syncExchangeWithPeer` signer hook);
// src/composables/useTauri.ts (REST_ROUTES + REST_FIRST); staticCybsh
// (`pex`/`relay` verbs). Rust twins: crates/sync/src/auth.rs + peers.rs +
// iroh.rs, crates/web/src/lib.rs (same shapes, same caps).
// Contracts: pure builders throw only `invalid:`/`too_large:`/`unsupported:`
// (AGENT-1); node has no RTCPeerConnection so WebRTC answers `unsupported:`;
/// every fetch is injected and abort-bounded in production.
import { describe, expect, it } from 'vitest'
import {
  ackRelayMessages,
  buildAnnounceBody,
  buildSignedSyncBody,
  canonicalOpsEnvelope,
  dropRelayMessage,
  ENVELOPE_DOMAIN,
  knownPeersUrl,
  normalizeOpsForEnvelope,
  openDataChannelViaRelay,
  pollRelayMailbox,
  relayDropBody,
  stableStringify,
  webrtcAvailable,
} from '@/utils/p2pPresence'
import { buildRelayEnvelope, relayDropPath, relayPollPath } from '@/utils/wanDial'
import { syncExchangeWithPeer } from '@/utils/peerSearch'

describe('canonical envelope (byte-identical to Rust)', () => {
  it('sorts keys deeply and prefixes the domain', () => {
    expect(canonicalOpsEnvelope([])).toBe(`${ENVELOPE_DOMAIN}[]`)
    const ops = [
      {
        seq: 7,
        opId: 'laptop-7',
        deviceId: 'laptop',
        createdAt: '2026-10-11T00:00:00Z',
        kind: 'tag',
        fileId: 'f1',
        payload: { z: 1, a: [3, 2] },
        bases: { laptop: 7 },
      },
    ]
    // Pinned against crates/sync/src/auth.rs `envelope_sign_verify_round_trip`.
    expect(canonicalOpsEnvelope(ops)).toBe(
      'cybermanju-peer-sync/v1:[{"bases":{"laptop":7},"createdAt":"2026-10-11T00:00:00Z","deviceId":"laptop","fileId":"f1","kind":"tag","opId":"laptop-7","payload":{"a":[3,2],"z":1},"seq":7}]',
    )
  })

  it('normalizes missing keys to explicit nulls (serde defaults)', () => {
    const [op] = normalizeOpsForEnvelope([{ kind: 'star' }])
    expect(op).toMatchObject({ seq: 0, opId: '', fileId: null, payload: null, bases: {} })
    expect(stableStringify({ b: 1, a: { d: 4, c: 3 } })).toBe('{"a":{"c":3,"d":4},"b":1}')
  })

  it('builds signed bodies and enforces the 10k cap', () => {
    const body = buildSignedSyncBody([{ kind: 'tag' }], 'sig-b64', 'ml-dsa-65')
    expect(body.signature).toBe('sig-b64')
    expect(body.ops[0]).toMatchObject({ kind: 'tag', fileId: null })
    expect(() => buildSignedSyncBody([], '', '')).toThrow(/^auth:/)
    expect(() => buildSignedSyncBody(new Array(10_001).fill({}), '', '')).toThrow(/^too_large:/)
  })
})

describe('relay envelopes + mailbox clients', () => {
  it('builds v1 envelopes with identical shape on both seams', () => {
    const env = JSON.parse(buildRelayEnvelope('phone-7', 'op-envelope', { ops: [] }))
    expect(env).toMatchObject({ v: 1, to: 'phone-7', kind: 'op-envelope' })
    expect(() => buildRelayEnvelope('', 'op-envelope', null)).toThrow(/^invalid:/)
    expect(relayDropPath('https://relay.example.com/')).toBe('https://relay.example.com/api/relay/drop')
    expect(relayPollPath('https://relay.example.com')).toBe('https://relay.example.com/api/relay/poll')
  })

  it('caps relay messages at 256 KiB like the server', () => {
    expect(() => relayDropBody('a', 'b', 'op-envelope', { ops: [] }).kind).not.toThrow()
    expect(() => relayDropBody('', 'b', 'op-envelope', null)).toThrow(/^invalid:/)
    expect(() => relayDropBody('a', 'b', '', null)).toThrow(/^invalid:/)
    expect(() => relayDropBody('a', 'b', 'op-envelope', { blob: 'x'.repeat(256 * 1024 + 1) })).toThrow(
      /^too_large:/,
    )
  })

  it('drops, polls and acks through injected fetch', async () => {
    const calls: unknown[][] = []
    const deps = {
      fetchJson: async (method: string, url: string, body?: unknown) => {
        calls.push([method, url, body])
        if (url.endsWith('/api/relay/drop')) return { accepted: true, id: 'abc123' }
        if (url.endsWith('/api/relay/poll')) {
          return { messages: [{ id: 'abc123', from: 'phone-7', to: 'laptop', kind: 'op-envelope', payload: {}, createdAtSecs: 1 }] }
        }
        return { cleared: 1 }
      },
    }
    const dropped = await dropRelayMessage('https://relay.example.com', 'laptop', 'phone-7', 'op-envelope', { ops: [] }, deps)
    expect(dropped).toMatchObject({ accepted: true, id: 'abc123' })
    const messages = await pollRelayMailbox('https://relay.example.com', 'laptop', deps)
    expect(messages).toHaveLength(1)
    expect(messages[0].from).toBe('phone-7')
    expect(await ackRelayMessages('https://relay.example.com', 'laptop', ['abc123'], deps)).toBe(1)
    await expect(pollRelayMailbox('https://relay.example.com', '  ', deps)).rejects.toThrow(/^invalid:/)
  })

  it('builds presence bodies and known-peers URLs', () => {
    expect(buildAnnounceBody('phone-7', '192.168.1.60', 3456)).toMatchObject({
      deviceId: 'phone-7',
      port: 3456,
    })
    expect(() => buildAnnounceBody('  ', 'h', 3456)).toThrow(/^invalid:/)
    expect(() => buildAnnounceBody('x', '', 3456)).toThrow(/^invalid:/)
    expect(() => buildAnnounceBody('x', 'h', 0)).toThrow(/^invalid:/)
    expect(knownPeersUrl('https://vault.example.com/')).toBe('https://vault.example.com/api/peers/known')
  })
})

describe('WebRTC handshake helper', () => {
  it('reports unavailability honestly in node', () => {
    expect(webrtcAvailable()).toBe(false)
  })

  it('refuses without WebRTC instead of hanging', async () => {
    await expect(
      openDataChannelViaRelay('https://relay.example.com', 'a', 'b', {
        fetchJson: async () => ({}),
      }),
    ).rejects.toThrow(/^unsupported:/)
  })
})

describe('attested op-log exchange', () => {
  const deps = (seen: { posts: unknown[][] }) => ({
    localDeviceId: 'laptop-1',
    peerToken: 'token-for-phone-7',
    signer: async (deviceId: string, normalized: Record<string, unknown>[]) => {
      expect(deviceId).toBe('laptop-1')
      expect(normalized.every((op) => op.deviceId === deviceId)).toBe(true)
      return { signature: 'sig-b64', algorithm: 'ml-dsa-65' }
    },
    fetchJson: async (method: string, url: string, body?: unknown, peerToken?: string) => {
      seen.posts.push([method, url, body, peerToken])
      if (url.includes('/api/oplog/since')) return { head: 1, ops: [] }
      return { appended: 1, verified: true }
    },
    pushOps: async (ops: unknown[]) => ({ appended: ops.length }),
  })

  it('refuses ops not owned by the local signing device before network access', async () => {
    const seen = { posts: [] } as { posts: unknown[][] }
    await expect(syncExchangeWithPeer(
      { deviceId: 'phone-7', baseUrl: 'http://192.168.1.60:3456', transport: 'lan-http' },
      0,
      [{ opId: 'ours', deviceId: 'other-device' }],
      deps(seen),
    )).rejects.toThrow(/^auth:/)
    expect(seen.posts).toHaveLength(0)
  })

  it('pushes signed envelopes under the local sender device id', async () => {
    const seen = { posts: [] } as { posts: unknown[][] }
    const out = await syncExchangeWithPeer(
      { deviceId: 'phone-7', baseUrl: 'http://192.168.1.60:3456', transport: 'lan-http' },
      0,
      [{ opId: 'ours', kind: 'tag', deviceId: 'laptop-1' }],
      deps(seen),
    )
    const push = seen.posts.find(([, url]) => String(url).includes('/api/peers/laptop-1/sync'))
    expect(push).toBeTruthy()
    expect((push![2] as Record<string, unknown>).signature).toBe('sig-b64')
    expect(seen.posts.every(([, , , token]) => token === 'token-for-phone-7')).toBe(true)
    expect(out.steps.join('\n')).toContain('verified: true')
  })
})

describe('new REST routes + cybsh verbs', () => {
  it('maps presence, relay and bundle-build over REST-first routes', async () => {
    const { REST_FIRST, REST_ROUTES } = await import('@/composables/useTauri')
    const paths: Record<string, string> = {
      peers_known: '/api/peers/known',
      peers_announce: '/api/peers/announce',
      relay_drop: '/api/relay/drop',
      relay_poll: '/api/relay/poll',
      relay_ack: '/api/relay/ack',
      bundle_build: '/api/sync/bundle/build',
    }
    for (const [cmd, path] of Object.entries(paths)) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
      expect(REST_ROUTES[cmd].buildPath({}), cmd).toBe(path)
      expect(REST_FIRST.has(cmd), cmd).toBe(true)
    }
    expect(REST_ROUTES.relay_poll.transformRequest!({ forDevice: 'phone-7' })).toMatchObject({
      for: 'phone-7',
    })
    expect(
      REST_ROUTES.peer_sync.transformRequest!({ id: 'p', ops: [], signature: 's', algorithm: 'a' }),
    ).toMatchObject({ ops: [], signature: 's', algorithm: 'a' })
    expect(
      REST_ROUTES.oplog_push.transformRequest!({
        deviceId: 'laptop-1',
        ops: [],
        signature: 's',
        algorithm: 'ml-dsa-65',
      }),
    ).toMatchObject({ deviceId: 'laptop-1', signature: 's', algorithm: 'ml-dsa-65' })
  })

  it('serves pex + relay verbs from the static shell', async () => {
    const { handlesStaticVerb, runStaticCybshLine } = await import('@/utils/staticCybsh')
    expect(handlesStaticVerb('pex')).toBe(true)
    expect(handlesStaticVerb('relay')).toBe(true)
    const deps = { execFallback: undefined } as never
    const announce = await runStaticCybshLine('pex announce phone-7 192.168.1.60 3456', deps)
    expect(JSON.stringify(announce)).toContain('/api/peers/announce')
    expect(JSON.stringify(await runStaticCybshLine('pex announce phone-7', deps))).toContain(
      'usage: pex announce',
    )
    const known = await runStaticCybshLine('pex known', deps)
    expect(JSON.stringify(known)).toContain('/api/peers/known')
    const drop = await runStaticCybshLine('relay drop phone-7 op-envelope', deps)
    expect(JSON.stringify(drop)).toContain('/api/relay/drop')
    const poll = await runStaticCybshLine('relay poll', deps)
    expect(JSON.stringify(poll)).toContain('needs the dashboard')
  })
})
