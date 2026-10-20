// CyberManju OS — remembered folder handles (native file access in browsers).
//
// Setup and Accounts remember *directory* handles (`showDirectoryPicker`):
// the one-folder `files/` sync root (`cybermanju.wizardLocalDir`) and one
// per local provider (`cybermanju.localDir:<configId>`). Handles persist in
// IndexedDB across reloads, but the browser gates *permission* per origin —
// after a reload a handle comes back as `prompt`, and only a user gesture
// may re-grant it. This module owns that whole lifecycle:
//
//   boot (no gesture)  → `ensureLocalDir` queries only: `ready` stays live,
//                        `prompt` parks as `needs-permission` (one click,
//                        never a re-pick);
//   user click         → `reallowLocalDir` requests readwrite and the folder
//                        works again — create, edit, read, delete, all real
//                        File System Access I/O, no server involved.
//
// Desktop (Tauri) and Docker-server paths never come here: native dialogs
// return absolute paths with no permission model, and server files are
// addressed over REST. This module is the Chromium/browser leg only, and
// every failure carries the house prefix the UI already renders.

// Minimal structural types (the TS DOM lib varies by version; these are
// the exact members used — real handles satisfy them structurally).
export interface FsFileHandle {
  readonly name: string
  getFile(): Promise<{ arrayBuffer(): Promise<ArrayBuffer>; size: number }>
  createWritable(): Promise<{
    write(data: Uint8Array): Promise<void>
    close(): Promise<void>
  }>
}

export interface FsDirHandle {
  readonly name: string
  keys(): AsyncIterableIterator<string>
  getFileHandle(name: string, opts?: { create?: boolean }): Promise<FsFileHandle>
  getDirectoryHandle(name: string, opts?: { create?: boolean }): Promise<FsDirHandle>
  removeEntry(name: string, opts?: { recursive?: boolean }): Promise<void>
  queryPermission?: (desc: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>
  requestPermission?: (desc: { mode: 'read' | 'readwrite' }) => Promise<PermissionState>
}

export type LocalDirStatus = 'ready' | 'needs-permission' | 'missing' | 'unsupported'

export interface LocalDirState {
  key: string
  label: string
  status: LocalDirStatus
  detail: string
}

export interface LocalDirEntry {
  name: string
  isDir: boolean
  sizeBytes: number
}

export const WIZARD_LOCAL_DIR_KEY = 'cybermanju.wizardLocalDir'
export const localDirKey = (configId: string): string => `cybermanju.localDir:${configId}`

import { reactive } from 'vue'
import { idbDel, idbGet, idbKeys, idbSet } from './idb'

/** Live registry for pickers and status dots (key → state). */
export const localDirs = reactive<Record<string, LocalDirState>>({})

function setState(key: string, label: string, status: LocalDirStatus, detail: string): void {
  localDirs[key] = { key, label, status, detail }
}

export function dirPickerSupported(): boolean {
  try {
    if (typeof window === 'undefined' || window.isSecureContext === false) return false
    return typeof (window as unknown as { showDirectoryPicker?: unknown }).showDirectoryPicker === 'function'
  } catch {
    return false
  }
}

function isHandle(v: unknown): v is FsDirHandle {
  return (
    !!v &&
    typeof v === 'object' &&
    typeof (v as FsDirHandle).getFileHandle === 'function' &&
    typeof (v as FsDirHandle).getDirectoryHandle === 'function'
  )
}

/** Remember a picked folder (overwrites any previous handle for the key). */
export async function rememberLocalDir(key: string, handle: unknown): Promise<boolean> {
  if (!isHandle(handle)) return false
  const ok = await idbSet(key, handle as unknown as Record<string, unknown>)
  if (ok) setState(key, handle.name || key, 'ready', 'remembered — works across reloads')
  return ok
}

export async function forgetLocalDir(key: string): Promise<void> {
  await idbDel(key)
  delete localDirs[key]
}

async function loadHandle(key: string): Promise<FsDirHandle | null> {
  const raw = await idbGet<unknown>(key)
  return isHandle(raw) ? raw : null
}

async function queryAccess(handle: FsDirHandle): Promise<PermissionState> {
  try {
    return (await handle.queryPermission?.({ mode: 'readwrite' })) ?? 'prompt'
  } catch {
    return 'prompt'
  }
}

/**
 * Boot-time check: never prompts (no gesture at boot). `ready` means file
 * I/O works now; `needs-permission` means one click re-allows it;
 * `missing` means nothing was ever remembered.
 */
export async function ensureLocalDir(key: string): Promise<LocalDirState> {
  if (!dirPickerSupported()) {
    const s: LocalDirState = { key, label: key, status: 'unsupported', detail: 'no folder picker in this browser' }
    localDirs[key] = s
    return s
  }
  const handle = await loadHandle(key)
  if (!handle) {
    const s: LocalDirState = { key, label: key, status: 'missing', detail: 'no folder remembered' }
    localDirs[key] = s
    return s
  }
  const label = handle.name || key
  // A handle that lost permission still reads its name — probe I/O cheaply.
  const perm = await queryAccess(handle)
  if (perm === 'granted') {
    try {
      // Touch the iterator: a revoked handle throws here, not later.
      const it = handle.keys()
      await it.next().catch(() => undefined)
      if (typeof (it as AsyncIterableIterator<string>).return === 'function') {
        await (it as AsyncIterableIterator<string>).return?.()
      }
      setState(key, label, 'ready', 'attached — create, edit and read without re-picking')
      return localDirs[key] as LocalDirState
    } catch {
      // Falls through to needs-permission below.
    }
  }
  setState(key, label, 'needs-permission', 'remembered — one click re-allows access (no re-pick)')
  return localDirs[key] as LocalDirState
}

/** User-gesture re-grant for a parked handle. Returns true when I/O works. */
export async function reallowLocalDir(key: string): Promise<boolean> {
  const handle = await loadHandle(key)
  if (!handle) {
    setState(key, key, 'missing', 'nothing remembered — pick the folder again')
    return false
  }
  try {
    const state = await handle.requestPermission?.({ mode: 'readwrite' })
    if (state !== 'granted') {
      setState(key, handle.name || key, 'needs-permission', 'permission denied — the folder stays remembered but closed')
      return false
    }
  } catch {
    setState(key, handle.name || key, 'needs-permission', 'permission request failed — retry from a click')
    return false
  }
  const checked = await ensureLocalDir(key)
  return checked.status === 'ready'
}

/** A ready-to-use handle, or `null` (missing / permission lapsed). */
export async function liveLocalDir(key: string): Promise<FsDirHandle | null> {
  const handle = await loadHandle(key)
  if (!handle) return null
  try {
    if ((await queryAccess(handle)) !== 'granted') return null
    return handle
  } catch {
    return null
  }
}

/** Restore every remembered folder (wizard root + all provider dirs). */
export async function bootLocalDirs(): Promise<LocalDirState[]> {
  const out: LocalDirState[] = []
  try {
    const keys = await idbKeys()
    const wanted = keys.filter(
      (k) => k === WIZARD_LOCAL_DIR_KEY || k.startsWith('cybermanju.localDir:'),
    )
    for (const key of wanted) {
      try {
        out.push(await ensureLocalDir(key))
      } catch {
        // One damaged entry never blocks the rest.
      }
    }
  } catch {
    // IndexedDB unavailable — folders simply stay unremembered.
  }
  return out
}

// ── real file I/O through a live handle ────────────────────────────────
// Paths are `/`-separated, relative to the handle (`a/b/c.txt`). Every
// failure carries the house prefix.

function splitRel(rel: string): string[] {
  return rel.replace(/\\/g, '/').split('/').filter((s) => s !== '' && s !== '.')
}

async function descend(handle: FsDirHandle, segments: string[], create: boolean): Promise<FsDirHandle> {
  let dir = handle
  for (const seg of segments) {
    if (seg === '..') throw new Error('invalid: `..` escapes the attached folder')
    try {
      dir = await dir.getDirectoryHandle(seg, { create })
    } catch (e) {
      throw new Error(`not_found: folder '${seg}' is not reachable (${e instanceof Error ? e.message : String(e)})`)
    }
  }
  return dir
}

/** List one directory level (default: the attached root). */
export async function listLocalDir(handle: FsDirHandle, rel = ''): Promise<LocalDirEntry[]> {
  const dir = await descend(handle, splitRel(rel), false)
  const out: LocalDirEntry[] = []
  try {
    for await (const name of dir.keys()) {
      let isDir = false
      let sizeBytes = 0
      try {
        await dir.getDirectoryHandle(name)
        isDir = true
      } catch {
        try {
          const f = await (await dir.getFileHandle(name)).getFile()
          sizeBytes = f.size
        } catch {
          continue
        }
      }
      out.push({ name, isDir, sizeBytes })
    }
  } catch (e) {
    throw new Error(`integrity: listing failed (${e instanceof Error ? e.message : String(e)})`)
  }
  out.sort((a, b) => Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name))
  return out
}

/** Read a whole file (callers enforce their own size caps). */
export async function readLocalFile(handle: FsDirHandle, rel: string): Promise<Uint8Array> {
  const segs = splitRel(rel)
  const name = segs.pop()
  if (!name) throw new Error('invalid: no file name given')
  const dir = await descend(handle, segs, false)
  try {
    const file = await (await dir.getFileHandle(name)).getFile()
    return new Uint8Array(await file.arrayBuffer())
  } catch {
    throw new Error(`not_found: ${rel}`)
  }
}

/** Create (parents included) or overwrite a file. */
export async function writeLocalFile(handle: FsDirHandle, rel: string, data: Uint8Array): Promise<void> {
  const segs = splitRel(rel)
  const name = segs.pop()
  if (!name) throw new Error('invalid: no file name given')
  const dir = await descend(handle, segs, true)
  try {
    const fh = await dir.getFileHandle(name, { create: true })
    const writable = await fh.createWritable()
    try {
      await writable.write(data)
    } finally {
      await writable.close()
    }
  } catch (e) {
    throw new Error(`integrity: write failed for ${rel} (${e instanceof Error ? e.message : String(e)})`)
  }
}

/** Create a directory path (parents included). */
export async function mkdirLocalDir(handle: FsDirHandle, rel: string): Promise<void> {
  const segs = splitRel(rel)
  if (segs.length === 0) return
  await descend(handle, segs, true)
}

/** Delete a file or (with `recursive`) a directory tree. */
export async function deleteLocalPath(handle: FsDirHandle, rel: string, recursive = false): Promise<void> {
  const segs = splitRel(rel)
  const name = segs.pop()
  if (!name) throw new Error('invalid: no path given')
  const dir = await descend(handle, segs, false)
  try {
    await dir.removeEntry(name, { recursive })
  } catch {
    throw new Error(`not_found: ${rel}`)
  }
}
