// CyberManju OS — AppearanceProfile (src/utils/appearanceProfile.ts).
// Role: strict v1 cosmetic export, parser, and staged restore for app/GPUI.
// Exports: AppearanceProfileV1 and validated build/serialize/parse/restore helpers.
// Integrates: existing theme persistence, custom wallpaper validation, UI tokens.
// Contracts: strict allowlist; local wallpaper metadata has no restorable bytes.
import {
  DEFAULT_SETTINGS,
  LEGACY_THEME_ALIASES,
  THEMES,
  WALLPAPERS,
  type ShellStyle,
  type ThemeId,
  type ThemeSettings,
} from '@/ui/tokens'
import {
  MAX_CUSTOM_WALLPAPER_BYTES,
  normalizeCustomWallpaperUrl,
  validateCustomWallpaperFile,
} from '@/utils/customWallpaper'

export type AppearanceWallpaperV1 =
  | { kind: 'preset'; id: string }
  | { kind: 'url'; url: string }
  | { kind: 'local-file'; name: string; mimeType: string; size: number }

export interface AppearanceProfileV1 {
  schema: 'com.cybermanju.appearance'
  version: 1
  settings: ThemeSettings
  shellStyle: ShellStyle
  panelDocked: boolean
  wallpaper: AppearanceWallpaperV1
}

export interface AppearanceProfileSource {
  settings: ThemeSettings
  shellStyle: ShellStyle
  panelDocked: boolean
  customWallpaper:
    | { kind: null }
    | { kind: 'url'; url: string }
    | { kind: 'file'; name: string; mimeType: string; size: number }
}

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i
const MOTIONS = new Set(['auto', 'full', 'reduced'])
const DENSITIES = new Set(['compact', 'comfortable'])
const SHELLS = new Set(['macos', 'plasma'])
const WALLPAPER_IDS = new Set(WALLPAPERS.map(({ id }) => id))
const SETTINGS_KEYS = new Set([
  'theme', 'shell', 'accent', 'accents', 'density', 'glass', 'motion',
  'followSystem', 'wallpaper', 'glow',
])
const PROFILE_KEYS = new Set(['schema', 'version', 'settings', 'shellStyle', 'panelDocked', 'wallpaper'])
const SOURCE_KEYS = new Set(['settings', 'shellStyle', 'panelDocked', 'customWallpaper'])
const SOURCE_PRESET_KEYS = new Set(['kind'])
const SOURCE_URL_KEYS = new Set(['kind', 'url'])
const SOURCE_FILE_KEYS = new Set(['kind', 'name', 'mimeType', 'size'])
const URL_WALLPAPER_KEYS = new Set(['kind', 'url'])
const PRESET_WALLPAPER_KEYS = new Set(['kind', 'id'])
const FILE_WALLPAPER_KEYS = new Set(['kind', 'name', 'mimeType', 'size'])
const SECRET_QUERY_KEY = /(?:token|key|secret|auth|password|credential|signature|^sig$)/i
const CONTROL_CHARACTERS = /[\u0000-\u001f\u007f-\u009f]/

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}

function exactKeys(value: Record<string, unknown>, allowed: Set<string>, label: string): void {
  for (const key of Object.keys(value)) {
    if (!allowed.has(key)) throw new Error(`${label} contains unsupported field: ${key}`)
  }
}

function knownTheme(value: unknown): value is ThemeId {
  return typeof value === 'string' && Object.prototype.hasOwnProperty.call(THEMES, value)
}

function validAccent(value: unknown): value is string {
  return typeof value === 'string' && HEX.test(value)
}

function normalizedSettings(input: ThemeSettings, strict = false): ThemeSettings {
  if (!isObject(input)) throw new Error('Appearance settings must be an object.')
  exactKeys(input, SETTINGS_KEYS, 'Appearance settings')

  let theme: ThemeId
  if (knownTheme(input.theme)) theme = input.theme
  else if (!strict && typeof input.theme === 'string' && LEGACY_THEME_ALIASES[input.theme]) {
    theme = LEGACY_THEME_ALIASES[input.theme]
  } else {
    throw new Error('Appearance theme is not recognized.')
  }
  if (input.shell !== undefined && !SHELLS.has(input.shell as string)) {
    throw new Error('Appearance shell mirror is not recognized.')
  }
  if (input.accent !== null && !validAccent(input.accent)) {
    throw new Error('Appearance accent must be a 3- or 6-digit hex color or null.')
  }
  if (!isObject(input.accents)) throw new Error('Per-theme accents must be an object.')
  const accents: Record<string, string> = {}
  for (const [id, color] of Object.entries(input.accents)) {
    if (!validAccent(color)) throw new Error(`Per-theme accent for ${id} is invalid.`)
    accents[id] = color
  }
  if (!DENSITIES.has(input.density as string)) throw new Error('Appearance density is not recognized.')
  // Existing persisted legacy levels 1 and 3 resolve to translucent level 2.
  const glass = input.glass === 1 || input.glass === 3 ? 2 : input.glass
  if (glass !== 0 && glass !== 2) throw new Error('Appearance glass level is not recognized.')
  if (!MOTIONS.has(input.motion as string)) throw new Error('Appearance motion preference is not recognized.')
  if (typeof input.followSystem !== 'boolean') throw new Error('Appearance followSystem must be boolean.')
  if (typeof input.glow !== 'boolean') throw new Error('Appearance glow must be boolean.')
  if (strict && !WALLPAPER_IDS.has(input.wallpaper as string)) {
    throw new Error('Appearance preset wallpaper is not recognized.')
  }
  const preset = WALLPAPER_IDS.has(input.wallpaper as string)
    ? input.wallpaper as string
    : DEFAULT_SETTINGS.wallpaper

  const settings: ThemeSettings = {
    theme,
    accent: input.accent as string | null,
    accents,
    density: input.density as ThemeSettings['density'],
    glass: glass as ThemeSettings['glass'],
    motion: input.motion as ThemeSettings['motion'],
    followSystem: input.followSystem,
    wallpaper: preset,
    glow: input.glow,
  }
  if (input.shell !== undefined) settings.shell = input.shell as ShellStyle
  return settings
}

function safeWallpaperUrl(raw: string): string {
  if (new TextEncoder().encode(raw).byteLength > 8192) throw new Error('Wallpaper URL exceeds the 8192-byte limit.')
  if (raw !== raw.trim() || CONTROL_CHARACTERS.test(raw)) {
    throw new Error('Wallpaper URL cannot contain surrounding whitespace or control characters.')
  }
  const normalized = normalizeCustomWallpaperUrl(raw)
  const url = new URL(normalized)
  for (const key of url.searchParams.keys()) {
    if (SECRET_QUERY_KEY.test(key)) {
      throw new Error('Wallpaper URL contains a credential-like query parameter and cannot be exported.')
    }
  }
  return normalized
}

function validateWallpaper(value: unknown): AppearanceWallpaperV1 {
  if (!isObject(value)) throw new Error('Appearance wallpaper must be an object.')
  if (value.kind === 'preset') {
    exactKeys(value, PRESET_WALLPAPER_KEYS, 'Preset wallpaper')
    if (typeof value.id !== 'string' || !WALLPAPER_IDS.has(value.id)) {
      throw new Error('Preset wallpaper id is not recognized.')
    }
    return { kind: 'preset', id: value.id }
  }
  if (value.kind === 'url') {
    exactKeys(value, URL_WALLPAPER_KEYS, 'URL wallpaper')
    if (typeof value.url !== 'string') throw new Error('Wallpaper URL must be a string.')
    const safeUrl = safeWallpaperUrl(value.url)
    if (safeUrl !== value.url) throw new Error('Wallpaper URL must be normalized before import.')
    return { kind: 'url', url: safeUrl }
  }
  if (value.kind === 'local-file') {
    exactKeys(value, FILE_WALLPAPER_KEYS, 'Local wallpaper')
    if (typeof value.name !== 'string' || value.name.trim() === '' || new TextEncoder().encode(value.name).byteLength > 255 || /[\\/]/.test(value.name) || CONTROL_CHARACTERS.test(value.name)) {
      throw new Error('Local wallpaper name must be a filename only, without a path.')
    }
    if (typeof value.mimeType !== 'string' || value.mimeType !== value.mimeType.toLowerCase()) {
      throw new Error('Local wallpaper MIME type is invalid.')
    }
    if (typeof value.size !== 'number' || !Number.isSafeInteger(value.size) || value.size <= 0 || value.size > MAX_CUSTOM_WALLPAPER_BYTES) {
      throw new Error('Local wallpaper metadata size is outside the supported range.')
    }
    validateCustomWallpaperFile({ size: value.size, type: value.mimeType })
    return { kind: 'local-file', name: value.name, mimeType: value.mimeType, size: value.size }
  }
  throw new Error('Appearance wallpaper kind is not recognized.')
}

/** Build a JSON-safe v1 profile from already-live, normalized application state. */
export function buildAppearanceProfileV1(source: AppearanceProfileSource): AppearanceProfileV1 {
  if (!isObject(source)) throw new Error('Appearance export source must be an object.')
  exactKeys(source, SOURCE_KEYS, 'Appearance export source')
  if (!SHELLS.has(source.shellStyle)) throw new Error('Shell style is not recognized.')
  if (typeof source.panelDocked !== 'boolean') throw new Error('Panel docked state must be boolean.')
  if (!isObject(source.customWallpaper)) throw new Error('Custom wallpaper state must be an object.')

  const settings = normalizedSettings(source.settings)
  // `shellStyle` is the live source of truth; keep the optional legacy mirror coherent.
  if (settings.shell !== undefined) settings.shell = source.shellStyle
  let wallpaper: AppearanceWallpaperV1
  if (source.customWallpaper.kind === 'url') {
    exactKeys(source.customWallpaper, SOURCE_URL_KEYS, 'Custom URL wallpaper source')
    wallpaper = { kind: 'url', url: safeWallpaperUrl(source.customWallpaper.url) }
  } else if (source.customWallpaper.kind === 'file') {
    exactKeys(source.customWallpaper, SOURCE_FILE_KEYS, 'Custom file wallpaper source')
    wallpaper = validateWallpaper({
      kind: 'local-file',
      name: source.customWallpaper.name,
      mimeType: source.customWallpaper.mimeType,
      size: source.customWallpaper.size,
    })
  } else if (source.customWallpaper.kind === null) {
    exactKeys(source.customWallpaper, SOURCE_PRESET_KEYS, 'Preset wallpaper source')
    const id = WALLPAPER_IDS.has(settings.wallpaper) ? settings.wallpaper : DEFAULT_SETTINGS.wallpaper
    wallpaper = { kind: 'preset', id }
  } else {
    throw new Error('Custom wallpaper state is not recognized.')
  }
  return {
    schema: 'com.cybermanju.appearance',
    version: 1,
    settings,
    shellStyle: source.shellStyle,
    panelDocked: source.panelDocked,
    wallpaper,
  }
}

/** Serialize only a validated, allowlisted profile; no browser storage is consulted. */
export function serializeAppearanceProfileV1(source: AppearanceProfileSource): string {
  const json = JSON.stringify(buildAppearanceProfileV1(source), null, 2)
  if (new TextEncoder().encode(json).byteLength > 64 * 1024) {
    throw new Error('Appearance profile exceeds the 64 KiB limit.')
  }
  return json
}

/** Strictly parse a v1 profile, rejecting unknown fields and malformed variants. */
export function parseAppearanceProfileV1(json: string): AppearanceProfileV1 {
  if (typeof json !== 'string' || new TextEncoder().encode(json).byteLength > 64 * 1024) {
    throw new Error('Appearance profile exceeds the 64 KiB limit.')
  }
  let raw: unknown
  try {
    raw = JSON.parse(json)
  } catch {
    throw new Error('Appearance profile is not valid JSON.')
  }
  if (!isObject(raw)) throw new Error('Appearance profile must be an object.')
  exactKeys(raw, PROFILE_KEYS, 'Appearance profile')
  if (raw.schema !== 'com.cybermanju.appearance' || raw.version !== 1) {
    throw new Error('Appearance profile schema or version is unsupported.')
  }
  if (!SHELLS.has(raw.shellStyle as string)) throw new Error('Shell style is not recognized.')
  if (typeof raw.panelDocked !== 'boolean') throw new Error('Panel docked state must be boolean.')
  if (!isObject(raw.settings)) throw new Error('Appearance settings must be an object.')
  const settings = normalizedSettings(raw.settings as unknown as ThemeSettings, true)
  if (settings.shell !== undefined && settings.shell !== raw.shellStyle) {
    throw new Error('Settings shell mirror must match shellStyle.')
  }
  const wallpaper = validateWallpaper(raw.wallpaper)
  if (wallpaper.kind === 'preset' && wallpaper.id !== settings.wallpaper) {
    throw new Error('Preset wallpaper descriptor must match settings.wallpaper.')
  }
  return {
    schema: 'com.cybermanju.appearance',
    version: 1,
    settings,
    shellStyle: raw.shellStyle as ShellStyle,
    panelDocked: raw.panelDocked,
    wallpaper,
  }
}

/** Validate an import before showing its preview or touching live preferences. */
export function prepareAppearanceProfileRestore(json: string): AppearanceProfileV1 {
  const profile = parseAppearanceProfileV1(json)
  if (profile.wallpaper.kind === 'local-file') {
    throw new Error('This profile describes a local wallpaper, but does not contain its image bytes. The current wallpaper was not changed; choose an image on this device or import a URL/preset profile.')
  }
  return profile
}

export interface AppearanceProfileRestoreActions {
  /** Use the existing wallpaper service; false means it made no change. */
  applyWallpaper(wallpaper: Extract<AppearanceWallpaperV1, { kind: 'preset' | 'url' }>): Promise<boolean>
  /** Synchronous mutation through the app's existing preference owners. */
  applyPreferences(profile: AppearanceProfileV1): void
}

/** Apply an already-previewed profile only after its wallpaper operation succeeds. */
export async function applyAppearanceProfileRestore(
  candidate: AppearanceProfileV1,
  actions: AppearanceProfileRestoreActions,
): Promise<boolean> {
  // Re-parse the preview object too: callers cannot bypass the strict v1 contract
  // by mutating it between preview and confirmation.
  const profile = prepareAppearanceProfileRestore(JSON.stringify(candidate))
  if (profile.wallpaper.kind === 'local-file') {
    throw new Error('A local wallpaper cannot be restored without its image bytes.')
  }
  const wallpaperApplied = await actions.applyWallpaper(profile.wallpaper)
  if (!wallpaperApplied) return false
  actions.applyPreferences(profile)
  return true
}
