// CyberManju OS — p2p-hardening.test (tests/frontend/p2p-hardening.test.ts).
// Role: pin the P2P hardening pass — token-based relay grammar (no `relayx`
// false positives), host-based LAN classification (no `110.` false positives),
// IPv6 peer URLs, WAN relay refusal before dial, relay-free federated fan-out,
// op-log size caps, and timeout clamps shared by every transport.
// Exports: none (vitest suite).
// Integrates: src/utils/peerSearch.ts; src/utils/wanDial.ts; src/utils/lanSync.ts.
// Contracts: pure builders throw only `invalid:`/`unsupported:`/`too_large:`
// (AGENT-1); relay endpoints never open a socket; nothing hangs.
import { describe, expect, it } from 'vitest'
import {
  buildFederatedBody,
  classifyAddr,
  deduplicatePeers,
  extractPeerHost,
  federatedSearch,
  isLanBase,
  isRelayToken,
  syncExchangeWithPeer,
} from '@/utils/peerSearch'
import { isRelayAddr, parseWanEndpoint, wanHttpBase, wanSyncExchange } from '@/utils/wanDial'
import { lanSyncSteps, peerBaseUrl } from '@/utils/lanSync'

describe('relay grammar is token-based (mirrors peers::is_relay_token)', () => {
  it('treats relay tokens as QUIC future', () => {
    expect(isRelayToken('relay.cybermanju.example')).toBe(true)
    expect(isRelayToken('relay:443')).toBe(true)
    expect(isRelayToken('my-relay-host.example')).toBe(true)
    expect(classifyAddr('relay.cybermanju.example')).toBe('relay-future')
    expect(classifyAddr('iroh:ticket-abc')).toBe('relay-future')
    expect(classifyAddr('/ip4/1.2.3.4/p2p/abcd')).toBe('relay-future')
    expect(isRelayAddr('relay.cybermanju.example')).toBe(true)
  })

  it('does not misfire on relayx hostnames', () => {
    expect(isRelayToken('relayx.example.com')).toBe(false)
    expect(isRelayToken('reliable.example.com:3456')).toBe(false)
    expect(classifyAddr('relayx.example.com:3456')).toBe('wan-http')
    expect(isRelayAddr('relayx.example.com:3456')).toBe(false)
  })
})

describe('LAN classification is host-based', () => {
  it('keeps true LAN hosts', () => {
    expect(isLanBase('http://192.168.1.60:3456')).toBe(true)
    expect(isLanBase('http://10.0.0.9:3456')).toBe(true)
    expect(isLanBase('http://172.20.4.1:3456')).toBe(true)
    expect(isLanBase('http://[::1]:3456')).toBe(true)
    expect(isLanBase('peer.local:3456')).toBe(true)
  })

  it('rejects lookalikes and public hosts', () => {
    expect(isLanBase('http://110.0.0.1:3456')).toBe(false)
    expect(isLanBase('https://vault.example.com')).toBe(false)
    expect(isLanBase('http://172.32.0.1:3456')).toBe(false)
    expect(isLanBase('http://172.15.0.1:3456')).toBe(false)
    expect(extractPeerHost('https://vault.example.com/api/search')).toBe('vault.example.com')
  })
})

describe('peer URLs handle IPv6 and defaults', () => {
  it('brackets bare IPv6 literals', () => {
    expect(peerBaseUrl({ deviceId: 'p', name: 'p', host: '::1', port: 3456 })).toBe('http://[::1]:3456')
    expect(peerBaseUrl({ deviceId: 'p', name: 'p', host: '[::1]', port: 3456 })).toBe('http://[::1]:3456')
    expect(peerBaseUrl({ deviceId: 'p', name: 'p', host: '192.168.1.60', port: 0 })).toBe('http://192.168.1.60:3456')
  })

  it('defaults bare WAN hosts to :3456 (https keeps implicit 443)', () => {
    expect(wanHttpBase(parseWanEndpoint('vault@vault.example.com'))).toBe('http://vault.example.com:3456')
    expect(wanHttpBase(parseWanEndpoint('vault@vault.example.com:9000'))).toBe('http://vault.example.com:9000')
    expect(wanHttpBase(parseWanEndpoint('vault@https://vault.example.com'))).toBe('https://vault.example.com')
  })

  it('clamps LAN since values instead of printing NaN', () => {
    const steps = lanSyncSteps({ deviceId: 'p', name: 'p', host: '192.168.1.60', port: 3456 }, NaN)
    expect(steps[0]).toContain('since=0')
  })
})

describe('relay peers never dial or fan out', () => {
  it('wanSyncExchange refuses relay endpoints before any fetch', async () => {
    const deps = {
      fetchJson: async () => ({ ops: [] }),
      pushOps: async () => ({ appended: 0 }),
    }
    await expect(
      wanSyncExchange({ deviceId: 'phone-7', addr: 'relay.cybermanju.example' }, 0, [], deps),
    ).rejects.toThrow(/^unsupported:/)
  })

  it('deduplicatePeers drops relay tickets and dedupes case-insensitively', () => {
    const out = deduplicatePeers([
      { baseUrl: 'http://10.0.0.1:3456', deviceId: 'a' },
      { baseUrl: 'HTTP://10.0.0.1:3456/', deviceId: 'a2' },
      { baseUrl: 'http://relay.example', deviceId: 'relay' },
    ])
    expect(out).toHaveLength(1)
    expect(out[0].deviceId).toBe('a')
  })

  it('syncExchangeWithPeer enforces the 10k op cap like the server', async () => {
    const deps = {
      fetchJson: async () => ({ ops: [] }),
      pushOps: async () => ({ appended: 0 }),
    }
    await expect(
      syncExchangeWithPeer(
        { deviceId: 'p', baseUrl: 'http://10.0.0.1:3456', transport: 'lan-http' },
        0,
        new Array(10_001).fill({}),
        deps,
      ),
    ).rejects.toThrow(/^too_large:/)
  })

  it('syncExchangeWithPeer refuses an unconfigured peer token before any fetch', async () => {
    let fetched = 0
    await expect(
      syncExchangeWithPeer(
        { deviceId: 'p', baseUrl: 'https://peer.example', transport: 'wan-http' },
        0,
        [],
        {
          localDeviceId: 'local-device',
          peerToken: '',
          fetchJson: async () => {
            fetched += 1
            return { ops: [] }
          },
          pushOps: async () => ({ appended: 0 }),
          signer: async () => ({ signature: 'unused', algorithm: 'ml-dsa-65' }),
        },
      ),
    ).rejects.toThrow(/^auth:/)
    expect(fetched).toBe(0)
  })

  it('federatedSearch skips relay peers without fetching', async () => {
    let fetched = 0
    ;(globalThis as { fetch?: unknown }).fetch = (async () => {
      fetched += 1
      return { ok: true, status: 200, json: async () => [] }
    }) as typeof fetch
    try {
      const hits = await federatedSearch('photo', {
        peers: [{ baseUrl: 'http://relay.example', deviceId: 'relay' }],
        localHits: [{ fileId: 'l1', fileName: 'l1.txt', score: 1, snippet: '', source: 'local' }],
      })
      expect(hits.map((h) => h.fileId)).toEqual(['l1'])
      expect(fetched).toBe(0)
    } finally {
      delete (globalThis as { fetch?: unknown }).fetch
    }
  })

  it('federated body clamps timeouts to the server range', () => {
    expect(buildFederatedBody({ query: 'q', timeoutSecs: 99 }).timeoutSecs).toBe(30)
    expect(buildFederatedBody({ query: 'q', timeoutSecs: 0 }).timeoutSecs).toBe(8)
  })
})
