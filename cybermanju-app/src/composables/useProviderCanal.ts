// CyberManju OS — provider canal orchestration (CONTROL Phase 5.5).
//
// TS half of canal B: mounts + directory cache live in the redb `kv`
// table (so `_save`/`_attach` of the `.cybermanju` container carries the
// whole provider namespace), the Rust half (`crates/os-wasm` canal.rs /
// artifact.rs) does transport + artifact unwrap. Reads never fetch
// a provider directly — every read byte goes through `canal_dispatch` /
// `canal_fetch`, so CORS/honest-prefix behaviour stays in one place.
// Writes go through `upload_remote_file` (desktop/Docker Rust backend) or
// direct provider fetch (static host), then invalidate the directory cache.
//
// Namespace: single virtual root `providers/<mountId>/<remotePath>`.
// Mounts are provider-backed directories (never merged into `/`).

import {
  wasmArtifactMagic,
  wasmArtifactOpen,
  wasmCanalDispatch,
  wasmCanalFetch,
  wasmDbDispatch,
} from './useWasmBackend'

export interface ProviderMount {
  id: string
  configId: string
  name: string
  backendType: string
  basePath?: string
  folderId?: string
  createdAt: string
  updatedAt: string
}

export interface VfsEntry {
  name: string
  path: string
  locator: string
  isDir: boolean
  sizeBytes: number
  modifiedAt: string
}

export interface VfsFile {
  bytes: Uint8Array
  magic: string
  text: string | null
}

export const VFS_MOUNT_INDEX_KEY = 'vfs:mount:index'
export const VFS_CACHE_TTL_MS = 60_000
export const VFS_CACHE_PREFIX = 'vfs:cache:'
/** Master passphrase for `CYBE1` artifacts — lives in `kv` so it ships
 *  inside the `.cybermanju` container with mounts + cache (CONTROL 5.8). */
export const VFS_SECRET_MASTER_KEY = 'vfs:secret:master'

export const mountKey = (id: string): string => `vfs:mount:${id}`
export const cacheKey = (mountId: string, remotePath: string): string =>
  `${VFS_CACHE_PREFIX}${mountId}:${remotePath.replace(/^\/+/, '')}`

/** `providers/<id>/a/b` → `{ mountId: <id>, remotePath: a/b }`. Null outside the namespace. */
export function parseProviderPath(path: string): { mountId: string; remotePath: string } | null {
  const norm = String(path ?? '').replace(/\\/g, '/').replace(/^\/+/, '')
  const segs = norm.split('/').filter(Boolean)
  if (segs.length < 2 || segs[0] !== 'providers') return null
  return { mountId: segs[1], remotePath: segs.slice(2).join('/') }
}

export function providerPathFor(mountId: string, remotePath: string): string {
  const rel = String(remotePath ?? '').replace(/^\/+/, '')
  return rel ? `/providers/${mountId}/${rel}` : `/providers/${mountId}`
}

/** True when a cached listing envelope is still usable. */
export function isCacheFresh(cachedAt: number, now = Date.now()): boolean {
  return Number.isFinite(cachedAt) && now - cachedAt < VFS_CACHE_TTL_MS
}

/** SAF trees may change in Files or another Android app, so their listings are never cached. */
export function shouldCacheVfsListing(backendType: string): boolean {
  return backendType !== 'scopedStorage'
}

function newMountId(): string {
  try {
    return `mnt-${crypto.randomUUID().slice(0, 8)}`
  } catch {
    return `mnt-${Date.now().toString(36)}`
  }
}

const LS_FALLBACK_PREFIX = 'cybermanju.vfs.fallback.'

function lsGet(key: string): string | null {
  try {
    return typeof localStorage !== 'undefined' ? localStorage.getItem(LS_FALLBACK_PREFIX + key) : null
  } catch {
    return null
  }
}

function lsSet(key: string, value: string): void {
  try {
    localStorage?.setItem(LS_FALLBACK_PREFIX + key, value)
  } catch {
    // Quota or private mode — kv remains the store of record.
  }
}

function lsDelete(key: string): void {
  try {
    localStorage?.removeItem(LS_FALLBACK_PREFIX + key)
  } catch {
    // Ignore.
  }
}

const MOBILE_KV_COMMANDS: Record<string, string> = {
  'kv.get': 'vault_kv_get',
  'kv.set': 'vault_kv_set',
  'kv.delete': 'vault_kv_delete',
}

async function mobileNativeKv(op: string, args: Record<string, unknown>): Promise<{ handled: boolean; value?: unknown }> {
  const command = MOBILE_KV_COMMANDS[op]
  if (!command) return { handled: false }
  const { isTauriMobile, invoke } = await import('./useTauri')
  if (!isTauriMobile()) return { handled: false }
  return { handled: true, value: await invoke<unknown>(command, args) }
}

async function kvGet(key: string): Promise<string | null> {
  const native = await mobileNativeKv('kv.get', { key })
  if (native.handled) {
    const row = native.value as { value?: unknown } | null
    if (typeof row?.value === 'string') return row.value
    const legacy = lsGet(key)
    if (legacy !== null) {
      await mobileNativeKv('kv.set', { key, value: legacy })
      lsDelete(key)
      return legacy
    }
    return null
  }
  try {
    const row = (await wasmDbDispatch('kv.get', { key })) as { value?: unknown } | null
    if (typeof row?.value === 'string') return row.value
  } catch {
    // Old bundles / non-static transports — fall through to localStorage.
  }
  return lsGet(key)
}

async function kvSet(key: string, value: string): Promise<void> {
  const native = await mobileNativeKv('kv.set', { key, value })
  if (native.handled) {
    lsDelete(key)
    return
  }
  lsSet(key, value)
  try {
    await wasmDbDispatch('kv.set', { key, value })
  } catch {
    // Static-only op — the localStorage mirror keeps every transport working.
  }
}

async function kvDelete(key: string): Promise<void> {
  const native = await mobileNativeKv('kv.delete', { key })
  if (native.handled) {
    lsDelete(key)
    return
  }
  lsDelete(key)
  try {
    await wasmDbDispatch('kv.delete', { key })
  } catch {
    // Missing op on old bundles — index rewrite still converges.
  }
}

/** Mount registry CRUD — `vfs:mount:*` rows + `vfs:mount:index`. */
export async function listVfsMounts(): Promise<ProviderMount[]> {
  const raw = await kvGet(VFS_MOUNT_INDEX_KEY)
  let ids: string[] = []
  if (raw) {
    try {
      const parsed: unknown = JSON.parse(raw)
      if (Array.isArray(parsed)) ids = parsed.filter((v): v is string => typeof v === 'string')
    } catch {
      ids = []
    }
  }
  const out: ProviderMount[] = []
  for (const id of ids) {
    const body = await kvGet(mountKey(id))
    if (!body) continue
    try {
      const m = JSON.parse(body) as Partial<ProviderMount>
      if (typeof m?.id === 'string' && typeof m?.configId === 'string') {
        out.push({
          id: m.id,
          configId: m.configId,
          name: typeof m.name === 'string' ? m.name : m.id,
          backendType: typeof m.backendType === 'string' ? m.backendType : 'github',
          basePath: typeof m.basePath === 'string' ? m.basePath : '',
          folderId: typeof m.folderId === 'string' ? m.folderId : undefined,
          createdAt: typeof m.createdAt === 'string' ? m.createdAt : new Date().toISOString(),
          updatedAt: typeof m.updatedAt === 'string' ? m.updatedAt : new Date().toISOString(),
        })
      }
    } catch {
      // Skip corrupt rows, keep the rest.
    }
  }
  return out
}

export async function saveVfsMount(input: {
  id?: string
  configId: string
  name: string
  backendType: string
  basePath?: string
  folderId?: string
}): Promise<ProviderMount> {
  const configId = String(input.configId ?? '').trim()
  if (!configId) throw new Error('invalid: configId is required')
  const now = new Date().toISOString()
  const existing = input.id ? (await listVfsMounts()).find(m => m.id === input.id) : undefined
  const mount: ProviderMount = {
    id: input.id || newMountId(),
    configId,
    name: String(input.name || configId),
    backendType: String(input.backendType || 'github'),
    basePath: String(input.basePath || ''),
    folderId: input.folderId ? String(input.folderId) : undefined,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  }
  const raw = await kvGet(VFS_MOUNT_INDEX_KEY)
  let ids: string[] = []
  if (raw) {
    try {
      const parsed: unknown = JSON.parse(raw)
      if (Array.isArray(parsed)) ids = parsed.filter((v): v is string => typeof v === 'string')
    } catch {
      ids = []
    }
  }
  if (!ids.includes(mount.id)) ids.push(mount.id)
  await kvSet(mountKey(mount.id), JSON.stringify(mount))
  await kvSet(VFS_MOUNT_INDEX_KEY, JSON.stringify(ids))
  return mount
}

export async function deleteVfsMount(id: string): Promise<void> {
  const raw = await kvGet(VFS_MOUNT_INDEX_KEY)
  let ids: string[] = []
  if (raw) {
    try {
      const parsed: unknown = JSON.parse(raw)
      if (Array.isArray(parsed)) ids = parsed.filter((v): v is string => typeof v === 'string')
    } catch {
      ids = []
    }
  }
  await kvSet(VFS_MOUNT_INDEX_KEY, JSON.stringify(ids.filter(v => v !== id)))
  await kvDelete(mountKey(id))
}

/** Resolve a mount's live sync config (token comes from the vault, never the mount row). */
async function canalConfigFor(mount: ProviderMount): Promise<Record<string, unknown>> {
  if (mount.backendType === 'scopedStorage') {
    const folderId = mount.folderId || mount.configId
    if (!folderId) throw new Error(`not_found: mobile folder handle for ${mount.name}`)
    return { id: mount.configId, folderId, name: mount.name, backendType: 'scopedStorage' }
  }
  const cfg = (await wasmDbDispatch('sync.get', { configId: mount.configId }).catch(() => null)) as Record<
    string,
    unknown
  > | null
  // Fallback: list + find (older bundles expose `sync.list` only).
  let row: Record<string, unknown> | null = cfg
  if (!row) {
    const all = (await wasmDbDispatch('sync.list', {}).catch(() => [])) as Array<Record<string, unknown>>
    row = (Array.isArray(all) ? all : []).find(c => c.id === mount.configId) ?? null
  }
  if (!row) throw new Error(`not_found: sync config ${mount.configId}`)
  let secret = ''
  try {
    const s = (await wasmDbDispatch('sync.secret', { configId: mount.configId }).catch(() => null)) as {
      token?: unknown
    } | null
    if (typeof s?.token === 'string') secret = s.token
  } catch {
    secret = ''
  }
  return { ...(row as object), token: secret || (row as { token?: string }).token || '' }
}

/** List one provider directory — cache first (`vfs:cache:*`, TTL), canal second. */
export async function listVfsDir(mountId: string, remotePath = ''): Promise<VfsEntry[]> {
  const mounts = await listVfsMounts()
  const mount = mounts.find(m => m.id === mountId)
  if (!mount) throw new Error(`not_found: provider mount ${mountId}`)
  const rel = String(remotePath ?? '').replace(/^\/+/, '')
  if (/(^|\/)\.\.(\/|$)/.test(rel)) throw new Error(`unsupported: provider path '${rel}' escapes the mount`)
  const key = cacheKey(mountId, rel)
  const config = await canalConfigFor(mount)
  const cacheable = shouldCacheVfsListing(String(config.backendType ?? ''))
  if (cacheable) {
    const cachedRaw = await kvGet(key)
    if (cachedRaw) {
      try {
        const cached = JSON.parse(cachedRaw) as { at?: number; entries?: VfsEntry[] }
        if (isCacheFresh(Number(cached?.at ?? 0)) && Array.isArray(cached.entries)) {
          return cached.entries as VfsEntry[]
        }
      } catch {
        // Fall through to a live listing.
      }
    }
  }
  let entries: VfsEntry[]
  if (config.backendType === 'scopedStorage') {
    const { listMobileFolder } = await import('@/utils/mobileScopedStorage')
    entries = (await listMobileFolder(String(config.folderId), rel)).map(e => ({
      name: String(e.name ?? ''),
      path: String(e.path ?? e.name ?? ''),
      locator: String(e.path ?? e.name ?? ''),
      isDir: !!e.isDir,
      sizeBytes: Number(e.size ?? 0),
      modifiedAt: String(e.lastModified ?? ''),
    }))
  } else {
    entries = await wasmCanalDispatch<VfsEntry[]>('list', { config, prefix: rel })
  }
  const rows = (Array.isArray(entries) ? entries : []).map(e => ({
    name: String(e.name ?? ''),
    path: String(e.path ?? e.name ?? ''),
    locator: String(e.locator ?? e.path ?? e.name ?? ''),
    isDir: !!e.isDir,
    sizeBytes: Number(e.sizeBytes ?? 0),
    modifiedAt: String(e.modifiedAt ?? ''),
  }))
  // SAF folder contents can change outside the app; always ask the native
  // document provider for a fresh view instead of hiding edits behind TTL.
  if (cacheable) {
    await kvSet(key, JSON.stringify({ at: Date.now(), entries: rows })).catch(() => undefined)
  }
  return rows
}

/**
 * Read one provider file: fetch raw bytes, sniff the header, unwrap `CYBE1`
 * artifacts with the mount passphrase, hand `CYBMJ01` to the caller (the TS
 * container codec owns it), decode the rest as UTF-8 text when possible.
 */
export async function readVfsFile(
  mountId: string,
  remotePath: string,
  opts: { locator?: string; passphrase?: string } = {},
): Promise<VfsFile> {
  const mounts = await listVfsMounts()
  const mount = mounts.find(m => m.id === mountId)
  if (!mount) throw new Error(`not_found: provider mount ${mountId}`)
  const rel = String(remotePath ?? '').replace(/^\/+/, '')
  if (!rel) throw new Error('invalid: remotePath is required')
  if (/(^|\/)\.\.(\/|$)/.test(rel)) throw new Error(`unsupported: provider path '${rel}' escapes the mount`)
  const config = await canalConfigFor(mount)
  const locator = opts.locator || rel
  if (/(^|\/)\.\.(\/|$)/.test(locator)) throw new Error(`unsupported: provider path '${locator}' escapes the mount`)
  const bytes = config.backendType === 'scopedStorage'
    ? await (await import('@/utils/mobileScopedStorage')).readMobileFolderFile(String(config.folderId), locator)
    : await wasmCanalFetch(config, locator)
  const magic = await wasmArtifactMagic(bytes).catch(() => 'raw')
  if (magic === 'CYBE1') {
    const passphrase = String(opts.passphrase ?? (await getVfsMasterPassphrase().catch(() => '')) ?? '')
    if (!passphrase) throw new Error('auth: artifact is encrypted — passphrase required')
    const open = await wasmArtifactOpen(bytes, passphrase)
    return { bytes: open, magic, text: decodeText(open) }
  }
  // CYBMJ01 / CYBMJU1 pass through untouched — their codecs live elsewhere.
  return { bytes, magic, text: magic === 'raw' ? decodeText(bytes) : null }
}

/** Master passphrase CRUD — `kv` row, so `_save`/`_attach` carries it. */
export async function getVfsMasterPassphrase(): Promise<string> {
  return (await kvGet(VFS_SECRET_MASTER_KEY)) ?? ''
}

export async function setVfsMasterPassphrase(passphrase: string): Promise<void> {
  const v = String(passphrase ?? '')
  if (!v) await kvDelete(VFS_SECRET_MASTER_KEY)
  else await kvSet(VFS_SECRET_MASTER_KEY, v)
}

function decodeText(bytes: Uint8Array): string | null {
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    return text.includes('\0') ? null : text
  } catch {
    return null
  }
}

function bytesToBase64(bytes: Uint8Array): string {
  let bin = ''
  const CHUNK = 8192
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return btoa(bin)
}

/** Parent directory of a remote path (`a/b/c.txt` → `a/b`, root → `''`). */
function parentDirOf(rel: string): string {
  const clean = String(rel ?? '').replace(/^\/+|\/+$/g, '')
  const idx = clean.lastIndexOf('/')
  return idx < 0 ? '' : clean.slice(0, idx)
}

/** Live sync config behind a mount (for write/delete callers). */
export async function getMountConfig(mountId: string): Promise<{
  mount: ProviderMount
  config: Record<string, unknown>
}> {
  const mounts = await listVfsMounts()
  const mount = mounts.find(m => m.id === mountId)
  if (!mount) throw new Error(`not_found: provider mount ${mountId}`)
  return { mount, config: await canalConfigFor(mount) }
}

/**
 * Write bytes to a provider path (creates or overwrites). Returns the
 * provider URL. Directory listings that could contain the file (its parent
 * + the mount root) are invalidated so the next read is live.
 */
export async function writeVfsFile(
  mountId: string,
  remotePath: string,
  data: Uint8Array | string,
  opts: { locator?: string } = {},
): Promise<string> {
  const rel = String(opts.locator ?? remotePath ?? '').replace(/^\/+/, '')
  if (!rel) throw new Error('invalid: remotePath is required')
  if (/(^|\/)\.\.(\/|$)/.test(rel)) throw new Error(`unsupported: provider path '${rel}' escapes the mount`)
  const bytes = typeof data === 'string' ? new TextEncoder().encode(data) : data
  if (bytes.length > 5 * 1024 * 1024) throw new Error(`too_large: '${rel}' exceeds the 5 MiB write cap`)
  const { mount, config } = await getMountConfig(mountId)
  const backend = String((config.backendType ?? config.backend ?? mount.backendType) ?? 'github')
  if (backend === 'scopedStorage') {
    const { writeMobileFolderFile } = await import('@/utils/mobileScopedStorage')
    await writeMobileFolderFile(String(config.folderId), rel, bytes)
    await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
    await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
    return rel
  }
  const { isStaticHost } = await import('./useTauri')
  if (isStaticHost()) {
    // No dashboard behind the page: git mounts speak the provider REST APIs
    // directly and Drive mounts speak the Drive API (both CORS-OK). Local
    // mounts stay read-only here; everywhere else every backend writes
    // through the Rust backend below.
    if (backend !== 'github' && backend !== 'gitlab' && backend !== 'googleDrive') {
      throw new Error(`unsupported: writes need the dashboard for '${backend}' mounts`)
    }
    const prov = await import('@/utils/gitProvision')
    const token = String(config.token ?? '')
    if (!token) throw new Error('auth: mount has no token — save one on the provider card first')
    if (backend === 'googleDrive') {
      const url = await prov.driveWriteDirect({
        token,
        folderId: typeof config.folderId === 'string' ? config.folderId : undefined,
        basePath: typeof config.basePath === 'string' ? config.basePath : undefined,
        remotePath: rel,
        contentBase64: bytesToBase64(bytes),
      })
      await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
      if (parentDirOf(rel)) await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
      return url
    }
    const repo = {
      backend,
      repoName: String(config.repoName ?? ''),
      fullName: String(config.repoName ?? ''),
      branch: String(config.branch ?? 'main'),
      url: '',
      projectId: backend === 'gitlab' ? String(config.repoName ?? '') : null,
    }
    if (!repo.repoName) throw new Error('unsupported: provider has no repo yet — create the private repo first')
    const urls = await prov.seedRepoDirect(
      backend as 'github' | 'gitlab',
      repo,
      token,
      [{ path: rel, contentBase64: bytesToBase64(bytes) }],
      typeof config.basePath === 'string' ? config.basePath : undefined,
    )
    await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
    if (parentDirOf(rel)) await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
    return urls[0] ?? rel
  }
  const { invoke } = await import('./useTauri')
  const url = await invoke<string>('upload_remote_file', {
    config,
    remotePath: rel,
    contentBase64: bytesToBase64(bytes),
  })
  await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
  if (parentDirOf(rel)) await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
  return url
}

/**
 * Delete one provider file. Directory caches for its parent + root are
 * invalidated. Directories themselves need no delete (blob stores fold
 * empty prefixes away).
 */
export async function deleteVfsFile(
  mountId: string,
  remotePath: string,
  opts: { locator?: string } = {},
): Promise<void> {
  const rel = String(opts.locator ?? remotePath ?? '').replace(/^\/+/, '')
  if (!rel) throw new Error('invalid: remotePath is required')
  if (/(^|\/)\.\.(\/|$)/.test(rel)) throw new Error(`unsupported: provider path '${rel}' escapes the mount`)
  const { mount, config } = await getMountConfig(mountId)
  const backend = String((config.backendType ?? config.backend ?? mount.backendType) ?? 'github')
  if (backend === 'scopedStorage') {
    const { removeMobileFolderEntry } = await import('@/utils/mobileScopedStorage')
    await removeMobileFolderEntry(String(config.folderId), rel)
    await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
    await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
    return
  }
  const { isStaticHost } = await import('./useTauri')
  if (isStaticHost()) {
    if (backend !== 'github' && backend !== 'gitlab' && backend !== 'googleDrive') {
      throw new Error(`unsupported: deletes need the dashboard for '${backend}' mounts`)
    }
    if (backend === 'googleDrive') {
      const provDrive = await import('@/utils/gitProvision')
      const tokenDrive = String(config.token ?? '')
      if (!tokenDrive) throw new Error('auth: mount has no token — save one on the provider card first')
      await provDrive.driveDeleteDirect({
        token: tokenDrive,
        folderId: typeof config.folderId === 'string' ? config.folderId : undefined,
        basePath: typeof config.basePath === 'string' ? config.basePath : undefined,
        remotePath: rel,
        locator: opts.locator,
      })
      await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
      if (parentDirOf(rel)) await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
      return
    }
    const prov = await import('@/utils/gitProvision')
    const token = String(config.token ?? '')
    if (!token) throw new Error('auth: mount has no token — save one on the provider card first')
    await prov.deleteFileDirect(
      backend as 'github' | 'gitlab',
      {
        backend,
        repoName: String(config.repoName ?? ''),
        fullName: String(config.repoName ?? ''),
        branch: String(config.branch ?? 'main'),
        url: '',
        projectId: backend === 'gitlab' ? String(config.repoName ?? '') : null,
      },
      token,
      rel,
      typeof config.basePath === 'string' ? config.basePath : undefined,
    )
  } else {
    const { invoke } = await import('./useTauri')
    await invoke('delete_remote_file', { configId: mount.configId, remotePath: rel })
  }
  await kvDelete(cacheKey(mountId, parentDirOf(rel))).catch(() => undefined)
  if (parentDirOf(rel)) await kvDelete(cacheKey(mountId, '')).catch(() => undefined)
}
