// CyberManju OS — Dedup.test (tests/frontend/dedup.test.ts).
// Storage-diet report (ANALYSIS.md §4.5): BLAKE3 grouping, reclaimable math,
// keep-newest suggestion, honest unhashable handling.
import { describe, expect, it } from 'vitest'
import {
  dedupSummary,
  groupDuplicates,
  normalizeHash,
  type DupFile,
} from '../../src/utils/dedup'

function file(over: Partial<DupFile> & { id: string }): DupFile {
  return {
    name: over.id,
    sizeBytes: 100,
    modifiedAt: '2026-01-01T00:00:00Z',
    hashBlake3: 'a'.repeat(64),
    ...over,
  }
}

describe('normalizeHash', () => {
  it('accepts 64-hex digests, case-insensitive', () => {
    expect(normalizeHash('A'.repeat(64))).toBe('a'.repeat(64))
    expect(normalizeHash(`  ${'b'.repeat(64)}  `)).toBe('b'.repeat(64))
  })

  it('rejects missing, short, and non-hex hashes', () => {
    expect(normalizeHash(undefined)).toBeNull()
    expect(normalizeHash(null)).toBeNull()
    expect(normalizeHash('')).toBeNull()
    expect(normalizeHash('abcd')).toBeNull()
    expect(normalizeHash('z'.repeat(64))).toBeNull()
    expect(normalizeHash(42)).toBeNull()
  })
})

describe('groupDuplicates', () => {
  it('returns no groups for unique hashes', () => {
    const files = [file({ id: 'a', hashBlake3: 'a'.repeat(64) }), file({ id: 'b', hashBlake3: 'b'.repeat(64) })]
    expect(groupDuplicates(files)).toEqual([])
  })

  it('groups identical hashes and reports reclaimable bytes', () => {
    const files = [
      file({ id: 'a', sizeBytes: 1000 }),
      file({ id: 'b', sizeBytes: 1000 }),
      file({ id: 'c', sizeBytes: 1000 }),
    ]
    const [group] = groupDuplicates(files)
    expect(group.files).toHaveLength(3)
    expect(group.totalBytes).toBe(3000)
    expect(group.reclaimableBytes).toBe(2000)
  })

  it('keeps the newest copy and orders members newest-first', () => {
    const files = [
      file({ id: 'old', modifiedAt: '2026-01-01T00:00:00Z' }),
      file({ id: 'new', modifiedAt: '2026-06-01T00:00:00Z' }),
    ]
    const [group] = groupDuplicates(files)
    expect(group.keepId).toBe('new')
    expect(group.files[0].id).toBe('new')
  })

  it('ignores unhashable files without dropping hashable ones', () => {
    const files = [
      file({ id: 'a' }),
      file({ id: 'b' }),
      file({ id: 'lonely', hashBlake3: undefined }),
      file({ id: 'short', hashBlake3: 'abcd' }),
    ]
    const groups = groupDuplicates(files)
    expect(groups).toHaveLength(1)
    expect(groups[0].files.map((f) => f.id).sort()).toEqual(['a', 'b'])
  })

  it('normalizes case so mixed-case digests still match', () => {
    const files = [file({ id: 'a', hashBlake3: 'A'.repeat(64) }), file({ id: 'b', hashBlake3: 'a'.repeat(64) })]
    expect(groupDuplicates(files)).toHaveLength(1)
  })

  it('ranks groups by reclaimable bytes, largest first', () => {
    const files = [
      file({ id: 's1', sizeBytes: 10, hashBlake3: 'a'.repeat(64) }),
      file({ id: 's2', sizeBytes: 10, hashBlake3: 'a'.repeat(64) }),
      file({ id: 'b1', sizeBytes: 5000, hashBlake3: 'b'.repeat(64) }),
      file({ id: 'b2', sizeBytes: 5000, hashBlake3: 'b'.repeat(64) }),
    ]
    const groups = groupDuplicates(files)
    expect(groups.map((g) => g.hash)).toEqual(['b'.repeat(64), 'a'.repeat(64)])
  })
})

describe('dedupSummary', () => {
  it('rolls groups into headline numbers plus unhashable count', () => {
    const files = [
      file({ id: 'a', sizeBytes: 1000 }),
      file({ id: 'b', sizeBytes: 1000 }),
      file({ id: 'unique', hashBlake3: 'c'.repeat(64) }),
      file({ id: 'nohash', hashBlake3: undefined }),
    ]
    const summary = dedupSummary(files)
    expect(summary).toEqual({
      groupCount: 1,
      fileCount: 2,
      totalBytes: 2000,
      reclaimableBytes: 1000,
      unhashableCount: 1,
    })
  })

  it('is empty-safe', () => {
    expect(dedupSummary([])).toEqual({
      groupCount: 0,
      fileCount: 0,
      totalBytes: 0,
      reclaimableBytes: 0,
      unhashableCount: 0,
    })
  })
})
