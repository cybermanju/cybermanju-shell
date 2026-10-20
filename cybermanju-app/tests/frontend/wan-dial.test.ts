// CyberManju OS — wan-dial.test (tests/frontend/wan-dial.test.ts).
// Role: pin the WAN dial seam (§2B.5) — same grammar + live dial plans as
// crates/sync/src/iroh.rs (direct HTTP live, relay live with a relay_url,
// honest `unsupported:` + way out only with no relay set), the REAL HTTP
// dial steps (readyz probe + op-log pull/push) and relay steps, and the
// `wan plan|dial` cybsh verbs.
// Exports: none (vitest suite).
// Integrates: src/utils/wanDial.ts; src/utils/staticCybsh.ts (`wan` verb).
// Contracts: pure builders + node-env safe probe; relay-without-relay
// answers `unsupported:` with the HTTP/LAN/bundle way out; bad endpoints
// are `invalid:`; the probe normalizes failures to `network:` and never
// hangs (AbortController).
import { describe, expect, it } from 'vitest'
import { parseWanEndpoint, relayDialSteps, wanDialPlan, wanDialSteps, wanHttpBase, wanHttpProbe, wanWayOut } from '@/utils/wanDial'

describe('wan endpoint grammar (mirrors iroh.rs)', () => {
  it('parses <device>@<addr>', () => {
    expect(parseWanEndpoint('phone-7@192.168.1.60:3456')).toEqual({
      deviceId: 'phone-7',
      addr: '192.168.1.60:3456',
    })
  })

  it('refuses missing halves and whitespace with invalid:', () => {
    for (const bad of ['no-at-sign', '@addr', 'id@', 'my id@addr', 'id@my addr']) {
      expect(() => parseWanEndpoint(bad)).toThrow(/^invalid:/)
    }
  })
})

describe('wan dial plan (direct + relay live)', () => {
  it('returns a live HTTP plan for direct addrs', () => {
    const ep = parseWanEndpoint('phone-7@192.168.1.60:3456')
    const plan = wanDialPlan(ep)
    expect(plan.startsWith('http:')).toBe(true)
    expect(plan).toContain('/api/readyz')
  })

  it('answers unsupported: for relay tickets with no relay set (way out included)', () => {
    const ep = parseWanEndpoint('phone-7@relay:443')
    let plan = ''
    try {
      wanDialPlan(ep)
    } catch (e) {
      plan = e instanceof Error ? e.message : String(e)
    }
    expect(plan.startsWith('unsupported:')).toBe(true)
    expect(plan).toContain('relay_url')
    const wayOut = wanWayOut(ep)
    expect(wayOut.join('\n')).toContain('/api/peers/phone-7/sync')
    expect(wayOut.join('\n')).toContain('/api/sync/bundle/export')
  })

  it('returns a live relay plan when a relay_url is set', () => {
    const ep = parseWanEndpoint('phone-7@relay:443')
    const plan = wanDialPlan(ep, 15, 'https://relay.example.com')
    expect(plan.startsWith('relay:')).toBe(true)
    expect(plan).toContain('/api/relay/drop')
    const steps = relayDialSteps(ep, 'https://relay.example.com', 41)
    expect(steps).toHaveLength(5)
    expect(steps[0]).toContain('/api/relay/drop')
    expect(steps[1]).toContain('/api/relay/poll')
    expect(steps[4]).toContain('/api/oplog/conflicts')
    expect(() => relayDialSteps(ep, '', 0)).toThrow(/^unsupported:/)
  })

  it('validates the dial timeout first (invalid:)', () => {
    const ep = parseWanEndpoint('phone-7@relay:443')
    expect(() => wanDialPlan(ep, 0)).toThrow(/^invalid:/)
    expect(() => wanDialPlan(ep, 121)).toThrow(/^invalid:/)
  })

  it('builds real HTTP dial steps (probe + op-log pull/push)', () => {
    const ep = parseWanEndpoint('phone-7@192.168.1.60:3456')
    expect(wanHttpBase(ep)).toBe('http://192.168.1.60:3456')
    expect(wanHttpBase(parseWanEndpoint('phone-7@https://vault.example.com'))).toBe(
      'https://vault.example.com',
    )
    const steps = wanDialSteps(ep, 41)
    expect(steps).toHaveLength(5)
    expect(steps[0]).toContain('/api/readyz')
    expect(steps[1]).toContain('/api/oplog/since?since=41')
    expect(steps[2]).toContain('/api/oplog/push')
    expect(steps[2]).toContain('signature, algorithm')
    expect(steps[4]).toContain('/api/oplog/conflicts')
  })

  it('probes honestly: closed port answers network:, never hangs', async () => {
    await expect(wanHttpProbe('http://127.0.0.1:9', 500)).rejects.toThrow(/^network:/)
  })
})

describe('wan cybsh verb', () => {
  it('is a handled static verb and plans offline (live direct, honest relay)', async () => {
    const { handlesStaticVerb, runStaticCybshLine } = await import('@/utils/staticCybsh')
    expect(handlesStaticVerb('wan')).toBe(true)
    expect(handlesStaticVerb('pex')).toBe(true)
    expect(handlesStaticVerb('relay')).toBe(true)
    const deps = { execFallback: undefined } as never
    const out = await runStaticCybshLine('wan plan phone-7@192.168.1.60:3456', deps)
    const text = JSON.stringify(out)
    expect(text).toContain('http:')
    expect(text).toContain('/api/readyz')
    const relay = await runStaticCybshLine('wan plan phone-7@relay:443', deps)
    const relayText = JSON.stringify(relay)
    expect(relayText).toContain('unsupported:')
    expect(relayText).toContain('/api/peers/phone-7/sync')
    const withRelay = await runStaticCybshLine('wan plan phone-7@relay:443 --relay https://relay.example.com', deps)
    expect(JSON.stringify(withRelay)).toContain('/api/relay/drop')
    const usage = await runStaticCybshLine('wan plan', deps)
    expect(JSON.stringify(usage)).toContain('usage: wan plan')
  })

  it('dials real HTTP steps offline (no socket, no guess)', async () => {
    const { runStaticCybshLine } = await import('@/utils/staticCybsh')
    const deps = { execFallback: undefined } as never
    const out = await runStaticCybshLine('wan dial phone-7@192.168.1.60:3456 41', deps)
    const text = JSON.stringify(out)
    expect(text).toContain('/api/readyz')
    expect(text).toContain('/api/oplog/since?since=41')
    expect(text).toContain('/api/oplog/push')
    expect(text).toContain('signature, algorithm')
    const usage = await runStaticCybshLine('wan dial', deps)
    expect(JSON.stringify(usage)).toContain('usage: wan dial')
  })
})
