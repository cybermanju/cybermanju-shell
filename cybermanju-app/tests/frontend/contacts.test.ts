// CyberManju OS — Contacts.test (tests/frontend/contacts.test.ts).
// ANALYSIS.md §4.4: contact record beside face groups.
import { describe, expect, it } from 'vitest'
import {
  addAlias,
  contactFromFaceGroup,
  contactPhoto,
  linkFile,
  parseContactsStore,
  removeAlias,
  serializeContactsStore,
  setContactNotes,
  unlinkFile,
} from '../../src/utils/contacts'

const group = {
  id: 'g1',
  name: 'Ada',
  fileIds: ['f1', 'f2'],
  embeddingCount: 2,
  createdAt: '2026-01-01T00:00:00Z',
}

describe('contact record', () => {
  it('seeds links from the face group', () => {
    const c = contactFromFaceGroup(group)
    expect(c.links).toEqual(['f1', 'f2'])
    expect(contactPhoto(c, ['f1'])).toBe('f1')
  })

  it('dedupes aliases case-insensitively and caps them', () => {
    let c = contactFromFaceGroup(group)
    c = addAlias(c, '  Ada L. ')
    c = addAlias(c, 'ada l.')
    c = addAlias(c, ' ')
    expect(c.aliases).toEqual(['Ada L.'])
    expect(removeAlias(c, 'Ada L.').aliases).toEqual([])
  })

  it('links files once and unlinks', () => {
    let c = contactFromFaceGroup(group)
    c = linkFile(c, 'f1')
    c = linkFile(c, 'f9')
    expect(c.links).toEqual(['f1', 'f2', 'f9'])
    expect(unlinkFile(c, 'f1').links).toEqual(['f2', 'f9'])
  })

  it('caps notes at 4000 chars', () => {
    const c = setContactNotes(contactFromFaceGroup(group), 'x'.repeat(5000))
    expect(c.notes).toHaveLength(4000)
  })

  it('round-trips the store codec and rejects garbage', () => {
    const c = addAlias(contactFromFaceGroup(group), 'A')
    const back = parseContactsStore(serializeContactsStore({ g1: c }))
    expect(back.g1.aliases).toEqual(['A'])
    expect(parseContactsStore('junk')).toEqual({})
    expect(parseContactsStore(null)).toEqual({})
  })
})
