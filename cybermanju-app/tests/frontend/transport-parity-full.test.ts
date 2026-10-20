// CyberManju OS — transport-parity-full.test (tests/frontend/transport-parity-full.test.ts).
// Role: full TS↔Rust static parity sweep (ANALYSIS.md §3 close-out) using REAL
// TS imports (not regex): every REST_ROUTES mapping has method + /api/ path,
// every path's second segment is a known Rust ROUTED_SEGMENT (else the router
// 404s before auth — this caught the missing `faces` entry), REST_FIRST
// resolves, WRITE_ONLY stays disjoint, spine twins exist in crates/web +
// src-tauri, and the AGENT-1 prefix matrix is pinned.
// Exports: none (vitest suite).
// Integrates: src/composables/useTauri.ts (real imports);
// crates/web/src/security.rs + lib.rs (fs reads, no cargo); CLI twin is
// scripts/check-transport-parity.mjs (node-only, CI meta-check).
// Contracts: node-env safe; static presence/shape only — live-boot shape
// equality (real crates/web + snake→camel) stays a CI follow-up needing both
// toolchains in one job.
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'
import { REST_FIRST, REST_ROUTES, WRITE_ONLY_COMMANDS } from '@/composables/useTauri'

function secondSegment(path: string): string {
  return path.split('/')[2] ?? ''
}

function rustSegments(): Set<string> {
  const security = fs.readFileSync('crates/web/src/security.rs', 'utf8')
  return new Set([...security.matchAll(/"([a-z0-9-]+)",/g)].map((m) => m[1]))
}

describe('full transport parity (both directions)', () => {
  it('maps every command to a method + /api/ path (>= 170)', () => {
    const keys = Object.keys(REST_ROUTES)
    expect(keys.length).toBeGreaterThanOrEqual(170)
    for (const key of keys) {
      const r = REST_ROUTES[key]
      expect(typeof r.method, key).toBe('string')
      expect(['GET', 'POST', 'PUT', 'DELETE', 'PATCH'].includes(r.method), key).toBe(true)
      const path = r.buildPath({})
      expect(typeof path, key).toBe('string')
      expect(path.startsWith('/api/'), `${key} → ${path}`).toBe(true)
    }
  })

  it('serves every REST second segment in Rust (no 404-before-auth drift)', () => {
    const segments = rustSegments()
    const seconds = new Set(
      Object.values(REST_ROUTES).map((r) => {
        const raw = r.buildPath({}).split('?')[0].split('$')[0]
        return secondSegment(raw.replace(/\/+$/, ''))
      }),
    )
    for (const seg of seconds) {
      expect(segments.has(seg), `ROUTED_SEGMENTS lacks '${seg}'`).toBe(true)
    }
    for (const spine of ['devices', 'todos', 'oplog', 'peers', 'sync', 'faces']) {
      expect(segments.has(spine), `spine '${spine}'`).toBe(true)
    }
  })

  it('resolves REST_FIRST and keeps WRITE_ONLY disjoint', () => {
    for (const cmd of REST_FIRST) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
    }
    for (const cmd of WRITE_ONLY_COMMANDS) {
      expect(REST_ROUTES[cmd], cmd).toBeUndefined()
      expect(REST_FIRST.has(cmd), cmd).toBe(false)
    }
  })

  it('pins the strategic spine on both sides (devices/peers/oplog/bundle/todos)', () => {
    const lib = fs.readFileSync('crates/web/src/lib.rs', 'utf8')
    for (const needle of ['["api", "peers"]', 'bundle', 'oplog', 'devices::route', 'todos::route']) {
      expect(lib.includes(needle), needle).toBe(true)
    }
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    for (const cmd of ['register_this_device', 'list_lan_peers', 'list_todos']) {
      expect(runtime.includes(cmd), cmd).toBe(true)
    }
  })

  it('pins the AGENT-1 prefix matrix', () => {
    const prefixTest = fs.readFileSync('tests/frontend/error-prefix.test.ts', 'utf8')
    for (const prefix of [
      'auth:',
      'rate_limited:',
      'not_found:',
      'unsupported:',
      'too_large:',
      'integrity:',
      'network:',
      'disk_full:',
      'conflict:',
    ]) {
      expect(prefixTest.includes(prefix), prefix).toBe(true)
    }
  })
})
