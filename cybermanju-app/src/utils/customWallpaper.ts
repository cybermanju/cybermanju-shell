// CyberManju OS — CustomWallpaper (src/utils/customWallpaper.ts).
// Role: pure TS helpers (unit-tested, transport-agnostic).
// Exports: MAX_CUSTOM_WALLPAPER_BYTES, CustomWallpaperRecord, CustomWallpaperStorage, customWallpaperDatabaseName, normalizeCustomWallpaperUrl, validateCustomWallpaperFile, createIndexedDbCustomWallpaperStorage.
// Integrates: ./….
// Contracts: none special (pure logic; errors bubble to caller).
import { IS_DEVELOP_PREVIEW } from './deploymentScope'

export const MAX_CUSTOM_WALLPAPER_BYTES = 20 * 1024 * 1024

const ALLOWED_IMAGE_TYPES = new Set([
  'image/avif',
  'image/gif',
  'image/jpg',
  'image/jpeg',
  'image/png',
  'image/webp',
])

export type CustomWallpaperRecord =
  | { kind: 'url'; url: string }
  | { kind: 'file'; blob: Blob; name: string; mimeType: string }

export interface CustomWallpaperStorage {
  read(): Promise<CustomWallpaperRecord | null>
  write(record: CustomWallpaperRecord): Promise<void>
  remove(): Promise<void>
}

export function customWallpaperDatabaseName(isDevelopPreview: boolean): string {
  return isDevelopPreview ? 'cybermanju-develop-wallpapers' : 'cybermanju-wallpapers'
}

export function normalizeCustomWallpaperUrl(raw: string): string {
  const value = raw.trim()
  if (!value) throw new Error('Enter an image URL.')

  let url: URL
  try {
    url = new URL(value)
  } catch {
    throw new Error('Enter a valid HTTP(S) image URL.')
  }

  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error('Image URLs must use HTTP or HTTPS.')
  }
  if (url.username || url.password) {
    throw new Error('Remove username and password from the image URL.')
  }

  return url.href
}

export function validateCustomWallpaperFile(file: Pick<Blob, 'size' | 'type'>): void {
  if (file.size <= 0) throw new Error('The selected image is empty.')
  if (file.size > MAX_CUSTOM_WALLPAPER_BYTES) {
    throw new Error('Choose an image smaller than 20 MiB.')
  }
  if (!ALLOWED_IMAGE_TYPES.has(file.type.toLowerCase())) {
    throw new Error('Choose a PNG, JPEG, WebP, GIF, or AVIF image.')
  }
}

function isCustomWallpaperRecord(value: unknown): value is CustomWallpaperRecord {
  if (!value || typeof value !== 'object') return false
  const record = value as Record<string, unknown>
  if (record.kind === 'url') return typeof record.url === 'string'
  if (record.kind !== 'file') return false

  const blob = record.blob as Partial<Blob> | null
  return Boolean(
    blob &&
    typeof blob.size === 'number' &&
    typeof blob.type === 'string' &&
    typeof record.name === 'string' &&
    typeof record.mimeType === 'string',
  )
}

function openDatabase(factory: IDBFactory, databaseName: string): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = factory.open(databaseName, 1)
    request.onupgradeneeded = () => {
      const database = request.result
      if (!database.objectStoreNames.contains('wallpapers')) {
        database.createObjectStore('wallpapers')
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Wallpaper storage could not be opened.'))
  })
}

/**
 * Per-installation wallpaper store. Browser IndexedDB is available in WASM,
 * Tauri WebViews, and the Docker dashboard browser; no image is sent to the
 * server. The preview gets its own database on the same Pages origin.
 */
export function createIndexedDbCustomWallpaperStorage(
  databaseName = customWallpaperDatabaseName(IS_DEVELOP_PREVIEW),
  getFactory: () => IDBFactory | undefined = () =>
    typeof indexedDB === 'undefined' ? undefined : indexedDB,
): CustomWallpaperStorage {
  const key = 'active-custom-wallpaper'

  return {
    async read() {
      let factory: IDBFactory | undefined
      try {
        factory = getFactory()
      } catch {
        return null
      }
      if (!factory) return null

      const database = await openDatabase(factory, databaseName)
      try {
        return await new Promise<CustomWallpaperRecord | null>((resolve, reject) => {
          const transaction = database.transaction('wallpapers', 'readonly')
          const request = transaction.objectStore('wallpapers').get(key)
          let result: unknown = null
          request.onsuccess = () => { result = request.result ?? null }
          request.onerror = () => reject(request.error ?? new Error('Wallpaper could not be read.'))
          transaction.oncomplete = () => resolve(isCustomWallpaperRecord(result) ? result : null)
          transaction.onerror = () => reject(transaction.error ?? new Error('Wallpaper could not be read.'))
          transaction.onabort = () => reject(transaction.error ?? new Error('Wallpaper read was cancelled.'))
        })
      } finally {
        database.close()
      }
    },

    async write(record) {
      let factory: IDBFactory | undefined
      try {
        factory = getFactory()
      } catch {
        factory = undefined
      }
      if (!factory) throw new Error('Persistent local storage is unavailable in this app.')

      const database = await openDatabase(factory, databaseName)
      try {
        await new Promise<void>((resolve, reject) => {
          const transaction = database.transaction('wallpapers', 'readwrite')
          transaction.objectStore('wallpapers').put(record, key)
          transaction.oncomplete = () => resolve()
          transaction.onerror = () => reject(transaction.error ?? new Error('Wallpaper could not be saved.'))
          transaction.onabort = () => reject(transaction.error ?? new Error('Wallpaper save was cancelled.'))
        })
      } finally {
        database.close()
      }
    },

    async remove() {
      let factory: IDBFactory | undefined
      try {
        factory = getFactory()
      } catch {
        factory = undefined
      }
      if (!factory) return

      const database = await openDatabase(factory, databaseName)
      try {
        await new Promise<void>((resolve, reject) => {
          const transaction = database.transaction('wallpapers', 'readwrite')
          transaction.objectStore('wallpapers').delete(key)
          transaction.oncomplete = () => resolve()
          transaction.onerror = () => reject(transaction.error ?? new Error('Wallpaper could not be removed.'))
          transaction.onabort = () => reject(transaction.error ?? new Error('Wallpaper removal was cancelled.'))
        })
      } finally {
        database.close()
      }
    },
  }
}
