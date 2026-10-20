// CyberManju OS — key/value vault
//
// Secrets, app config and small blobs belong inside `.cybermanju`, so they
// live in redb's `kv` table through the worker (`kv.get/set/delete/list`,
// crates/os-wasm/src/db.rs). Everything is namespaced by convention:
//
//   secret:<account>/<name>   provider PATs, Supabase service secrets
//   config:<name>             app + provider configuration
//   content:<fileId>          file bodies (text-ish payloads)
//   volume:<path>             shell file mirror (see useVolumeMirror)
//
// The shipped `pkg/` bundle may predate `kv.*`; only its explicit unsupported
// response enables the legacy localStorage fallback. Real worker/storage
// failures must surface rather than silently creating a divergent shadow.

import { wasmDbDispatch } from './useWasmBackend'

export type VaultBackend = 'kv' | 'localStorage' | 'unknown'

const LS_PREFIX = 'cybermanju.vault.'
const MAX_KEY = 512

/** `null` = not probed yet / transient failure (retry next call). */
let kvKnownBroken = false
let kvKnownWorking = false

function lsRead(key: string): string | null {
  return localStorage.getItem(LS_PREFIX + key)
}

function lsWrite(key: string, value: string | null) {
  if (value === null) localStorage.removeItem(LS_PREFIX + key)
  else localStorage.setItem(LS_PREFIX + key, value)
}

function lsKeys(prefix: string): Array<{ key: string; bytes: number }> {
  const out: Array<{ key: string; bytes: number }> = []
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i)
    if (!k || !k.startsWith(LS_PREFIX)) continue
    const short = k.slice(LS_PREFIX.length)
    if (prefix && !short.startsWith(prefix)) continue
    const value = localStorage.getItem(k)
    if (value === null) throw new Error(`vault: localStorage entry disappeared while listing: ${short}`)
    out.push({ key: short, bytes: value.length })
  }
  return out
}

/** Best-effort only: KV is already authoritative when this removes an old shadow. */
function discardLocalShadow(key: string): void {
  try {
    lsWrite(key, null)
  } catch {
    // A stale shadow is harmless once the durable KV operation succeeded.
  }
}

/** Which store the vault is using right now (UI copy + tests). */
export function vaultBackend(): VaultBackend {
  if (kvKnownWorking) return 'kv'
  if (kvKnownBroken) return 'localStorage'
  return 'unknown'
}

function isUnsupported(e: unknown): boolean {
  const text = e instanceof Error ? e.message : String(e)
  return /unsupported: unknown demo-db op/i.test(text)
}

/**
 * Only an explicit unsupported-op response enables localStorage fallback.
 * All other worker/database failures remain errors and fail closed.
 */
async function kvDispatch(
  op: string,
  args: Record<string, unknown>,
): Promise<{ ok: true; value: unknown } | { ok: false; unsupported: boolean }> {
  if (kvKnownBroken) return { ok: false, unsupported: true }
  try {
    const value = await wasmDbDispatch(op, args, 30000)
    kvKnownWorking = true
    return { ok: true, value }
  } catch (e) {
    if (isUnsupported(e)) {
      kvKnownBroken = true
      return { ok: false, unsupported: true }
    }
    throw e
  }
}

function readKvValue(value: unknown): string | null {
  if (value === null) return null
  if (typeof value !== 'object' || !('value' in value)) {
    throw new Error('vault: invalid kv.get response')
  }
  const stored = (value as { value?: unknown }).value
  if (typeof stored !== 'string') throw new Error('vault: invalid kv.get value')
  return stored
}

export async function vaultGet(key: string): Promise<string | null> {
  const res = await kvDispatch('kv.get', { key })
  if (res.ok) return readKvValue(res.value)
  return lsRead(key)
}

export async function vaultSet(key: string, value: string): Promise<void> {
  if (key.length > MAX_KEY) throw new Error(`vault: key must be at most ${MAX_KEY} characters`)
  const res = await kvDispatch('kv.set', { key, value })
  // Drop any stale localStorage shadow once the file owns this key.
  if (res.ok) discardLocalShadow(key)
  else lsWrite(key, value)
}

export async function vaultDelete(key: string): Promise<void> {
  const res = await kvDispatch('kv.delete', { key })
  if (res.ok) discardLocalShadow(key)
  else lsWrite(key, null)
}

export async function vaultList(prefix = ''): Promise<Array<{ key: string; bytes: number }>> {
  const res = await kvDispatch('kv.list', { prefix })
  if (res.ok) {
    if (!Array.isArray(res.value)) throw new Error('vault: invalid kv.list response')
    return (res.value as Array<{ key: string; bytes: number }>).filter((r) => !!r?.key)
  }
  return lsKeys(prefix)
}

export async function vaultGetJson<T>(key: string): Promise<T | null> {
  const raw = await vaultGet(key)
  if (!raw) return null
  try {
    return JSON.parse(raw) as T
  } catch {
    return null
  }
}

export async function vaultSetJson(key: string, value: unknown): Promise<void> {
  await vaultSet(key, JSON.stringify(value))
}

/**
 * Copy anything an older build left in localStorage into the file. Runs once
 * per origin (guarded by a `config:vault.migrated` marker) and is a no-op
 * while `kv.*` is unsupported, so the next launch after CI rebuilds the pkg
 * picks the data up automatically.
 */
export async function migrateVaultFromLocalStorage(): Promise<number> {
  const marker = await kvDispatch('kv.get', { key: 'config:vault.migrated' })
  if (!marker.ok) return 0 // Old package: preserve localStorage for a later build.
  const alreadyMigrated = Boolean(readKvValue(marker.value))

  const rows = lsKeys('')
  if (alreadyMigrated) {
    // Older migration code left duplicate localStorage shadows behind. Remove
    // only rows that still have an authoritative value in KV.
    for (const row of rows) {
      const current = await kvDispatch('kv.get', { key: row.key })
      if (!current.ok) return 0
      if (readKvValue(current.value) !== null) discardLocalShadow(row.key)
    }
    return 0
  }

  let moved = 0
  const shadowKeys: string[] = []
  for (const row of rows) {
    const current = await kvDispatch('kv.get', { key: row.key })
    if (!current.ok) return 0 // Unsupported package; leave every source value intact.
    const existing = readKvValue(current.value)
    const value = lsRead(row.key)
    if (existing !== null) {
      shadowKeys.push(row.key)
    } else if (value !== null) {
      const written = await kvDispatch('kv.set', { key: row.key, value })
      if (!written.ok) return 0 // Do not clear source data on the legacy path.
      moved++
      shadowKeys.push(row.key)
    }
  }

  // Write the completion marker before deleting source values. If any KV write
  // fails, the error propagates and all localStorage originals remain recoverable.
  await vaultSet('config:vault.migrated', String(Date.now()))
  for (const key of shadowKeys) discardLocalShadow(key)
  return moved
}
