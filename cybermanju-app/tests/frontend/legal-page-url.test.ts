// CyberManju OS — Legal page url.test (tests/frontend/legal-page-url.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import { legalPageUrl } from '@/utils/legalPageUrl'

describe('legalPageUrl', () => {
  it('keeps legal pages inside the develop preview path', () => {
    expect(legalPageUrl('privacy.html', '/develop/')).toBe('/develop/privacy.html')
    expect(legalPageUrl('terms.html', '/develop/')).toBe('/develop/terms.html')
  })

  it('preserves canonical links for root and native builds', () => {
    expect(legalPageUrl('privacy.html', '/')).toBe('https://cybermanju.github.io/privacy.html')
    expect(legalPageUrl('terms.html', '/')).toBe('https://cybermanju.github.io/terms.html')
  })
})
