// CyberManju OS — Timeline.test (tests/frontend/timeline.test.ts).
// ANALYSIS.md §4.3: day bucketing, "On this day", range.
import { describe, expect, it } from 'vitest'
import { groupByDay, onThisDay, timelineRange } from '../../src/utils/timeline'

describe('groupByDay', () => {
  it('buckets by UTC day, newest first, with gps/face counts', () => {
    const files = [
      { id: 'a', name: 'a', createdAt: '2026-03-01T10:00:00Z', gpsLat: 10, faceGroupIds: ['g'] },
      { id: 'b', name: 'b', createdAt: '2026-03-01T12:00:00Z' },
      { id: 'c', name: 'c', createdAt: '2026-03-05T00:00:00Z' },
      { id: 'bad', name: 'bad', createdAt: 'not-a-date' },
    ]
    const buckets = groupByDay(files)
    expect(buckets.map((b) => b.day)).toEqual(['2026-03-05', '2026-03-01'])
    expect(buckets[1].count).toBe(2)
    expect(buckets[1].withGps).toBe(1)
    expect(buckets[1].withFaces).toBe(1)
  })
})

describe('onThisDay', () => {
  it('matches month/day across years', () => {
    const files = [
      { id: 'a', name: 'a', createdAt: '2024-10-11T00:00:00Z' },
      { id: 'b', name: 'b', createdAt: '2026-10-11T00:00:00Z' },
      { id: 'c', name: 'c', createdAt: '2026-10-12T00:00:00Z' },
    ]
    expect(onThisDay(files, 10, 11).map((f) => f.id).sort()).toEqual(['a', 'b'])
  })
})

describe('timelineRange', () => {
  it('returns min/max day or null when nothing parses', () => {
    expect(timelineRange([{ id: 'x', name: 'x', createdAt: 'junk' }])).toBeNull()
    expect(
      timelineRange([
        { id: 'a', name: 'a', createdAt: '2026-01-02T00:00:00Z' },
        { id: 'b', name: 'b', createdAt: '2026-05-01T00:00:00Z' },
      ]),
    ).toEqual({ min: '2026-01-02', max: '2026-05-01' })
  })
})
