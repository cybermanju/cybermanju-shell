// CyberManju OS — Shell layout.test (tests/frontend/shell-layout.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  swipeDirection,
  computeTileRects,
  stripColumnWidth,
  computeStripRects,
  clampStripOffset,
  resolveStripLine,
  classifyTwoFingerGesture,
} from '../../src/utils/shellLayout'

describe('swipeDirection', () => {
  it('picks the dominant axis', () => {
    expect(swipeDirection(80, 10)).toBe('right')
    expect(swipeDirection(-80, 10)).toBe('left')
    expect(swipeDirection(10, 80)).toBe('down')
    expect(swipeDirection(10, -80)).toBe('up')
  })
  it('prefers horizontal on exact ties', () => {
    expect(swipeDirection(50, 50)).toBe('right')
    expect(swipeDirection(-50, 50)).toBe('left')
  })
})

describe('computeTileRects', () => {
  it('tiles a single window with uniform margins', () => {
    const [r] = computeTileRects(1, 1280, 800)
    expect(r).toEqual({ x: 10, y: 10, width: 1260, height: 780 })
  })
  it('splits two windows into two columns', () => {
    const rects = computeTileRects(2, 1280, 800)
    expect(rects).toHaveLength(2)
    // 2 cols × 1 row: cw = (1280-30)/2 = 625, ch = 800-20 = 780
    expect(rects[0]).toEqual({ x: 10, y: 10, width: 625, height: 780 })
    expect(rects[1].x).toBe(10 + 625 + 10)
    expect(rects[1].y).toBe(10)
  })
  it('lays four windows in a 2×2 grid with no overlaps', () => {
    const rects = computeTileRects(4, 1280, 800)
    expect(rects).toHaveLength(4)
    for (let i = 0; i < rects.length; i++) {
      for (let j = i + 1; j < rects.length; j++) {
        const a = rects[i]
        const b = rects[j]
        const overlaps =
          a.x < b.x + b.width && b.x < a.x + a.width &&
          a.y < b.y + b.height && b.y < a.y + a.height
        expect(overlaps).toBe(false)
      }
    }
    // Grid fills left→right, top→bottom
    expect(rects[0].x).toBeLessThan(rects[1].x)
    expect(rects[0].y).toBeLessThan(rects[2].y)
  })
  it('returns nothing for zero windows', () => {
    expect(computeTileRects(0, 1280, 800)).toEqual([])
  })
})

describe('strip geometry (niri: columns never shrink)', () => {
  it('clamps the column width to a usable range', () => {
    expect(stripColumnWidth(1280)).toBe(Math.floor(1280 * 0.55))
    expect(stripColumnWidth(400)).toBe(380)
    expect(stripColumnWidth(4000)).toBe(760)
  })
  it('appends horizontal columns right with constant size', () => {
    const rects = computeStripRects(3, 1280, 800, 12, false)
    expect(rects).toHaveLength(3)
    const colW = stripColumnWidth(1280)
    rects.forEach((r, i) => {
      expect(r.width).toBe(colW)
      expect(r.height).toBe(800 - 24)
      expect(r.x).toBe(12 + i * (colW + 12))
      expect(r.y).toBe(12)
    })
  })
  it('stacks vertical columns downward with full width', () => {
    const rects = computeStripRects(2, 1280, 800, 12, true)
    expect(rects[0]).toEqual({ x: 12, y: 12, width: 1280 - 24, height: 800 - 24 })
    expect(rects[1].y).toBe(12 + (800 - 24) + 12)
    expect(rects[1].x).toBe(12)
  })
})

describe('clampStripOffset', () => {
  it('keeps the viewport on a live column', () => {
    expect(clampStripOffset(0, 3)).toBe(0)
    expect(clampStripOffset(5, 3)).toBe(2)
    expect(clampStripOffset(-2, 3)).toBe(0)
    expect(clampStripOffset(0, 0)).toBe(0)
  })
})

describe('resolveStripLine (niri dynamic workspaces)', () => {
  it('moves between occupied lines', () => {
    expect(resolveStripLine([0, 1, 2], 1, 1)).toBe(2)
    expect(resolveStripLine([0, 1, 2], 1, -1)).toBe(0)
  })
  it('clamps at the top', () => {
    expect(resolveStripLine([0, 1], 0, -1)).toBe(0)
    expect(resolveStripLine([0, 1], 0, -5)).toBe(0)
  })
  it('always keeps one empty line below the lowest occupied one', () => {
    // Moving down from the last occupied line lands on a fresh empty line…
    expect(resolveStripLine([0, 1], 1, 1)).toBe(2)
    // …but there is only ever ONE empty line — going further stays put.
    expect(resolveStripLine([0, 1], 2, 1)).toBe(2)
    // And coming back up returns to the occupied line.
    expect(resolveStripLine([0, 1], 2, -1)).toBe(1)
  })
  it('starts at line 0 when nothing is open', () => {
    expect(resolveStripLine([], 0, 1)).toBe(0)
    expect(resolveStripLine([], 0, -1)).toBe(0)
  })
})

describe('classifyTwoFingerGesture', () => {
  const base = { startDist: 100, swipeThreshold: 50 }
  it('detects pinches in both directions', () => {
    expect(classifyTwoFingerGesture({ ...base, endDist: 140, dx: 0, dy: 0 })).toBe('pinch')
    expect(classifyTwoFingerGesture({ ...base, endDist: 60, dx: 0, dy: 0 })).toBe('pinch')
  })
  it('detects parallel swipes on all four axes', () => {
    expect(classifyTwoFingerGesture({ ...base, endDist: 105, dx: -80, dy: 5 })).toBe('swipe-left')
    expect(classifyTwoFingerGesture({ ...base, endDist: 95, dx: 80, dy: -5 })).toBe('swipe-right')
    expect(classifyTwoFingerGesture({ ...base, endDist: 100, dx: 5, dy: -80 })).toBe('swipe-up')
    expect(classifyTwoFingerGesture({ ...base, endDist: 100, dx: -5, dy: 80 })).toBe('swipe-down')
  })
  it('prefers pinch when spread AND travel change together', () => {
    expect(classifyTwoFingerGesture({ ...base, endDist: 160, dx: 120, dy: 0 })).toBe('pinch')
  })
  it('reports taps for near-stationary touches', () => {
    expect(classifyTwoFingerGesture({ ...base, endDist: 102, dx: 3, dy: -4 })).toBe('tap')
  })
  it('reports none for in-between drift', () => {
    // Moved, but neither a pinch nor far enough to swipe.
    expect(classifyTwoFingerGesture({ ...base, endDist: 100, dx: 30, dy: 0 })).toBe('none')
  })
  it('honours custom thresholds', () => {
    expect(
      classifyTwoFingerGesture({ ...base, endDist: 100, dx: 30, dy: 0, swipeThreshold: 20 }),
    ).toBe('swipe-right')
    expect(
      classifyTwoFingerGesture({ ...base, endDist: 112, dx: 0, dy: 0, pinchThreshold: 20 }),
    ).toBe('tap')
  })
})
