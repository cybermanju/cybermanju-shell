// CyberManju OS — host (device) filesystem bridge for Tauri native builds
//   (+ WASM delegation to the File System Access API leg).
//
// FileManager namespace: `/host` + the absolute host path, so every venue
// stays addressable in one tree:
//
//   /host                  → the `-os` default dir (sdcard on Android, the
//                            user's home dir on desktop; a virtual list of
//                            explicitly mounted folders on WASM/static builds)
//   /host/sdcard/Download  → that host dir (Android / Linux / macOS)
//   /host/C:/Users/me      → that host dir (Windows)
//   /host/                 → the filesystem root (native) / virtual mount list
//                            (WASM — browsers expose no filesystem root)
//
// Metadata ops (`ls/stat/mkdir/cp/mv/rm/write/… -os`) run through cybsh over
// the existing `os_exec` bridge — desktop IPC-or-REST, Android IPC — so no
// new Tauri command is needed and every transport (desktop, Docker server,
// Android) speaks the same verbs. Byte transport uses
// `@tauri-apps/plugin-fs` (binary-safe, no shell caps); vault bytes resolve
// through each node's `original_path`, provider bytes through the canal.
//
// On WASM/static builds (no `window.__TAURI__`) every verb below delegates
// to `@/utils/hostWasm` — same namespace, same `host:<abs>` ids, real File
// System Access I/O behind the picked folder — so the FileManager keeps one
// code path and only the bytes move differently.
//
// `moveAnywhere` is the one gesture for "move anything to anywhere":
// vault ⇄ provider ⇄ host, files and trees, copy or move, with
// verify-before-delete on every destructive leg.

import type { FileNode, ShellResult } from '@/types'
import { useTransport } from '@/composables/useTransport'
import {
  HOST_WRITE_LIMIT_BYTES,
  isHostFilesystemRoot,
  isSameOrDescendantHostPath,
  joinHostPath,
  normalizeHostPath,
} from './hostFsSafety'

export const HOST_ROOT = '/host'

export type HostRunner = (line: string) => Promise<string>

/**
 * True only on the explicitly classified WASM/static transport.
 * Web/REST dashboards also run in a browser without Tauri and must continue
 * through the REST-backed host shell instead of the local folder picker.
 */
function useWasmHost(): boolean {
  try {
    return useTransport().backend === 'wasm'
  } catch {
    return false
  }
}

async function defaultRunner(line: string): Promise<string> {
  const { invoke } = await import('@/composables/useTauri')
  const res = await invoke<ShellResult>('os_exec', { line })
  if (!res || !res.ok) throw new Error((res && res.output) || 'host command failed')
  return res.output || ''
}

/** Quote one shell operand (mirrors the terminal's `sh()` helper). */
export function shellQuote(p: string): string {
  return `"${String(p ?? '').replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`
}

/** True for FileManager paths inside the host namespace. */
export function isHostPath(p: string): boolean {
  const norm = String(p ?? '').replace(/\\/g, '/')
  return norm === HOST_ROOT || norm.startsWith(`${HOST_ROOT}/`)
}

/** True for FileNode ids that address host files (`host:<abs>`). */
export function isHostId(id: string): boolean {
  return String(id ?? '').startsWith('host:')
}

/**
 * `/host/C:/Users/me` → `C:/Users/me`; `/host/sdcard/x` → `/sdcard/x`;
 * `/host/` → `/`; `/host` → `''` (the `-os` default dir — callers resolve
 * it with `pwd -os`).
 */
export function hostAbsFromVm(vmPath: string): string {
  const norm = String(vmPath ?? '').replace(/\\/g, '/')
  if (norm === HOST_ROOT || norm === `${HOST_ROOT}/`) {
    return norm.endsWith('/') && norm.length > HOST_ROOT.length ? '/' : ''
  }
  if (!norm.startsWith(`${HOST_ROOT}/`)) return ''
  const rest = norm.slice(HOST_ROOT.length)
  if (rest === '/') return '/'
  // `/host/C:/…` carries a Windows drive (the leading slash is the vm
  // separator, not part of the path); everything else keeps its leading
  // slash — POSIX absolutes and UNC `//server/share` alike.
  if (/^\/[A-Za-z]:(\/|$)/.test(rest)) return rest.slice(1)
  return rest
}

/** `C:/Users/me` → `/host/C:/Users/me`; `/` → `/host/`; `''` → `/host`. */
export function vmForHost(abs: string): string {
  const clean = String(abs ?? '').replace(/\\/g, '/')
  if (!clean) return HOST_ROOT
  if (clean === '/') return `${HOST_ROOT}/`
  return `${HOST_ROOT}/${clean.replace(/^\/+/, '')}`
}

/** Parent FileManager path of a host listing (`/host/a` → `/host`). */
export function hostParentVm(vmPath: string): string {
  const norm = String(vmPath ?? '').replace(/\/+$/, '')
  if (norm === HOST_ROOT || !norm.startsWith(`${HOST_ROOT}/`)) return '/'
  const cut = norm.split('/').slice(0, -1).join('/') || HOST_ROOT
  return cut || '/'
}

export interface HostLsEntry {
  path: string
  name: string
  kind: string
  sizeBytes: number
  modifiedMs: number
  isDir: boolean
}

function msToIso(ms: number): string {
  try {
    const n = Number(ms ?? 0)
    if (!Number.isFinite(n) || n <= 0) return new Date().toISOString()
    return new Date(n).toISOString()
  } catch {
    return new Date().toISOString()
  }
}

export function hostEntriesToNodes(entries: HostLsEntry[], vmDir: string): FileNode[] {
  const now = new Date().toISOString()
  return (Array.isArray(entries) ? entries : []).map((e) => {
    const name = String(e.name ?? '')
    const abs = String(e.path ?? '')
    const isDir = !!e.isDir || e.kind === 'dir'
    const stamp = msToIso(e.modifiedMs)
    return {
      id: `host:${abs}`,
      name,
      fileType: isDir ? 'folder' : 'file',
      parentId: vmDir,
      path: vmForHost(abs),
      sizeBytes: Number(e.sizeBytes ?? 0),
      encrypted: false,
      compressionLayers: [],
      createdAt: stamp || now,
      modifiedAt: stamp || now,
      isHidden: name.startsWith('.'),
    } as FileNode
  })
}

/** Resolve the `-os` default dir (`/host` with no absolute behind it). */
export async function resolveHostDir(vmPath: string, run: HostRunner = defaultRunner): Promise<string> {
  if (useWasmHost()) {
    const { resolveHostWasmDir } = await import('@/utils/hostWasm')
    return resolveHostWasmDir(vmPath)
  }
  const abs = hostAbsFromVm(vmPath)
  if (abs) return abs
  const out = (await run('pwd -os')).trim()
  if (!out) throw new Error('not_found: host default directory is unreadable')
  return out
}

/** List one host directory as FileManager rows. */
export async function listHostDir(vmPath: string, run: HostRunner = defaultRunner): Promise<FileNode[]> {
  if (useWasmHost()) {
    const { listHostWasmDir } = await import('@/utils/hostWasm')
    return listHostWasmDir(vmPath)
  }
  const abs = await resolveHostDir(vmPath, run)
  const nicerVm = vmForHost(abs)
  const out = await run(`ls --json -os ${shellQuote(abs)}`)
  let parsed: { entries?: HostLsEntry[] }
  try {
    parsed = JSON.parse(out) as { entries?: HostLsEntry[] }
  } catch {
    throw new Error(`integrity: host listing of '${abs}' is not JSON`)
  }
  return hostEntriesToNodes(parsed.entries ?? [], nicerVm)
}

export async function statHost(abs: string, run: HostRunner = defaultRunner): Promise<HostLsEntry> {
  if (useWasmHost()) {
    const { statHostWasm } = await import('@/utils/hostWasm')
    return statHostWasm(abs)
  }
  const out = await run(`stat --json -os ${shellQuote(abs)}`)
  try {
    return JSON.parse(out) as HostLsEntry
  } catch {
    throw new Error(`integrity: host stat of '${abs}' is not JSON`)
  }
}

export async function mkdirHost(abs: string, run: HostRunner = defaultRunner): Promise<void> {
  if (useWasmHost()) {
    const { mkdirHostWasm } = await import('@/utils/hostWasm')
    await mkdirHostWasm(abs)
    return
  }
  await run(`mkdir -os -p ${shellQuote(abs)}`)
}

export async function writeHostText(abs: string, content: string, run: HostRunner = defaultRunner): Promise<void> {
  const byteLength = new TextEncoder().encode(content).byteLength
  if (byteLength > HOST_WRITE_LIMIT_BYTES) {
    throw new Error(`too_large: content is ${byteLength} UTF-8 bytes, host text write limit is ${HOST_WRITE_LIMIT_BYTES}`)
  }
  if (useWasmHost()) {
    const { writeHostWasmText } = await import('@/utils/hostWasm')
    await writeHostWasmText(abs, content)
    return
  }
  // Content rides as one quoted argv word. Binary goes through
  // writeHostBytes (plugin-fs) instead.
  await run(`write -os ${shellQuote(abs)} ${shellQuote(content)}`)
}

export async function rmHost(abs: string, recursive: boolean, run: HostRunner = defaultRunner): Promise<void> {
  if (isHostFilesystemRoot(abs)) {
    throw new Error('invalid: refusing to delete the host filesystem root')
  }
  if (useWasmHost()) {
    const { rmHostWasm } = await import('@/utils/hostWasm')
    await rmHostWasm(abs, recursive)
    return
  }
  await run(recursive ? `rm -os -r ${shellQuote(abs)}` : `rm -os ${shellQuote(abs)}`)
}

export async function cpHost(src: string, dst: string, recursive: boolean, run: HostRunner = defaultRunner): Promise<void> {
  if (isHostFilesystemRoot(src)) {
    throw new Error('invalid: refusing to copy the host filesystem root')
  }
  if (useWasmHost()) {
    const { cpHostWasm } = await import('@/utils/hostWasm')
    await cpHostWasm(src, dst, recursive)
    return
  }
  await run(recursive ? `cp -os -r ${shellQuote(src)} ${shellQuote(dst)}` : `cp -os ${shellQuote(src)} ${shellQuote(dst)}`)
}

export async function mvHost(src: string, dst: string, run: HostRunner = defaultRunner): Promise<void> {
  if (isHostFilesystemRoot(src)) {
    throw new Error('invalid: refusing to move the host filesystem root')
  }
  if (useWasmHost()) {
    const { mvHostWasm } = await import('@/utils/hostWasm')
    await mvHostWasm(src, dst)
    return
  }
  await run(`mv -os ${shellQuote(src)} ${shellQuote(dst)}`)
}

/** Binary-safe host read (plugin-fs — no shell caps, no text mangling). */
export async function readHostBytes(abs: string): Promise<Uint8Array> {
  if (useWasmHost()) {
    const { readHostWasmBytes } = await import('@/utils/hostWasm')
    return readHostWasmBytes(abs)
  }
  try {
    const { readFile } = await import('@tauri-apps/plugin-fs')
    const data = await readFile(abs)
    return data instanceof Uint8Array ? data : new Uint8Array(data as ArrayBuffer)
  } catch (e) {
    throw new Error(`not_found: host file '${abs}' is unreadable (${e instanceof Error ? e.message : String(e)})`)
  }
}

/** Binary-safe host write (parents included). */
export async function writeHostBytes(abs: string, data: Uint8Array): Promise<void> {
  if (useWasmHost()) {
    const { writeHostWasmBytes } = await import('@/utils/hostWasm')
    await writeHostWasmBytes(abs, data)
    return
  }
  try {
    const { mkdir, writeFile } = await import('@tauri-apps/plugin-fs')
    const parent = abs.replace(/\\/g, '/').split('/').slice(0, -1).join('/')
    if (parent && parent !== abs) await mkdir(parent, { recursive: true }).catch(() => undefined)
    await writeFile(abs, data)
  } catch (e) {
    throw new Error(`integrity: host write failed for '${abs}' (${e instanceof Error ? e.message : String(e)})`)
  }
}

// ── move anything → anywhere ──────────────────────────────────────────

export type MoveSource =
  | { kind: 'vault'; file: FileNode }
  | { kind: 'provider'; mountId: string; remotePath: string; locator: string; name: string; isDir: boolean }
  | { kind: 'host'; absPath: string; name: string; isDir: boolean }

export type MoveDest =
  | { kind: 'vault'; vaultPath: string }
  | { kind: 'provider'; mountId: string; remoteDir: string }
  | { kind: 'host'; absDir: string }

export interface CanalDeps {
  readProviderFile: (mountId: string, remotePath: string, locator?: string) => Promise<Uint8Array>
  writeProviderFile: (mountId: string, remotePath: string, data: Uint8Array) => Promise<void>
  deleteProviderFile: (mountId: string, remotePath: string, locator?: string) => Promise<void>
  listProviderDir: (mountId: string, remotePath: string) => Promise<{ name: string; path: string; locator: string; isDir: boolean; sizeBytes: number }[]>
}

export interface MoveDeps {
  run: HostRunner
  invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>
  canal: CanalDeps
  listVaultDir: (vaultPath: string) => Promise<FileNode[]>
  readHostBytes: (abs: string) => Promise<Uint8Array>
  writeHostBytes: (abs: string, data: Uint8Array) => Promise<void>
}

async function liveCanal(): Promise<CanalDeps> {
  const canal = await import('@/composables/useProviderCanal')
  return {
    readProviderFile: async (mountId, remotePath, locator) =>
      (await canal.readVfsFile(mountId, remotePath, { locator: locator || remotePath })).bytes,
    writeProviderFile: async (mountId, remotePath, data) => {
      await canal.writeVfsFile(mountId, remotePath, data)
    },
    deleteProviderFile: async (mountId, remotePath, locator) => {
      await canal.deleteVfsFile(mountId, remotePath, { locator: locator || remotePath })
    },
    listProviderDir: async (mountId, remotePath) => {
      const rows = await canal.listVfsDir(mountId, remotePath)
      return rows.map((e) => ({
        name: e.name,
        path: e.path,
        locator: e.locator || e.path,
        isDir: e.isDir,
        sizeBytes: Number(e.sizeBytes ?? 0),
      }))
    },
  }
}

/** Default (live) dependency set — dynamic imports keep this tree-shakable. */
export async function liveMoveDeps(): Promise<MoveDeps> {
  if (useWasmHost()) {
    const { liveWasmMoveDeps } = await import('@/utils/hostWasm')
    return liveWasmMoveDeps()
  }
  const { invoke } = await import('@/composables/useTauri')
  return {
    run: defaultRunner,
    invoke,
    canal: await liveCanal(),
    listVaultDir: async (vaultPath: string) =>
      invoke<FileNode[]>('list_files', { parentPath: vaultPath }),
    readHostBytes,
    writeHostBytes,
  }
}

/** Vault bytes: the node's `original_path` on the device, else editor text. */
export async function readVaultBytes(file: FileNode, deps: MoveDeps): Promise<Uint8Array> {
  const ctx = (file.contextData ?? {}) as Record<string, unknown>
  const orig = typeof ctx.original_path === 'string' ? ctx.original_path : ''
  if (orig) {
    try {
      return await deps.readHostBytes(orig)
    } catch {
      // The referenced device file is gone — fall through to editor text.
    }
  }
  try {
    const res = await deps.invoke<{ content?: string }>('read_file_content', { fileId: file.id })
    if (res && typeof res.content === 'string') return new TextEncoder().encode(res.content)
  } catch {
    // Reports honestly below.
  }
  throw new Error(`not_found: '${file.name}' has no readable bytes on this device`)
}

async function uploadVaultBytes(name: string, data: Uint8Array, vaultPath: string, deps: MoveDeps): Promise<void> {
  await deps.invoke('upload_file', { fileName: name, fileData: Array.from(data), parentPath: vaultPath })
}

function joinRemote(dir: string, name: string): string {
  const clean = String(dir ?? '').replace(/^\/+|\/+$/g, '')
  return clean ? `${clean}/${name}` : name
}

function srcName(src: MoveSource): string {
  if (src.kind === 'vault') return src.file.name
  return src.name
}

function srcIsDir(src: MoveSource): boolean {
  if (src.kind === 'vault') return src.file.fileType === 'folder'
  return src.isDir
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false
  return true
}

async function deleteVaultSource(file: FileNode, deps: MoveDeps): Promise<void> {
  await deps.invoke('delete_file', { fileId: file.id })
}

async function readSourceBytes(src: MoveSource, deps: MoveDeps): Promise<Uint8Array> {
  if (src.kind === 'host') return deps.readHostBytes(src.absPath)
  if (src.kind === 'provider') return deps.canal.readProviderFile(src.mountId, src.remotePath, src.locator)
  return readVaultBytes(src.file, deps)
}

async function deleteSource(src: MoveSource, deps: MoveDeps): Promise<void> {
  if (src.kind === 'host') {
    await deps.run(`rm -os ${shellQuote(src.absPath)}`)
    return
  }
  if (src.kind === 'provider') {
    await deps.canal.deleteProviderFile(src.mountId, src.remotePath, src.locator)
    return
  }
  await deleteVaultSource(src.file, deps)
}

async function writeDestBytes(dest: MoveDest, name: string, data: Uint8Array, deps: MoveDeps): Promise<void> {
  if (dest.kind === 'host') {
    const clean = dest.absDir.replace(/\/+$/, '')
    await deps.writeHostBytes(`${clean}/${name}`, data)
    return
  }
  if (dest.kind === 'provider') {
    await deps.canal.writeProviderFile(dest.mountId, joinRemote(dest.remoteDir, name), data)
    return
  }
  await uploadVaultBytes(name, data, dest.vaultPath, deps)
}

async function verifyDestBytes(dest: MoveDest, name: string, data: Uint8Array, deps: MoveDeps): Promise<void> {
  let back: Uint8Array
  if (dest.kind === 'host') {
    const clean = dest.absDir.replace(/\/+$/, '')
    back = await deps.readHostBytes(`${clean}/${name}`)
  } else if (dest.kind === 'provider') {
    back = await deps.canal.readProviderFile(dest.mountId, joinRemote(dest.remoteDir, name))
  } else {
    // Vault rows are content-addressed metadata (hash + size refresh on
    // write) — the byte-exact store is the device/imports path, so a
    // re-read here would only echo the pipeline. Size agreement is the
    // honest check available at this layer.
    return
  }
  if (!bytesEqual(data, back)) {
    throw new Error(`integrity: '${name}' differs after write — source kept`)
  }
}

/** One file across namespaces: write → verify → delete-source on move. */
async function moveFileAcross(src: MoveSource, dest: MoveDest, op: 'copy' | 'move', deps: MoveDeps): Promise<number> {
  const name = srcName(src)
  const data = await readSourceBytes(src, deps)
  await writeDestBytes(dest, name, data, deps)
  await verifyDestBytes(dest, name, data, deps)
  if (op === 'move') await deleteSource(src, deps)
  return data.length
}

/** Recursive tree walkers per namespace (one level each; callers recurse). */
async function listSourceDir(src: MoveSource, deps: MoveDeps): Promise<{ name: string; child: MoveSource }[]> {
  if (src.kind === 'host') {
    const out = await deps.run(`ls --json -os ${shellQuote(src.absPath)}`)
    const parsed = JSON.parse(out) as { entries?: HostLsEntry[] }
    return (parsed.entries ?? []).map((e) => ({
      name: e.name,
      child: {
        kind: 'host',
        absPath: e.path,
        name: e.name,
        isDir: !!e.isDir || e.kind === 'dir',
      } as MoveSource,
    }))
  }
  if (src.kind === 'provider') {
    const rows = await deps.canal.listProviderDir(src.mountId, src.remotePath)
    return rows.map((e) => ({
      name: e.name,
      child: {
        kind: 'provider',
        mountId: src.mountId,
        remotePath: joinRemote(src.remotePath, e.name),
        locator: e.locator,
        name: e.name,
        isDir: e.isDir,
      } as MoveSource,
    }))
  }
  // Vault hierarchy is keyed by parent PATH, not by node id: children of
  // a folder live under its vault path, resolved from the node's own path.
  const dirPath = src.file.path && src.file.fileType === 'folder'
    ? src.file.path
    : `/${src.file.name}`
  const rows = await deps.listVaultDir(dirPath)
  return rows.map((f) => ({
    name: f.name,
    child: { kind: 'vault', file: f } as MoveSource,
  }))
}

async function ensureDestDir(dest: MoveDest, name: string, deps: MoveDeps): Promise<MoveDest> {
  if (dest.kind === 'host') {
    const next = joinHostPath(dest.absDir, name)
    await deps.run(`mkdir -os -p ${shellQuote(next)}`)
    return { kind: 'host', absDir: next }
  }
  if (dest.kind === 'provider') {
    return { kind: 'provider', mountId: dest.mountId, remoteDir: joinRemote(dest.remoteDir, name) }
  }
  const clean = dest.vaultPath === '/' ? '' : dest.vaultPath.replace(/\/+$/, '')
  const next = `${clean}/${name}` || '/'
  await deps.invoke('create_folder', { name, parentId: dest.vaultPath })
  return { kind: 'vault', vaultPath: next }
}

/**
 * Move or copy anything to anywhere. Same-namespace fast paths stay on
 * their native verbs (`mv -os`, vault `move_file`, canal copy); everything
 * else crosses through bytes with verify-before-delete, files and trees.
 * Returns a human summary (`3 files · 12 KiB`).
 */
export async function moveAnywhere(
  src: MoveSource,
  dest: MoveDest,
  op: 'copy' | 'move',
  deps: MoveDeps,
): Promise<string> {
  if (src.kind === 'host') {
    const normalizedSource = normalizeHostPath(src.absPath)
    const wasmMountRoot = useWasmHost() && /^\/[^/]+$/.test(normalizedSource)
    if (isHostFilesystemRoot(normalizedSource) || wasmMountRoot) {
      throw new Error('invalid: refusing to copy or move a host or mounted-folder root')
    }
    if (dest.kind === 'host') {
      if (!dest.absDir) throw new Error('invalid: host destination directory is unresolved')
      const target = joinHostPath(dest.absDir, srcName(src))
      // Conservative case folding covers case-insensitive desktop volumes and
      // prevents copying/moving a directory into itself through a path alias.
      if (isSameOrDescendantHostPath(src.absPath, target, true)) {
        throw new Error(`invalid: host destination '${target}' is the source or lies inside it`)
      }
    }
  }
  // ── same-namespace fast paths ──
  if (src.kind === 'host' && dest.kind === 'host') {
    const clean = normalizeHostPath(dest.absDir)
    if (!clean) throw new Error('invalid: host destination directory is unresolved')
    if (srcIsDir(src)) {
      if (op === 'copy') await deps.run(`cp -os -r ${shellQuote(src.absPath)} ${shellQuote(clean)}`)
      else await deps.run(`mv -os ${shellQuote(src.absPath)} ${shellQuote(clean)}`)
      return `${op === 'move' ? 'Moved' : 'Copied'} '${srcName(src)}' on this device`
    }
    const fileTarget = joinHostPath(clean, srcName(src))
    if (op === 'copy') await deps.run(`cp -os ${shellQuote(src.absPath)} ${shellQuote(fileTarget)}`)
    else await deps.run(`mv -os ${shellQuote(src.absPath)} ${shellQuote(fileTarget)}`)
    return `${op === 'move' ? 'Moved' : 'Copied'} '${srcName(src)}' on this device`
  }
  if (src.kind === 'vault' && dest.kind === 'vault' && !srcIsDir(src)) {
    if (op === 'move') {
      await deps.invoke('move_file', { fileId: src.file.id, newParentId: dest.vaultPath })
      return `Moved '${srcName(src)}' in the vault`
    }
    const dup = await deps.invoke<FileNode>('duplicate_file_context', { fileId: src.file.id })
    const newId = (dup as FileNode | null)?.id
    if (!newId) throw new Error('integrity: vault copy returned no node — retry')
    if (dup.parentId !== dest.vaultPath) {
      await deps.invoke('move_file', { fileId: newId, newParentId: dest.vaultPath })
    }
    return `Copied '${srcName(src)}' in the vault`
  }
  if (src.kind === 'vault' && dest.kind === 'vault') {
    // Folders move by id (one `move_file` per child): re-parenting rows
    // keeps the parent index exact, where deleting a folder row would
    // orphan its children.
    return moveVaultDirToVault(src.file, dest.vaultPath, op, deps)
  }

  // ── trees cross through bytes, one verified file at a time ──
  if (srcIsDir(src)) {
    const name = srcName(src)
    const next = await ensureDestDir(dest, name, deps)
    const kids = await listSourceDir(src, deps)
    let files = 0
    let bytes = 0
    for (const k of kids) {
      if (srcIsDir(k.child)) {
        const sub = await moveAnywhere(k.child, next, op, deps)
        const m = /(\d+) files? · /.exec(sub)
        files += m ? Number(m[1]) : 0
      } else {
        bytes += await moveFileAcross(k.child, next, op, deps)
        files++
      }
    }
    if (op === 'move') {
      // The tree copied over verified: drop the (now empty) source dir.
      try {
        await deleteSourceDir(src, deps)
      } catch {
        // A straggler raced us — the bytes already landed verified.
      }
    }
    return `${op === 'move' ? 'Moved' : 'Copied'} '${name}' (${files} files · ${humanSize(bytes)})`
  }

  const bytes = await moveFileAcross(src, dest, op, deps)
  return `${op === 'move' ? 'Moved' : 'Copied'} '${srcName(src)}' (${humanSize(bytes)}, verified)`
}

/** Vault folder → vault folder: re-parent every row by id, recursively. */
async function moveVaultDirToVault(
  folder: FileNode,
  destVaultPath: string,
  op: 'copy' | 'move',
  deps: MoveDeps,
): Promise<string> {
  const dirPath = folder.path && folder.fileType === 'folder' ? folder.path : `/${folder.name}`
  const kids = await deps.listVaultDir(dirPath)
  const clean = destVaultPath === '/' ? '' : destVaultPath.replace(/\/+$/, '')
  // Copying a folder onto its own parent would merge into itself — fork
  // the name like every other duplicate in the vault.
  const targetName = dirPath === `${clean}/${folder.name}` && op === 'copy'
    ? `${folder.name} (copy)`
    : folder.name
  const subPath = `${clean}/${targetName}` || '/'
  if (op === 'copy' || dirPath !== subPath) {
    // `create_folder` is idempotent enough to attempt blindly; an existing
    // name merges (vault names are not unique keys).
    await deps.invoke('create_folder', { name: targetName, parentId: destVaultPath }).catch(() => undefined)
  }
  let files = 0
  for (const k of kids) {
    if (k.fileType === 'folder') {
      const sub = await moveVaultDirToVault(k, subPath, op, deps)
      const m = /(\d+) files?/.exec(sub)
      files += m ? Number(m[1]) : 0
    } else if (op === 'move') {
      await deps.invoke('move_file', { fileId: k.id, newParentId: subPath })
      files++
    } else {
      const dup = await deps.invoke<FileNode>('duplicate_file_context', { fileId: k.id })
      const newId = (dup as FileNode | null)?.id
      if (!newId) throw new Error('integrity: vault copy returned no node — retry')
      if (dup.parentId !== subPath) {
        await deps.invoke('move_file', { fileId: newId, newParentId: subPath })
      }
      files++
    }
  }
  if (op === 'move' && dirPath !== subPath) {
    const left = await deps.listVaultDir(dirPath).catch(() => [] as FileNode[])
    if (left.length === 0) await deleteVaultSource(folder, deps).catch(() => undefined)
  }
  return `${op === 'move' ? 'Moved' : 'Copied'} '${folder.name}' (${files} files)`
}

async function deleteSourceDir(src: MoveSource, deps: MoveDeps): Promise<void> {  if (src.kind === 'host') {
    await deps.run(`rm -os -r ${shellQuote(src.absPath)}`)
    return
  }
  if (src.kind === 'provider') return // blob prefixes fold away on their own
  await deleteVaultSource(src.file, deps)
}

function humanSize(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`
  return `${(n / 1024 / 1024).toFixed(1)} MiB`
}

/** Describe a FileManager selection row as a move source. */
export function sourceFromNode(f: FileNode, currentVmPath: string): MoveSource | null {
  const id = String(f.id || '')
  if (id.startsWith('host:')) {
    return {
      kind: 'host',
      absPath: id.slice('host:'.length),
      name: f.name,
      isDir: f.fileType === 'folder',
    }
  }
  const segs = id.replace(/\\/g, '/').replace(/^\/+/, '').split('/').filter(Boolean)
  if (segs[0] === 'providers' && segs.length >= 2) {
    const remotePath = segs.slice(2).join('/')
    if (!remotePath) return null // the mount row itself is not movable
    return {
      kind: 'provider',
      mountId: segs[1],
      remotePath,
      locator: remotePath,
      name: f.name,
      isDir: f.fileType === 'folder',
    }
  }
  void currentVmPath
  if (!f.id) return null
  return { kind: 'vault', file: f }
}

/** Describe the FileManager's current folder as a move destination. */
export function destFromVmPath(vmPath: string): MoveDest | null {
  const norm = String(vmPath ?? '').replace(/\\/g, '/')
  if (isHostPath(norm)) {
    const abs = hostAbsFromVm(norm)
    // Native `/host` carries no absolute — callers resolve the `-os`
    // default dir first. On WASM `/host` is the virtual folders list
    // (mounts only) — never a move destination either.
    if (abs === '' || (useWasmHost() && abs === '/')) return null
    return { kind: 'host', absDir: abs }
  }
  if (norm === '/providers' || norm === '/providers/') return null
  const segs = norm.replace(/^\/+/, '').split('/').filter(Boolean)
  if (segs[0] === 'providers' && segs.length >= 2) {
    return { kind: 'provider', mountId: segs[1], remoteDir: segs.slice(2).join('/') }
  }
  return { kind: 'vault', vaultPath: norm || '/' }
}
