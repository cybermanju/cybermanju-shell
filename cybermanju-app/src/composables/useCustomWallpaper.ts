// CyberManju OS — UseCustomWallpaper (src/composables/useCustomWallpaper.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: createCustomWallpaperController, useCustomWallpaper.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { ref } from 'vue'
import {
  createIndexedDbCustomWallpaperStorage,
  normalizeCustomWallpaperUrl,
  validateCustomWallpaperFile,
  type CustomWallpaperRecord,
  type CustomWallpaperStorage,
} from '@/utils/customWallpaper'

type ObjectUrlApi = Pick<typeof URL, 'createObjectURL' | 'revokeObjectURL'>

export function createCustomWallpaperController(
  storage: CustomWallpaperStorage,
  objectUrls: ObjectUrlApi,
) {
  const source = ref<string | null>(null)
  const kind = ref<'url' | 'file' | null>(null)
  const url = ref('')
  const fileName = ref('')
  const fileMimeType = ref('')
  const fileSize = ref(0)
  const error = ref('')
  const busy = ref(false)
  let activeObjectUrl: string | null = null
  let loaded = false
  let loading: Promise<void> | null = null

  function releaseObjectUrl() {
    if (!activeObjectUrl) return
    objectUrls.revokeObjectURL(activeObjectUrl)
    activeObjectUrl = null
  }

  async function load(): Promise<void> {
    if (loaded) return
    if (loading) return loading

    loading = (async () => {
      busy.value = true
      try {
        const saved = await storage.read()
        if (!saved) return
        if (saved.kind === 'url') {
          const safeUrl = normalizeCustomWallpaperUrl(saved.url)
          source.value = safeUrl
          url.value = safeUrl
          kind.value = 'url'
        } else {
          validateCustomWallpaperFile(saved.blob)
          activeObjectUrl = objectUrls.createObjectURL(saved.blob)
          source.value = activeObjectUrl
          fileName.value = saved.name
          fileMimeType.value = saved.mimeType
          fileSize.value = saved.blob.size
          kind.value = 'file'
        }
        error.value = ''
      } catch {
        error.value = 'The saved wallpaper could not be loaded. Choose it again in Appearance settings.'
      } finally {
        loaded = true
        busy.value = false
        loading = null
      }
    })()
    return loading
  }

  async function setUrl(raw: string): Promise<boolean> {
    await load()
    let safeUrl: string
    try {
      safeUrl = normalizeCustomWallpaperUrl(raw)
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : 'Enter a valid HTTP(S) image URL.'
      return false
    }

    busy.value = true
    try {
      const record: CustomWallpaperRecord = { kind: 'url', url: safeUrl }
      await storage.write(record)
      releaseObjectUrl()
      source.value = safeUrl
      url.value = safeUrl
      fileName.value = ''
      fileMimeType.value = ''
      fileSize.value = 0
      kind.value = 'url'
      error.value = ''
      return true
    } catch {
      error.value = 'The wallpaper could not be saved on this device. Check available storage and try again.'
      return false
    } finally {
      busy.value = false
    }
  }

  async function setFile(file: File): Promise<boolean> {
    await load()
    try {
      validateCustomWallpaperFile(file)
    } catch (cause) {
      error.value = cause instanceof Error ? cause.message : 'Choose a supported image file.'
      return false
    }

    busy.value = true
    let nextObjectUrl: string | null = null
    try {
      const blob = file.slice(0, file.size, file.type)
      const record: CustomWallpaperRecord = {
        kind: 'file',
        blob,
        name: file.name,
        mimeType: file.type,
      }
      nextObjectUrl = objectUrls.createObjectURL(blob)
      await storage.write(record)
      releaseObjectUrl()
      activeObjectUrl = nextObjectUrl
      source.value = nextObjectUrl
      url.value = ''
      fileName.value = file.name
      fileMimeType.value = file.type.toLowerCase()
      fileSize.value = file.size
      kind.value = 'file'
      error.value = ''
      return true
    } catch {
      if (nextObjectUrl) objectUrls.revokeObjectURL(nextObjectUrl)
      error.value = 'The image could not be saved on this device. Check available storage and try again.'
      return false
    } finally {
      busy.value = false
    }
  }

  async function clear(): Promise<boolean> {
    await load()
    busy.value = true
    try {
      await storage.remove()
      releaseObjectUrl()
      source.value = null
      url.value = ''
      fileName.value = ''
      fileMimeType.value = ''
      fileSize.value = 0
      kind.value = null
      error.value = ''
      loaded = true
      return true
    } catch {
      error.value = 'The custom wallpaper could not be removed from this device.'
      return false
    } finally {
      busy.value = false
    }
  }

  function imageFailed() {
    releaseObjectUrl()
    source.value = null
    error.value = 'The wallpaper image could not be loaded. Check its URL or choose another image.'
  }

  return { source, kind, url, fileName, fileMimeType, fileSize, error, busy, load, setUrl, setFile, clear, imageFailed }
}

const customWallpaper = createCustomWallpaperController(
  createIndexedDbCustomWallpaperStorage(),
  URL,
)

export function useCustomWallpaper() {
  return customWallpaper
}
