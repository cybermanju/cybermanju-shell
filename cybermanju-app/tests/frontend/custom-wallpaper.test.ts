// CyberManju OS — Custom wallpaper.test (tests/frontend/custom-wallpaper.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  customWallpaperDatabaseName,
  MAX_CUSTOM_WALLPAPER_BYTES,
  normalizeCustomWallpaperUrl,
  validateCustomWallpaperFile,
  type CustomWallpaperRecord,
  type CustomWallpaperStorage,
} from '@/utils/customWallpaper'
import { createCustomWallpaperController } from '@/composables/useCustomWallpaper'

function memoryStorage() {
  let value: CustomWallpaperRecord | null = null
  const storage: CustomWallpaperStorage = {
    read: async () => value,
    write: async (record) => { value = record },
    remove: async () => { value = null },
  }
  return { storage, snapshot: () => value }
}

function objectUrlHarness() {
  let sequence = 0
  const created: string[] = []
  const revoked: string[] = []
  return {
    created,
    revoked,
    api: {
      createObjectURL: (_blob: Blob) => {
        const value = `blob:wallpaper/${++sequence}`
        created.push(value)
        return value
      },
      revokeObjectURL: (value: string) => { revoked.push(value) },
    },
  }
}

describe('custom wallpaper configuration', () => {
  it('normalizes safe HTTP(S) URLs and rejects other schemes or credentials', () => {
    expect(normalizeCustomWallpaperUrl('  https://images.example/wall paper.png  '))
      .toBe('https://images.example/wall%20paper.png')
    expect(() => normalizeCustomWallpaperUrl('javascript:alert(1)')).toThrow(/HTTP or HTTPS/)
    expect(() => normalizeCustomWallpaperUrl('https://user:pass@images.example/bg.png')).toThrow(/username and password/)
    expect(() => normalizeCustomWallpaperUrl('   ')).toThrow(/image URL/)
  })

  it('accepts supported raster formats within the local size limit', () => {
    expect(() => validateCustomWallpaperFile({ size: 100, type: 'image/png' })).not.toThrow()
    expect(() => validateCustomWallpaperFile({ size: 100, type: 'image/avif' })).not.toThrow()
    expect(() => validateCustomWallpaperFile({ size: 100, type: 'image/svg+xml' })).toThrow(/PNG, JPEG/)
    expect(() => validateCustomWallpaperFile({ size: MAX_CUSTOM_WALLPAPER_BYTES + 1, type: 'image/png' }))
      .toThrow(/20 MiB/)
    expect(() => validateCustomWallpaperFile({ size: 0, type: 'image/png' })).toThrow(/empty/)
  })

  it('keeps the Pages preview database separate from the production root', () => {
    expect(customWallpaperDatabaseName(false)).toBe('cybermanju-wallpapers')
    expect(customWallpaperDatabaseName(true)).toBe('cybermanju-develop-wallpapers')
  })

  it('persists a selected file locally, restores it, and releases object URLs on reset', async () => {
    const store = memoryStorage()
    const urls = objectUrlHarness()
    const selected = createCustomWallpaperController(store.storage, urls.api)
    const file = Object.assign(new Blob(['image bytes'], { type: 'image/png' }), {
      name: 'desktop.png',
    }) as File

    expect(await selected.setFile(file)).toBe(true)
    expect(selected.kind.value).toBe('file')
    expect(selected.source.value).toBe('blob:wallpaper/1')
    expect(store.snapshot()).toMatchObject({ kind: 'file', name: 'desktop.png', mimeType: 'image/png' })

    const restored = createCustomWallpaperController(store.storage, urls.api)
    await restored.load()
    expect(restored.kind.value).toBe('file')
    expect(restored.fileName.value).toBe('desktop.png')
    expect(restored.source.value).toBe('blob:wallpaper/2')

    expect(await restored.clear()).toBe(true)
    expect(store.snapshot()).toBeNull()
    expect(restored.kind.value).toBeNull()
    expect(urls.revoked).toContain('blob:wallpaper/2')
  })

  it('stores a normalized URL and falls back safely if its image later fails to load', async () => {
    const store = memoryStorage()
    const urls = objectUrlHarness()
    const controller = createCustomWallpaperController(store.storage, urls.api)

    expect(await controller.setUrl(' https://images.example/wallpaper.png ')).toBe(true)
    expect(controller.source.value).toBe('https://images.example/wallpaper.png')
    expect(store.snapshot()).toEqual({ kind: 'url', url: 'https://images.example/wallpaper.png' })

    controller.imageFailed()
    expect(controller.source.value).toBeNull()
    expect(controller.kind.value).toBe('url')
    expect(controller.error.value).toMatch(/could not be loaded/)
  })
})
