// CyberManju OS — Ui theme.test (tests/frontend/ui-theme.test.ts).
// Theme registries + accent resolution + WCAG contrast guards.
// Flat AMOLED remake: 2 themes (os-dark, os-light) + optional os-graphite,
// one fixed geometry, no neon/glow/aurora. Legacy ids resolve via aliases.
import { describe, it, expect, vi } from 'vitest'
import { useTheme } from '@/composables/useTheme'
import {
  THEMES,
  THEME_IDS,
  THEME_GROUPS,
  LEGACY_THEME_ALIASES,
  SHAPE_RADII,
  FONT_STACKS,
  FONT_TRACKING_WIDE,
  buildCssVars,
  compositeOver,
  contrastRatio,
  parseColor,
  DEFAULT_SETTINGS,
  type ThemeId,
} from '@/ui/tokens'

describe('theme registries', () => {
  it('ships exactly the flat-system + plasma ids', () => {
    expect([...THEME_IDS].sort()).toEqual([
      'os-dark',
      'os-graphite',
      'os-light',
      'plasma-dark',
      'plasma-light',
    ])
    for (const id of THEME_IDS) {
      const def = THEMES[id]
      expect(def, id).toBeDefined()
      expect(def.palette.accent, `${id} accent`).toMatch(/^#[0-9a-f]{6}$/i)
      expect(def.blurb.length, `${id} blurb`).toBeGreaterThan(0)
      expect(['dark', 'light'], `${id} mode`).toContain(def.mode)
      // Single design language: every theme resolves to the same geometry.
      expect(def.design.shape).toBe('soft')
      expect(def.design.font).toBe('system')
      expect(SHAPE_RADII[def.design.shape], `${id} shape`).toBeDefined()
      expect(FONT_STACKS[def.design.font], `${id} font`).toBeDefined()
      expect(FONT_TRACKING_WIDE[def.design.font], `${id} tracking`).toBeDefined()
    }
  })

  it('every group id resolves and every theme is grouped', () => {
    const grouped = new Set(THEME_GROUPS.flatMap((g) => g.ids))
    for (const id of THEME_IDS) expect(grouped.has(id), id).toBe(true)
    for (const id of grouped) expect(THEMES[id as ThemeId], id).toBeDefined()
  })

  it('legacy 17-theme ids all migrate by mode', () => {
    for (const old of [
      'mac-light', 'mac-graphite-light', 'ocean-light', 'sunset-light',
      'forest-light', 'lavender-light', 'rose-light', 'daylight',
      'mac-dark', 'mac-graphite-dark', 'mac-midnight', 'ocean-night',
      'forest-night', 'ember-night', 'nebula-night', 'cyber-night',
      'matrix-night', 'cyberpunk-night', 'midnight', 'nebula', 'ember', 'ghostline',
    ]) {
      expect(LEGACY_THEME_ALIASES[old], old).toBeDefined()
      expect(THEMES[LEGACY_THEME_ALIASES[old]], old).toBeDefined()
    }
    // Spot-check mode mapping.
    expect(THEMES[LEGACY_THEME_ALIASES['mac-light']].mode).toBe('light')
    expect(THEMES[LEGACY_THEME_ALIASES['cyberpunk-night']].mode).toBe('dark')
  })

  it('os-dark is true AMOLED, os-light is paper', () => {
    expect(THEMES['os-dark'].palette.bg).toBe('#000000')
    expect(THEMES['os-dark'].palette.surface).toBe('#0B0B0D')
    expect(THEMES['os-light'].palette.bg).toBe('#ECECEC')
    expect(THEMES['os-light'].palette.surface).toBe('#FFFFFF')
  })

  it('plasma themes carry panel + hover surfaces', () => {
    for (const id of ['plasma-dark', 'plasma-light'] as const) {
      const p = THEMES[id].palette
      expect(p.panel, `${id} panel`).toMatch(/rgba\(/)
      expect(p.hover, `${id} hover`).toMatch(/rgba\(/)
    }
    expect(THEMES['plasma-dark'].palette.bg).toBe('#1b1e20')
    expect(THEMES['plasma-light'].palette.surface).toBe('#fcfcfc')
  })

  it('shell geometry tokens switch per shell style', () => {
    const base = { ...DEFAULT_SETTINGS, accents: {} }
    const macos = buildCssVars({ ...base, theme: 'os-dark', shell: 'macos' })
    const plasma = buildCssVars({ ...base, theme: 'plasma-dark', shell: 'plasma' })
    expect(macos['--ui-titlebar-h']).toBe('28px')
    expect(plasma['--ui-titlebar-h']).toBe('30px')
    expect(macos['--ui-panel-h']).toBe('0px')
    expect(plasma['--ui-panel-h']).toBe('44px')
    expect(macos['--ui-menubar-h']).toBe('24px')
    expect(plasma['--ui-menubar-h']).toBe('0px')
    expect(macos['--ui-radius-xl']).toBe('12px')
    expect(plasma['--ui-radius-xl']).toBe('6px')
    expect(macos['--ui-dur-fast']).toBe('120ms')
    expect(plasma['--ui-dur-fast']).toBe('100ms')
    expect(plasma['--ui-focus-ring']).toBe(`0 0 0 2px ${THEMES['plasma-dark'].palette.accent}`)
    expect(plasma['--ui-shadow-popup']).toBeTruthy()
    expect(plasma['--ui-panel']).toBeTruthy()
    expect(plasma['--ui-hover']).toBeTruthy()
  })
})

describe('theme preference recovery', () => {
  it.each([
    { name: 'malformed settings', getItem: () => 'not-json' },
    { name: 'blocked local storage', getItem: () => { throw new Error('blocked') } },
  ])('follows the OS when $name is encountered', async ({ getItem }) => {
    vi.resetModules()
    vi.stubGlobal('localStorage', { getItem, setItem: vi.fn() })
    vi.stubGlobal('window', {
      matchMedia: () => ({ matches: true, addEventListener: vi.fn() }),
    })

    try {
      const { useTheme: useFreshTheme } = await import('@/composables/useTheme')
      const theme = useFreshTheme()
      expect(theme.themeId.value).toBe('os-light')
      expect(theme.settings.followSystem).toBe(true)
    } finally {
      vi.unstubAllGlobals()
      vi.resetModules()
    }
  })

  it('tracks OS changes through the legacy MediaQueryList listener', async () => {
    const listeners: Array<(event: MediaQueryListEvent) => void> = []
    vi.resetModules()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => key === 'cybermanju_theme_v1'
        ? JSON.stringify({ theme: 'os-dark', followSystem: true })
        : null,
      setItem: vi.fn(),
    })
    vi.stubGlobal('window', {
      matchMedia: () => ({
        matches: false,
        addListener: (
          listener: (event: MediaQueryListEvent) => void,
        ) => listeners.push(listener),
      }),
    })

    try {
      const { useTheme: useFreshTheme } = await import('@/composables/useTheme')
      const theme = useFreshTheme()
      expect(theme.themeId.value).toBe('os-dark')
      expect(listeners).toHaveLength(1)

      listeners[0]({ matches: true } as MediaQueryListEvent)
      expect(theme.themeId.value).toBe('os-light')

      theme.setTheme('os-dark')
      listeners[0]({ matches: true } as MediaQueryListEvent)
      expect(theme.themeId.value).toBe('os-dark')
    } finally {
      vi.unstubAllGlobals()
      vi.resetModules()
    }
  })

  it('normalizes invalid persisted appearance values', async () => {
    vi.resetModules()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => key === 'cybermanju_theme_v1'
        ? JSON.stringify({
            theme: 'os-light',
            density: 'expanded',
            followSystem: 'false',
            wallpaper: '../unlisted-wallpaper',
          })
        : null,
      setItem: vi.fn(),
    })
    vi.stubGlobal('window', {
      matchMedia: () => ({ matches: false, addEventListener: vi.fn() }),
    })

    try {
      const { useTheme: useFreshTheme } = await import('@/composables/useTheme')
      const theme = useFreshTheme()
      expect(theme.themeId.value).toBe('os-light')
      expect(theme.settings.density).toBe(DEFAULT_SETTINGS.density)
      expect(theme.settings.followSystem).toBe(false)
      expect(theme.settings.wallpaper).toBe(DEFAULT_SETTINGS.wallpaper)
      expect(() => buildCssVars(theme.settings)).not.toThrow()
    } finally {
      vi.unstubAllGlobals()
      vi.resetModules()
    }
  })
})

describe('wallpaper persistence', () => {
  it('saves preset changes through the shared theme settings watcher', async () => {
    const values = new Map<string, string>()
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, String(value)) },
      removeItem: (key: string) => { values.delete(key) },
    })

    try {
      const theme = useTheme()
      const current = theme.settings.wallpaper
      const next = current === 'dunes' ? 'slopes-light' : 'dunes'
      theme.setWallpaper(next)
      await new Promise<void>((resolve) => setTimeout(resolve, 0))

      const saved = values.get('cybermanju_theme_v1')
      expect(saved).toBeDefined()
      expect(JSON.parse(saved!).wallpaper).toBe(next)

      theme.setWallpaper('../unlisted-wallpaper')
      expect(theme.settings.wallpaper).toBe(next)
      expect(JSON.parse(saved!).wallpaper).toBe(next)
    } finally {
      vi.unstubAllGlobals()
    }
  })
})

describe('effective accent resolution', () => {
  it('general accent wins, then per-theme, then built-in', () => {
    const base = { ...DEFAULT_SETTINGS, accents: {} }
    const vars = (s: typeof base) => buildCssVars(s)
    // Built-in.
    expect(vars({ ...base, theme: 'os-dark' })['--ui-accent']).toBe(
      THEMES['os-dark'].palette.accent,
    )
    // Per-theme override.
    expect(
      vars({ ...base, theme: 'os-dark', accents: { 'os-dark': '#123456' } })[
        '--ui-accent'
      ],
    ).toBe('#123456')
    // Per-theme entry for ANOTHER theme does not leak in.
    expect(
      vars({ ...base, theme: 'os-dark', accents: { 'os-light': '#123456' } })[
        '--ui-accent'
      ],
    ).toBe(THEMES['os-dark'].palette.accent)
    // General override wins over per-theme.
    expect(
      vars({
        ...base,
        theme: 'os-dark',
        accent: '#abcdef',
        accents: { 'os-dark': '#123456' },
      })['--ui-accent'],
    ).toBe('#abcdef')
    // Invalid values fall through to built-in.
    expect(
      vars({ ...base, theme: 'os-dark', accent: 'bogus', accents: { 'os-dark': 'xx' } })[
        '--ui-accent'
      ],
    ).toBe(THEMES['os-dark'].palette.accent)
  })

  it('single geometry: all themes ship identical radius/type/motion tokens', () => {
    const base = { ...DEFAULT_SETTINGS, accents: {} }
    const dark = buildCssVars({ ...base, theme: 'os-dark' })
    const light = buildCssVars({ ...base, theme: 'os-light' })
    const graphite = buildCssVars({ ...base, theme: 'os-graphite' })
    for (const v of [dark, light, graphite]) {
      expect(v['--ui-radius-xl']).toBe('12px')
      expect(v['--ui-radius-lg']).toBe('10px')
      expect(v['--ui-radius-sm']).toBe('6px')
      expect(v['--ui-radius-xs']).toBe('4px')
      expect(v['--ui-dur-fast']).toBe('120ms')
      expect(v['--ui-dur']).toBe('200ms')
      expect(v['--ui-ease-spring']).toBe('ease-out')
      // No neon leftovers.
      expect(v['--ui-aurora-a']).toBe('transparent')
      expect(v['--ui-aurora-b']).toBe('transparent')
      expect(v['--ui-glass-sheen']).toBe('none')
      // Chrome-only glass + flat shadows + focus ring present.
      expect(v['--ui-sidebar']).toBeTruthy()
      expect(v['--ui-separator']).toBeTruthy()
      expect(v['--ui-shadow-window']).toContain('0 0 0 0.5px')
      expect(v['--ui-shadow-window-idle']).toBeTruthy()
      expect(v['--ui-shadow-menu']).toBeTruthy()
      expect(v['--ui-focus-ring']).toContain('0 0 0 3px')
      expect(v['--ui-titlebar-h']).toBe('28px')
    }
    expect(dark['--ui-font']).toBe(light['--ui-font'])
  })
})

describe('contrast guards (WCAG)', () => {
  it('body text and secondary text hit ≥ 4.5:1 on the content surface', () => {
    for (const id of THEME_IDS) {
      const p = THEMES[id].palette
      // Content areas are flat solids, so composite text over surface.
      const bg = parseColor(p.surface).slice(0, 3) as [number, number, number]
      const text = compositeOver(p.text, p.surface)
      const text2 = compositeOver(p.text2, p.surface)
      expect(contrastRatio(text, bg), `${id} text/surface`).toBeGreaterThanOrEqual(4.5)
      expect(contrastRatio(text2, bg), `${id} text-2/surface`).toBeGreaterThanOrEqual(4.5)
    }
  })

  it('accent on the surfaces it renders on hits ≥ 3:1 (graphical + large/bold)', () => {
    for (const id of THEME_IDS) {
      const p = THEMES[id].palette
      const accent = parseColor(p.accent).slice(0, 3) as [number, number, number]
      // Accent glyphs/fills live on content surfaces and on the panel
      // (composited over bg) — never on the bare desktop fallback.
      const surface = parseColor(p.surface).slice(0, 3) as [number, number, number]
      expect(contrastRatio(accent, surface), `${id} accent/surface`).toBeGreaterThanOrEqual(3)
      const panel = compositeOver(p.panel, p.bg)
      expect(contrastRatio(accent, panel), `${id} accent/panel`).toBeGreaterThanOrEqual(3)
    }
  })

  it('on-accent text on the accent fill hits ≥ 3:1', () => {
    for (const id of THEME_IDS) {
      const p = THEMES[id].palette
      const accent = parseColor(p.accent).slice(0, 3) as [number, number, number]
      const onAccent = compositeOver(p.onAccent, p.accent)
      expect(contrastRatio(onAccent, accent), `${id} on-accent/accent`).toBeGreaterThanOrEqual(3)
    }
  })
})
