// CyberManju OS — AppearanceProfile tests (tests/frontend/appearance-profile.test.ts).
// Role: verify the secret-free, versioned appearance export contract and its strict parser.
// Exports: Vitest tests for canonical fields, accents, URLs, local-file metadata, and round trips.
// Integrates: appearanceProfile, theme tokens, custom wallpaper validation.
// Contracts: local wallpaper bytes and device URLs never enter exported JSON.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import {
  applyAppearanceProfileRestore,
  buildAppearanceProfileV1,
  parseAppearanceProfileV1,
  prepareAppearanceProfileRestore,
  serializeAppearanceProfileV1,
  type AppearanceProfileSource,
} from '@/utils/appearanceProfile'
import { DEFAULT_SETTINGS } from '@/ui/tokens'
import { createCustomWallpaperController } from '@/composables/useCustomWallpaper'
import type { CustomWallpaperRecord } from '@/utils/customWallpaper'

const fixturePath = fileURLToPath(new URL('../fixtures/appearance-profile-v1.json', import.meta.url))

function source(overrides: Partial<AppearanceProfileSource> = {}): AppearanceProfileSource {
  return {
    settings: {
      ...DEFAULT_SETTINGS,
      theme: 'os-graphite',
      shell: 'plasma',
      accent: '#abcdef',
      accents: { 'plasma-light': '#123456', 'os-dark': '#aa11bb' },
      density: 'compact',
      glass: 0,
      motion: 'reduced',
      followSystem: false,
      wallpaper: 'dunes',
      glow: false,
    },
    shellStyle: 'plasma',
    panelDocked: true,
    customWallpaper: { kind: null },
    ...overrides,
  }
}

describe('appearance profile v1', () => {
  it('exports the complete canonical ThemeSettings and current shell/panel state', () => {
    const profile = buildAppearanceProfileV1(source())
    expect(profile).toEqual(JSON.parse(readFileSync(fixturePath, 'utf8')))
    expect(profile).toMatchObject({
      schema: 'com.cybermanju.appearance',
      version: 1,
      shellStyle: 'plasma',
      panelDocked: true,
      settings: {
        theme: 'os-graphite', accent: '#abcdef',
        accents: { 'plasma-light': '#123456', 'os-dark': '#aa11bb' },
        density: 'compact', glass: 0, motion: 'reduced', followSystem: false,
        wallpaper: 'dunes', glow: false, shell: 'plasma',
      },
      wallpaper: { kind: 'preset', id: 'dunes' },
    })
    expect(Object.keys(profile.settings).sort()).toEqual([
      'accent', 'accents', 'density', 'followSystem', 'glass', 'glow', 'motion',
      'shell', 'theme', 'wallpaper',
    ])
  })

  it('preserves global and per-theme accent overrides independently', () => {
    const result = buildAppearanceProfileV1(source())
    expect(result.settings.accent).toBe('#abcdef')
    expect(result.settings.accents).toEqual({ 'plasma-light': '#123456', 'os-dark': '#aa11bb' })
  })

  it('normalizes safe wallpaper URLs and refuses non-HTTP(S), credentials, and credential-like query keys', () => {
    const profile = buildAppearanceProfileV1(source({
      customWallpaper: { kind: 'url', url: 'HTTPS://cdn.example.test/wallpaper.png' },
    }))
    expect(profile.wallpaper).toEqual({ kind: 'url', url: 'https://cdn.example.test/wallpaper.png' })
    for (const url of [
      'file:///tmp/wallpaper.png',
      'blob:https://example.test/local-image',
      ' https://example.test/image.png',
      `https://example.test/${'a'.repeat(8200)}`,
      'https://example.test/image.png\n',
      'https://user:pass@example.test/image.png',
      'https://example.test/image.png?access_token=private',
      'https://example.test/image.png?api_key=private',
    ]) {
      expect(() => buildAppearanceProfileV1(source({ customWallpaper: { kind: 'url', url } }))).toThrow()
    }
  })

  it('exports local-file metadata only and never leaks object URLs, file URLs, paths, or bytes', () => {
    const profile = buildAppearanceProfileV1(source({
      customWallpaper: { kind: 'file', name: 'wallpaper.png', mimeType: 'image/png', size: 4096 },
    }))
    const json = JSON.stringify(profile)
    expect(profile.wallpaper).toEqual({
      kind: 'local-file', name: 'wallpaper.png', mimeType: 'image/png', size: 4096,
    })
    expect(json).not.toMatch(/blob:|file:|data:|\/home\/|\/tmp\//i)
    expect(json).not.toMatch(/"blob"|"source"|"path"|"bytes"/i)
    for (const metadata of [
      { name: '/home/user/private.png', mimeType: 'image/png', size: 4096 },
      { name: `${'é'.repeat(128)}.png`, mimeType: 'image/png', size: 4096 },
      { name: 'bad\nname.png', mimeType: 'image/png', size: 4096 },
      { name: 'empty.png', mimeType: 'image/png', size: 0 },
      { name: 'huge.png', mimeType: 'image/png', size: 20 * 1024 * 1024 + 1 },
      { name: 'bad.svg', mimeType: 'image/svg+xml', size: 100 },
    ]) {
      expect(() => buildAppearanceProfileV1(source({ customWallpaper: { kind: 'file', ...metadata } }))).toThrow()
    }
  })

  it('reads device-local file metadata from the existing wallpaper controller without exporting its blob URL or bytes', async () => {
    let stored: CustomWallpaperRecord | null = null
    const controller = createCustomWallpaperController({
      async read() { return stored },
      async write(record) { stored = record },
      async remove() { stored = null },
    }, {
      createObjectURL() { return 'blob:device-local-preview-only' },
      revokeObjectURL() {},
    })
    const file = new File(['private wallpaper bytes'], 'wallpaper.png', { type: 'image/png' })
    expect(await controller.setFile(file)).toBe(true)
    expect(controller.source.value).toBe('blob:device-local-preview-only')
    const json = serializeAppearanceProfileV1(source({
      customWallpaper: {
        kind: 'file',
        name: controller.fileName.value,
        mimeType: controller.fileMimeType.value,
        size: controller.fileSize.value,
      },
    }))
    expect(json).toContain('"mimeType": "image/png"')
    expect(json).toContain('"size": 23')
    expect(json).not.toMatch(/blob:|private wallpaper bytes|"blob"/)
  })

  it('rejects injected secret/config fields and malformed caller input instead of silently succeeding', () => {
    const withExtra = {
      ...source(),
      settings: { ...source().settings, serverUrl: 'https://private.example.test', apiKey: 'secret' },
    } as unknown as AppearanceProfileSource
    expect(() => buildAppearanceProfileV1(withExtra)).toThrow(/unsupported field/)
    expect(() => buildAppearanceProfileV1(source({ panelDocked: 'yes' as unknown as boolean }))).toThrow()
  })

  it('strictly validates parsed JSON and round-trips the shared fixture', () => {
    const fixture = readFileSync(fixturePath, 'utf8')
    const parsed = parseAppearanceProfileV1(fixture)
    expect(parseAppearanceProfileV1(serializeAppearanceProfileV1(source()))).toEqual(parsed)
    expect(() => parseAppearanceProfileV1(fixture.replace('"version": 1', '"version": 2'))).toThrow(/schema or version/)
    expect(() => parseAppearanceProfileV1(fixture.replace('"panelDocked": true', '"panelDocked": "yes"'))).toThrow()
    expect(() => parseAppearanceProfileV1(fixture.replace('"shell": "plasma"', '"shell": "macos"'))).toThrow(/must match shellStyle/)
    expect(() => parseAppearanceProfileV1(fixture.replace('"id": "dunes"', '"id": "slopes-dark"'))).toThrow(/must match settings.wallpaper/)
    expect(() => parseAppearanceProfileV1(fixture.replace('"shellStyle": "plasma"', '"shellStyle": "plasma", "authToken": "nope"'))).toThrow(/unsupported field/)
    expect(() => parseAppearanceProfileV1(`${fixture.trimEnd()}${' '.repeat(64 * 1024)}`)).toThrow(/64 KiB/)
    expect(() => parseAppearanceProfileV1('{')).toThrow(/valid JSON/)
  })

  it('normalizes legacy glass levels during strict v1 import parsing', () => {
    const profile = JSON.parse(readFileSync(fixturePath, 'utf8'))
    profile.settings.glass = 3
    expect(parseAppearanceProfileV1(JSON.stringify(profile)).settings.glass).toBe(2)
  })

  it('refuses local-file restore because exports contain no bytes and leaves wallpaper untouched', async () => {
    let stored: CustomWallpaperRecord | null = { kind: 'url', url: 'https://old.example.test/wallpaper.png' }
    const controller = createCustomWallpaperController({
      async read() { return stored },
      async write(record) { stored = record },
      async remove() { stored = null },
    }, {
      createObjectURL() { return 'blob:local-file' },
      revokeObjectURL() {},
    })
    await controller.load()
    const before = { source: controller.source.value, kind: controller.kind.value, url: controller.url.value }
    const json = serializeAppearanceProfileV1(source({
      customWallpaper: { kind: 'file', name: 'wallpaper.png', mimeType: 'image/png', size: 4096 },
    }))
    const currentPreferences = { theme: 'os-dark' }
    await expect(Promise.resolve().then(() => prepareAppearanceProfileRestore(json))).rejects.toThrow(/does not contain its image bytes/)
    expect(controller.source.value).toBe(before.source)
    expect(controller.kind.value).toBe(before.kind)
    expect(controller.url.value).toBe(before.url)
    expect(currentPreferences).toEqual({ theme: 'os-dark' })
  })

  it('keeps preferences and the active wallpaper unchanged when the existing wallpaper service fails', async () => {
    let stored: CustomWallpaperRecord | null = { kind: 'url', url: 'https://old.example.test/wallpaper.png' }
    const controller = createCustomWallpaperController({
      async read() { return stored },
      async write() { throw new Error('simulated storage failure') },
      async remove() { stored = null },
    }, {
      createObjectURL() { return 'blob:unused' },
      revokeObjectURL() {},
    })
    await controller.load()
    const beforeWallpaper = { source: controller.source.value, kind: controller.kind.value, url: controller.url.value }
    const preferences = { theme: 'os-dark', shellStyle: 'macos', panelDocked: false }
    const beforePreferences = { ...preferences }
    const candidate = buildAppearanceProfileV1(source({
      customWallpaper: { kind: 'url', url: 'https://new.example.test/wallpaper.png' },
    }))
    const restored = await applyAppearanceProfileRestore(candidate, {
      applyWallpaper: (wallpaper) => wallpaper.kind === 'url'
        ? controller.setUrl(wallpaper.url)
        : Promise.resolve(controller.clear()),
      applyPreferences(profile) {
        preferences.theme = profile.settings.theme
        preferences.shellStyle = profile.shellStyle
        preferences.panelDocked = profile.panelDocked
      },
    })
    expect(restored).toBe(false)
    expect(preferences).toEqual(beforePreferences)
    expect(controller.source.value).toBe(beforeWallpaper.source)
    expect(controller.kind.value).toBe(beforeWallpaper.kind)
    expect(controller.url.value).toBe(beforeWallpaper.url)
    expect(controller.error.value).toMatch(/could not be saved/)
  })

  it('restores a URL profile through the wallpaper service before applying normalized preferences', async () => {
    const candidate = buildAppearanceProfileV1(source({
      customWallpaper: { kind: 'url', url: 'https://cdn.example.test/image.png' },
      settings: { ...source().settings, glass: 3 as 0 },
    }))
    const events: string[] = []
    const restored = await applyAppearanceProfileRestore(candidate, {
      async applyWallpaper(wallpaper) {
        events.push(`wallpaper:${wallpaper.kind}`)
        return true
      },
      applyPreferences(profile) {
        events.push(`preferences:${profile.settings.glass}`)
      },
    })
    expect(restored).toBe(true)
    expect(events).toEqual(['wallpaper:url', 'preferences:2'])
  })
})
