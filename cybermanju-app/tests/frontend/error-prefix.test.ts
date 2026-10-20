// CyberManju OS — Error-prefix.test (tests/frontend/error-prefix.test.ts).
// ANALYSIS.md §3 + TASKS.md P3-5: the AGENT-1 contract parity guard
// (frontend half) — every documented prefix maps to a UI hint, never raw,
// and every REST_FIRST command resolves to a shaped REST_ROUTES entry.
import { describe, expect, it } from 'vitest'
import { REST_FIRST, REST_ROUTES } from '@/composables/useTauri'
import { agentErrorHint, describeSyncError } from '@/types'

const PREFIXES = [
  'auth:',
  'rate_limited:',
  'not_found:',
  'unsupported:',
  'too_large:',
  'integrity:',
  'network:',
  'disk_full:',
  'conflict:',
]

describe('AGENT-1 prefix → hint matrix', () => {
  for (const prefix of PREFIXES) {
    it(`maps ${prefix} to a hint, never raw`, () => {
      for (const describe of [describeSyncError, agentErrorHint]) {
        const { hint } = describe(`${prefix} something broke`)
        expect(typeof hint).toBe('string')
        expect(hint.length).toBeGreaterThan(0)
        expect(hint).not.toContain('undefined')
      }
    })
  }

  it('handles unknown errors with a default hint', () => {
    expect(describeSyncError('weird boom').hint.length).toBeGreaterThan(0)
    expect(agentErrorHint('weird boom').hint.length).toBeGreaterThan(0)
  })
})

describe('REST route shape invariants', () => {
  it('gives every mapping a method + path builder', () => {
    const methods = new Set(['GET', 'POST', 'PUT', 'DELETE', 'PATCH'])
    for (const [cmd, mapping] of Object.entries(REST_ROUTES)) {
      expect(methods.has(mapping.method), `${cmd} method`).toBe(true)
      expect(typeof mapping.buildPath, `${cmd} buildPath`).toBe('function')
      if (mapping.transformRequest !== undefined) {
        expect(typeof mapping.transformRequest, `${cmd} transformRequest`).toBe('function')
      }
    }
  })

  it('resolves every REST_FIRST command to a route entry', () => {
    for (const cmd of REST_FIRST) {
      expect(REST_ROUTES[cmd], `REST_FIRST ${cmd}`).toBeTruthy()
    }
  })
})
