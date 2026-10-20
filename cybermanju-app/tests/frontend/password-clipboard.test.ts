// CyberManju OS — Password clipboard.test (tests/frontend/password-clipboard.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — password generator + clipboard auto-clear (Phase 3).

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  PASSWORD_DEFAULTS,
  generatePassword,
  passwordCharset,
  passwordEntropyBits,
  passwordStrength,
  strengthOf,
} from '@/utils/password'
import {
  CLIPBOARD_CLEAR_MS,
  cancelClipboardClear,
  copySecret,
  copyText,
} from '@/utils/clipboard'

describe('password generator', () => {
  it('honours the charset flags and the ambiguous filter', () => {
    const all = passwordCharset(PASSWORD_DEFAULTS)
    expect(all).toContain('A')
    expect(all).toContain('a')
    expect(all).toContain('0')
    expect(all).toContain('!')
    const noAmbiguous = passwordCharset({ ...PASSWORD_DEFAULTS, ambiguous: true })
    for (const ch of '0O1lI') expect(noAmbiguous).not.toContain(ch)
    const digitsOnly = passwordCharset({
      length: 12, upper: false, lower: false, digits: true, symbols: false, ambiguous: false,
    })
    expect(digitsOnly).toBe('0123456789')
    expect(passwordCharset({ length: 8, upper: false, lower: false, digits: false, symbols: false, ambiguous: false })).toBe('')
  })

  it('computes entropy as length × log2(charset)', () => {
    const opts = { length: 20, upper: true, lower: true, digits: true, symbols: true, ambiguous: false }
    const pool = passwordCharset(opts).length
    expect(passwordEntropyBits(opts)).toBeCloseTo(20 * Math.log2(pool), 6)
    expect(passwordEntropyBits({ ...opts, length: 0 })).toBe(0)
    expect(passwordEntropyBits({ ...opts, upper: false, lower: false, digits: false, symbols: false })).toBe(0)
  })

  it('buckets strength the usual way', () => {
    expect(passwordStrength(39)).toBe('weak')
    expect(passwordStrength(40)).toBe('fair')
    expect(passwordStrength(59)).toBe('fair')
    expect(passwordStrength(60)).toBe('good')
    expect(passwordStrength(79)).toBe('good')
    expect(passwordStrength(80)).toBe('strong')
  })

  it('generates the requested length from the active charset', () => {
    const pw = generatePassword({ ...PASSWORD_DEFAULTS, length: 32 })
    expect(pw).toHaveLength(32)
    const pool = passwordCharset(PASSWORD_DEFAULTS)
    for (const ch of pw) expect(pool).toContain(ch)
    // Two draws are astronomically unlikely to match.
    expect(generatePassword()).not.toBe(generatePassword())
  })

  it('rejects empty charsets and out-of-range lengths', () => {
    expect(() => generatePassword({ length: 12, upper: false, lower: false, digits: false, symbols: false, ambiguous: false }))
      .toThrow(/invalid: enable at least one/)
    expect(() => generatePassword({ ...PASSWORD_DEFAULTS, length: 2 })).toThrow(/invalid: length/)
    expect(() => generatePassword({ ...PASSWORD_DEFAULTS, length: 200 })).toThrow(/invalid: length/)
  })

  it('estimates strength of a pasted password from unique charset size', () => {
    expect(strengthOf('')).toEqual({ bits: 0, strength: 'weak' })
    // 24 chars, all unique → 24 × log2(24) ≈ 108 bits.
    const strong = strengthOf('Xk9!mQ2#vLp$8RnZwTy3Bh')
    expect(strong.strength).toBe('strong')
    expect(strong.bits).toBeGreaterThan(80)
  })
})

describe('clipboard auto-clear', () => {
  beforeEach(() => {
    cancelClipboardClear()
  })
  afterEach(() => {
    cancelClipboardClear()
    vi.useRealTimers()
  })

  it('writes the secret now and an empty string after 30 s', async () => {
    vi.useFakeTimers()
    const writes: string[] = []
    await copySecret('hunter2', { writeText: t => { writes.push(t) } })
    expect(writes).toEqual(['hunter2'])
    vi.advanceTimersByTime(CLIPBOARD_CLEAR_MS - 1)
    expect(writes).toEqual(['hunter2'])
    vi.advanceTimersByTime(1)
    expect(writes).toEqual(['hunter2', ''])
  })

  it('a second copy cancels the first pending clear', async () => {
    vi.useFakeTimers()
    const writes: string[] = []
    await copySecret('one', { writeText: t => { writes.push(t) } })
    vi.advanceTimersByTime(10_000)
    await copySecret('two', { writeText: t => { writes.push(t) } })
    vi.advanceTimersByTime(CLIPBOARD_CLEAR_MS)
    // Only the latest value is cleared, exactly once.
    expect(writes).toEqual(['one', 'two', ''])
  })

  it('cancelClipboardClear leaves the clipboard alone', async () => {
    vi.useFakeTimers()
    const writes: string[] = []
    await copySecret('stays', { writeText: t => { writes.push(t) } })
    cancelClipboardClear()
    vi.advanceTimersByTime(CLIPBOARD_CLEAR_MS * 2)
    expect(writes).toEqual(['stays'])
  })

  it('copyText never auto-clears', async () => {
    vi.useFakeTimers()
    const writes: string[] = []
    await copyText('plain note', { writeText: t => { writes.push(t) } })
    vi.advanceTimersByTime(CLIPBOARD_CLEAR_MS * 2)
    expect(writes).toEqual(['plain note'])
  })
})
