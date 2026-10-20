// CyberManju OS — Shell swipe.test (tests/frontend/shell-swipe.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, ../….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import { createSwipeMachine, type SwipePointer } from '../../src/composables/useSwipe'

function ptr(id: number, type: string, x: number, y: number): SwipePointer {
  return { pointerId: id, pointerType: type, clientX: x, clientY: y }
}

function machineWith(capture: Record<string, unknown[]>, opts = {}) {
  const m = createSwipeMachine({
    threshold: 50,
    onSwipe: (d) => void ((capture.swipe ??= []).push(d)),
    onTwoFingerSwipe: (d) => void ((capture.two ??= []).push(d)),
    onThreeFingerSwipe: (d) => void ((capture.three ??= []).push(d)),
    onFourFingerSwipe: (d) => void ((capture.four ??= []).push(d)),
    onPinchStart: (e) => void ((capture.pinchStart ??= []).push(e.scale)),
    onPinchMove: (e) => void ((capture.pinchMove ??= []).push(e.scale)),
    onPinchEnd: (e) => void ((capture.pinchEnd ??= []).push(e.scale)),
    onTap: (t, x, y) => void ((capture.tap ??= []).push([t, x, y])),
    onLongPress: () => void ((capture.long ??= []).push(1)),
    ...opts,
  })
  m.setViewBox(500, 500)
  return m
}

describe('swipe machine (Pointer Events)', () => {
  it('fires a single-finger swipe', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 200, 250))
    m.onPointerMove(ptr(1, 'touch', 290, 252))
    m.onPointerUp(ptr(1, 'touch', 290, 252))
    expect(c.swipe).toEqual(['right'])
  })

  it('fires a single tap with coordinates', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 250, 250))
    m.onPointerUp(ptr(1, 'touch', 251, 249))
    expect(c.tap).toEqual([['single', 251, 249]])
    expect(c.swipe).toBeUndefined()
  })

  it('ignores mouse pointers entirely', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'mouse', 200, 250))
    m.onPointerMove(ptr(1, 'mouse', 400, 250))
    m.onPointerUp(ptr(1, 'mouse', 400, 250))
    expect(c.swipe).toBeUndefined()
    expect(c.tap).toBeUndefined()
  })

  it('treats pen like touch', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(7, 'pen', 300, 250))
    m.onPointerMove(ptr(7, 'pen', 220, 250))
    m.onPointerUp(ptr(7, 'pen', 220, 250))
    expect(c.swipe).toEqual(['left'])
  })

  it('fires a two-finger parallel swipe (the old code made this impossible)', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 300, 200))
    m.onPointerDown(ptr(2, 'touch', 300, 300))
    // Both fingers travel left together; spread stays ~100 the whole way.
    m.onPointerMove(ptr(1, 'touch', 230, 200))
    m.onPointerMove(ptr(2, 'touch', 230, 300))
    m.onPointerUp(ptr(1, 'touch', 230, 200))
    m.onPointerUp(ptr(2, 'touch', 230, 300))
    expect(c.two).toEqual(['left'])
    expect(c.pinchStart).toBeUndefined()
  })

  it('fires pinch start/move/end with scale when the spread changes', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 200, 250))
    m.onPointerDown(ptr(2, 'touch', 300, 250))
    m.onPointerMove(ptr(2, 'touch', 360, 250)) // spread 100 → 160
    m.onPointerUp(ptr(2, 'touch', 360, 250))
    m.onPointerUp(ptr(1, 'touch', 200, 250))
    expect(c.pinchStart).toEqual([1])
    expect((c.pinchMove as number[])[0]).toBeCloseTo(1.6, 5)
    expect((c.pinchEnd as number[])[0]).toBeCloseTo(1.6, 5)
    expect(c.two).toBeUndefined()
  })

  it('rebases the surviving finger after a pinch so it can keep scrolling', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 200, 250))
    m.onPointerDown(ptr(2, 'touch', 300, 250))
    m.onPointerMove(ptr(2, 'touch', 360, 250))
    m.onPointerUp(ptr(2, 'touch', 360, 250)) // pinch ends, finger 1 stays
    m.onPointerMove(ptr(1, 'touch', 120, 250)) // 80px travel from rebase
    m.onPointerUp(ptr(1, 'touch', 120, 250))
    expect(c.pinchEnd).toHaveLength(1)
    expect(c.swipe).toEqual(['left'])
  })

  it('fires three- and four-finger swipes', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 250, 300))
    m.onPointerDown(ptr(2, 'touch', 250, 300))
    m.onPointerDown(ptr(3, 'touch', 250, 300))
    m.onPointerUp(ptr(1, 'touch', 250, 200))
    m.onPointerUp(ptr(2, 'touch', 250, 200))
    m.onPointerUp(ptr(3, 'touch', 250, 200))
    expect(c.three).toEqual(['up'])

    const c2: Record<string, unknown[]> = {}
    const m2 = machineWith(c2)
    m2.onPointerDown(ptr(1, 'touch', 250, 200))
    m2.onPointerDown(ptr(2, 'touch', 250, 200))
    m2.onPointerDown(ptr(3, 'touch', 250, 200))
    m2.onPointerDown(ptr(4, 'touch', 250, 200))
    m2.onPointerUp(ptr(1, 'touch', 250, 320))
    m2.onPointerUp(ptr(2, 'touch', 250, 320))
    m2.onPointerUp(ptr(3, 'touch', 250, 320))
    m2.onPointerUp(ptr(4, 'touch', 250, 320))
    expect(c2.four).toEqual(['down'])
  })

  it('fires nothing on cancel (browser took over — no double-handling)', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 200, 250))
    m.onPointerMove(ptr(1, 'touch', 400, 250))
    m.onPointerCancel(ptr(1, 'touch', 400, 250))
    expect(c.swipe).toBeUndefined()
  })

  it('ignores below-threshold drift', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 250, 250))
    m.onPointerMove(ptr(1, 'touch', 270, 250))
    m.onPointerUp(ptr(1, 'touch', 270, 250))
    expect(c.swipe).toBeUndefined()
    expect(c.tap).toBeUndefined()
  })

  it('prefers pinch when spreading dominates translation (pinch-and-pan)', () => {
    const c: Record<string, unknown[]> = {}
    const m = machineWith(c)
    m.onPointerDown(ptr(1, 'touch', 200, 250))
    m.onPointerDown(ptr(2, 'touch', 300, 250))
    m.onPointerMove(ptr(1, 'touch', 150, 250)) // spread 100 → 150 beats 25px drift
    m.onPointerUp(ptr(1, 'touch', 150, 250))
    m.onPointerUp(ptr(2, 'touch', 300, 250))
    expect(c.pinchEnd).toHaveLength(1)
    expect(c.two).toBeUndefined()
  })
})
