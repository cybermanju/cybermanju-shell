// CyberManju OS — volume-caps.test (tests/frontend/volume-caps.test.ts).
// Role: pin the shared 1 MiB single-write cap (P1-17) — one TS constant,
// re-exported by staticCybsh, equal to Rust MAX_WRITE_BYTES (read statically).
// Exports: none (vitest suite).
// Integrates: src/utils/volumeCaps.ts; src/utils/staticCybsh.ts;
// crates/os-wasm/src/os.rs (static read, no cargo).
// Contracts: cap is exactly 1048576; over-cap refuses with `too_large:`.
import fs from 'node:fs'
import { describe, expect, it } from 'vitest'
import { VOLUME_WRITE_LIMIT, isOverWriteCap, writeCapError } from '@/utils/volumeCaps'

describe('shared volume write cap', () => {
  it('is exactly 1 MiB', () => {
    expect(VOLUME_WRITE_LIMIT).toBe(1024 * 1024)
  })

  it('staticCybsh re-exports the same cap (no divergent constant)', async () => {
    const { STATIC_WRITE_LIMIT } = await import('@/utils/staticCybsh')
    expect(STATIC_WRITE_LIMIT).toBe(VOLUME_WRITE_LIMIT)
  })

  it('matches Rust MAX_WRITE_BYTES (static read, no cargo)', () => {
    const rs = fs.readFileSync('crates/os-wasm/src/os.rs', 'utf8')
    const m = rs.match(/const MAX_WRITE_BYTES:\s*usize\s*=\s*([^;]+);/)
    expect(m).toBeTruthy()
    const expr = (m?.[1] ?? '').replace(/\s+/g, '')
    expect(expr).toBe('1024*1024')
  })

  it('refuses over-cap with too_large:', () => {
    expect(isOverWriteCap(VOLUME_WRITE_LIMIT)).toBe(false)
    expect(isOverWriteCap(VOLUME_WRITE_LIMIT + 1)).toBe(true)
    expect(writeCapError(VOLUME_WRITE_LIMIT + 1, '/note.txt').startsWith('too_large:')).toBe(true)
  })
})
