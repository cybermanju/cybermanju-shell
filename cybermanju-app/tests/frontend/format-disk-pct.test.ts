// CyberManju OS — Disk percentage formatting (tests/frontend/format-disk-pct.test.ts).
// Role: keep storage progress widths finite and bounded across transports.
// Exports: none.
// Integrates: shared format utility, Vitest.
// Contracts: diskPct always returns a finite number in the inclusive 0–100 range.
import { describe, expect, it } from 'vitest'
import { diskPct } from '@/utils/format'

describe('diskPct', () => {
  it.each([
    ['empty volume', 0, 100, 0],
    ['half full', 50, 100, 50],
    ['fractional percentage', 37.5, 100, 37.5],
    ['full volume', 100, 100, 100],
    ['over capacity', 150, 100, 100],
    ['negative usage', -10, 100, 0],
    ['negative usage and capacity', -10, -100, 0],
  ] as const)('handles %s', (_label, used, capacity, expected) => {
    expect(diskPct(used, capacity)).toBe(expected)
  })

  it.each([0, -1, NaN, Infinity, -Infinity, null, undefined])(
    'returns zero for invalid capacity %s', (capacity) => {
      expect(diskPct(50, capacity)).toBe(0)
    },
  )

  it.each([NaN, Infinity, -Infinity, null, undefined])(
    'returns zero for missing or non-finite usage %s', (used) => {
      expect(diskPct(used, 100)).toBe(0)
    },
  )

  it('saturates at 100 when finite operands overflow during division', () => {
    expect(diskPct(Number.MAX_VALUE, Number.MIN_VALUE)).toBe(100)
  })

  it('keeps representative telemetry combinations safe for CSS widths', () => {
    const values: Array<number | null | undefined> = [
      null, undefined, NaN, -Infinity, Infinity, -Number.MAX_VALUE,
      -100, -0, 0, Number.MIN_VALUE, 0.5, 100, Number.MAX_VALUE,
    ]
    for (const used of values) {
      for (const capacity of values) {
        const percentage = diskPct(used, capacity)
        expect(Number.isFinite(percentage)).toBe(true)
        expect(percentage).toBeGreaterThanOrEqual(0)
        expect(percentage).toBeLessThanOrEqual(100)
      }
    }
  })
})
