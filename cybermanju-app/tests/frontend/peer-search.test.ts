// CyberManju OS — peer-search.test (tests/frontend/peer-search.test.ts).
// Role: pin the cross-device search port (§2B P2P expansion) — one peer
// grammar (`<device>@<addr>`), one merge order (dedup by file id, max score
// wins, desc sort, same as `peers::merge_federated_hits`), the per-target
// capability matrix (android/ios/docker/wasm have no mDNS discovery), the
// manual peer cache, the fetch fan-out (dead peers degrade to zero hits),
// and the executable LAN/WAN op-log exchange (relay refuses `unsupported:`).
// Exports: none (vitest suite).
// Integrates: src/utils/peerSearch.ts (twin: crates/sync/src/peers.rs);
// src/utils/wanDial.ts `wanSyncExchange`; src/utils/lanSync.ts
// `lanSyncExchange`; REST twin `POST /api/search/federated`.
// Contracts: pure builders throw only `invalid:`/`unsupported:` (AGENT-1);
// network failures normalize to `network:`/`auth:`; nothing hangs (every
// fetch races an AbortController timeout).
import { describe, expect, it, afterEach } from 'vitest'
import {
  classifyAddr,
  federatedSearch,
  fetchPeerHits,
  isLanBase,
  loadCachedPeers,
  mergeFederatedHits,
  normalizeBaseUrl,
  parsePeerEndpoint,
  peerCapabilityMatrix,
  saveCachedPeers,
  syncExchangeWithPeer,
  type FederatedHit,
} from '@/utils/peerSearch'

const realFetch = (globalThis as { fetch?: typeof fetch }).fetch

afterEach(() => {
  if (realFetch) (globalThis as { fetch?: typeof fetch }).fetch = realFetch
  else delete (globalThis as { fetch?: typeof fetch }).fetch
})

describe('peer endpoint grammar (mirrors peers.rs)', () => {
  it('parses <device>@<addr> and classifies the transport', () => {
    expect(parsePeerEndpoint('phone-7@192.168.1.60:3456')).toEqual({
      deviceId: 'phone-7',
      baseUrl: 'http://192.168.1.60:3456',
      transport: 'lan-http',
    })
    expect(parsePeerEndpoint('vault@https://vault.example.com').transport).toBe('wan-http')
    expect(parsePeerEndpoint('phone-7@relay.cybermanju.example').transport).toBe('relay-future')
  })

  it('refuses missing halves and whitespace with invalid:', () => {
    for (const bad of ['no-at-sign', '@addr', 'id@', 'my id@addr', 'id@my addr']) {
      expect(() => parsePeerEndpoint(bad)).toThrow(/^invalid:/)
    }
    expect(() => normalizeBaseUrl('  ')).toThrow(/^invalid:/)
    expect(() => normalizeBaseUrl('my host')).toThrow(/^invalid:/)
  })

  it('keeps explicit schemes and spots LAN bases', () => {
    expect(normalizeBaseUrl('https://vault.example.com/')).toBe('https://vault.example.com')
    expect(isLanBase('http://192.168.1.60:3456')).toBe(true)
    expect(isLanBase('http://localhost:3456')).toBe(true)
    expect(isLanBase('https://vault.example.com')).toBe(false)
    expect(classifyAddr('iroh:ticket-abc')).toBe('relay-future')
  })
})

describe('federated merge order (mirrors merge_federated_hits)', () => {
  const mk = (fileId: string, score: number, source: string, snippet = ''): FederatedHit => ({
    fileId,
    fileName: `${fileId}.txt`,
    score,
    snippet,
    source,
  })

  it('dedups by file id, max score wins, desc sort, limit', () => {
    const out = mergeFederatedHits(
      [mk('a', 1.0, 'local'), mk('a', 9.0, 'peer'), mk('b', 5.0, 'peer'), mk('', 99.0, 'peer')],
      10,
    )
    expect(out.map((h) => h.fileId)).toEqual(['a', 'b'])
    expect(out[0]).toMatchObject({ score: 9.0, source: 'peer' })
  })

  it('prefers the richer snippet on score ties', () => {
    const out = mergeFederatedHits([mk('a', 2.0, 'x', 'long snippet here'), mk('a', 2.0, 'y')], 10)
    expect(out[0].source).toBe('x')
  })
})

describe('capability matrix + peer cache', () => {
  it('covers all six targets and names the mDNS gaps honestly', () => {
    const matrix = peerCapabilityMatrix()
    expect(matrix.map((m) => m.target).sort()).toEqual(
      ['android', 'docker', 'ios', 'linux', 'wasm', 'windows'].sort(),
    )
    for (const row of matrix) {
      expect(row.httpSync).toBe(true)
      expect(row.federatedSearch).toBe(true)
      expect(row.note.length).toBeGreaterThan(0)
    }
    expect(matrix.find((m) => m.target === 'android')!.discovery).toBe(false)
    expect(matrix.find((m) => m.target === 'wasm')!.discovery).toBe(false)
    expect(matrix.find((m) => m.target === 'linux')!.discovery).toBe(true)
  })

  it('caches manual peers without throwing (node has no localStorage)', () => {
    expect(loadCachedPeers()).toEqual([])
    expect(() =>
      saveCachedPeers([{ deviceId: 'phone-7', baseUrl: 'http://192.168.1.60:3456', transport: 'lan-http' }]),
    ).not.toThrow()
  })
})

describe('peer fetch fan-out', () => {
  function stubFetch(handler: (url: string, init?: RequestInit) => unknown): void {
    ;(globalThis as { fetch?: unknown }).fetch = (async (url: string, init?: RequestInit) => {
      const body = handler(url, init)
      return {
        ok: true,
        status: 200,
        json: async () => body,
      }
    }) as typeof fetch
  }

  it('tags peer hits with the base url and merges with local hits', async () => {
    stubFetch(() => [{ fileId: 'p1', fileName: 'p1.txt', score: 7.0, snippet: 'peer hit' }])
    const hits = await federatedSearch('photo', {
      peers: [{ baseUrl: 'http://192.168.1.60:3456', deviceId: 'phone-7', token: 'peer-token' }],
      limit: 10,
      localHits: [{ fileId: 'l1', fileName: 'l1.txt', score: 3.0, snippet: '', source: 'local' }],
    })
    expect(hits.map((h) => h.fileId)).toEqual(['p1', 'l1'])
    // deviceId retags the peer source for display.
    expect(hits[0].source).toBe('phone-7')
    expect(hits[1].source).toBe('local')
  })

  it('degrades a dead peer to zero hits, never a failed search', async () => {
    ;(globalThis as { fetch?: unknown }).fetch = (async () => {
      throw new Error('connection refused')
    }) as typeof fetch
    const hits = await federatedSearch('photo', {
      peers: [{ baseUrl: 'http://192.168.1.60:3456', deviceId: 'phone-7', token: 'peer-token' }],
      localHits: [{ fileId: 'l1', fileName: 'l1.txt', score: 3.0, snippet: '', source: 'local' }],
    })
    expect(hits.map((h) => h.fileId)).toEqual(['l1'])
  })

  it('sends only an explicitly assigned peer token and ignores a legacy shared token', async () => {
    const headersSeen: Array<Record<string, string> | undefined> = []
    stubFetch((_url, init) => {
      headersSeen.push(init?.headers as Record<string, string> | undefined)
      return []
    })
    const legacyOptions = {
      peers: [
        { baseUrl: 'https://peer-a.example', token: 'peer-a-secret' },
        { baseUrl: 'https://peer-b.example', token: 'peer-b-secret' },
        { baseUrl: 'https://peer-c.example' },
      ],
      token: 'caller-secret',
    } as unknown as Parameters<typeof federatedSearch>[1]

    await federatedSearch('photo', legacyOptions)

    expect(headersSeen[0]?.Authorization).toBe('Bearer peer-a-secret')
    expect(headersSeen[1]?.Authorization).toBe('Bearer peer-b-secret')
    expect(headersSeen).toHaveLength(2)
    expect(JSON.stringify(headersSeen)).not.toContain('caller-secret')
  })

  it('does not issue a request to peers without their own token', async () => {
    let calls = 0
    stubFetch(() => {
      calls += 1
      return []
    })
    await federatedSearch('photo', {
      peers: [{ baseUrl: 'https://peer.example', deviceId: 'peer-1' }],
    })
    expect(calls).toBe(0)
  })

  it('answers auth: on 401/403 and network: otherwise', async () => {
    ;(globalThis as { fetch?: unknown }).fetch = (async () => ({
      ok: false,
      status: 401,
      json: async () => [],
    })) as typeof fetch
    await expect(fetchPeerHits('http://10.0.0.9:3456', 'photo', 20, 8000, 'peer-token', 'peer-1')).rejects.toThrow(/^auth:/)
    expect(await fetchPeerHits('http://10.0.0.9:3456', '   ')).toEqual([])
  })
})

describe('executable op-log exchange', () => {
  const deps = (seen: { pushed: unknown[][]; merged: unknown[][] }) => ({
    localDeviceId: 'laptop-1',
    peerToken: 'token-for-phone-7',
    signer: async (deviceId: string) => ({
      signature: `sig-${deviceId}`,
      algorithm: 'ml-dsa-65',
    }),
    fetchJson: async (method: string, url: string, body?: unknown) => {
      if (url.includes('/api/oplog/since')) return { head: 2, ops: [{ op_id: 'op-1' }] }
      seen.pushed.push([method, url, body])
      return { appended: 1 }
    },
    pushOps: async (ops: unknown[]) => {
      seen.merged.push(ops)
      return { appended: ops.length }
    },
  })

  it('pulls, pushes, merges and reports steps', async () => {
    const seen = { pushed: [], merged: [] } as { pushed: unknown[][]; merged: unknown[][] }
    const out = await syncExchangeWithPeer(
      { deviceId: 'phone-7', baseUrl: 'http://192.168.1.60:3456', transport: 'lan-http' },
      0,
      [{ op_id: 'ours', device_id: 'laptop-1' }],
      deps(seen),
    )
    expect(out.pulled).toBe(1)
    expect(out.pushed).toBe(1)
    expect(out.steps.join('\n')).toContain('/api/oplog/since?since=0')
    expect(seen.merged[0]).toHaveLength(1)
  })

  it('refuses relay endpoints with unsupported: (QUIC sequenced)', async () => {
    const seen = { pushed: [], merged: [] } as { pushed: unknown[][]; merged: unknown[][] }
    await expect(
      syncExchangeWithPeer(
        { deviceId: 'phone-7', baseUrl: 'http://relay.example', transport: 'relay-future' },
        0,
        [],
        deps(seen),
      ),
    ).rejects.toThrow(/^unsupported:/)
  })

  it('wan + lan wrappers reuse the same exchange', async () => {
    const { wanSyncExchange } = await import('@/utils/wanDial')
    const { lanSyncExchange } = await import('@/utils/lanSync')
    const seen = { pushed: [], merged: [] } as { pushed: unknown[][]; merged: unknown[][] }
    const wan = await wanSyncExchange(
      { deviceId: 'vault', addr: 'vault.example.com:3456' },
      3,
      [],
      deps(seen),
    )
    expect(wan.steps.join('\n')).toContain('since=3')
    const lan = await lanSyncExchange(
      { deviceId: 'phone-7', name: 'Phone', host: '192.168.1.60', port: 3456 },
      0,
      [],
      deps(seen),
    )
    expect(lan.pulled).toBe(1)
  })
})

describe('search_files_federated transport route', () => {
  it('maps to POST /api/search/federated with peer normalization', async () => {
    const { REST_ROUTES } = await import('@/composables/useTauri')
    const r = REST_ROUTES.search_files_federated
    expect(r.method).toBe('POST')
    expect(r.buildPath({})).toBe('/api/search/federated')
    const body = r.transformRequest!({
      query: 'photo',
      peers: [{ deviceId: 'phone-7', baseUrl: 'http://192.168.1.60:3456' }],
    }) as Record<string, unknown>
    expect(body.query).toBe('photo')
    expect(body.peers).toEqual([{ baseUrl: 'http://192.168.1.60:3456', deviceId: 'phone-7' }])
    const shaped = r.transformResponse!([
      { fileId: 'a', fileName: 'a.txt', score: 9, snippet: 'hit', source: 'phone-7' },
    ]) as FederatedHit[]
    expect(shaped[0]).toMatchObject({ fileId: 'a', source: 'phone-7' })
  })
})
