// CyberManju OS — MobileScopedStorage (src/utils/mobileScopedStorage.ts).
// Role: native SAF bridge + live vault mirror state — scoped-folder CRUD
// through `tauri-plugin-scoped-storage-api` (dynamic import, native builds
// only) plus the debounced `.cybermanju` mirror snapshot over
// `snapshot_native_database`.
// Exports: MobileFolderHandle, MobileFolderEntry, mobileVaultMirrorState,
// supportsNativeScopedStorage, normalizeMobileVaultFileName, pickMobileFolder,
// listMobileFolder, readMobileFolderFile, writeMobileFolderFile,
// removeMobileFolderEntry, configureMobileVaultMirror, unlockMobileVaultMirror,
// lockMobileVaultMirror, syncMobileVaultMirrorNow, scheduleMobileVaultMirrorSync,
// flushMobileVaultMirror.
// Integrates: vue (reactive), @/composables/useTauri, @/utils/container,
// tauri-plugin-scoped-storage-api + @tauri-apps/api/core (dynamic imports).
// Contracts: SAF picker cancel rejects with `ScopedStorageError
// { code: 'CANCELLED' }` (not a DOMException AbortError) — callers must treat
// cancel as a no-op; the mirror passphrase lives in memory only (never
// persisted) and errors surface as plain messages for the UI to map.
import { reactive } from 'vue'
import { isAndroidApp, isTauri } from '@/composables/useTauri'
import {
  decodeContainer,
  encodeContainer,
  isContainer,
  isEncryptedContainer,
} from '@/utils/container'

export interface MobileFolderHandle {
  id: string
  name: string
  uri?: string
}

export interface MobileFolderEntry {
  name: string
  path: string
  isFile: boolean
  isDir: boolean
  size?: number | null
  lastModified?: number | null
}

interface PersistedMirrorConfig {
  folderId: string
  folderName: string
  fileName: string
  lastSyncedAt: number | null
}

const MIRROR_CONFIG_KEY = 'cybermanju.mobileVaultMirror.v1'
const MIN_PASSPHRASE_LENGTH = 8
const MIRROR_DEBOUNCE_MS = 700

function readPersistedConfig(): PersistedMirrorConfig | null {
  try {
    const raw = localStorage.getItem(MIRROR_CONFIG_KEY)
    if (!raw) return null
    const value = JSON.parse(raw) as Partial<PersistedMirrorConfig>
    if (typeof value.folderId !== 'string' || !value.folderId ||
        typeof value.fileName !== 'string' || !value.fileName) return null
    return {
      folderId: value.folderId,
      folderName: typeof value.folderName === 'string' ? value.folderName : 'Phone folder',
      fileName: value.fileName,
      lastSyncedAt: typeof value.lastSyncedAt === 'number' ? value.lastSyncedAt : null,
    }
  } catch {
    return null
  }
}

let mirrorConfig: PersistedMirrorConfig | null = readPersistedConfig()
let activePassphrase: string | null = null
let syncTimer: ReturnType<typeof setTimeout> | null = null
let syncPromise: Promise<void> | null = null
let syncAgain = false

export const mobileVaultMirrorState = reactive({
  configured: !!mirrorConfig,
  folderName: mirrorConfig?.folderName ?? '',
  fileName: mirrorConfig?.fileName ?? '',
  unlocked: false,
  busy: false,
  lastSyncedAt: mirrorConfig?.lastSyncedAt ?? null as number | null,
  lastError: '',
})

/** The scoped-storage bridge is intentionally limited to native phone/tablet builds. */
export function supportsNativeScopedStorage(): boolean {
  if (!isTauri()) return false
  try {
    const ua = navigator.userAgent || ''
    const ipadOs = navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1
    return isAndroidApp() || /iPhone|iPad|iPod/i.test(ua) || ipadOs
  } catch {
    return isAndroidApp()
  }
}

/** Normalize a user-visible basename and prevent path traversal through SAF. */
export function normalizeMobileVaultFileName(input: string): string {
  const trimmed = input.trim()
  const basename = trimmed.replace(/(?:\.cybermanju)+$/i, '')
  if (!basename || !/^[\p{L}\p{N}][\p{L}\p{N}._ -]{0,79}$/u.test(basename) || basename === '..') {
    throw new Error('Use a simple file name (letters, numbers, spaces, dots, hyphens or underscores).')
  }
  return `${basename}.cybermanju`
}

export async function pickMobileFolder(): Promise<MobileFolderHandle> {
  if (!supportsNativeScopedStorage()) {
    throw new Error('Choose a folder from the native Android or iOS app to grant persistent access.')
  }
  const { pickFolder } = await import('tauri-plugin-scoped-storage-api')
  const folder = await pickFolder()
  if (!folder?.id) throw new Error('The system folder picker returned no folder handle.')
  return { id: String(folder.id), name: String(folder.name || 'Folder'), uri: folder.uri ?? undefined }
}

export async function listMobileFolder(folderId: string, path = ''): Promise<MobileFolderEntry[]> {
  const { readDir } = await import('tauri-plugin-scoped-storage-api')
  return await readDir(folderId, path) as MobileFolderEntry[]
}

export async function readMobileFolderFile(folderId: string, path: string): Promise<Uint8Array> {
  const { readFile } = await import('tauri-plugin-scoped-storage-api')
  const data = await readFile(folderId, path)
  return data instanceof Uint8Array ? data : new Uint8Array(data)
}

export async function writeMobileFolderFile(folderId: string, path: string, data: Uint8Array): Promise<void> {
  const { writeFile } = await import('tauri-plugin-scoped-storage-api')
  await writeFile(folderId, path, data, { recursive: true })
}

export async function removeMobileFolderEntry(folderId: string, path: string): Promise<void> {
  const { stat, removeDir, removeFile } = await import('tauri-plugin-scoped-storage-api')
  const entry = await stat(folderId, path)
  if (entry.isDir) await removeDir(folderId, path, true)
  else await removeFile(folderId, path)
}

function persistConfig(): void {
  if (!mirrorConfig) return
  try {
    localStorage.setItem(MIRROR_CONFIG_KEY, JSON.stringify(mirrorConfig))
  } catch {
    // Keep the in-memory mirror usable when platform storage is temporarily unavailable.
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function requirePassphrase(passphrase: string): void {
  if (passphrase.length < MIN_PASSPHRASE_LENGTH) {
    throw new Error(`Choose a passphrase with at least ${MIN_PASSPHRASE_LENGTH} characters.`)
  }
}

function fileExists(entries: MobileFolderEntry[], fileName: string): boolean {
  return entries.some(entry => {
    const leaf = (entry.path || entry.name).split(/[\\/]/).pop()
    return entry.isFile && (entry.name === fileName || leaf === fileName)
  })
}

/**
 * Configure a new live mirror. It stores only the SAF folder handle and file
 * name; the passphrase remains in memory and is never written to app storage.
 */
export async function configureMobileVaultMirror(
  folder: MobileFolderHandle,
  requestedFileName: string,
  passphrase: string,
): Promise<void> {
  if (!supportsNativeScopedStorage()) throw new Error('Vault Mirror requires the native Android or iOS app.')
  requirePassphrase(passphrase)
  const fileName = normalizeMobileVaultFileName(requestedFileName)

  const sameTarget = mirrorConfig?.folderId === folder.id && mirrorConfig.fileName === fileName
  if (sameTarget) {
    await unlockMobileVaultMirror(passphrase)
    return
  }

  const entries = await listMobileFolder(folder.id)
  if (fileExists(entries, fileName)) {
    throw new Error(`“${fileName}” already exists in this folder. Choose another name or unlock the configured mirror in Settings.`)
  }

  mirrorConfig = {
    folderId: folder.id,
    folderName: folder.name,
    fileName,
    lastSyncedAt: null,
  }
  activePassphrase = passphrase
  mobileVaultMirrorState.configured = true
  mobileVaultMirrorState.folderName = folder.name
  mobileVaultMirrorState.fileName = fileName
  mobileVaultMirrorState.unlocked = true
  mobileVaultMirrorState.lastSyncedAt = null
  mobileVaultMirrorState.lastError = ''
  persistConfig()
  await syncMobileVaultMirrorNow()
}

/** Verify an existing mirror's passphrase before permitting any overwrite. */
export async function unlockMobileVaultMirror(passphrase: string): Promise<void> {
  if (!mirrorConfig) throw new Error('No mobile vault mirror is configured yet.')
  requirePassphrase(passphrase)
  mobileVaultMirrorState.busy = true
  mobileVaultMirrorState.lastError = ''
  try {
    const entries = await listMobileFolder(mirrorConfig.folderId)
    if (fileExists(entries, mirrorConfig.fileName)) {
      const bytes = await readMobileFolderFile(mirrorConfig.folderId, mirrorConfig.fileName)
      if (!isContainer(bytes) || !isEncryptedContainer(bytes)) {
        throw new Error('The selected file is not an encrypted web-compatible .cybermanju mirror.')
      }
      // Decrypt only for verification; the live database remains authoritative.
      await decodeContainer(bytes, passphrase)
    }
    activePassphrase = passphrase
    mobileVaultMirrorState.unlocked = true
  } catch (error) {
    activePassphrase = null
    mobileVaultMirrorState.unlocked = false
    mobileVaultMirrorState.lastError = errorMessage(error)
    mobileVaultMirrorState.busy = false
    throw error
  }
  try {
    // A missing file is safely recreated from the authoritative live database.
    await syncMobileVaultMirrorNow()
  } catch (error) {
    mobileVaultMirrorState.lastError = errorMessage(error)
    throw error
  } finally {
    mobileVaultMirrorState.busy = false
  }
}

/** Lock the mirror in this app session without removing its SAF configuration. */
export function lockMobileVaultMirror(): void {
  activePassphrase = null
  mobileVaultMirrorState.unlocked = false
  mobileVaultMirrorState.lastError = ''
}

/** Snapshot the native redb database and overwrite its SAF file through the plugin. */
export async function syncMobileVaultMirrorNow(): Promise<void> {
  if (!mirrorConfig || !activePassphrase) return
  if (!supportsNativeScopedStorage()) throw new Error('Vault Mirror requires the native Android or iOS app.')
  if (syncPromise) {
    syncAgain = true
    return syncPromise
  }

  mobileVaultMirrorState.busy = true
  mobileVaultMirrorState.lastError = ''
  const run = async () => {
    try {
      do {
        syncAgain = false
        const config = mirrorConfig
        const passphrase = activePassphrase
        if (!config || !passphrase || !mobileVaultMirrorState.unlocked) return
        const { invoke } = await import('@tauri-apps/api/core')
        const snapshot = await invoke<number[] | Uint8Array>('snapshot_native_database')
        const image = snapshot instanceof Uint8Array ? snapshot : Uint8Array.from(snapshot)
        const container = await encodeContainer(image, passphrase)
        if (activePassphrase !== passphrase || mirrorConfig !== config || !mobileVaultMirrorState.unlocked) return
        await writeMobileFolderFile(config.folderId, config.fileName, container)
        const timestamp = Date.now()
        mirrorConfig = { ...config, lastSyncedAt: timestamp }
        mobileVaultMirrorState.lastSyncedAt = timestamp
        persistConfig()
      } while (syncAgain)
    } catch (error) {
      mobileVaultMirrorState.lastError = errorMessage(error)
      throw error
    } finally {
      mobileVaultMirrorState.busy = false
    }
  }
  syncPromise = run().finally(() => { syncPromise = null })
  return syncPromise
}

/** Debounced hook for successful database mutations in the Tauri bridge. */
export function scheduleMobileVaultMirrorSync(): void {
  if (!mirrorConfig || !activePassphrase || !mobileVaultMirrorState.unlocked) return
  if (syncTimer) clearTimeout(syncTimer)
  syncTimer = setTimeout(() => {
    syncTimer = null
    void syncMobileVaultMirrorNow().catch(() => { /* status is exposed on mobileVaultMirrorState */ })
  }, MIRROR_DEBOUNCE_MS)
}

/** Flush pending mutations immediately, e.g. when the app is backgrounded. */
export async function flushMobileVaultMirror(): Promise<void> {
  if (syncTimer) {
    clearTimeout(syncTimer)
    syncTimer = null
  }
  await syncMobileVaultMirrorNow()
}
