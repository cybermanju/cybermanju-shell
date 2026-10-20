// CyberManju OS — UseTouchConfig (src/composables/useTouchConfig.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: GestureType, TouchAction, TouchConfigOptions, useTouchConfig.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { reactive, watch, readonly } from 'vue'

export type GestureType =
  | 'swipe_left' | 'swipe_right' | 'swipe_up' | 'swipe_down'
  | 'two_finger_left' | 'two_finger_right' | 'two_finger_up' | 'two_finger_down'
  | 'three_finger_left' | 'three_finger_right' | 'three_finger_up' | 'three_finger_down'
  | 'four_finger_left' | 'four_finger_right' | 'four_finger_up' | 'four_finger_down'
  | 'pinch_in' | 'pinch_out'
  | 'tap_single' | 'tap_double'
  | 'long_press'
  | 'edge_swipe_left' | 'edge_swipe_right' | 'edge_swipe_up' | 'edge_swipe_down'

export type TouchAction =
  | 'toggle_sidebar'
  | 'toggle_palette'
  | 'toggle_help'
  | 'focus_search'
  | 'go_back'
  | 'go_forward'
  | 'go_home'
  | 'prev_panel'
  | 'next_panel'
  | 'open_trash'
  | 'open_activity'
  | 'open_collections'
  | 'open_faces'
  | 'open_map'
  | 'open_code'
  | 'open_settings'
  | 'open_storage'
  | 'scroll_up'
  | 'scroll_down'
  | 'zoom_in'
  | 'zoom_out'
  | 'context_menu'
  | 'select_item'
  | 'toggle_fullscreen'
  | 'new_folder'
  | 'refresh'
  | 'escape'
  | 'overview_toggle'
  | 'strip_left'
  | 'strip_right'
  | 'strip_up'
  | 'strip_down'
  | 'strip_direction'
  | 'autotile_toggle'
  | 'focus_next'
  | 'focus_prev'
  | 'close_all_windows'
  | 'layout_floating'
  | 'layout_tiled'
  | 'layout_strip'
  | 'layout_overview'
  | 'none'

const DEFAULT_GESTURE_MAP: Record<GestureType, TouchAction> = {
  swipe_left: 'go_back',
  swipe_right: 'go_forward',
  swipe_up: 'scroll_up',
  swipe_down: 'scroll_down',
  // Two fingers drive the niri-style strip: left/right travel along the
  // columns of the current line, up/down switches between lines.
  two_finger_left: 'strip_left',
  two_finger_right: 'strip_right',
  two_finger_up: 'strip_up',
  two_finger_down: 'strip_down',
  three_finger_left: 'prev_panel',
  three_finger_right: 'next_panel',
  // Three fingers up zooms out to every screen (overview); swiping back
  // down dismisses it.
  three_finger_up: 'overview_toggle',
  three_finger_down: 'overview_toggle',
  four_finger_left: 'escape',
  four_finger_right: 'toggle_fullscreen',
  four_finger_up: 'open_settings',
  four_finger_down: 'go_home',
  pinch_in: 'zoom_out',
  pinch_out: 'zoom_in',
  tap_single: 'select_item',
  tap_double: 'none',
  long_press: 'context_menu',
  edge_swipe_left: 'go_back',
  edge_swipe_right: 'go_forward',
  edge_swipe_up: 'toggle_sidebar',
  edge_swipe_down: 'toggle_sidebar',
}

const GESTURE_LABELS: Record<GestureType, string> = {
  swipe_left: 'SWIPE LEFT (1 FINGER)',
  swipe_right: 'SWIPE RIGHT (1 FINGER)',
  swipe_up: 'SWIPE UP (1 FINGER)',
  swipe_down: 'SWIPE DOWN (1 FINGER)',
  two_finger_left: '2-FINGER SWIPE LEFT',
  two_finger_right: '2-FINGER SWIPE RIGHT',
  two_finger_up: '2-FINGER SWIPE UP',
  two_finger_down: '2-FINGER SWIPE DOWN',
  three_finger_left: '3-FINGER SWIPE LEFT',
  three_finger_right: '3-FINGER SWIPE RIGHT',
  three_finger_up: '3-FINGER SWIPE UP',
  three_finger_down: '3-FINGER SWIPE DOWN',
  four_finger_left: '4-FINGER SWIPE LEFT',
  four_finger_right: '4-FINGER SWIPE RIGHT',
  four_finger_up: '4-FINGER SWIPE UP',
  four_finger_down: '4-FINGER SWIPE DOWN',
  pinch_in: 'PINCH IN',
  pinch_out: 'PINCH OUT',
  tap_single: 'SINGLE TAP',
  tap_double: 'DOUBLE TAP',
  long_press: 'LONG PRESS',
  edge_swipe_left: 'EDGE SWIPE LEFT',
  edge_swipe_right: 'EDGE SWIPE RIGHT',
  edge_swipe_up: 'EDGE SWIPE UP',
  edge_swipe_down: 'EDGE SWIPE DOWN',
}

const ACTION_LABELS: Record<TouchAction, string> = {
  toggle_sidebar: 'Toggle Sidebar',
  toggle_palette: 'Command Palette',
  toggle_help: 'Shortcuts Help',
  focus_search: 'Focus Search',
  go_back: 'Go Back',
  go_forward: 'Go Forward',
  go_home: 'Go Home',
  prev_panel: 'Previous Panel',
  next_panel: 'Next Panel',
  open_trash: 'Open Trash',
  open_activity: 'Open Activity',
  open_collections: 'Open Collections',
  open_faces: 'Open Faces',
  open_map: 'Open Map',
  open_code: 'Open Code',
  open_settings: 'Open Settings',
  open_storage: 'Open Storage',
  scroll_up: 'Scroll Up',
  scroll_down: 'Scroll Down',
  zoom_in: 'Zoom In',
  zoom_out: 'Zoom Out',
  context_menu: 'Context Menu',
  select_item: 'Select Item',
  toggle_fullscreen: 'Toggle Fullscreen',
  new_folder: 'New Folder',
  refresh: 'Refresh',
  escape: 'Escape / Close',
  overview_toggle: 'Overview (all screens)',
  strip_left: 'Strip: column left',
  strip_right: 'Strip: column right',
  strip_up: 'Strip: line up',
  strip_down: 'Strip: line down',
  strip_direction: 'Strip: flip ↔/↕ axis',
  autotile_toggle: 'Toggle Autotile',
  focus_next: 'Focus Next Window',
  focus_prev: 'Focus Previous Window',
  close_all_windows: 'Close All Windows',
  layout_floating: 'Layout: Floating',
  layout_tiled: 'Layout: Tiled',
  layout_strip: 'Layout: Strip ∞',
  layout_overview: 'Layout: Overview',
  none: 'No Action',
}

export interface TouchConfigOptions {
  storageKey?: string
  gestureMap?: Partial<Record<GestureType, TouchAction>>
  threshold?: number
  longPressThreshold?: number
  edgeZoneSize?: number
  doubleTapTimeout?: number
  autoDetect?: boolean
}

interface TouchConfigState {
  gestureMap: Record<GestureType, TouchAction>
  threshold: number
  longPressThreshold: number
  edgeZoneSize: number
  doubleTapTimeout: number
  touchSupported: boolean
  isMobile: boolean
  autoDetected: boolean
}

function detectTouch(): { touchSupported: boolean; isMobile: boolean } {
  if (typeof window === 'undefined') return { touchSupported: false, isMobile: false }
  const touchSupported = 'ontouchstart' in window || navigator.maxTouchPoints > 0
  const isMobile = touchSupported && window.innerWidth < 768
  return { touchSupported, isMobile }
}

export function useTouchConfig(options: TouchConfigOptions = {}) {
  const storageKey = options.storageKey || 'cybermanju_touch_config'

  let savedMap: Partial<Record<GestureType, TouchAction>> = {}
  let savedThreshold: number | undefined
  let savedLongPress: number | undefined
  let savedEdge: number | undefined
  let savedDoubleTap: number | undefined

  try {
    const raw = localStorage.getItem(storageKey)
    if (raw) {
      const parsed = JSON.parse(raw)
      savedMap = parsed.gestureMap || {}
      savedThreshold = parsed.threshold
      savedLongPress = parsed.longPressThreshold
      savedEdge = parsed.edgeZoneSize
      savedDoubleTap = parsed.doubleTapTimeout
    }
  } catch {}

  const mergedMap = { ...DEFAULT_GESTURE_MAP, ...options.gestureMap, ...savedMap }
  const { touchSupported, isMobile } = detectTouch()

  const state = reactive<TouchConfigState>({
    gestureMap: mergedMap,
    threshold: savedThreshold ?? options.threshold ?? 50,
    longPressThreshold: savedLongPress ?? options.longPressThreshold ?? 600,
    edgeZoneSize: savedEdge ?? options.edgeZoneSize ?? 30,
    doubleTapTimeout: savedDoubleTap ?? options.doubleTapTimeout ?? 300,
    touchSupported,
    isMobile,
    autoDetected: options.autoDetect ?? true,
  })

  function persist() {
    try {
      localStorage.setItem(storageKey, JSON.stringify({
        gestureMap: state.gestureMap,
        threshold: state.threshold,
        longPressThreshold: state.longPressThreshold,
        edgeZoneSize: state.edgeZoneSize,
        doubleTapTimeout: state.doubleTapTimeout,
      }))
    } catch {}
  }

  watch(() => ({ ...state.gestureMap, threshold: state.threshold, longPressThreshold: state.longPressThreshold, edgeZoneSize: state.edgeZoneSize, doubleTapTimeout: state.doubleTapTimeout }), persist, { deep: true })

  function getAction(gesture: GestureType): TouchAction {
    return state.gestureMap[gesture] || 'none'
  }

  function setAction(gesture: GestureType, action: TouchAction) {
    state.gestureMap[gesture] = action
  }

  /**
   * `state` is only exposed `readonly` — Settings must mutate thresholds
   * through these setters or the write is silently dropped. Non-finite and
   * negative values fall back to their default (never stored as garbage).
   */
  function sane(v: number, fallback: number): number {
    return Number.isFinite(v) && v >= 0 ? Math.round(v) : fallback
  }
  function setThreshold(v: number) { state.threshold = sane(v, 50) }
  function setLongPressThreshold(v: number) { state.longPressThreshold = sane(v, 600) }
  function setEdgeZoneSize(v: number) { state.edgeZoneSize = sane(v, 30) }
  function setDoubleTapTimeout(v: number) { state.doubleTapTimeout = sane(v, 300) }

  function resetGesture(gesture: GestureType) {
    state.gestureMap[gesture] = DEFAULT_GESTURE_MAP[gesture] || 'none'
  }

  function resetAll() {
    state.gestureMap = { ...DEFAULT_GESTURE_MAP }
    state.threshold = 50
    state.longPressThreshold = 600
    state.edgeZoneSize = 30
    state.doubleTapTimeout = 300
    persist()
  }

  function exportConfig(): string {
    return JSON.stringify({
      gestureMap: state.gestureMap,
      threshold: state.threshold,
      longPressThreshold: state.longPressThreshold,
      edgeZoneSize: state.edgeZoneSize,
      doubleTapTimeout: state.doubleTapTimeout,
      exportedAt: new Date().toISOString(),
    }, null, 2)
  }

  function importConfig(json: string): boolean {
    try {
      const data = JSON.parse(json)
      if (data.gestureMap) Object.assign(state.gestureMap, data.gestureMap)
      if (data.threshold) state.threshold = data.threshold
      if (data.longPressThreshold) state.longPressThreshold = data.longPressThreshold
      if (data.edgeZoneSize) state.edgeZoneSize = data.edgeZoneSize
      if (data.doubleTapTimeout) state.doubleTapTimeout = data.doubleTapTimeout
      persist()
      return true
    } catch { return false }
  }

  function getSwipeOptions() {
    return {
      threshold: state.threshold,
      longPressThreshold: state.longPressThreshold,
      edgeZoneSize: state.edgeZoneSize,
      doubleTapTimeout: state.doubleTapTimeout,
      onSwipe: (dir: string) => {
        const key = `swipe_${dir}` as GestureType
        execAction(getAction(key))
      },
      onTwoFingerSwipe: (dir: string) => {
        const key = `two_finger_${dir}` as GestureType
        execAction(getAction(key))
      },
      onThreeFingerSwipe: (dir: string) => {
        const key = `three_finger_${dir}` as GestureType
        execAction(getAction(key))
      },
      onFourFingerSwipe: (dir: string) => {
        const key = `four_finger_${dir}` as GestureType
        execAction(getAction(key))
      },
      onPinchEnd: (e: { scale: number }) => {
        const key = e.scale > 1 ? 'pinch_out' as GestureType : 'pinch_in' as GestureType
        execAction(getAction(key))
      },
      onTap: (type: string) => {
        const key = `tap_${type}` as GestureType
        execAction(getAction(key))
      },
      onLongPress: () => {
        execAction(getAction('long_press'))
      },
      onEdgeSwipe: (e: { direction: string }) => {
        const key = `edge_swipe_${e.direction}` as GestureType
        execAction(getAction(key))
      },
    }
  }

  let actionHandler: ((action: TouchAction) => void) | null = null

  function onAction(handler: (action: TouchAction) => void) {
    actionHandler = handler
  }

  function execAction(action: TouchAction) {
    if (action !== 'none' && actionHandler) {
      actionHandler(action)
    }
  }

  function getAllGestures(): GestureType[] {
    return Object.keys(DEFAULT_GESTURE_MAP) as GestureType[]
  }

  function getGestureLabel(gesture: GestureType): string {
    return GESTURE_LABELS[gesture] || gesture
  }

  function getActionLabel(action: TouchAction): string {
    return ACTION_LABELS[action] || action
  }

  function getAllActions(): TouchAction[] {
    return Object.keys(ACTION_LABELS) as TouchAction[]
  }

  return {
    state: readonly(state) as typeof state,
    getAction,
    setAction,
    setThreshold,
    setLongPressThreshold,
    setEdgeZoneSize,
    setDoubleTapTimeout,
    resetGesture,
    resetAll,
    exportConfig,
    importConfig,
    getSwipeOptions,
    onAction,
    getAllGestures,
    getGestureLabel,
    getActionLabel,
    getAllActions,
  }
}

export type { TouchConfigState }
