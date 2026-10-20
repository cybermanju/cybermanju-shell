// CyberManju OS — Shamir.test (tests/frontend/shamir.test.ts).
// ANALYSIS.md §3: m-of-n recovery primitive.
import { describe, expect, it } from 'vitest'
import { combineShares, combineText, splitSecret, splitText } from '../../src/utils/shamir'

describe('shamir secret sharing', () => {
  it('round-trips any m-subset of n shares', () => {
    const shares = splitText('correct horse battery staple', 5, 3)
    expect(shares).toHaveLength(5)
    expect(combineText([shares[0], shares[2], shares[4]])).toBe('correct horse battery staple')
    expect(combineText([shares[4], shares[1], shares[0]])).toBe('correct horse battery staple')
  })

  it('round-trips raw bytes incl. UTF-8', () => {
    const bytes = new TextEncoder().encode('frase secreta çãõ 🔑')
    const shares = splitSecret(bytes, 3, 2)
    expect(combineShares([shares[1], shares[2]])).toEqual(bytes)
  })

  it('fewer than m shares do not restore the secret', () => {
    const shares = splitText('top secret words here', 5, 3)
    expect(combineText([shares[0], shares[1]])).not.toBe('top secret words here')
  })

  it('rejects bad parameters and duplicate indexes', () => {
    expect(() => splitText('x', 2, 1)).toThrow()
    expect(() => splitText('x', 1, 2)).toThrow()
    expect(() => splitText('', 3, 2)).toThrow()
    const shares = splitText('abcdef', 3, 2)
    expect(() => combineShares([shares[0], shares[0]])).toThrow()
    expect(() => combineShares([shares[0]])).toThrow()
  })
})
