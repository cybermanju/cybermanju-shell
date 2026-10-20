// CyberManju OS — UseShortcuts (src/composables/useShortcuts.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: ShortcutGroup, KplData, KpdData, ShortcutEntry, isBrowserShell, BROWSER_RESERVED, isReservedInBrowser, BROWSER_FALLBACKS.
// Integrates: vue, @vueuse, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { onMounted, watch, computed, type Ref } from 'vue'
import { useEventListener } from '@vueuse/core'
import { isEditableTarget } from '@/utils/dom'

export type ShortcutGroup =
  | 'Global Shortcuts'
  | 'Navigation'
  | 'File Operations'
  | 'View'
  | 'Panels'
  | 'Windows'
  | 'Workspace'
  | 'Touchpad'
  | 'Touch'

export interface KplData {
  global: { name: string; version: string; description: string }
  shortcuts: Record<string, string>
  groups: Record<string, string>
}

export interface KpdData {
  modifiers: Record<string, string>
  components: Record<string, string>
  contextActions: Record<string, string>
  touchpadGestures: Record<string, string>
}

export interface ShortcutEntry {
  action: string
  keys: string
  /** Primary (.kpl) binding. */
  primary: string
  /** Browser-safe fallback (Alt+…) — empty when not needed. */
  fallback: string
  /** True when the primary can never fire in a browser tab. */
  blockedInBrowser: boolean
  group: ShortcutGroup
  description: string
}

type ShortcutHandler = () => void

interface ChordState {
  buffer: string[]
  timeout: number | null
}

function parseIni(text: string): Record<string, Record<string, string>> {
  const result: Record<string, Record<string, string>> = {}
  let currentGroup = '__root__'
  result[currentGroup] = {}
  for (const raw of text.split('\n')) {
    const line = raw.trim()
    if (!line || line.startsWith('#') || line.startsWith(';')) continue
    const groupMatch = line.match(/^\[(.+)\]$/)
    if (groupMatch) {
      currentGroup = groupMatch[1]
      if (!result[currentGroup]) result[currentGroup] = {}
      continue
    }
    const eqIdx = line.indexOf('=')
    if (eqIdx === -1) continue
    const key = line.slice(0, eqIdx).trim()
    const val = line.slice(eqIdx + 1).trim()
    if (key) result[currentGroup][key] = val
  }
  return result
}

function normalizeKeys(keys: string, modifiers: Record<string, string>): string {
  let normalized = keys
  for (const [alias, mod] of Object.entries(modifiers)) {
    normalized = normalized.replace(new RegExp(`\\b${alias}\\b`, 'gi'), mod)
  }
  return normalized
}

function keyEventToSequence(e: KeyboardEvent): string {
  const parts: string[] = []
  if (e.ctrlKey) parts.push('Ctrl')
  if (e.altKey) parts.push('Alt')
  if (e.shiftKey) parts.push('Shift')
  if (e.metaKey) parts.push('Meta')
  if (!['Control', 'Alt', 'Shift', 'Meta'].includes(e.key)) {
    parts.push(e.key.length === 1 ? e.key.toUpperCase() : e.key)
  }
  return parts.join('+')
}

function sequencesMatch(pressed: string, binding: string): boolean {
  const normalize = (s: string) => s.replace(/\s+/g, '').toLowerCase()
  return normalize(pressed) === normalize(binding)
}

/** True when running inside a real browser tab (WASM/Pages/web), not Tauri. */
export function isBrowserShell(): boolean {
  if (typeof window === 'undefined') return false
  return !('__TAURI__' in window)
}

/**
 * Bindings the browser owns: the tab handles them before page JS runs, so
 * `preventDefault()` is silently ignored. These can NEVER work in WASM —
 * the shell must offer an Alt+ fallback instead.
 */
export const BROWSER_RESERVED = new Set(
  ['ctrl+t', 'ctrl+w', 'ctrl+n', 'ctrl+tab', 'ctrl+shift+tab'].map((s) =>
    s.replace(/\s+/g, '').toLowerCase(),
  ),
)

export function isReservedInBrowser(binding: string): boolean {
  const norm = binding.replace(/\s+/g, '').toLowerCase()
  if (BROWSER_RESERVED.has(norm)) return true
  // Ctrl+Tab chords contain a reserved step.
  return norm.split(',').some((step) => BROWSER_RESERVED.has(step.trim()))
}

/**
 * Browser-safe fallbacks for reserved primaries. Anything not listed here
 * falls back automatically: leading `Ctrl` → `Alt` (and `Ctrl+Shift` →
 * `Alt+Shift`), which the browser always delivers to the page.
 */
export const BROWSER_FALLBACKS: Record<string, string> = {
  open_trash: 'Alt+T',
  autotile_toggle: 'Alt+T',
  close_window: 'Alt+W',
  close_window_alt: 'Alt+W',
  focus_next: 'Alt+]',
  focus_prev: 'Alt+[',
  focus_next_alt: 'Alt+]',
  focus_prev_alt: 'Alt+[',
}

function autoFallback(binding: string): string {
  return binding
    .replace(/\bCtrl\+Shift\b/gi, 'Alt+Shift')
    .replace(/\bCtrl\b/gi, 'Alt')
}

export function fallbackFor(action: string, primary: string): string {
  if (!isReservedInBrowser(primary)) return ''
  return BROWSER_FALLBACKS[action] || autoFallback(primary)
}

function prettyAction(action: string): string {
  return action.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

export function useShortcuts(
  kplSource: string | (() => Promise<string>),
  kpdSource: string | (() => Promise<string>),
  scopeRef?: Ref<HTMLElement | null>,
  overrides?: Ref<Record<string, string>>,
) {
  const handlers = new Map<string, ShortcutHandler[]>()
  const chord: ChordState = { buffer: [], timeout: null }
  let kpl: KplData = {
    global: { name: '', version: '', description: '' },
    shortcuts: {},
    groups: {},
  }
  let kpd: KpdData = { modifiers: {}, components: {}, contextActions: {}, touchpadGestures: {} }
  /** action → primary binding */
  const activeBindings = new Map<string, string>()
  /** action → fallback binding (browser-safe alternative) */
  const fallbackBindings = new Map<string, string>()
  let paused = false
  let loaded = false

  const inBrowser = computed(() => isBrowserShell())

  async function load() {
    const kplText = typeof kplSource === 'string' ? kplSource : await kplSource()
    const kpdText = typeof kpdSource === 'string' ? kpdSource : await kpdSource()
    const kplRaw = parseIni(kplText)
    const kpdRaw = parseIni(kpdText)
    const shortcuts: Record<string, string> = {}
    const groups: Record<string, string> = {}
    for (const [group, vals] of Object.entries(kplRaw)) {
      if (group === 'Global' || group === '__root__') continue
      for (const [action, keys] of Object.entries(vals)) {
        shortcuts[action] = keys
        groups[action] = group
      }
    }
    kpl = {
      global: {
        name: kplRaw.Global?.name || '',
        version: kplRaw.Global?.version || '',
        description: kplRaw.Global?.description || '',
      },
      shortcuts,
      groups,
    }
    kpd = {
      modifiers: kpdRaw.Modifiers || {},
      components: kpdRaw.Components || {},
      contextActions: kpdRaw.ContextActions || {},
      touchpadGestures: kpdRaw.TouchpadGestures || {},
    }
    buildBindings()
    loaded = true
  }

  function buildBindings() {
    activeBindings.clear()
    fallbackBindings.clear()
    const merged = { ...kpl.shortcuts }
    if (overrides?.value) {
      for (const [action, keys] of Object.entries(overrides.value)) {
        if (keys) merged[action] = keys
      }
    }
    for (const [action, keys] of Object.entries(merged)) {
      const primary = normalizeKeys(keys, kpd.modifiers)
      activeBindings.set(action, primary)
      const fb = fallbackFor(action, primary)
      if (fb && sequencesMatch(fb, primary) === false) fallbackBindings.set(action, fb)
    }
  }

  if (overrides) {
    watch(
      overrides,
      () => {
        buildBindings()
      },
      { deep: true },
    )
  }

  function on(action: string, handler: ShortcutHandler) {
    if (!handlers.has(action)) handlers.set(action, [])
    handlers.get(action)!.push(handler)
    return () => {
      const arr = handlers.get(action)
      if (arr) {
        const idx = arr.indexOf(handler)
        if (idx !== -1) arr.splice(idx, 1)
      }
    }
  }

  function off(action: string, handler: ShortcutHandler) {
    const arr = handlers.get(action)
    if (arr) {
      const idx = arr.indexOf(handler)
      if (idx !== -1) arr.splice(idx, 1)
    }
  }

  function handleKey(e: KeyboardEvent) {
    if (paused) return
    // Ctrl/Cmd combos must still work inside inputs (they are app commands,
    // not text); plain typing must never trigger single-key shortcuts.
    if (!e.ctrlKey && !e.metaKey && isEditableTarget(e.target)) return
    const seq = keyEventToSequence(e)
    if (chord.timeout) {
      clearTimeout(chord.timeout)
      chord.timeout = null
    }
    const fullChord = [...chord.buffer, seq].join(', ')
    const candidates: Array<[string, string]> = []
    for (const [action, binding] of activeBindings) candidates.push([action, binding])
    for (const [action, fb] of fallbackBindings) candidates.push([action, fb])
    for (const [action, binding] of candidates) {
      if (sequencesMatch(seq, binding)) {
        if (binding.includes(',')) {
          chord.buffer.push(seq)
          chord.timeout = window.setTimeout(() => {
            chord.buffer = []
          }, 1000)
          return
        }
        fire(action, e)
        return
      }
      if (binding.includes(',') && sequencesMatch(fullChord, binding)) {
        chord.buffer = []
        fire(action, e)
        return
      }
    }
    chord.buffer = []
  }

  function fire(action: string, e: KeyboardEvent) {
    const arr = handlers.get(action)
    if (arr && arr.length > 0) {
      // For reserved browser combos this is a no-op (the browser already
      // stole the key) — the Alt+ fallback registered beside it is what
      // actually fires in WASM. Still call it: harmless in Tauri.
      e.preventDefault()
      e.stopPropagation()
      for (const h of [...arr]) h()
    }
  }

  // VueUse-managed listener: auto-cleaned on unmount, capture phase so app
  // shortcuts win over nested panel handlers (code editor, file grid).
  useEventListener(document, 'keydown', handleKey as EventListener, { capture: true })

  onMounted(() => {
    void load()
  })

  function pause() {
    paused = true
  }
  function resume() {
    paused = false
  }

  /** Transport-aware display binding: fallback in browsers when blocked. */
  function getShortcut(action: string): string {
    const primary = activeBindings.get(action) || ''
    const fb = fallbackBindings.get(action) || ''
    if (inBrowser.value && fb && isReservedInBrowser(primary)) return fb
    return primary
  }

  function getPrimary(action: string): string {
    return activeBindings.get(action) || ''
  }

  function getFallback(action: string): string {
    return fallbackBindings.get(action) || ''
  }

  function isBlocked(action: string): boolean {
    const primary = activeBindings.get(action) || ''
    return inBrowser.value && !!fallbackBindings.get(action) && isReservedInBrowser(primary)
  }

  function getAllShortcuts(): ShortcutEntry[] {
    const entries: ShortcutEntry[] = []
    for (const [action, primary] of activeBindings) {
      const rawGroup = kpl.groups[action] || 'Global Shortcuts'
      const group = (
        ['Global Shortcuts', 'Navigation', 'File Operations', 'View', 'Panels', 'Windows', 'Workspace', 'Touchpad', 'Touch'].includes(
          rawGroup,
        )
          ? rawGroup
          : 'Global Shortcuts'
      ) as ShortcutGroup
      const fb = fallbackBindings.get(action) || ''
      const blocked = isReservedInBrowser(primary) && !!fb
      entries.push({
        action,
        keys: inBrowser.value && blocked && fb ? fb : primary.replace(/,/g, ', '),
        primary: primary.replace(/,/g, ', '),
        fallback: fb,
        blockedInBrowser: blocked,
        group,
        description: prettyAction(action),
      })
    }
    return entries.sort((a, b) => a.description.localeCompare(b.description))
  }

  function getComponentActions(componentId: string): string[] {
    const raw = kpd.components[componentId]
    if (!raw) return []
    return raw.split(',').map((s) => s.trim())
  }

  function getContextActions(fileType: string): string[] {
    const raw = kpd.contextActions[fileType]
    if (!raw) return kpd.contextActions.file?.split(',').map((s) => s.trim()) || []
    return raw.split(',').map((s) => s.trim())
  }

  function getTouchpadGesture(gesture: string): string {
    return kpd.touchpadGestures[gesture] || ''
  }

  function getModifier(key: string): string {
    return kpd.modifiers[key] || key
  }

  return {
    load,
    on,
    off,
    pause,
    resume,
    getShortcut,
    getPrimary,
    getFallback,
    isBlocked,
    isReservedInBrowser,
    getAllShortcuts,
    getComponentActions,
    getContextActions,
    getTouchpadGesture,
    getModifier,
    activeBindings,
    fallbackBindings,
    inBrowser,
    isLoaded: () => loaded,
    kpl,
    kpd,
  }
}
