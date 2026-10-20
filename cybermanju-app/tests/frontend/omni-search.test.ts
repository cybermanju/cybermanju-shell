// CyberManju OS — Omni search grouping test (tests/frontend/omni-search.test.ts).
// The merged FACES + COLLECTIONS + SEARCH window renders sections in one
// fixed order (app > collection > person | provider > folder > file > content)
// with tag + divider separation; this pins that contract on the pure helper.
import { describe, expect, it } from 'vitest'
import { buildOmniSections, OMNI_SECTION_ORDER, OMNI_SECTION_TAG } from '@/utils/omniSearch'

const INPUT = {
  apps: [{ key: 'files', name: 'FILES', sub: 'Browse files' }],
  collections: [{ key: 'c1', name: 'Summer Album', sub: 'CUSTOM · 4 ITEMS' }],
  people: [{ key: 'p1', name: 'Ada', sub: '3 FILES' }],
  providers: [{ key: 'g1', name: 'Drive', sub: 'GOOGLEDRIVE · OPEN FILE MANAGER IN PROVIDER' }],
  folders: [{ key: 'f1', name: 'Summer', sub: '/Summer' }],
  files: [{ key: 'f2', name: 'summer.jpg', sub: '/Summer/summer.jpg' }],
  content: [{ key: 'f2', name: 'summer.jpg', sub: '…at the summer cabin…' }],
}

describe('omni search sections', () => {
  it('exposes the canonical order app > collection > person | provider > folder > file > content', () => {
    expect(OMNI_SECTION_ORDER).toEqual([
      'apps', 'collections', 'people', 'providers', 'folders', 'files', 'content',
    ])
    for (const id of OMNI_SECTION_ORDER) {
      expect(OMNI_SECTION_TAG[id], `tag for ${id}`).toMatch(/^[A-Z]+$/)
    }
  })

  it('returns every non-empty section in order on an empty query', () => {
    const sections = buildOmniSections('', INPUT)
    expect(sections.map((s) => s.id)).toEqual(OMNI_SECTION_ORDER)
  })

  it('filters every section by one query and keeps the order', () => {
    const sections = buildOmniSections('summer', INPUT)
    const ids = sections.map((s) => s.id)
    // Apps/files descriptions do not mention summer — those drop out, but
    // the survivors keep canonical order with dividers between them.
    expect(ids).toEqual(['collections', 'folders', 'files', 'content'])
    let last = -1
    for (const id of ids) {
      const pos = OMNI_SECTION_ORDER.indexOf(id)
      expect(pos).toBeGreaterThan(last)
      last = pos
    }
  })

  it('matches people and providers by name', () => {
    expect(buildOmniSections('ada', INPUT).map((s) => s.id)).toEqual(['people'])
    expect(buildOmniSections('drive', INPUT).map((s) => s.id)).toContain('providers')
  })

  it('drops empty sections entirely (no lone dividers)', () => {
    const sections = buildOmniSections('zzz-no-match', INPUT)
    expect(sections).toEqual([])
  })

  it('ranks prefix hits above plain substring hits, stably', () => {
    const rows = [
      { key: 'a', name: 'my summer', sub: '' },
      { key: 'b', name: 'Summer Mix', sub: '' },
      { key: 'c', name: 'high summer', sub: '' },
    ]
    const sections = buildOmniSections('summer', {
      apps: [], collections: [], people: [], providers: [], folders: [],
      files: rows, content: [],
    })
    expect(sections.map((s) => s.id)).toEqual(['files'])
    expect(sections[0].rows.map((r) => r.key)).toEqual(['b', 'a', 'c'])
  })

  it('returns caller rows by reference (no rehydrate step downstream)', () => {
    const row = { key: 'f1', name: 'note.txt', sub: '/note.txt', extra: 42 }
    const sections = buildOmniSections('', {
      apps: [], collections: [], people: [], providers: [], folders: [],
      files: [row], content: [],
    })
    expect(sections[0].rows[0]).toBe(row)
  })
})
