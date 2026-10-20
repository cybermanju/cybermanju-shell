// CyberManju OS — transport-exhaustive.test (tests/frontend/transport-exhaustive.test.ts).
// Role: close the ANALYSIS.md §3 parity follow-up on the frontend half —
// exhaustively pin every REST_ROUTES mapping (method + /api/ path), every
// REST_FIRST entry resolving, WRITE_ONLY disjointness, and the Rust twins for
// the strategic spine arms (devices/peers/oplog/bundle/todos) via static reads.
// Exports: none (vitest suite).
// Integrates: src/composables/useTauri.ts; crates/web + src-tauri (fs reads,
// no cargo — full TS↔Rust equality stays a CI job with both toolchains).
// Contracts: node-env safe; static Rust reads assert presence, never behavior;
// new mappings must extend (never shrink) the pinned spine set.
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'
import { REST_FIRST, REST_ROUTES, WRITE_ONLY_COMMANDS } from '@/composables/useTauri'

const SPINE_ROUTES = [
  'list_devices',
  'register_device',
  'register_this_device',
  'revoke_device',
  'remove_device',
  'list_lan_peers',
  'peer_sync',
  'oplog_since',
  'oplog_push',
  'oplog_conflicts',
  'oplog_resolve',
  'bundle_export',
  'bundle_import',
  'list_todos',
  'create_todo',
  'toggle_todo',
  'delete_todo',
] as const

describe('exhaustive REST mapping invariants', () => {
  it('maps every command to a method + /api/ path', () => {
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

  it('resolves every REST_FIRST entry (no split-brain phantom)', () => {
    for (const cmd of REST_FIRST) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
    }
  })

  it('keeps WRITE_ONLY disjoint from REST (desktop-only is explicit)', () => {
    for (const cmd of WRITE_ONLY_COMMANDS) {
      expect(REST_ROUTES[cmd], cmd).toBeUndefined()
      expect(REST_FIRST.has(cmd), cmd).toBe(false)
    }
  })

  it('pins the strategic spine routes (devices/peers/oplog/bundle/todos)', () => {
    for (const cmd of SPINE_ROUTES) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
      expect(REST_FIRST.has(cmd), cmd).toBe(true)
    }
    expect(REST_ROUTES.oplog_resolve.method).toBe('POST')
    expect(REST_ROUTES.oplog_resolve.buildPath({})).toBe('/api/oplog/resolve')
    expect(REST_ROUTES.peer_sync.buildPath({ deviceId: 'd1' })).toContain('/api/peers/d1/sync')
  })
})

describe('Rust twins for the spine (static reads, no cargo)', () => {
  it('serves devices/peers/oplog/bundle arms in crates/web', () => {
    const lib = fs.readFileSync('crates/web/src/lib.rs', 'utf8')
    for (const needle of ['/api/peers', '/api/sync/bundle/export', '/api/sync/bundle/import']) {
      expect(lib.includes(needle), needle).toBe(true)
    }
    const devices = fs.readFileSync('crates/web/src/api/devices.rs', 'utf8')
    expect(devices).toContain('/api/devices')
    const oplog = fs.readFileSync('crates/web/src/api/oplog.rs', 'utf8')
    expect(oplog).toContain('/api/oplog/resolve')
  })

  it('registers the Tauri twins in src-tauri', () => {
    const runtime = fs.readFileSync('src-tauri/src/lib.rs', 'utf8')
    // Device identity + todos have native twins; op-log / peer-sync / bundle
    // are REST-first by design (the dashboard owns peer auth + merge), so
    // `invoke()` routes them over restFetch with no Tauri twin.
    for (const cmd of ['register_this_device', 'list_lan_peers', 'list_todos']) {
      expect(runtime.includes(cmd), cmd).toBe(true)
    }
    for (const cmd of ['oplog_resolve', 'peer_sync', 'bundle_export']) {
      expect(REST_ROUTES[cmd], cmd).toBeTruthy()
      expect(REST_FIRST.has(cmd), cmd).toBe(true)
    }
  })
})
