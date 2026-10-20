// CyberManju OS — database Web Worker (static/WASM build only)
//
// redb's `StorageBackend` is fully synchronous, and the only synchronous
// file primitive in a browser is `FileSystemSyncAccessHandle`, which exists
// **only inside Dedicated Workers**. So the database runs here: this worker
// loads the wasm bundle, opens the real `cybermanju.db` in OPFS (in-memory
// fallback), and answers `{id, op, args}` messages with `{id, ok, data?,
// error?}`.
//
// The main thread never touches redb directly — `wasmDbDispatch()` in
// `useWasmBackend.ts` is the only caller, and `invoke()` is already async
// everywhere, so no Atomics/spinlock ferry is needed (those only exist for
// callers that insist on *synchronous* calls from the main thread).
import init, { db_dispatch, db_open, db_restore, db_snapshot } from 'cybermanju-os-wasm'
import { decodeContainer, encodeContainer } from '../utils/container'
import { IS_DEVELOP_PREVIEW } from '../utils/deploymentScope'

interface DbRequest {
  id: number
  op: string
  args?: Record<string, unknown>
}

const DB_FILE = 'cybermanju.db'
const BACKUP_FILE = 'cybermanju.backup.db'
const SNAPSHOT_MS = 5 * 60 * 1000
const AUTOSAVE_MS = 2000

let opened: Promise<{ backend: string }> | null = null
let dirty = false
let snapshotTimer = 0

// ── the user's `.cybermanju` file (File System Access API handle) ─────────
// The handle is structured-cloneable, so the main thread posts it here once
// and every save writes the encoded container back to *that* file. redb keeps
// living in OPFS (sync handle); the picked file is the durable, shareable
// copy the user actually sees on their disk.
interface DiskState {
  handle: { createWritable(): Promise<{ write(data: Uint8Array): Promise<void>; close(): Promise<void> }> }
  name: string
  passphrase: string
  savedAt: number
  savedBytes: number
  lastError: string | null
}

let disk: DiskState | null = null
let saveTimer = 0
let saving = false

function diskStatus() {
  return {
    attached: !!disk,
    name: disk?.name ?? '',
    savedAt: disk?.savedAt ?? 0,
    savedBytes: disk?.savedBytes ?? 0,
    dirty,
    lastError: disk?.lastError ?? null,
  }
}

// Read-only ops never dirty the database.
function isReadOnly(op: string): boolean {
  return (
    op === '_status' ||
    op === '_snapshot' ||
    op === '_restore' ||
    op.endsWith('.list') ||
    op.endsWith('.get') ||
    op === 'volume.df' ||
    op === 'disks.check' ||
    op === 'sync.secret'
  )
}

async function opfsRoot(): Promise<any> {
  const nav = navigator as unknown as { storage: { getDirectory: () => Promise<any> } }
  const root = await nav.storage.getDirectory()
  return IS_DEVELOP_PREVIEW
    ? root.getDirectoryHandle('cybermanju-develop', { create: true })
    : root
}

async function readBackupBytes(): Promise<Uint8Array | null> {
  try {
    const root = await opfsRoot()
    const fh = await root.getFileHandle(BACKUP_FILE)
    const file = await fh.getFile()
    const buf: ArrayBuffer = await file.arrayBuffer()
    if (buf.byteLength === 0) return null
    return new Uint8Array(buf)
  } catch {
    return null
  }
}

async function mainSize(): Promise<number | null> {
  try {
    const root = await opfsRoot()
    const fh = await root.getFileHandle(DB_FILE)
    const file = await fh.getFile()
    return file.size as number
  } catch {
    return null
  }
}

async function writeBackup(bytes: Uint8Array): Promise<number> {
  const root = await opfsRoot()
  const fh = await root.getFileHandle(BACKUP_FILE, { create: true })
  const writable = await fh.createWritable()
  await writable.write(bytes)
  await writable.close()
  return bytes.byteLength
}

async function snapshotNow(): Promise<number> {
  const view = db_snapshot() as unknown as Uint8Array
  const n = await writeBackup(view)
  dirty = false
  return n
}

function armSnapshotTimer() {
  if (snapshotTimer) return
  snapshotTimer = self.setInterval(() => {
    if (!dirty || !opened) return
    snapshotNow().catch(() => {
      // Best-effort: the live database is unaffected by backup failure.
    })
  }, SNAPSHOT_MS)
}

function ensureOpen(): Promise<{ backend: string }> {
  if (!opened) {
    opened = (async () => {
      await init()
      // Crash recovery: a missing/empty main file with a valid backup
      // means the previous session died mid-write — restore first.
      try {
        const [size, backup] = await Promise.all([mainSize(), readBackupBytes()])
        if (backup && backup.byteLength > 0 && (size === null || size === 0)) {
          const root = await opfsRoot()
          const fh = await root.getFileHandle(DB_FILE, { create: true })
          const writable = await fh.createWritable()
          await writable.write(backup)
          await writable.close()
        }
      } catch {
        // Recovery is best-effort; a fresh database still opens below.
      }
      const raw = await db_open()
      const env = JSON.parse(raw as string) as { ok: boolean; data?: { backend?: string }; error?: string }
      if (!env.ok) throw new Error(env.error || 'db_open failed')
      armSnapshotTimer()
      return { backend: String(env.data?.backend ?? 'unknown') }
    })()
    // A failed open must be retryable, not sticky.
    opened.catch(() => {
      opened = null
    })
  }
  return opened
}

/** Encode the live redb image and write it to the user's `.cybermanju` file. */
async function flushToDisk(): Promise<{ bytes: number; name: string }> {
  if (!disk) throw new Error('no .cybermanju file attached')
  const image = db_snapshot() as unknown as Uint8Array
  const payload = await encodeContainer(image, disk.passphrase)
  const writable = await disk.handle.createWritable()
  await writable.write(payload)
  await writable.close()
  dirty = false
  disk.savedAt = Date.now()
  disk.savedBytes = payload.byteLength
  disk.lastError = null
  return { bytes: payload.byteLength, name: disk.name }
}

/** Debounced write-back after every mutating op (2 s of quiet). */
function scheduleAutoSave() {
  if (!disk || saveTimer) return
  saveTimer = self.setTimeout(() => {
    saveTimer = 0
    if (!disk || !dirty || saving) return
    saving = true
    flushToDisk()
      .catch((e) => {
        if (disk) disk.lastError = e instanceof Error ? e.message : String(e)
      })
      .finally(() => {
        saving = false
      })
  }, AUTOSAVE_MS)
}

// Last-ditch flush when the tab goes away — the 5-minute OPFS backup and the
// debounced autosave are the real durability; this is the bonus.
self.addEventListener('pagehide', () => {
  if (disk && dirty && !saving) {
    void flushToDisk().catch(() => {
      /* page is going away regardless */
    })
  }
})

self.onmessage = async (ev: MessageEvent<DbRequest>) => {
  const { id, op, args } = ev.data ?? ({} as DbRequest)
  const post = (msg: Record<string, unknown>) => {
    self.postMessage({ id, ...msg })
  }
  try {
    const info = await ensureOpen()
    if (op === '_status') {
      post({ ok: true, data: { backend: info.backend, file: 'cybermanju.db', disk: diskStatus() } })
      return
    }
    // ── `.cybermanju` file attach / save / detach ─────────────────────────
    if (op === '_attach') {
      const handle = (args?.handle ?? null) as DiskState['handle'] | null
      const passphrase = String(args?.passphrase ?? '')
      const bytes = (args?.bytes ?? null) as Uint8Array | null
      const name = String(args?.name ?? 'cybermanju.cybermanju')
      // Load the picked file first — a wrong passphrase must not clobber
      // the live database with a half-restored image.
      if (bytes && bytes.byteLength > 0) {
        const { image } = await decodeContainer(bytes, passphrase)
        const out = db_restore(image) as string
        const env = JSON.parse(out) as { ok: boolean; data?: unknown; error?: string }
        if (!env.ok) throw new Error(String(env.error ?? 'restore failed'))
      }
      disk = handle
        ? {
            handle,
            name,
            passphrase,
            savedAt: Date.now(),
            savedBytes: bytes?.byteLength ?? 0,
            lastError: null,
          }
        : null
      dirty = false
      post({ ok: true, data: diskStatus() })
      return
    }
    if (op === '_save') {
      if (!disk) throw new Error('no .cybermanju file attached — open or create one first')
      const saved = await flushToDisk()
      post({ ok: true, data: { ...diskStatus(), ...saved } })
      return
    }
    if (op === '_export') {
      // Same bytes a save would write, handed back for a download — this is
      // the fallback path where the browser has no File System Access API.
      const passphrase = String(args?.passphrase ?? disk?.passphrase ?? '')
      const image = db_snapshot() as unknown as Uint8Array
      const payload = await encodeContainer(image, passphrase)
      post({ ok: true, data: { bytes: payload, name: disk?.name ?? 'cybermanju.cybermanju' } })
      return
    }
    if (op === '_detach') {
      disk = null
      if (saveTimer) {
        self.clearTimeout(saveTimer)
        saveTimer = 0
      }
      post({ ok: true, data: diskStatus() })
      return
    }
    if (op === '_snapshot') {
      const n = await snapshotNow()
      post({ ok: true, data: { bytes: n, file: BACKUP_FILE } })
      return
    }
    if (op === '_restore') {
      const backup = await readBackupBytes()
      if (!backup) {
        post({ ok: false, error: 'not_found: no backup image (cybermanju.backup.db)' })
        return
      }
      const out = db_restore(backup) as string
      const env = JSON.parse(out) as { ok: boolean; data?: unknown; error?: string }
      if (env.ok) {
        dirty = false
        post({ ok: true, data: env.data ?? null })
      } else {
        post({ ok: false, error: String(env.error ?? 'restore failed') })
      }
      return
    }
    if (!isReadOnly(op)) {
      dirty = true
      scheduleAutoSave()
    }
    const out = db_dispatch(
      op,
      JSON.stringify({ ...(args ?? {}), now: new Date().toISOString() }),
    ) as string
    const env = JSON.parse(out) as { ok: boolean; data?: unknown; error?: string }
    if (env.ok) post({ ok: true, data: env.data ?? null })
    else post({ ok: false, error: String(env.error ?? 'unknown db error') })
  } catch (e) {
    post({ ok: false, error: e instanceof Error ? e.message : String(e) })
  }
}
