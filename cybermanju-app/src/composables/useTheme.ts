// CyberManju OS — UseTheme (src/composables/useTheme.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useTheme, ThemeApi.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { computed, reactive, ref, watch, type ComputedRef } from 'vue'
import {
  DEFAULT_SETTINGS,
  LEGACY_THEME_ALIASES,
  THEMES,
  WALLPAPERS,
  buildCssVars,
  type GlassLevel,
  type Density,
  type MotionPref,
  type ShellStyle,
  type ThemeId,
  type ThemeSettings,
} from '@/ui/tokens'

const STORAGE_KEY = 'cybermanju_theme_v1'
const SHELL_KEY = 'cybermanju_shell_style'

function loadShell(): ShellStyle {
  try {
    const raw = localStorage.getItem(SHELL_KEY)
    if (raw === 'plasma' || raw === 'macos') return raw
  } catch { /* default stands */ }
  return 'macos'
}

function systemTheme(): ThemeId {
  const plasma = loadShell() === 'plasma'
  try {
    if (typeof window !== 'undefined' && window.matchMedia?.('(prefers-color-scheme: light)').matches) {
      return plasma ? 'plasma-light' : 'os-light'
    }
  } catch { /* default stands */ }
  return plasma ? 'plasma-dark' : 'os-dark'
}

function defaultSettingsFollowingSystem(): ThemeSettings {
  return { ...DEFAULT_SETTINGS, theme: systemTheme(), followSystem: true }
}

function loadSettings(): ThemeSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) {
      // First run: follow the host OS preference (auto appearance).
      return defaultSettingsFollowingSystem()
    }
    const parsed = JSON.parse(raw) as Partial<ThemeSettings> & { theme?: string }
    const rawTheme = parsed.theme as string | undefined
    // Migrate any pre-remake id (17 themes) onto os-dark / os-light by mode.
    // Unknown ids fall back to the OS preference, not a fixed theme.
    let theme: ThemeId
    if (rawTheme && rawTheme in THEMES) {
      theme = rawTheme as ThemeId
    } else if (rawTheme && LEGACY_THEME_ALIASES[rawTheme]) {
      theme = LEGACY_THEME_ALIASES[rawTheme]
    } else {
      theme = systemTheme()
    }
    // Per-theme accents survive only with valid hex values on live ids.
    const accents: Record<string, string> = {}
    if (parsed.accents && typeof parsed.accents === 'object') {
      for (const [k, v] of Object.entries(parsed.accents)) {
        if (typeof v === 'string' && /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(v)) accents[k] = v
      }
    }
    // Collapse legacy glass levels 1/3 onto translucent (2); 0 stays solid.
    let glass = parsed.glass
    if (glass === 1 || glass === 3) glass = 2
    if (glass !== 0 && glass !== 2) glass = DEFAULT_SETTINGS.glass
    const density = parsed.density === 'compact' || parsed.density === 'comfortable'
      ? parsed.density
      : DEFAULT_SETTINGS.density
    const followSystem = typeof parsed.followSystem === 'boolean' ? parsed.followSystem : false
    const wallpaper = typeof parsed.wallpaper === 'string' && WALLPAPERS.some(({ id }) => id === parsed.wallpaper)
      ? parsed.wallpaper
      : DEFAULT_SETTINGS.wallpaper
    return {
      ...DEFAULT_SETTINGS,
      ...parsed,
      theme: followSystem ? systemTheme() : theme,
      accents,
      glass,
      density,
      followSystem,
      wallpaper,
    }
  } catch {
    // Corrupt or unavailable storage should recover like a first run.
    return defaultSettingsFollowingSystem()
  }
}

/**
 * Module-level singleton: every caller of `useTheme()` shares this reactive
 * state, so theme/accent/density/glass/motion really are a single source of
 * truth for the whole operating system.
 */
const state = reactive<ThemeSettings>(loadSettings())

/**
 * Shell style singleton: `macos` (menu bar + floating Dock) or `plasma`
 * (bottom panel + Kickoff). Persisted separately so theme migrations never
 * clobber it; default is macOS (non-breaking).
 */
const shellState = ref<ShellStyle>(loadShell())

let applyScheduled = false

function applyNow() {
  if (typeof document === 'undefined') return
  const vars = buildCssVars({ ...state, shell: shellState.value })
  const root = document.documentElement
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value)
  }
  root.dataset.uiTheme = state.theme
  root.dataset.uiMode = THEMES[state.theme].mode
  root.dataset.uiDensity = state.density
  root.dataset.uiShell = shellState.value
}

function persist() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...state }))
    localStorage.setItem(SHELL_KEY, shellState.value)
  } catch {
    /* private mode / quota — the theme still applies for this session */
  }
}

function scheduleApply() {
  if (applyScheduled) return
  applyScheduled = true
  queueMicrotask(() => {
    applyScheduled = false
    applyNow()
    persist()
  })
}

watch(
  () => [
    state.theme,
    state.accent,
    state.density,
    state.glass,
    state.motion,
    state.glow,
    state.followSystem,
    state.wallpaper,
    shellState.value,
  ],
  () => scheduleApply(),
  { deep: true }
)

// First paint: write the variables synchronously so there is no flash of the
// default theme before Vue mounts.
if (typeof document !== 'undefined') applyNow()

// Auto appearance: track the OS scheme while followSystem is on.
if (typeof window !== 'undefined' && window.matchMedia) {
  try {
    const mq = window.matchMedia('(prefers-color-scheme: light)')
    const onChange = (e: MediaQueryListEvent) => {
      if (state.followSystem) {
        const plasma = shellState.value === 'plasma'
        state.theme = e.matches
          ? (plasma ? 'plasma-light' : 'os-light')
          : (plasma ? 'plasma-dark' : 'os-dark')
      }
    }
    if (typeof mq.addEventListener === 'function') {
      mq.addEventListener('change', onChange)
    } else {
      mq.addListener?.(onChange)
    }
  } catch { /* unsupported media-query API — stored theme stands */ }
}

export function useTheme() {
  const themeId = computed(() => state.theme)
  const mode = computed(() => THEMES[state.theme].mode)
  const density = computed(() => state.density)
  const glass = computed(() => state.glass)
  const accent = computed(() => state.accent)
  const motion = computed(() => state.motion)
  const cssVars: ComputedRef<Record<string, string>> = computed(() => buildCssVars({ ...state, shell: shellState.value }))
  const shellStyle = computed(() => shellState.value)

  function setTheme(id: ThemeId) {
    state.theme = id
    state.followSystem = false
  }
  /**
   * Shell style: `macos` (menu bar + floating Dock) or `plasma`
   * (bottom panel + Kickoff). When following the system appearance the
   * theme re-resolves into the new shell's pair so light/dark keeps meaning.
   */
  function setShellStyle(style: ShellStyle) {
    if (style !== 'macos' && style !== 'plasma') return
    shellState.value = style
    if (state.followSystem) {
      state.theme = systemTheme()
    }
  }
  /** Appearance: Light / Dark / Graphite / Auto (follow OS). */
  function setAppearance(mode: 'auto' | ThemeId) {
    if (mode === 'auto') {
      state.followSystem = true
      state.theme = systemTheme()
    } else {
      state.followSystem = false
      state.theme = mode
    }
  }
  function setAccent(value: string | null) {
    state.accent = value || null
  }
  /**
   * Per-theme accent (`ui accent <#hex> --for <theme>`): recolors one theme
   * without touching the general override. Pass null to clear it back to
   * the theme's built-in accent.
   */
  function setAccentFor(themeId: string, value: string | null) {
    if (!(themeId in THEMES)) return
    if (value && /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.test(value)) {
      state.accents = { ...state.accents, [themeId]: value }
    } else {
      const next = { ...state.accents }
      delete next[themeId]
      state.accents = next
    }
  }
  function setDensity(value: Density) {
    state.density = value
  }
  function setGlass(value: GlassLevel) {
    // Collapse legacy levels: solid (0) or translucent (everything else).
    state.glass = value === 0 ? 0 : 2
  }
  function setMotion(value: MotionPref) {
    state.motion = value
  }
  function setGlow(value: boolean) {
    state.glow = value
  }
  function setWallpaper(id: string) {
    if (WALLPAPERS.some((wallpaper) => wallpaper.id === id)) state.wallpaper = id
  }
  function cycleTheme() {
    const ids = Object.keys(THEMES) as ThemeId[]
    state.theme = ids[(ids.indexOf(state.theme) + 1) % ids.length]
  }
  function reset() {
    Object.assign(state, DEFAULT_SETTINGS)
  }

  /** Inline style carrying the chrome glass — teleported overlays only. */
  const glassStyle = computed(() => ({
    background: 'var(--ui-glass-2)',
    backdropFilter: 'blur(var(--ui-blur-strong)) saturate(var(--ui-saturate))',
    WebkitBackdropFilter: 'blur(var(--ui-blur-strong)) saturate(var(--ui-saturate))',
    border: '1px solid var(--ui-border)',
    boxShadow: 'var(--ui-shadow-menu)',
  }))

  return {
    settings: state,
    themeId,
    mode,
    density,
    glass,
    accent,
    motion,
    cssVars,
    glassStyle,
    shellStyle,
    setTheme,
    setAppearance,
    setShellStyle,
    setAccent,
    setAccentFor,
    setDensity,
    setGlass,
    setMotion,
    setGlow,
    setWallpaper,
    cycleTheme,
    reset,
    themes: THEMES,
  }
}

export type ThemeApi = ReturnType<typeof useTheme>
