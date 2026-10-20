// CyberManju OS — Dom.test (tests/frontend/dom.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, ../….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import { isEditableTarget } from '../../src/utils/dom'

function el(tagName: string, extra: Record<string, unknown> = {}) {
  return { tagName, ...extra } as unknown as EventTarget
}

describe('isEditableTarget', () => {
  it('treats text-entry elements as editable', () => {
    expect(isEditableTarget(el('INPUT'))).toBe(true)
    expect(isEditableTarget(el('input'))).toBe(true)
    expect(isEditableTarget(el('TEXTAREA'))).toBe(true)
    expect(isEditableTarget(el('SELECT'))).toBe(true)
    expect(isEditableTarget(el('DIV', { isContentEditable: true }))).toBe(true)
  })

  it('does not treat chrome elements as editable', () => {
    expect(isEditableTarget(el('DIV'))).toBe(false)
    expect(isEditableTarget(el('BUTTON'))).toBe(false)
    expect(isEditableTarget(el('BODY'))).toBe(false)
    expect(isEditableTarget(el('DIV', { isContentEditable: false }))).toBe(false)
  })

  it('handles targets that are not elements', () => {
    expect(isEditableTarget(null)).toBe(false)
    expect(isEditableTarget({} as EventTarget)).toBe(false)
  })
})
