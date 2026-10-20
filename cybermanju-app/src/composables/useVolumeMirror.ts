// CyberManju OS — shell volume ⇄ `.cybermanju` mirror
//
// The OS layer keeps a virtual volume (`{ path: text }`) in localStorage
// (`cybermanju.os.volume`, capped at 1 MiB — see crates/os-wasm/src/os.rs).
// That copy dies with "clear site data" and never reaches the user's file, so
// every entry is also mirrored into redb's `kv` table under `volume:<path>`:
// the WASM DB on web, and the native DB on Android/iOS so the SAF container
// snapshot contains the same edit.
//
// Direction is deliberately two-step because the OS volume and the vault DB
// are separate stores (os_* runs in WASM; the database is WASM or native redb):
//
//   write/touch/rm/cat … → localStorage volume → diff → `kv volume:*`
//   boot / file open     → `kv volume:*` → `os write` → localStorage volume
//
// While `kv.*` is unavailable on web, the mirror turns itself off silently —
// the volume keeps working exactly as before. Mobile uses the Tauri KV bridge.

import { addOsDispatchHook, wasmDbDispatch, wasmOsDispatch } from './useWasmBackend'
import { withVolumeWriteQueue } from '@/utils/volumeWriteQueue'

const LS_KEY = 'cybermanju.os.volume'
const PREFIX = 'volume:'
const DEBOUNCE_MS = 400

let lastJson = ''
let timer = 0
let started = false
let kvBroken = false

function readVolume(): Record<string, string> {
  try {
    const raw = localStorage.getItem(LS_KEY)
    if (!raw) return {}
    const parsed: unknown = JSON.parse(raw)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, string>
    }
  } catch {
    // Missing or corrupt — treat as an empty volume.
  }
  return {}
}

async function kv(
  op: string,
  args: Record<string, unknown>,
): Promise<{ ok: true; value: unknown } | { ok: false }> {
  if (kvBroken) return { ok: false }
  try {
    const { isTauriMobile, invoke } = await import('./useTauri')
    if (isTauriMobile()) {
      const nativeCommand: Record<string, string> = {
        'kv.get': 'vault_kv_get',
        'kv.set': 'vault_kv_set',
        'kv.delete': 'vault_kv_delete',
        'kv.list': 'vault_kv_list',
      }
      const command = nativeCommand[op]
      if (!command) return { ok: false }
      return { ok: true, value: await invoke<unknown>(command, args) }
    }
    const value = await wasmDbDispatch(op, args, 60000)
    return { ok: true, value }
  } catch (e) {
    const text = e instanceof Error ? e.message : String(e)
    if (/unsupported: unknown demo-db op/i.test(text)) kvBroken = true
    return { ok: false }
  }
}

async function flushMobileMirror(): Promise<void> {
  const { isTauriMobile } = await import('./useTauri')
  if (!isTauriMobile()) return
  const { flushMobileVaultMirror } = await import('@/utils/mobileScopedStorage')
  await flushMobileVaultMirror()
}

/** Diff localStorage against the last pushed snapshot and write the delta. */
async function pushVolume(): Promise<boolean> {
  return withVolumeWriteQueue(0, async () => {
    // Read only after reaching the queue head. Otherwise a queued older
    // snapshot could overwrite a newer localStorage image in the vault.
    const current = readVolume()
    const nextJson = JSON.stringify(current)
    if (nextJson === lastJson) return false
    const prev = lastJson ? (JSON.parse(lastJson) as Record<string, string>) : {}

    for (const [path, text] of Object.entries(current)) {
      if (prev[path] === text) continue
      if ((await kv('kv.set', { key: PREFIX + path, value: text })).ok === false) return false
    }
    for (const path of Object.keys(prev)) {
      if (path in current) continue
      if ((await kv('kv.delete', { key: PREFIX + path })).ok === false) return false
    }
    // Advance only after the full delta was stored; a partial failure must be
    // retried against the same previous image on the next flush.
    lastJson = nextJson
    await flushMobileMirror()
    return true
  })
}

/**
 * Start mirroring. Registers one debounced hook on the os/* dispatcher, so
 * any command that touches the volume (the terminal included) schedules a
 * push — no call sites to remember.
 */
export function startVolumeMirror(): void {
  if (started) return
  started = true
  lastJson = JSON.stringify(readVolume())
  addOsDispatchHook(() => {
    if (timer) window.clearTimeout(timer)
    timer = window.setTimeout(() => {
      timer = 0
      void pushVolume().catch(() => {
        // Mirror is best-effort; the volume itself already succeeded.
      })
    }, DEBOUNCE_MS)
  })
}

/**
 * Fill the OS volume from the vault. At boot (`overwrite = false`) only
 * missing paths are written, so a local volume that is ahead of the file is
 * never clobbered; after the user *opens* a file (`overwrite = true`) the
 * file wins — it is the thing they just asked to load.
 */
export async function replayVolumeFromVault(overwrite = false): Promise<number> {
  const listed = await kv('kv.list', { prefix: PREFIX })
  if (!listed.ok || !Array.isArray(listed.value)) return 0
  const local = readVolume()
  let restored = 0
  for (const row of listed.value as Array<{ key?: string }>) {
    const key = String(row?.key ?? '')
    const path = key.slice(PREFIX.length)
    if (!path) continue
    if (!overwrite && local[path] !== undefined) continue
    const got = await kv('kv.get', { key })
    if (!got.ok) break
    const value = (got.value as { value?: unknown } | null)?.value
    if (typeof value !== 'string') continue
    try {
      await withVolumeWriteQueue(new TextEncoder().encode(value).length, () =>
        wasmOsDispatch('write', { path, content: value })
      )
      restored++
    } catch {
      // One bad path must not abort the rest of the replay.
    }
  }
  if (restored) lastJson = JSON.stringify(readVolume())
  return restored
}

/** Flush a pending push immediately (file save / page hide). */
export async function flushVolumeMirror(): Promise<void> {
  if (timer) {
    window.clearTimeout(timer)
    timer = 0
  }
  const pushed = started ? await pushVolume().catch(() => false) : false
  // Also flush a database mutation that was queued by another UI path when
  // the OS volume itself had no new delta.
  if (!pushed) await flushMobileMirror().catch(() => undefined)
}
