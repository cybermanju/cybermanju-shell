// CyberManju OS — Provider text.test (tests/frontend/provider-text.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  decodeProviderText,
  isTextEditableProviderName,
  MAX_PROVIDER_TEXT_BYTES,
} from '../../src/utils/providerText'

describe('local provider text editing', () => {
  it('offers common source and text formats, but not binary vault containers', () => {
    expect(isTextEditableProviderName('notes.md')).toBe(true)
    expect(isTextEditableProviderName('.env')).toBe(true)
    expect(isTextEditableProviderName('readme', 'text/plain')).toBe(true)
    expect(isTextEditableProviderName('private.cybermanju')).toBe(false)
    expect(isTextEditableProviderName('backup.cybermanju.old')).toBe(false)
    expect(isTextEditableProviderName('secret.cyb3')).toBe(false)
    expect(isTextEditableProviderName('photo.png', 'image/png')).toBe(false)
  })

  it('decodes valid UTF-8, including non-ASCII content', () => {
    expect(decodeProviderText(new TextEncoder().encode('Olá, mundo\n🙂'))).toBe('Olá, mundo\n🙂')
    expect(decodeProviderText(new Uint8Array())).toBe('')
  })

  it('refuses binary containers, NUL bytes, invalid UTF-8 and oversized files', () => {
    expect(() => decodeProviderText(new TextEncoder().encode('CYBMJ01payload'))).toThrow(/binary_container/)
    expect(() => decodeProviderText(new Uint8Array([0x61, 0, 0x62]))).toThrow(/binary_file/)
    expect(() => decodeProviderText(new Uint8Array([0xc3, 0x28]))).toThrow(/invalid_text/)
    expect(() => decodeProviderText(new Uint8Array(MAX_PROVIDER_TEXT_BYTES + 1))).toThrow(/too_large/)
  })
})
