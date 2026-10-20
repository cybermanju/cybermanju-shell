// CyberManju OS — Notes.test (tests/frontend/notes.test.ts).
// ANALYSIS.md §4.2: note detection, wikilink/md-link extraction, backlink graph.
import { describe, expect, it } from 'vitest'
import { buildNoteIndex, extractMdLinks, extractWikilinks, isNoteFile } from '../../src/utils/notes'

describe('isNoteFile', () => {
  it('accepts markdown/text, never folders', () => {
    expect(isNoteFile('a.md')).toBe(true)
    expect(isNoteFile('A.MARKDOWN')).toBe(true)
    expect(isNoteFile('n.txt')).toBe(true)
    expect(isNoteFile('photo.jpg')).toBe(false)
    expect(isNoteFile('a.md', 'folder')).toBe(false)
  })
})

describe('extractWikilinks', () => {
  it('parses targets, strips labels, dedupes', () => {
    expect(extractWikilinks('see [[Lisbon]] and [[lisbon-trip|trip]] and [[Lisbon]]')).toEqual([
      'Lisbon',
      'lisbon-trip',
    ])
  })
})

describe('extractMdLinks', () => {
  it('takes relative links, skips http/anchors', () => {
    const links = extractMdLinks('[a](other.md) [b](https://x) [c](#frag)')
    expect(links.map((l) => l.target)).toEqual(['other.md'])
  })
})

describe('buildNoteIndex', () => {
  it('resolves links case-insensitively, tracks backlinks/orphans/broken', () => {
    const notes = [
      { id: '1', name: 'Lisbon.md', contentText: 'see [[Porto]] and [[Nowhere]]' },
      { id: '2', name: 'porto.md', contentText: 'back to [[lisbon]]' },
      { id: '3', name: 'lonely.md', contentText: 'nothing' },
    ]
    const idx = buildNoteIndex(notes)
    expect(idx.links['1']).toEqual(['2'])
    expect(idx.backlinks['2']).toEqual(['1'])
    expect(idx.backlinks['1']).toEqual(['2'])
    expect(idx.orphans).toEqual(['3'])
    expect(idx.broken).toEqual([{ from: '1', target: 'Nowhere' }])
  })
})
