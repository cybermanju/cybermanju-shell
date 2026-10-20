// CyberManju OS — Clipboard fallback (tests/frontend/clipboard-fallback.test.ts).
// Role: regression tests for cross-browser clipboard writes.
// Exports: none.
// Integrates: Vitest, src/utils/clipboard.ts.
// Contracts: fallback writes clean up and preserve the previous editing focus/selection.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { copyToClipboard } from '@/utils/clipboard'

afterEach(() => vi.unstubAllGlobals())

function mockFallback(execCommand = vi.fn(() => true), activeElement?: object) {
  const textarea = {
    value: '',
    style: {} as Record<string, string>,
    setAttribute: vi.fn(),
    select: vi.fn(),
    remove: vi.fn(),
  }
  vi.stubGlobal('navigator', { clipboard: undefined })
  vi.stubGlobal('document', {
    activeElement,
    body: { appendChild: vi.fn() },
    createElement: vi.fn(() => textarea),
    execCommand,
  })
  return { textarea, execCommand }
}

function focusedInput() {
  return {
    isConnected: true,
    focus: vi.fn(),
    selectionStart: 2,
    selectionEnd: 7,
    selectionDirection: 'backward',
    setSelectionRange: vi.fn(),
  }
}

describe('copyToClipboard', () => {
  it('prefers the Async Clipboard API when it succeeds', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    const fallbackCopy = vi.fn(() => true)
    vi.stubGlobal('navigator', { clipboard: { writeText } })

    await expect(copyToClipboard('share URL', { fallbackCopy })).resolves.toBe(true)
    expect(writeText).toHaveBeenCalledWith('share URL')
    expect(fallbackCopy).not.toHaveBeenCalled()
  })

  it('uses a temporary textarea when the Async Clipboard API is rejected', async () => {
    const { textarea, execCommand } = mockFallback()
    vi.stubGlobal('navigator', { clipboard: { writeText: vi.fn().mockRejectedValue(new Error('denied')) } })

    await expect(copyToClipboard('share URL')).resolves.toBe(true)
    expect(textarea.value).toBe('share URL')
    expect(textarea.setAttribute).toHaveBeenCalledWith('readonly', '')
    expect(textarea.select).toHaveBeenCalledOnce()
    expect(execCommand).toHaveBeenCalledWith('copy')
    expect(textarea.remove).toHaveBeenCalledOnce()
  })

  it('returns false and still removes the temporary field when fallback copying fails', async () => {
    const { textarea } = mockFallback(vi.fn(() => false))

    await expect(copyToClipboard('share URL')).resolves.toBe(false)
    expect(textarea.remove).toHaveBeenCalledOnce()
  })

  it.each([true, false])('restores editing focus and selection when copying returns %s', async (copied) => {
    const previous = focusedInput()
    const { textarea } = mockFallback(vi.fn(() => copied), previous)

    await expect(copyToClipboard('share URL')).resolves.toBe(copied)
    expect(textarea.remove).toHaveBeenCalledOnce()
    expect(previous.focus).toHaveBeenCalledWith({ preventScroll: true })
    expect(previous.setSelectionRange).toHaveBeenCalledWith(2, 7, 'backward')
  })

  it('restores focus and removes the temporary field when execCommand throws', async () => {
    const previous = focusedInput()
    const { textarea } = mockFallback(vi.fn(() => { throw new Error('copy denied') }), previous)

    await expect(copyToClipboard('share URL')).resolves.toBe(false)
    expect(textarea.remove).toHaveBeenCalledOnce()
    expect(previous.focus).toHaveBeenCalledOnce()
    expect(previous.setSelectionRange).toHaveBeenCalledWith(2, 7, 'backward')
  })

  it('retries focus without options on legacy browsers', async () => {
    const previous = focusedInput()
    previous.focus.mockImplementationOnce(() => { throw new Error('unsupported focus options') })
    mockFallback(undefined, previous)

    await expect(copyToClipboard('share URL')).resolves.toBe(true)
    expect(previous.focus).toHaveBeenNthCalledWith(1, { preventScroll: true })
    expect(previous.focus).toHaveBeenNthCalledWith(2)
    expect(previous.setSelectionRange).toHaveBeenCalledWith(2, 7, 'backward')
  })

  it('does not turn a successful copy into failure when selection restoration throws', async () => {
    const previous = focusedInput()
    previous.setSelectionRange.mockImplementation(() => { throw new Error('unsupported control') })
    mockFallback(undefined, previous)

    await expect(copyToClipboard('share URL')).resolves.toBe(true)
  })
})
