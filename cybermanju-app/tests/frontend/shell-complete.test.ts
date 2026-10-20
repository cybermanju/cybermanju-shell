// CyberManju OS — Shell complete.test (tests/frontend/shell-complete.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it, vi } from 'vitest'
import {
  currentWord,
  ghostSuffix,
  historyTokenHints,
  isVerbPosition,
  loadHistory,
  nextHistoryMatch,
  prevHistoryMatch,
  pushHistory,
  replaceWord,
  saveHistory,
  wordStart,
} from '../../src/utils/shellComplete'

describe('word bounds', () => {
  it('finds the word under the caret', () => {
    expect(currentWord('disk li')).toBe('li')
    expect(currentWord('disk ')).toBe('')
    expect(currentWord('ls')).toBe('ls')
    expect(currentWord('')).toBe('')
  })
  it('detects verb position', () => {
    expect(isVerbPosition('l')).toBe(true)
    expect(isVerbPosition('disk l')).toBe(false)
  })
  it('replaces the word under the caret', () => {
    expect(replaceWord('disk li', 7, 'list')).toEqual({ line: 'disk list', caret: 9 })
  })
})

describe('history', () => {
  it('dedupes consecutive repeats and caps', () => {
    expect(pushHistory(['a'], 'a')).toEqual(['a'])
    expect(pushHistory(['a'], 'b')).toEqual(['a', 'b'])
  })
  it('finds previous prefix matches walking backwards', () => {
    const h = ['ls', 'disk list', 'ls /tmp']
    expect(prevHistoryMatch(h, 'ls', 2)).toBe(2)
    // An entry identical to the prefix is the line itself — skipped.
    expect(prevHistoryMatch(h, 'l', 1)).toBe(0)
    expect(prevHistoryMatch(h, 'ls /tmp', 2)).toBe(-1)
    expect(prevHistoryMatch(h, 'zzz', 2)).toBe(-1)
  })
  it('walks forwards too', () => {
    const h = ['ls', 'disk list', 'ls /tmp']
    expect(nextHistoryMatch(h, 'ls', 1)).toBe(2)
    expect(nextHistoryMatch(h, 'ls', 3)).toBe(-1)
  })
  it('suggests past argument tokens', () => {
    const hints = historyTokenHints(['ls /projects/2026', 'cat notes.txt'], '/pro')
    expect(hints).toContain('/projects/2026')
  })
  it('round-trips through localStorage', () => {
    // Node test env has no DOM storage — stub the surface we use.
    const backing = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: (k: string) => backing.get(k) ?? null,
      setItem: (k: string, v: string) => void backing.set(k, v),
      removeItem: (k: string) => void backing.delete(k),
    })
    saveHistory(['ls', 'pwd'])
    expect(loadHistory()).toEqual(['ls', 'pwd'])
    saveHistory([])
  })
})

describe('ghostSuffix', () => {
  it('prefers history over the static table', () => {
    expect(ghostSuffix('disk', ['disk list'], ['disks'])).toBe(' list')
  })
  it('falls back to table hits', () => {
    expect(ghostSuffix('dis', [], ['disk', 'disk create'])).toBe('k')
  })
  it('is empty with nothing to offer', () => {
    expect(ghostSuffix('', ['ls'], ['ls'])).toBe('')
    expect(ghostSuffix('zzz', [], [])).toBe('')
  })
})
