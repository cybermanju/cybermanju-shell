// CyberManju OS — Shell gestures.test (tests/frontend/shell-gestures.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, ../….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import { useTouchConfig } from '../../src/composables/useTouchConfig'

describe('shell gesture defaults (niri strip + overview)', () => {
  it('maps three fingers up/down to the all-screens overview', () => {
    const touch = useTouchConfig()
    expect(touch.getAction('three_finger_up')).toBe('overview_toggle')
    expect(touch.getAction('three_finger_down')).toBe('overview_toggle')
  })

  it('maps two fingers to strip travel: columns ↔, lines ↕', () => {
    const touch = useTouchConfig()
    expect(touch.getAction('two_finger_left')).toBe('strip_left')
    expect(touch.getAction('two_finger_right')).toBe('strip_right')
    expect(touch.getAction('two_finger_up')).toBe('strip_up')
    expect(touch.getAction('two_finger_down')).toBe('strip_down')
  })

  it('exposes every strip/layout action for rebinding in Settings', () => {
    const touch = useTouchConfig()
    const actions = touch.getAllActions()
    for (const a of [
      'overview_toggle',
      'strip_left',
      'strip_right',
      'strip_up',
      'strip_down',
      'strip_direction',
      'autotile_toggle',
      'focus_next',
      'focus_prev',
      'close_all_windows',
      'layout_floating',
      'layout_tiled',
      'layout_strip',
      'layout_overview',
    ] as const) {
      expect(actions).toContain(a)
      expect(touch.getActionLabel(a).length).toBeGreaterThan(0)
    }
  })

  it('dispatches two/three-finger swipes to the mapped strip/overview actions', () => {
    const touch = useTouchConfig()
    const seen: string[] = []
    touch.onAction((action) => { seen.push(action) })
    const opts = touch.getSwipeOptions()
    opts.onTwoFingerSwipe?.('left')
    opts.onTwoFingerSwipe?.('right')
    opts.onTwoFingerSwipe?.('up')
    opts.onTwoFingerSwipe?.('down')
    opts.onThreeFingerSwipe?.('up')
    expect(seen).toEqual([
      'strip_left',
      'strip_right',
      'strip_up',
      'strip_down',
      'overview_toggle',
    ])
  })

  it('keeps single-finger navigation on history by default', () => {
    const touch = useTouchConfig()
    expect(touch.getAction('swipe_left')).toBe('go_back')
    expect(touch.getAction('swipe_right')).toBe('go_forward')
  })
})
