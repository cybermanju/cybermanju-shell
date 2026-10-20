// CyberManju OS — p2p-bonds.test (tests/frontend/p2p-bonds.test.ts).
// Role: pin the bond-expanding layer — human pairing codes (same grammar +
// wordlist as Rust `peers::`), erasure-coded swarm framing (layout math +
// symbol validation + backend-injected round trip), relay rooms (topic +
// hello/say/read splits), and the `pair`/`swarm`/`room` cybsh verbs.
// Exports: none (vitest suite).
// Integrates: src/utils/pairing.ts; src/utils/p2pSwarm.ts;
// src/utils/p2pRoom.ts; staticCybsh verbs; Rust twins are
// crates/sync/src/peers.rs (pairing) + swarm.rs (RS math) + os-wasm/src/
// swarm.rs (browser math). Cross-language pins use static Rust reads
// (node-env safe — full behavior stays a CI job with both toolchains).
// Contracts: pure builders throw only `invalid:`/`too_large:`/`unsupported:`
// (AGENT-1); RS/RaptorQ math without a backend answers `unsupported:` naming
// the exact wiring; nothing hangs.
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'
import {
  claimPairingEnvelopes,
  dropPairingEnvelope,
  isPairingTarget,
  makePairingCode,
  PAIRING_WORDS,
  pairingTarget,
  parsePairingCode,
} from '@/utils/pairing'
import {
  buildSwarmPlan,
  decodeSwarm,
  encodeSwarm,
  parseSwarmSymbols,
  swarmBackendFromWasm,
  SWARM_KIND,
  swarmLayout,
  validateSwarmSymbol,
} from '@/utils/p2pSwarm'
import { postRoomMessage, readRoomMessages, roomHello, roomTopic } from '@/utils/p2pRoom'
import {
  buildIrohTicket,
  parseIrohTicket,
  ticketRelay,
  wanDialPlan,
  parseWanEndpoint,
  relayDialSteps,
} from '@/utils/wanDial'
import {
  buildFountainPlan,
  decodeFountain,
  encodeFountain,
  FOUNTAIN_KIND,
  parseFountainSymbols,
  validateFountainSymbol,
} from '@/utils/p2pSwarm'

function rustWordlist(): string[] {
  const src = fs.readFileSync('crates/sync/src/peers.rs', 'utf8')
  const block = src.slice(src.indexOf('PAIRING_WORDS'))
  const quoted = [...block.slice(0, block.indexOf('];')).matchAll(/"([a-z]+)"/g)].map((m) => m[1])
  return quoted
}

describe('pairing codes (mirror peers::)', () => {
  it('shares the exact wordlist with Rust (static read, no cargo)', () => {
    expect(PAIRING_WORDS).toHaveLength(64)
    expect([...PAIRING_WORDS].sort()).toEqual([...rustWordlist()].sort())
  })

  it('mints shaped, unique codes and normalizes input', () => {
    const seen = new Set<string>()
    for (let i = 0; i < 50; i++) {
      const code = makePairingCode()
      expect(code).toMatch(/^\d{3}-[a-z]+-[a-z]+$/)
      seen.add(code)
    }
    expect(seen.size).toBe(50)
    expect(parsePairingCode('482-OAK-River')).toBe('482-oak-river')
    expect(pairingTarget('482-oak-river')).toBe('pair:482-oak-river')
    expect(isPairingTarget('pair:482-oak-river')).toBe(true)
    expect(isPairingTarget('phone-7')).toBe(false)
    expect(isPairingTarget('pair:nope')).toBe(false)
    for (const bad of ['', '48-oak-river', '482-oak-nope', 'no-dashes-here!']) {
      expect(() => parsePairingCode(bad)).toThrow(/^invalid:/)
    }
  })

  it('drops + claims through the relay mailbox (injected fetch)', async () => {
    const calls: unknown[][] = []
    const deps = {
      fetchJson: async (method: string, url: string, body?: unknown) => {
        calls.push([method, url, body])
        if (url.endsWith('/api/relay/drop')) return { accepted: true, id: 'p1' }
        return { messages: [{ id: 'p1', from: 'laptop', to: 'pair:482-oak-river', kind: 'op-envelope', payload: {}, createdAtSecs: 1 }] }
      },
    }
    const dropped = await dropPairingEnvelope('https://relay.example.com', 'laptop', '482-oak-river', 'op-envelope', { ops: [] }, deps)
    expect(dropped).toMatchObject({ accepted: true, id: 'p1' })
    expect(JSON.stringify(calls[0])).toContain('pair:482-oak-river')
    const claimed = await claimPairingEnvelopes('https://relay.example.com', '482-oak-river', deps)
    expect(claimed).toHaveLength(1)
    // Claim acks what it read (non-destructive poll + ack = no replay).
    expect(calls.some(([, url]) => String(url).endsWith('/api/relay/ack'))).toBe(true)
  })
})

describe('swarm framing (mirror sync::swarm)', () => {
  it('computes the same layout heuristic as Rust', () => {
    expect(swarmLayout(0)).toEqual({ k: 1, m: 1 })
    expect(swarmLayout(100)).toEqual({ k: 1, m: 1 })
    const { k, m } = swarmLayout(1024 * 1024)
    expect(k).toBeGreaterThanOrEqual(16)
    expect(k).toBeLessThanOrEqual(32)
    expect(m).toBeGreaterThanOrEqual(1)
    // Rust unit test pins (1,1) for tiny payloads on both seams.
    const src = fs.readFileSync('crates/sync/src/swarm.rs', 'utf8')
    expect(src).toContain('SWARM_TARGET_SHARD')
  })

  it('plans offline and caps objects like the server', () => {
    const steps = buildSwarmPlan(1000)
    expect(steps.join('\n')).toContain(`kind: ${SWARM_KIND}`)
    expect(steps.join('\n')).toContain('/api/relay/drop')
    expect(() => buildSwarmPlan(50 * 1024 * 1024 + 1)).toThrow(/^too_large:/)
    expect(() => buildSwarmPlan(100, 0, 2)).toThrow(/^invalid:/)
  })

  it('validates symbol envelopes strictly', () => {
    const good = {
      object: 'a'.repeat(64),
      dataShards: 4,
      parityShards: 2,
      index: 3,
      shardLen: 16,
      blake3: 'b'.repeat(64),
      bytesBase64: 'eA==',
    }
    expect(validateSwarmSymbol(good).index).toBe(3)
    expect(parseSwarmSymbols(JSON.stringify([good]))).toHaveLength(1)
    expect(() => validateSwarmSymbol({ ...good, object: 'zz' })).toThrow(/^invalid:/)
    expect(() => validateSwarmSymbol({ ...good, index: 9 })).toThrow(/^invalid:/)
    expect(() => parseSwarmSymbols('not json')).toThrow(/^invalid:/)
    expect(() => parseSwarmSymbols({})).toThrow(/^invalid:/)
  })

  it('round-trips through an injected backend (any k suffice)', async () => {
    // Fake backend: k data echoes + parity ignored (framing contract only).
    const backend = {
      encode: async (payload: Uint8Array, k: number, m: number) => {
        const out = []
        for (let i = 0; i < k + m; i++) {
          out.push({
            object: 'a'.repeat(64),
            dataShards: k,
            parityShards: m,
            index: i,
            shardLen: payload.length,
            blake3: 'b'.repeat(64),
            bytesBase64: Buffer.from(payload).toString('base64'),
          })
        }
        return out
      },
      decode: async (symbols: { bytesBase64: string }[]) => {
        if (symbols.length === 0) throw new Error('integrity: empty')
        return Uint8Array.from(Buffer.from(symbols[0].bytesBase64, 'base64'))
      },
    }
    const payload = new TextEncoder().encode('swarm across lossy paths')
    const symbols = await encodeSwarm(payload, backend, 4, 2)
    expect(symbols).toHaveLength(6)
    const back = await decodeSwarm(symbols.slice(0, 4), backend)
    expect(new TextDecoder().decode(back)).toBe('swarm across lossy paths')
    await expect(encodeSwarm(payload, null)).rejects.toThrow(/^unsupported:/)
    await expect(decodeSwarm([], backend)).rejects.toThrow()
  })

  it('wraps wasm-loader failures honestly', async () => {
    const backend = swarmBackendFromWasm(async () => {
      throw new Error('pkg missing')
    })
    await expect(backend.encode(new Uint8Array([1]), 1, 1)).rejects.toThrow(/^unsupported:/)
    await expect(backend.decode([])).rejects.toThrow(/^unsupported:/)
  })
})

describe('relay rooms', () => {
  it('builds topics strictly', () => {
    expect(roomTopic('com.example.chat', 'lobby')).toBe('room:com.example.chat/lobby')
    for (const [app, room] of [['', 'x'], ['a', ''], ['has space', 'x'], ['a', 'has@at']]) {
      expect(() => roomTopic(app, room)).toThrow(/^invalid:/)
    }
  })

  it('hellos, says and reads through injected fetch', async () => {
    const calls: unknown[][] = []
    const deps = {
      fetchJson: async (method: string, url: string, body?: unknown) => {
        calls.push([method, url, body])
        if (url.endsWith('/api/relay/drop')) return { accepted: true, id: 'r1' }
        return {
          messages: [
            { id: 'h', from: 'phone', to: 't', kind: 'room-hello', payload: {}, createdAtSecs: 1 },
            { id: 'a', from: 'phone', to: 't', kind: 'room-action', payload: { action: 'draw', body: { x: 1 }, atSecs: 2 }, createdAtSecs: 2 },
            { id: 'm', from: 'me', to: 't', kind: 'room-action', payload: { action: 'echo', body: null, atSecs: 3 }, createdAtSecs: 3 },
          ],
        }
      },
    }
    await roomHello('https://r.example', 'com.example.board', 'lobby', 'me', 'Ada', deps)
    await postRoomMessage('https://r.example', 'com.example.board', 'lobby', 'me', 'draw', { x: 1 }, deps)
    const { hellos, actions } = await readRoomMessages('https://r.example', 'com.example.board', 'lobby', 'me', deps)
    expect(hellos.map((h) => h.from)).toEqual(['phone'])
    // Own messages are skipped (no echo).
    expect(actions.map((a) => a.action)).toEqual(['draw'])
    expect(actions[0]).toMatchObject({ from: 'phone', atSecs: 2 })
    await expect(postRoomMessage('https://r.example', 'a', 'b', 'me', '  ', null, deps)).rejects.toThrow(/^invalid:/)
  })
})

describe('pair/swarm/room cybsh verbs', () => {
  it('serves the new verbs offline-first', async () => {
    const { handlesStaticVerb, runStaticCybshLine } = await import('@/utils/staticCybsh')
    for (const verb of ['pair', 'swarm', 'room']) expect(handlesStaticVerb(verb)).toBe(true)
    const deps = { execFallback: undefined } as never
    const made = await runStaticCybshLine('pair make', deps)
    expect(JSON.stringify(made)).toMatch(/code: \d{3}-[a-z]+-[a-z]+/)
    const dropped = await runStaticCybshLine('pair drop 482-oak-river op-envelope', deps)
    expect(JSON.stringify(dropped)).toContain('/api/relay/drop')
    expect(JSON.stringify(await runStaticCybshLine('pair drop', deps))).toContain('usage: pair drop')
    expect(JSON.stringify(await runStaticCybshLine('pair claim 482-oak-river', deps))).toContain('needs the dashboard')
    const plan = await runStaticCybshLine('swarm plan 1000', deps)
    expect(JSON.stringify(plan)).toContain('swarm-symbol')
    expect(JSON.stringify(await runStaticCybshLine('swarm plan', deps))).toContain('usage: swarm plan')
    const join = await runStaticCybshLine('room join com.example.chat lobby', deps)
    expect(JSON.stringify(join)).toContain('room:com.example.chat/lobby')
    const say = await runStaticCybshLine('room say com.example.chat lobby draw', deps)
    expect(JSON.stringify(say)).toContain('/api/relay/drop')
    const fountain = await runStaticCybshLine('swarm fountain 100000', deps)
    expect(JSON.stringify(fountain)).toContain(FOUNTAIN_KIND)
    expect(JSON.stringify(await runStaticCybshLine('swarm fountain', deps))).toContain('usage: swarm fountain')
  })
})

describe('iroh tickets (self-contained rendezvous)', () => {
  const ID = 'a'.repeat(64)

  it('builds + parses canonical tickets (shape-checked; curve checked natively)', () => {
    expect(buildIrohTicket(ID, 'https://relay.example.com/')).toBe(`iroh:${ID}@https://relay.example.com`)
    expect(parseIrohTicket(`iroh:${ID}@https://relay.example.com/`)).toEqual({
      endpointId: ID,
      relayUrl: 'https://relay.example.com',
    })
    expect(ticketRelay(`iroh:${ID}@https://relay.example.com`)).toBe('https://relay.example.com')
    expect(ticketRelay('relay.example.com:3456')).toBe('')
    for (const bad of ['https://relay.example.com', 'iroh:no-at-sign', `iroh:short@https://r.example`, 'iroh:@https://r.example']) {
      expect(() => parseIrohTicket(bad)).toThrow(/^invalid:/)
    }
    expect(() => buildIrohTicket('short', 'https://r.example')).toThrow(/^invalid:/)
    // Rust twin pins the same grammar (static read, no cargo).
    const src = fs.readFileSync('crates/sync/src/iroh.rs', 'utf8')
    expect(src).toContain('parse_iroh_ticket')
    expect(src).toContain('ticket_or_config_relay')
  })

  it('dials ticketed endpoints with no configured relay_url', () => {
    const ep = parseWanEndpoint(`phone-7@iroh:${ID}@https://relay.example.com`)
    const plan = wanDialPlan(ep, 15, '')
    expect(plan.startsWith('relay:')).toBe(true)
    expect(plan).toContain('https://relay.example.com/api/relay/drop')
    const steps = relayDialSteps(ep, '', 3)
    expect(steps[0]).toContain('https://relay.example.com/api/relay/drop')
  })
})

describe('fountain envelopes (mirror sync::swarm fountain_*)', () => {
  it('validates fountain symbol envelopes strictly', () => {
    const good = {
      object: 'a'.repeat(64),
      otiBase64: 'AAAAAAAAAAAAAAAA',
      mtu: 32768,
      blake3: 'b'.repeat(64),
      bytesBase64: 'eA==',
    }
    expect(validateFountainSymbol(good).mtu).toBe(32768)
    expect(parseFountainSymbols(JSON.stringify([good]))).toHaveLength(1)
    expect(() => validateFountainSymbol({ ...good, object: 'zz' })).toThrow(/^invalid:/)
    expect(() => validateFountainSymbol({ ...good, otiBase64: '  ' })).toThrow(/^invalid:/)
    expect(() => validateFountainSymbol({ ...good, mtu: 0 })).toThrow(/^invalid:/)
    expect(() => parseFountainSymbols('not json')).toThrow(/^invalid:/)
  })

  it('plans fountain sprays offline and caps objects like the server', () => {
    const steps = buildFountainPlan(1_000_000)
    expect(steps.join('\n')).toContain(`kind: ${FOUNTAIN_KIND}`)
    expect(steps.join('\n')).toContain('/api/relay/drop')
    expect(() => buildFountainPlan(0)).toThrow(/^invalid:/)
    expect(() => buildFountainPlan(50 * 1024 * 1024 + 1)).toThrow(/^too_large:/)
    expect(() => buildFountainPlan(100, 0)).toThrow(/^invalid:/)
  })

  it('round-trips through an injected backend; refuses without one', async () => {
    const backend = {
      encode: async (payload: Uint8Array) => [
        { object: 'a'.repeat(64), otiBase64: 'AAAAAAAAAAAAAAAA', mtu: 1024, blake3: 'b'.repeat(64), bytesBase64: Buffer.from(payload).toString('base64') },
      ],
      decode: async (symbols: { bytesBase64: string }[]) => {
        if (symbols.length === 0) throw new Error('integrity: empty')
        return Uint8Array.from(Buffer.from(symbols[0].bytesBase64, 'base64'))
      },
    }
    const payload = new TextEncoder().encode('fountain spray, any subset decodes')
    const symbols = await encodeFountain(payload, backend, 1024, 4)
    expect(symbols).toHaveLength(1)
    expect(new TextDecoder().decode(await decodeFountain(symbols, backend))).toBe(
      'fountain spray, any subset decodes',
    )
    await expect(encodeFountain(payload, null)).rejects.toThrow(/^unsupported:/)
    await expect(decodeFountain([], backend)).rejects.toThrow()
  })
})
