// CyberManju OS — Mobile scoped storage.test (tests/frontend/mobile-scoped-storage.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, ../….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import { normalizeMobileVaultFileName } from '../../src/utils/mobileScopedStorage'

describe('mobile .cybermanju mirror filenames', () => {
  it('adds the extension exactly once and preserves user-friendly Unicode names', () => {
    expect(normalizeMobileVaultFileName('cybermanju-vault')).toBe('cybermanju-vault.cybermanju')
    expect(normalizeMobileVaultFileName('meu vault.CYBERMANJU')).toBe('meu vault.cybermanju')
    expect(normalizeMobileVaultFileName('cofre café')).toBe('cofre café.cybermanju')
  })

  it('rejects empty names, path separators, and names longer than 80 characters', () => {
    expect(() => normalizeMobileVaultFileName('')).toThrow(/file name/i)
    expect(() => normalizeMobileVaultFileName('../shared')).toThrow(/file name/i)
    expect(() => normalizeMobileVaultFileName('a\\b')).toThrow(/file name/i)
    expect(() => normalizeMobileVaultFileName('a'.repeat(81))).toThrow(/file name/i)
  })
})
