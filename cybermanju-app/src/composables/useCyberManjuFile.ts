// CyberManju OS — `.cybermanju` file binding (browser side)
//
// Three layers, in order of what the browser allows:
//
//   1. File System Access API (Chromium): pick/create a real file on disk.
//      The `FileSystemFileHandle` is stored in IndexedDB, posted to the DB
//      worker, and every save writes an encoded container back to *that* file.
//   2. Export / import downloads: works everywhere. The container bytes come
//      out of the worker (`_export`) and land in a download; import re-opens
//      them from a `<input type=file>` file.
//   3. Nothing at all (no worker/OPFS): the vault stays in the browser
//      session and the UI says so instead of failing silently.
//
// Passphrases are never persisted — they live for the session only, which is
// the whole point of encrypting the file in the first place.

import { reactive } from 'vue'
import { idbGet, idbSet, idbDel } from '@/utils/idb'
import { isEncryptedContainer } from '@/utils/container'
import {
  wasmAttachDisk,
  wasmDetachDisk,
  wasmDiskStatus,
  wasmExportDisk,
  wasmSaveDisk,
  type WasmDiskStatus,
} from './useWasmBackend'

const HANDLE_KEY = 'cybermanju.diskHandle'

const PICKER_TYPES = [
  {
    description: 'CyberManju vault',
    accept: { 'application/octet-stream': ['.cybermanju'] },
  },
] as const

interface PickerWindow {
  showOpenFilePicker?: (opts?: Record<string, unknown>) => Promise<FileSystemFileHandle[]>
  showSaveFilePicker?: (opts?: Record<string, unknown>) => Promise<FileSystemFileHandle>
}

interface DiskState {
  supported: boolean
  busy: boolean
  bound: boolean
  attached: boolean
  name: string
  savedAt: number
  savedBytes: number
  dirty: boolean
  booted: boolean
  /** A remembered file still needs a browser permission grant (after reload). */
  needsPermission: boolean
  /** Remembered file is encrypted and this session has no passphrase yet. */
  needsPassphrase: boolean
  lastError: string | null
  lastMessage: string | null
}

export const disk = reactive<DiskState>({
  supported: false,
  busy: false,
  bound: false,
  attached: false,
  name: '',
  savedAt: 0,
  savedBytes: 0,
  dirty: false,
  booted: false,
  needsPermission: false,
  needsPassphrase: false,
  lastError: null,
  lastMessage: null,
})

/** File waiting on a permission grant or a passphrase (non-reactive bytes). */
let pending: {
  handle: FileSystemFileHandle | null
  name: string
  bytes: Uint8Array | null
  encrypted: boolean
} | null = null

let bootPromise: Promise<void> | null = null

function pickerWindow(): PickerWindow {
  return window as unknown as PickerWindow
}

export function diskSupported(): boolean {
  if (typeof window === 'undefined' || window.isSecureContext === false) return false
  return typeof pickerWindow().showOpenFilePicker === 'function'
}

function msg(text: string) {
  disk.lastMessage = text
  disk.lastError = null
}

function fail(e: unknown): never {
  const text = e instanceof Error ? e.message : String(e)
  disk.lastError = text
  throw e instanceof Error ? e : new Error(text)
}

function apply(status: WasmDiskStatus, bound: boolean): WasmDiskStatus {
  disk.attached = status.attached
  disk.bound = bound && status.attached
  disk.name = status.name
  disk.savedAt = status.savedAt
  disk.savedBytes = status.savedBytes
  disk.dirty = status.dirty
  if (status.lastError) disk.lastError = status.lastError
  return status
}

async function readHandleBytes(handle: FileSystemFileHandle): Promise<Uint8Array> {
  const file = await handle.getFile()
  return new Uint8Array(await file.arrayBuffer())
}

async function queryPermission(handle: FileSystemFileHandle): Promise<PermissionState> {
  const h = handle as FileSystemFileHandle & {
    queryPermission?: (desc: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>
  }
  try {
    return (await h.queryPermission?.({ mode: 'readwrite' })) ?? 'prompt'
  } catch {
    return 'prompt'
  }
}

async function requestPermission(handle: FileSystemFileHandle): Promise<boolean> {
  const h = handle as FileSystemFileHandle & {
    requestPermission?: (desc: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>
  }
  try {
    const state = await h.requestPermission?.({ mode: 'readwrite' })
    return state === 'granted'
  } catch {
    return false
  }
}

/**
 * Hand bytes to the worker. Encrypted files without this session's passphrase
 * park themselves in `pending` and the UI asks — nothing is touched until the
 * passphrase arrives (and a wrong one fails cleanly before any restore).
 */
async function attach(
  handle: FileSystemFileHandle | null,
  name: string,
  bytes: Uint8Array | null,
  passphrase: string,
): Promise<void> {
  const encrypted = bytes ? isEncryptedContainer(bytes) : false
  if (encrypted && !passphrase) {
    pending = { handle, name, bytes, encrypted }
    disk.needsPassphrase = true
    msg(`${name} is encrypted — enter its passphrase to open it`)
    return
  }
  try {
    const status = await wasmAttachDisk({ handle, name, bytes, passphrase })
    pending = null
    disk.needsPassphrase = false
    disk.needsPermission = false
    if (handle) await idbSet(HANDLE_KEY, handle)
    apply(status, !!handle)
    if (!handle) msg(`${name} imported (session only) — use CREATE FILE to bind it to disk`)
    else if (bytes) msg(`${name} opened (${status.savedBytes} bytes on disk)`)
    else msg(`${name} bound — saving writes the vault into it`)
  } catch (e) {
    disk.lastError = e instanceof Error ? e.message : String(e)
    if (passphrase && /passphrase|decrypt/i.test(disk.lastError)) disk.needsPassphrase = true
    throw e
  }
}

/** Restore the remembered handle on startup (permission/passphrase gated). */
export function bootCyberManjuDisk(): Promise<void> {
  if (bootPromise) return bootPromise
  bootPromise = (async () => {
    disk.supported = diskSupported()
    try {
      const handle = await idbGet<FileSystemFileHandle>(HANDLE_KEY)
      if (!handle) return
      disk.name = handle.name
      const perm = await queryPermission(handle)
      if (perm !== 'granted') {
        pending = { handle, name: handle.name, bytes: null, encrypted: false }
        disk.needsPermission = true
        msg(`${handle.name} is remembered — the browser wants a permission click to re-open it`)
        return
      }
      const bytes = await readHandleBytes(handle)
      await attach(handle, handle.name, bytes, '')
    } catch (e) {
      disk.lastError = e instanceof Error ? e.message : String(e)
    } finally {
      disk.booted = true
    }
  })()
  return bootPromise
}

/** User click that grants readwrite and then opens the remembered file. */
export async function reattachCyberManjuDisk(passphrase = ''): Promise<boolean> {
  const target = pending
  if (!target?.handle) return false
  disk.busy = true
  try {
    if (disk.needsPermission) {
      if (!(await requestPermission(target.handle))) {
        disk.lastError = 'permission denied — the file stays remembered but closed'
        return false
      }
      disk.needsPermission = false
      target.bytes = await readHandleBytes(target.handle)
    }
    await attach(target.handle, target.handle.name, target.bytes, passphrase)
    return !disk.needsPassphrase
  } catch (e) {
    if (e instanceof Error) disk.lastError = e.message
    return false
  } finally {
    disk.busy = false
  }
}

export async function openCyberManjuFile(): Promise<boolean> {
  if (!diskSupported()) {
    disk.supported = false
    disk.lastError = 'this browser has no File System Access API — use IMPORT / EXPORT instead'
    return false
  }
  try {
    const [handle] = await pickerWindow().showOpenFilePicker!({
      multiple: false,
      types: [...PICKER_TYPES],
    })
    if (!handle) return false
    disk.busy = true
    const bytes = await readHandleBytes(handle)
    await attach(handle, handle.name, bytes, '')
    return true
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return false
    fail(e)
    return false
  } finally {
    disk.busy = false
  }
}

/**
 * One-folder setup: create/open `vault.cybermanju` inside an already-picked
 * directory handle (`showDirectoryPicker`). The `files/` sync root lives
 * beside it and is never a sync source itself (see `vaultFolder.ts`).
 */
export async function createCyberManjuFileInDirectory(
  dir: FileSystemDirectoryHandle,
  filename = 'vault.cybermanju',
  passphrase = '',
): Promise<{ fileHandle: FileSystemFileHandle; dirName: string } | null> {
  if (!dir || typeof dir.getFileHandle !== 'function') return null
  try {
    const handle = await dir.getFileHandle(filename, { create: true })
    if (!handle) return null
    disk.busy = true
    // Existing file → open its bytes; fresh file → keep live DB, save into it.
    let bytes: Uint8Array | null = null
    try {
      const f = await handle.getFile()
      if (f.size > 0) bytes = new Uint8Array(await f.arrayBuffer())
    } catch {
      bytes = null
    }
    await attach(handle, handle.name, bytes, passphrase)
    if (!bytes) await saveCyberManjuFile()
    const dirName = (dir as FileSystemDirectoryHandle & { name?: string }).name ?? ''
    return { fileHandle: handle, dirName }
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return null
    fail(e)
    return null
  } finally {
    disk.busy = false
  }
}

export async function createCyberManjuFile(passphrase = ''): Promise<boolean> {
  if (!diskSupported()) {
    disk.supported = false
    disk.lastError = 'this browser has no File System Access API — use EXPORT instead'
    return false
  }
  try {
    const handle = await pickerWindow().showSaveFilePicker!({
      suggestedName: disk.name || 'vault.cybermanju',
      types: [...PICKER_TYPES],
    })
    if (!handle) return false
    disk.busy = true
    // No bytes → the live database is kept and written into the new file.
    await attach(handle, handle.name, null, passphrase)
    await saveCyberManjuFile()
    return true
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return false
    fail(e)
    return false
  } finally {
    disk.busy = false
  }
}

export async function saveCyberManjuFile(): Promise<boolean> {
  if (!disk.bound) {
    disk.lastError = 'no .cybermanju file bound — CREATE FILE or IMPORT one first'
    return false
  }
  disk.busy = true
  try {
    const status = await wasmSaveDisk()
    apply(status, true)
    msg(`${status.name} saved (${status.savedBytes} bytes, ${new Date(status.savedAt).toLocaleTimeString()})`)
    return true
  } catch (e) {
    disk.lastError = e instanceof Error ? e.message : String(e)
    return false
  } finally {
    disk.busy = false
  }
}

/** Download the container — the fallback when there is no FSA API. */
export async function exportCyberManjuFile(passphrase?: string): Promise<boolean> {
  disk.busy = true
  try {
    const { bytes, name } = await wasmExportDisk(passphrase)
    const blob = new Blob([bytes as unknown as BlobPart], { type: 'application/octet-stream' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = name || 'vault.cybermanju'
    document.body.appendChild(a)
    a.click()
    a.remove()
    setTimeout(() => URL.revokeObjectURL(url), 10_000)
    msg(`${a.download} exported (${bytes.byteLength} bytes)`)
    return true
  } catch (e) {
    disk.lastError = e instanceof Error ? e.message : String(e)
    return false
  } finally {
    disk.busy = false
  }
}

/** Open a `.cybermanju` picked through `<input type=file>` (all browsers). */
export async function importCyberManjuFile(file: File): Promise<boolean> {
  disk.busy = true
  try {
    const bytes = new Uint8Array(await file.arrayBuffer())
    await attach(null, file.name, bytes, '')
    return !disk.needsPassphrase
  } catch (e) {
    disk.lastError = e instanceof Error ? e.message : String(e)
    return false
  } finally {
    disk.busy = false
  }
}

export async function detachCyberManjuFile(): Promise<void> {
  disk.busy = true
  try {
    pending = null
    disk.needsPassphrase = false
    disk.needsPermission = false
    const status = await wasmDetachDisk()
    apply(status, false)
    await idbDel(HANDLE_KEY)
    disk.name = ''
    msg('file detached — the vault keeps living in this browser session')
  } catch (e) {
    disk.lastError = e instanceof Error ? e.message : String(e)
  } finally {
    disk.busy = false
  }
}

/** Refresh disk fields from the worker (called by periodic UI ticks). */
export async function refreshCyberManjuDisk(): Promise<void> {
  try {
    const status = await wasmDiskStatus()
    apply(status, disk.bound && status.attached)
  } catch {
    // Worker may be booting; the next tick will catch up.
  }
}
