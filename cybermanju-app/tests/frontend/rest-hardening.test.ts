// CyberManju OS — rest-hardening.test (tests/frontend/rest-hardening.test.ts).
// Role: pin the P0-22 frontend mitigations (ANALYSIS.md §5 close-out) —
// `restFetch` never hangs (15 s AbortController), transient AGENT-1 prefixes
// get one backoff retry, and the Rust single-opener + 100 MB body cap exist
// beside it (static reads, no cargo).
// Exports: none (vitest suite).
// Integrates: src/composables/useTauri.ts (restFetch); crates/web/src/lib.rs
// (MAX_BODY_SIZE + P0-22 discipline comment); crates/db/src/database.rs
// (OpenerLock single-opener guard).
// Contracts: node-env safe; static presence only — live timeout behavior is
// exercised by wanHttpProbe's closed-port test (wan-dial.test.ts).
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'

const tauri = fs.readFileSync('src/composables/useTauri.ts', 'utf8')

describe('restFetch never hangs, retries transients once (P0-22)', () => {
  it('aborts every request after 15 s', () => {
    expect(tauri).toContain('TIMEOUT_MS = 15000')
    expect(tauri).toContain('new AbortController()')
    expect(tauri).toContain('ctrl.abort()')
  })

  it('retries network:/rate_limited: once with backoff, then throws prefixed', () => {
    expect(tauri).toContain('MAX_ATTEMPTS = 2')
    expect(tauri).toContain("msg.startsWith('network:')")
    expect(tauri).toContain("msg.startsWith('rate_limited:')")
    expect(tauri).toContain('network: request timed out after')
  })
})

describe('Rust P0-22 halves exist (static reads, no cargo)', () => {
  it('caps request bodies at 100 MB aligned with the Caddy sidecar', () => {
    const lib = fs.readFileSync('crates/web/src/lib.rs', 'utf8')
    expect(lib).toContain('MAX_BODY_SIZE')
    expect(lib).toContain('104_857_600')
  })

  it('documents the write-lock serialization discipline at the lock site', () => {
    const lib = fs.readFileSync('crates/web/src/lib.rs', 'utf8')
    expect(lib).toContain('P0-22')
  })

  it('enforces the single-opener guard in the database crate', () => {
    const db = fs.readFileSync('crates/db/src/database.rs', 'utf8')
    expect(db).toContain('OpenerLock')
    expect(db).toContain('conflict: database')
    expect(db).toContain('opener_lock_path')
  })
})
