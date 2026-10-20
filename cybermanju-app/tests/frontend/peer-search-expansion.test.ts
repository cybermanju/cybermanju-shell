// CyberManju OS — peer-search-expansion.test (tests/frontend/peer-search-expansion.test.ts).
// Role: pin the P2P expansion — shared URL builders (same shape as
// `peers.rs`), peer dedup/cap, wasm mixed-content/loopback warnings, the
// federated body builder (per-peer token passthrough), and the WAN relay
// gate + federated plan (same grammar as `iroh.rs`).
// Exports: none (vitest suite).
// Integrates: src/utils/peerSearch.ts, src/utils/wanDial.ts; Rust twins are
// crates/sync/src/peers.rs + crates/sync/src/iroh.rs.
// Contracts: pure builders throw only `invalid:`/`unsupported:` (AGENT-1);
// nothing opens a socket; nothing hangs.
import { describe, expect, it } from 'vitest'
import {
  buildFederatedBody,
  deduplicatePeers,
  FEDERATED_MARKER_HEADER,
  MAX_FEDERATED_PEERS,
  peerOplogPushUrl,
  peerOplogSinceUrl,
  peerReadyzUrl,
  peerSearchUrl,
  wasmPeerWarning,
} from '@/utils/peerSearch'
import { isRelayAddr, wanFederatedPlan, wanPeekUrls } from '@/utils/wanDial'

describe('shared peer URL builders (mirror peers.rs)', () => {
  it('spells readyz/search/oplog/push identically', () => {
    expect(peerReadyzUrl('192.168.1.60:3456')).toBe('http://192.168.1.60:3456/api/readyz')
    expect(peerSearchUrl('http://192.168.1.60:3456/', 'hello world', 20)).toBe(
      'http://192.168.1.60:3456/api/search?q=hello%20world&limit=20',
    )
    expect(peerOplogSinceUrl('http://vault.example/', 41)).toBe(
      'http://vault.example/api/oplog/since?since=41&limit=500',
    )
    expect(peerOplogPushUrl('vault.example:3456')).toBe('http://vault.example:3456/api/oplog/push')
    expect(FEDERATED_MARKER_HEADER).toBe('x-cybermanju-federated')
  })
})

describe('peer dedup + cap', () => {
  it('keeps first base, drops blanks, caps at 8', () => {
    const peers = [
      { baseUrl: 'http://10.0.0.1:3456', deviceId: 'a' },
      { baseUrl: 'http://10.0.0.1:3456/', deviceId: 'a2' },
      { baseUrl: '  ', deviceId: 'blank' },
      ...Array.from({ length: 20 }, (_, i) => ({
        baseUrl: `http://10.0.0.${i + 2}:3456`,
        deviceId: `p${i}`,
      })),
    ]
    const out = deduplicatePeers(peers)
    expect(out.length).toBe(MAX_FEDERATED_PEERS)
    expect(out[0].deviceId).toBe('a')
  })
})

describe('wasm peer warnings', () => {
  it('names mixed-content and loopback before fetch runs', () => {
    expect(wasmPeerWarning('http://192.168.1.60:3456', true)).toMatch(/^mixed-content:/)
    expect(wasmPeerWarning('https://vault.example.com', true)).toBeNull()
    expect(wasmPeerWarning('http://localhost:3456', false)).toMatch(/^loopback:/)
    expect(wasmPeerWarning('  ', false)).toMatch(/^invalid:/)
  })
})

describe('federated body builder', () => {
  it('dedups peers and passes per-peer tokens through', async () => {
    const body = buildFederatedBody({
      query: 'photo',
      peers: [
        { baseUrl: 'http://10.0.0.1:3456', deviceId: 'a', token: 'peer-secret' },
        { baseUrl: 'http://10.0.0.1:3456/', deviceId: 'a2' },
        { baseUrl: '', deviceId: 'blank' },
      ],
    })
    expect(body.query).toBe('photo')
    expect(body.peers).toHaveLength(1)
    expect(body.peers[0]).toMatchObject({ baseUrl: 'http://10.0.0.1:3456', token: 'peer-secret' })
    const { REST_ROUTES } = await import('@/composables/useTauri')
    const shaped = REST_ROUTES.search_files_federated.transformRequest!({
      query: 'photo',
      peers: [{ baseUrl: 'http://10.0.0.1:3456', deviceId: 'a', token: 'peer-secret' }],
      token: 'caller-secret',
    }) as { peers: Array<Record<string, unknown>> }
    expect(shaped.peers[0]).toMatchObject({ baseUrl: 'http://10.0.0.1:3456', token: 'peer-secret' })
    expect(shaped).not.toHaveProperty('token')
  })
})

describe('wan relay gate + federated plan (mirror iroh.rs)', () => {
  it('shares one relay grammar with the peer seam', () => {
    expect(isRelayAddr('relay.cybermanju.example')).toBe(true)
    expect(isRelayAddr('iroh:ticket-abc')).toBe(true)
    expect(isRelayAddr('192.168.1.60:3456')).toBe(false)
  })

  it('plans local BM25 + one bounded peer fetch, refuses relays', () => {
    const plan = wanFederatedPlan({ deviceId: 'phone-7', addr: '192.168.1.60:3456' }, 'photo', 20)
    expect(plan).toHaveLength(2)
    expect(plan[1]).toContain('/api/search?q=photo')
    expect(() =>
      wanFederatedPlan({ deviceId: 'phone-7', addr: 'relay.example' }, 'photo'),
    ).toThrow(/^unsupported:/)
    expect(() =>
      wanFederatedPlan({ deviceId: 'phone-7', addr: '192.168.1.60:3456' }, '   '),
    ).toThrow(/^invalid:/)
  })

  it('peeks the same three URLs the Rust seam builds', () => {
    const urls = wanPeekUrls({ deviceId: 'phone-7', addr: '192.168.1.60:3456' })
    expect(urls.readyz).toBe('http://192.168.1.60:3456/api/readyz')
    expect(urls.search('photo', 20)).toContain('/api/search?q=photo')
    expect(urls.oplogSince(41)).toContain('/api/oplog/since?since=41')
  })
})
