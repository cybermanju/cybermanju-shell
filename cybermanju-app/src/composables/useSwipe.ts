// CyberManju OS — UseSwipe (src/composables/useSwipe.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: SwipeDirection, TapType, EdgeZone, PinchEvent, LongPressEvent, EdgeSwipeEvent, SwipeOptions, SwipePointer.
// Integrates: vue, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { onMounted, onUnmounted, type Ref } from 'vue'
import { swipeDirection, classifyTwoFingerGesture } from '@/utils/shellLayout'

export type SwipeDirection = 'left' | 'right' | 'up' | 'down'
export type TapType = 'single' | 'double'
export type EdgeZone = 'top' | 'bottom' | 'left' | 'right' | 'none'

export interface PinchEvent {
  scale: number
  initialDistance: number
  currentDistance: number
}

export interface LongPressEvent {
  x: number
  y: number
  duration: number
}

export interface EdgeSwipeEvent {
  direction: SwipeDirection
  edge: EdgeZone
  distance: number
}

export interface SwipeOptions {
  threshold?: number
  longPressThreshold?: number
  edgeZoneSize?: number
  doubleTapTimeout?: number

  onSwipe?: (direction: SwipeDirection) => void
  onTwoFingerSwipe?: (direction: SwipeDirection) => void
  onThreeFingerSwipe?: (direction: SwipeDirection) => void
  onFourFingerSwipe?: (direction: SwipeDirection) => void
  onPinchStart?: (e: PinchEvent) => void
  onPinchMove?: (e: PinchEvent) => void
  onPinchEnd?: (e: PinchEvent) => void
  onTap?: (type: TapType, x: number, y: number) => void
  onLongPress?: (e: LongPressEvent) => void
  onEdgeSwipe?: (e: EdgeSwipeEvent) => void
}

function getEdge(x: number, y: number, w: number, h: number, zoneSize: number): EdgeZone {
  if (x <= zoneSize) return 'left'
  if (x >= w - zoneSize) return 'right'
  if (y <= zoneSize) return 'top'
  if (y >= h - zoneSize) return 'bottom'
  return 'none'
}

/** Minimal pointer shape the machine needs — real PointerEvents satisfy it. */
export interface SwipePointer {
  pointerId: number
  pointerType: string
  clientX: number
  clientY: number
}

interface Tracked {
  startX: number
  startY: number
  lastX: number
  lastY: number
}

/**
 * Framework-free gesture state machine, driven by Pointer Events.
 *
 * One model for mouse/pen/touch (mouse is ignored — desktop drags belong
 * to the windows), `pointerId`-keyed so fingers can join/leave mid-gesture.
 * Two fingers are classified by the shared `classifyTwoFingerGesture`:
 * spread change → pinch, parallel travel → swipe. Extracted from the
 * composable so the whole recognizer is unit-testable without DOM.
 */
export function createSwipeMachine(options: SwipeOptions = {}) {
  const threshold = options.threshold ?? 50
  const longPressThreshold = options.longPressThreshold ?? 600
  const edgeZoneSize = options.edgeZoneSize ?? 30
  const doubleTapTimeout = options.doubleTapTimeout ?? 300

  /** Two-finger spread change (px) that counts as a pinch, not a swipe. */
  const PINCH_DEADZONE = 20

  const tracked = new Map<number, Tracked>()
  /** Final states of lifted pointers in this gesture (for centroid math). */
  const lifted: Tracked[] = []
  /** Peak concurrent touch-like pointers in this gesture. */
  let peakCount = 0
  let tracking = false
  let pinchFired = false
  let initialPinchDist = 0
  let lastPinchDist = 0
  let startTime = 0
  let edgeAtStart: EdgeZone = 'none'
  let longPressTimer: ReturnType<typeof setTimeout> | null = null
  let lastTapTime = 0
  let lastTapX = 0
  let lastTapY = 0
  /** Viewport box of the gesture surface, for edge detection. */
  let viewBox = { w: 0, h: 0 }

  function setViewBox(w: number, h: number) {
    viewBox = { w, h }
  }

  function clearTimer() {
    if (longPressTimer) { clearTimeout(longPressTimer); longPressTimer = null }
  }

  function reset() {
    tracked.clear()
    peakCount = 0
    tracking = false
    pinchFired = false
    clearTimer()
  }

  function spread(): number {
    const pts = [...tracked.values()]
    if (pts.length < 2) return 0
    const dx = pts[0].lastX - pts[1].lastX
    const dy = pts[0].lastY - pts[1].lastY
    return Math.sqrt(dx * dx + dy * dy)
  }

  /** Live centroid travel of the tracked pointers vs their starts. */
  function centroidTravel(): number {
    const pts = [...tracked.values()]
    if (pts.length === 0) return 0
    let dx = 0
    let dy = 0
    for (const p of pts) {
      dx += p.lastX - p.startX
      dy += p.lastY - p.startY
    }
    return Math.max(Math.abs(dx / pts.length), Math.abs(dy / pts.length))
  }

  function isTouchLike(p: SwipePointer): boolean {
    return p.pointerType === 'touch' || p.pointerType === 'pen'
  }

  function onPointerDown(e: SwipePointer) {
    if (!isTouchLike(e)) return
    const first = tracked.size === 0
    tracked.set(e.pointerId, { startX: e.clientX, startY: e.clientY, lastX: e.clientX, lastY: e.clientY })
    peakCount = Math.max(peakCount, tracked.size)
    if (first) {
      startTime = Date.now()
      edgeAtStart = getEdge(e.clientX, e.clientY, viewBox.w, viewBox.h, edgeZoneSize)
      tracking = true
      pinchFired = false
      lifted.length = 0
      longPressTimer = setTimeout(() => {
        if (tracking && tracked.size === 1) {
          options.onLongPress?.({ x: e.clientX, y: e.clientY, duration: longPressThreshold })
        }
      }, longPressThreshold)
    } else {
      // A second (or third…) finger joined: long-press is off the table,
      // but the gesture continues — peakCount remembers the widest chord.
      clearTimer()
      tracking = true
      if (tracked.size === 2) {
        initialPinchDist = spread()
        lastPinchDist = initialPinchDist
      }
    }
  }

  function onPointerMove(e: SwipePointer) {
    const t = tracked.get(e.pointerId)
    if (!t || !tracking || pinchFired) {
      // Still feed pinch moves once pinching started.
      if (pinchFired && t && tracked.size === 2) {
        t.lastX = e.clientX
        t.lastY = e.clientY
        const cur = spread()
        lastPinchDist = cur
        options.onPinchMove?.({ scale: cur / initialPinchDist, initialDistance: initialPinchDist, currentDistance: cur })
      }
      return
    }
    t.lastX = e.clientX
    t.lastY = e.clientY
    if (tracked.size === 2) {
      const cur = spread()
      lastPinchDist = cur
      const spreadDelta = Math.abs(cur - initialPinchDist)
      // Pinch only when the spread change DOMINATES the translation:
      // during a parallel two-finger swipe the leading finger moves first,
      // which transiently stretches the spread — without this guard every
      // fast swipe would misfire as a pinch.
      if (spreadDelta > PINCH_DEADZONE && spreadDelta > centroidTravel()) {
        // Spread is changing: pinch, not a two-finger swipe.
        pinchFired = true
        tracking = false
        clearTimer()
        options.onPinchStart?.({ scale: 1, initialDistance: initialPinchDist, currentDistance: initialPinchDist })
        options.onPinchMove?.({ scale: cur / initialPinchDist, initialDistance: initialPinchDist, currentDistance: cur })
        return
      }
      return
    }
    if (tracked.size === 1 && longPressTimer) {
      if (Math.abs(t.lastX - t.startX) > 10 || Math.abs(t.lastY - t.startY) > 10) {
        clearTimer()
      }
    }
  }

  function fireSwipe(count: number, dx: number, dy: number, endX: number, endY: number) {
    const absDx = Math.abs(dx)
    const absDy = Math.abs(dy)
    if (count === 1) {
      const elapsed = Date.now() - startTime
      if (absDx < 10 && absDy < 10 && elapsed < 300) {
        const now = Date.now()
        if (now - lastTapTime < doubleTapTimeout && Math.abs(endX - lastTapX) < 30 && Math.abs(endY - lastTapY) < 30) {
          options.onTap?.('double', endX, endY)
          lastTapTime = 0
        } else {
          options.onTap?.('single', endX, endY)
          lastTapTime = now
          lastTapX = endX
          lastTapY = endY
        }
        return
      }
      if (Math.max(absDx, absDy) < threshold) return
      emitDirection(1, swipeDirection(dx, dy), Math.max(absDx, absDy))
      return
    }
    if (count === 2) {
      const cls = classifyTwoFingerGesture({
        startDist: initialPinchDist,
        endDist: lastPinchDist,
        dx,
        dy,
        swipeThreshold: threshold,
      })
      if (cls === 'pinch') {
        options.onPinchEnd?.({ scale: lastPinchDist / initialPinchDist, initialDistance: initialPinchDist, currentDistance: lastPinchDist })
        return
      }
      if (cls === 'tap' || cls === 'none') return
      emitDirection(2, cls.replace('swipe-', '') as SwipeDirection, Math.max(absDx, absDy))
      return
    }
    if (count === 3 || count === 4) {
      if (Math.max(absDx, absDy) < threshold) return
      emitDirection(count, swipeDirection(dx, dy), Math.max(absDx, absDy))
    }
  }

  function emitDirection(count: number, direction: SwipeDirection, distance: number) {
    if (edgeAtStart !== 'none') {
      options.onEdgeSwipe?.({ direction, edge: edgeAtStart, distance })
    }
    switch (count) {
      case 4: options.onFourFingerSwipe?.(direction); break
      case 3: options.onThreeFingerSwipe?.(direction); break
      case 2: options.onTwoFingerSwipe?.(direction); break
      default: options.onSwipe?.(direction); break
    }
  }

  function finishPointer(e: SwipePointer, cancelled: boolean) {
    const t = tracked.get(e.pointerId)
    if (!t) return
    t.lastX = e.clientX
    t.lastY = e.clientY
    tracked.delete(e.pointerId)
    if (pinchFired) {
      if (tracked.size < 2) {
        pinchFired = false
        if (!cancelled) {
          options.onPinchEnd?.({ scale: lastPinchDist / initialPinchDist, initialDistance: initialPinchDist, currentDistance: lastPinchDist })
        }
        lifted.length = 0
        if (tracked.size === 0) {
          tracking = false
          peakCount = 0
        } else {
          // One finger stayed down after the pinch: rebase it as a fresh
          // single-finger gesture so the user can keep scrolling.
          const rest = [...tracked.values()][0]
          rest.startX = rest.lastX
          rest.startY = rest.lastY
          peakCount = 1
          startTime = Date.now()
          tracking = true
        }
        clearTimer()
      }
      return
    }
    lifted.push(t)
    if (!tracking) {
      if (tracked.size === 0) {
        peakCount = 0
        lifted.length = 0
      }
      return
    }
    if (tracked.size > 0) return // fingers still down — classify at lift-off
    // Last finger lifted: classify the whole chord from its centroid.
    tracking = false
    if (cancelled) {
      // The browser took over (scroll/zoom) — never double-handle.
      peakCount = 0
      lifted.length = 0
      clearTimer()
      return
    }
    let dx = 0
    let dy = 0
    for (const p of lifted) {
      dx += p.lastX - p.startX
      dy += p.lastY - p.startY
    }
    dx /= lifted.length
    dy /= lifted.length
    const count = peakCount
    lifted.length = 0
    peakCount = 0
    clearTimer()
    fireSwipe(count, dx, dy, e.clientX, e.clientY)
  }

  return {
    setViewBox,
    onPointerDown,
    onPointerMove,
    onPointerUp: (e: SwipePointer) => finishPointer(e, false),
    onPointerCancel: (e: SwipePointer) => finishPointer(e, true),
    reset,
    /** Peak chord width of the current/last gesture (for tests). */
    peakCount: () => peakCount,
  }
}

export function useSwipe(
  elementRef: Ref<HTMLElement | null>,
  options: SwipeOptions = {}
) {
  const machine = createSwipeMachine(options)

  function refreshViewBox() {
    const el = elementRef.value
    if (el) machine.setViewBox(el.clientWidth, el.clientHeight)
  }

  function handlePointerDown(e: PointerEvent) {
    refreshViewBox()
    machine.onPointerDown(e)
  }
  function handlePointerMove(e: PointerEvent) {
    machine.onPointerMove(e)
  }
  function handlePointerUp(e: PointerEvent) {
    machine.onPointerUp(e)
  }
  function handlePointerCancel(e: PointerEvent) {
    machine.onPointerCancel(e)
  }

  onMounted(() => {
    const el = elementRef.value
    if (el) {
      el.addEventListener('pointerdown', handlePointerDown)
      el.addEventListener('pointermove', handlePointerMove)
      el.addEventListener('pointerup', handlePointerUp)
      el.addEventListener('pointercancel', handlePointerCancel)
    }
  })

  onUnmounted(() => {
    const el = elementRef.value
    if (el) {
      el.removeEventListener('pointerdown', handlePointerDown)
      el.removeEventListener('pointermove', handlePointerMove)
      el.removeEventListener('pointerup', handlePointerUp)
      el.removeEventListener('pointercancel', handlePointerCancel)
    }
    machine.reset()
  })
}
