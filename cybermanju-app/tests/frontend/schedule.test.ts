// CyberManju OS — Schedule.test (tests/frontend/schedule.test.ts).
// Schedule expressions — TypeScript twin of `crates/os/src/schedule.rs`.
// Every case here has a Rust twin test in that file; a divergence means the
// daemon (UTC) and the browser preview/tick disagree by a minute.
import { describe, expect, it } from 'vitest'
import {
  describeSchedule,
  formatIn,
  nextFire,
  parseSchedule,
  previewSchedule,
  scheduleNextAfter,
} from '@/utils/schedule'

const at = (y: number, mo: number, d: number, h: number, mi: number) =>
  new Date(Date.UTC(y, mo - 1, d, h, mi, 0, 0))

describe('schedule: parse', () => {
  it('parses classic 5-field cron', () => {
    const s = parseSchedule('* * * * *')
    if (s.kind !== 'cron') throw new Error('expected cron')
    expect(s.m).toHaveLength(60) // 0-59
    expect(s.h).toHaveLength(24) // 0-23
    expect(s.dom).toHaveLength(31) // 1-31
    expect(s.mon).toHaveLength(12) // 1-12
    expect(s.dow).toEqual([0, 1, 2, 3, 4, 5, 6]) // 7 folded to 0
  })

  it('expands steps, ranges and lists', () => {
    const s = parseSchedule('*/15 9-17 1,15 * 1-5')
    if (s.kind !== 'cron') throw new Error('expected cron')
    expect(s.m).toEqual([0, 15, 30, 45])
    expect(s.h).toEqual([9, 10, 11, 12, 13, 14, 15, 16, 17])
    expect(s.dom).toEqual([1, 15])
    expect(s.dow).toEqual([1, 2, 3, 4, 5])
  })

  it('normalizes day-of-week 7 to Sunday (0)', () => {
    const s = parseSchedule('0 0 * * 7')
    if (s.kind !== 'cron') throw new Error('expected cron')
    expect(s.dow).toEqual([0])
  })

  it('parses aliases', () => {
    expect(describeSchedule(parseSchedule('@hourly'))).toBe('0 * * * *')
    expect(describeSchedule(parseSchedule('@daily'))).toBe('0 0 * * *')
    expect(describeSchedule(parseSchedule('@weekly'))).toBe('0 0 * * 0')
    expect(describeSchedule(parseSchedule('@yearly'))).toBe('0 0 1 1 *')
  })

  it('parses intervals', () => {
    expect(parseSchedule('every 10m')).toEqual({ kind: 'every', n: 10, unit: 'm' })
    expect(parseSchedule('  EVERY 2H ')).toEqual({ kind: 'every', n: 2, unit: 'h' })
    expect(parseSchedule('every 30s')).toEqual({ kind: 'every', n: 30, unit: 's' })
    expect(parseSchedule('every 1d')).toEqual({ kind: 'every', n: 1, unit: 'd' })
  })

  it('rejects bad expressions with the invalid: prefix', () => {
    for (const bad of ['', '* * *', '61 * * * *', 'every 0m', 'every 5x', '*/0 * * * *', '5-1 * * * *']) {
      expect(() => parseSchedule(bad), bad).toThrow(/^invalid:/)
    }
  })
})

describe('schedule: next fire', () => {
  it('fires every minute', () => {
    const spec = parseSchedule('* * * * *')
    expect(scheduleNextAfter(spec, at(2026, 1, 1, 12, 0))).toEqual(at(2026, 1, 1, 12, 1))
  })

  it('skips past today for a daily time', () => {
    const spec = parseSchedule('30 2 * * *')
    expect(scheduleNextAfter(spec, at(2026, 1, 1, 3, 0))).toEqual(at(2026, 1, 2, 2, 30))
  })

  it('honours minute steps', () => {
    const spec = parseSchedule('*/15 * * * *')
    expect(scheduleNextAfter(spec, at(2026, 1, 1, 12, 1))).toEqual(at(2026, 1, 1, 12, 15))
  })

  it('lands on the next weekday at 09:00', () => {
    const spec = parseSchedule('0 9-17 * * 1-5')
    const next = scheduleNextAfter(spec, at(2026, 1, 3, 12, 0)) // Saturday
    expect(next).toEqual(at(2026, 1, 5, 9, 0)) // Monday
    expect(next?.getUTCDay()).toBe(1)
  })

  it('fires on Sunday for dow 7', () => {
    const next = nextFire('0 0 * * 7', at(2026, 1, 1, 0, 0))
    expect(next).toEqual(at(2026, 1, 4, 0, 0)) // Jan 4 2026 is a Sunday
  })

  it('supports aliases and intervals', () => {
    expect(nextFire('@daily', at(2026, 1, 1, 6, 0))).toEqual(at(2026, 1, 2, 0, 0))
    expect(nextFire('every 10m', at(2026, 1, 1, 12, 0))).toEqual(at(2026, 1, 1, 12, 10))
    expect(nextFire('every 2h', at(2026, 1, 1, 12, 0))).toEqual(at(2026, 1, 1, 14, 0))
  })

  it('returns null for an impossible date', () => {
    expect(nextFire('0 0 30 2 *', at(2026, 1, 1, 0, 0))).toBeNull()
  })

  it('returns null (never throws) for a bad expression', () => {
    expect(nextFire('nope', at(2026, 1, 1, 0, 0))).toBeNull()
  })

  it('combines dom and dow with OR when both are restricted', () => {
    // Fires on the 1st OR on Sundays — Jan 4 2026 is a Sunday and comes
    // before Feb 1.
    const next = nextFire('0 0 1 * 0', at(2026, 1, 1, 1, 0))
    expect(next).toEqual(at(2026, 1, 4, 0, 0))
    const after = nextFire('0 0 1 * 0', at(2026, 1, 4, 1, 0))
    expect(after).toEqual(at(2026, 1, 11, 0, 0))
  })
})

describe('schedule: preview helpers', () => {
  it('round-trips the described expression', () => {
    expect(describeSchedule(parseSchedule('every 10m'))).toBe('every 10m')
    expect(describeSchedule(parseSchedule('30 2 * * *'))).toBe('30 2 * * *')
    expect(describeSchedule(parseSchedule('@hourly'))).toBe('0 * * * *')
  })

  it('previews a valid expression with a countdown', () => {
    const now = at(2026, 1, 1, 12, 0)
    const p = previewSchedule('* * * * *', now)
    expect(p.ok).toBe(true)
    expect(p.label).toBe('* * * * *')
    expect(p.next).toEqual(at(2026, 1, 1, 12, 1))
    expect(p.in).toBe('in 1m')
  })

  it('previews errors honestly', () => {
    const p = previewSchedule('61 * * * *', at(2026, 1, 1, 12, 0))
    expect(p.ok).toBe(false)
    expect(p.error).toMatch(/^invalid:/)
  })

  it('formats countdowns', () => {
    expect(formatIn(-1)).toBe('now')
    expect(formatIn(0)).toBe('now')
    expect(formatIn(45_000)).toBe('in 45s')
    expect(formatIn(4 * 60_000)).toBe('in 4m')
    expect(formatIn(2 * 3_600_000 + 5 * 60_000)).toBe('in 2h 5m')
    expect(formatIn(3 * 86_400_000 + 4 * 3_600_000)).toBe('in 3d 4h')
  })
})
