// CyberManju OS — Stars.test (tests/frontend/stars.test.ts).
// Persisted favorites — star-id list codec (mirrors localStorage + `.cybermanju` kv).
import { describe, expect, it } from 'vitest'
import { parseStarIds, serializeStarIds } from '../../src/utils/stars'

describe('parseStarIds', () => {
  it('parses a JSON string array', () => {
    expect(parseStarIds('["a","b"]')).toEqual(['a', 'b'])
  })

  it('passes a live array through', () => {
    expect(parseStarIds(['a', 'b'])).toEqual(['a', 'b'])
  })

  it('drops non-string and empty entries', () => {
    expect(parseStarIds(['a', 1, '', null, 'b'])).toEqual(['a', 'b'])
    expect(parseStarIds('["a",1,""]')).toEqual(['a'])
  })

  it('rejects garbage without throwing', () => {
    expect(parseStarIds('not-json')).toEqual([])
    expect(parseStarIds('{"a":1}')).toEqual([])
    expect(parseStarIds(null)).toEqual([])
    expect(parseStarIds(undefined)).toEqual([])
    expect(parseStarIds(42)).toEqual([])
  })
})

describe('serializeStarIds', () => {
  it('round-trips through parseStarIds', () => {
    const raw = serializeStarIds(['a', 'b', 'a'])
    expect(parseStarIds(raw).sort()).toEqual(['a', 'b'])
  })

  it('serializes an empty set', () => {
    expect(serializeStarIds([])).toBe('[]')
  })
})
