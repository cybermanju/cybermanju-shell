// CyberManju OS — Terminal ux.test (tests/frontend/terminal-ux.test.ts).
// Role: Vitest frontend suite for the Unix-emulator terminal helpers.
// Exports: module root (see items below).
// Integrates: vitest, ../../src/utils/shellComplete.
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  commonPrefix,
  containsClearSequence,
  expandHistory,
  formatPrompt,
  linkifyLine,
  localCompletions,
  quoteCompletionName,
  resolveCompletionDir,
  shortenCwd,
  splitCompletionWord,
  stripClearSequences,
  trackCd,
  wantsDirsOnly,
} from '../../src/utils/shellComplete'

describe('shortenCwd', () => {
  it('keeps short paths literal', () => {
    expect(shortenCwd('/')).toBe('/')
    expect(shortenCwd('/docs')).toBe('/docs')
  })
  it('truncates long paths with an ellipsis tail', () => {
    const long = '/providers/drive-a/very/deep/folder/with/many/parts'
    const short = shortenCwd(long, 24)
    expect(short.length).toBeLessThanOrEqual(25)
    expect(short.startsWith('…')).toBe(true)
    expect(long.endsWith(short.slice(1))).toBe(true)
  })
})

describe('formatPrompt', () => {
  it('renders a bash-style user@host:cwd prompt', () => {
    expect(formatPrompt('/docs')).toBe('cybsh@os:/docs')
  })
})

describe('clear sequences', () => {
  it('detects the clear-screen escape', () => {
    expect(containsClearSequence('\x1b[2J\x1b[H')).toBe(true)
    expect(containsClearSequence('plain output')).toBe(false)
  })
  it('strips clear sequences without touching text', () => {
    expect(stripClearSequences('\x1b[2J\x1b[Hhello')).toBe('hello')
    expect(stripClearSequences('a\nb')).toBe('a\nb')
  })
})

describe('expandHistory', () => {
  const h = ['ls /docs', 'cat notes.txt', 'disk list']
  it('expands !! to the last line', () => {
    expect(expandHistory('!!', h)).toMatchObject({ line: 'disk list', expanded: true })
  })
  it('expands !n and !-n', () => {
    expect(expandHistory('!1', h)?.line).toBe('ls /docs')
    expect(expandHistory('!-1', h)?.line).toBe('disk list')
  })
  it('expands !prefix and !$', () => {
    expect(expandHistory('!cat', h)?.line).toBe('cat notes.txt')
    expect(expandHistory('!$', h)?.line).toBe('list')
  })
  it('reports event-not-found instead of running nothing', () => {
    expect(expandHistory('!!', [])?.error).toMatch(/event not found/)
    expect(expandHistory('!zzz', h)?.error).toMatch(/event not found/)
    expect(expandHistory('!99', h)?.error).toMatch(/event not found/)
  })
  it('leaves ordinary lines untouched', () => {
    expect(expandHistory('ls -la', h)).toMatchObject({ line: 'ls -la' })
  })
})

describe('trackCd', () => {
  it('resolves absolute and relative cds', () => {
    expect(trackCd('cd /docs', '/')).toBe('/docs')
    expect(trackCd('cd sub', '/docs')).toBe('/docs/sub')
    expect(trackCd('cd ..', '/docs/sub')).toBe('/docs')
    expect(trackCd('cd', '/docs')).toBe('/')
  })
  it('returns null for non-cd lines and cd -', () => {
    expect(trackCd('ls /docs', '/')).toBeNull()
    expect(trackCd('cd -', '/docs')).toBeNull()
  })
})

describe('localCompletions', () => {
  it('completes verbs offline', () => {
    expect(localCompletions('di')).toContain('disk')
  })
  it('completes sub-commands', () => {
    expect(localCompletions('disk ')).toEqual(
      expect.arrayContaining(['disk create', 'disk list']),
    )
    expect(localCompletions('disk l')).toEqual(['disk list'])
  })
  it('returns nothing unknown', () => {
    expect(localCompletions('zzz')).toEqual([])
  })
})

describe('commonPrefix', () => {
  it('extends to the shared stem', () => {
    expect(commonPrefix(['disk create', 'disk list', 'disk check'])).toBe('disk ')
  })
  it('is exact for a single candidate', () => {
    expect(commonPrefix(['ls'])).toBe('ls')
  })
  it('is empty with nothing shared', () => {
    expect(commonPrefix(['ls', 'df'])).toBe('')
    expect(commonPrefix([])).toBe('')
  })
})

describe('path completion words', () => {
  it('splits dir and fragment', () => {
    expect(splitCompletionWord('/docs/a')).toEqual({ rawDir: '/docs', frag: 'a' })
    expect(splitCompletionWord('notes')).toEqual({ rawDir: '', frag: 'notes' })
    expect(splitCompletionWord('/')).toEqual({ rawDir: '/', frag: '' })
  })
  it('resolves against the cwd', () => {
    expect(resolveCompletionDir('', '/docs')).toBe('/docs')
    expect(resolveCompletionDir('/a/b', '/docs')).toBe('/a/b')
    expect(resolveCompletionDir('sub', '/docs')).toBe('/docs/sub')
    expect(resolveCompletionDir('../x', '/docs/sub')).toBe('/docs/x')
    expect(resolveCompletionDir('/', '/docs')).toBe('/')
  })
  it('quotes names that need it', () => {
    expect(quoteCompletionName('notes.txt')).toBe('notes.txt')
    expect(quoteCompletionName('my notes.txt')).toBe('"my notes.txt"')
  })
})

describe('wantsDirsOnly', () => {
  it('is true for cd lines', () => {
    expect(wantsDirsOnly('cd ')).toBe(true)
    expect(wantsDirsOnly('cd /do')).toBe(true)
    expect(wantsDirsOnly('  cd sub')).toBe(true)
  })
  it('is false for everything else', () => {
    expect(wantsDirsOnly('ls ')).toBe(false)
    expect(wantsDirsOnly('cat /do')).toBe(false)
    expect(wantsDirsOnly('cdrom')).toBe(false)
    expect(wantsDirsOnly('')).toBe(false)
  })
})

describe('linkifyLine', () => {
  it('marks urls and absolute paths', () => {
    const parts = linkifyLine('see https://example.com/x and /docs/a.txt ok')
    expect(parts.some((p) => p.url && p.text === 'https://example.com/x')).toBe(true)
    expect(parts.some((p) => p.path && p.text === '/docs/a.txt')).toBe(true)
  })
  it('keeps plain text as one run', () => {
    expect(linkifyLine('hello world')).toEqual([{ text: 'hello world' }])
  })
})
