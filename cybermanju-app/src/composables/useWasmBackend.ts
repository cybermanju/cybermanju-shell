// CyberManju OS — WASM Backend Bridge
//
// The GitHub Pages build ships no dashboard behind it, so REST calls to
// `http://localhost:3456` are doomed to `ERR_CONNECTION_REFUSED`. Instead,
// when the app runs from a non-3456 origin (i.e. the WASM / Pages pack),
// the OS-layer commands are served by the `cybermanju-os-wasm` crate:
// `os_dispatch(cmd, args_json)` answers the terminal, task table, volume df
// and workers against a virtual volume kept in localStorage.
//
// Data APIs (files, accounts, collections, …) have NO wasm implementation —
// they are database-backed and stay REST-only. Those callers get a single
// clear "dashboard not reachable" error instead of per-request fetch spam.

interface WasmBackend {
  os_dispatch: (cmd: string, argsJson: string) => string
}

let wasmModule: WasmBackend | null = null
let wasmLoad: Promise<WasmBackend> | null = null

interface WasmAgent {
  agent_catalog: () => string
  agent_prompt: (reqJson: string) => Promise<string>
}

/** Provider catalog from the shared Rust core (no network, works offline). */
export async function wasmAgentCatalog(): Promise<unknown[]> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  const raw = ((mod as unknown as WasmAgent).agent_catalog() ?? '[]') as string
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

export interface WasmAgentTurn {
  ok: boolean
  turn?: {
    content: string
    tool_calls: Array<{ id: string; name: string; input: Record<string, unknown> }>
    usage: { inputTokens: number; outputTokens: number }
    finish: string
  }
  error?: string
}

/** One provider turn over browser fetch (see os-wasm agent_prompt). */
export async function wasmAgentPrompt(req: {
  url: string
  dialect: string
  model: string
  headers: Array<[string, string]>
  system: string
  messages: Array<Record<string, unknown>>
  tools: boolean
  /** Canonical MCP defs merged per-dialect by the wasm bridge. */
  extraTools?: Array<Record<string, unknown>>
}): Promise<WasmAgentTurn> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  const raw = (await (mod as unknown as WasmAgent).agent_prompt(JSON.stringify(req))) as string
  try {
    return JSON.parse(raw) as WasmAgentTurn
  } catch {
    return { ok: false, error: 'network: unreadable agent reply' }
  }
}

// ── Provider canal (Phase 5, canal B) ─────────────────────────────────
// probe/list/fetch and the artifact codec live in `crates/os-wasm`
// (canal.rs / artifact.rs). Main-thread load path, same as the agent —
// the browser fetch needs a real origin, so these never run in the
// db-worker.

interface WasmCanal {
  canal_dispatch: (op: string, argsJson: string) => Promise<string>
  canal_fetch: (configJson: string, locator: string) => Promise<Uint8Array>
  artifact_magic: (bytes: Uint8Array) => string
  artifact_open: (bytes: Uint8Array, passphrase: string) => Uint8Array
}

const CANAL_STALE =
  "the wasm bundle predates the provider canal — rebuild it with 'npm run build:wasm:frontend'"

function canalFn<K extends keyof WasmCanal>(mod: WasmBackend, key: K): WasmCanal[K] {
  const fn = (mod as unknown as Partial<WasmCanal>)[key]
  if (typeof fn !== 'function') throw new Error(CANAL_STALE)
  return fn as WasmCanal[K]
}

interface CanalEnvelope<T> {
  ok?: boolean
  data?: T
  error?: string
}

/**
 * One canal op (`probe` | `list`) — envelope in, `data` out; failures
 * throw with the house prefixes the Rust side produced (`cors:`, `auth:` …).
 */
export async function wasmCanalDispatch<T>(op: string, args: Record<string, unknown>): Promise<T> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  const raw = await canalFn(mod, 'canal_dispatch')(op, JSON.stringify(args))
  let env: CanalEnvelope<T>
  try {
    env = JSON.parse(raw) as CanalEnvelope<T>
  } catch {
    throw new Error('network: unreadable canal reply')
  }
  if (!env.ok) throw new Error(String(env.error ?? 'unknown canal error'))
  return env.data as T
}

/** One provider file's raw bytes; rejects with the same house prefixes. */
export async function wasmCanalFetch(config: unknown, locator: string): Promise<Uint8Array> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  return canalFn(mod, 'canal_fetch')(JSON.stringify(config), locator)
}

/** Header sniff: `CYBE1` | `CYBMJ01` | `CYBMJU1` | `raw` — no key needed. */
export async function wasmArtifactMagic(bytes: Uint8Array): Promise<string> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  return canalFn(mod, 'artifact_magic')(bytes)
}

/** Unlock + triple-decompress an artifact (Argon2id, byte-identical to desktop). */
export async function wasmArtifactOpen(bytes: Uint8Array, passphrase: string): Promise<Uint8Array> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  return canalFn(mod, 'artifact_open')(bytes, passphrase)
}

/**
 * Load the wasm-pack bundle (`--target web` output). Vite sees the
 * virtual module via the resolved alias in `vite.config.wasm.ts`; when the
 * bundle isn't present (plain dev server, dashboard origin) the load fails
 * and we degrade back to REST.
 */
async function loadWasm(): Promise<typeof wasmModule> {
  if (wasmModule) return wasmModule
  if (!wasmLoad) {
    const pending = (async () => {
      const mod = (await import('cybermanju-os-wasm')) as unknown as WasmBackend & {
        default: () => Promise<void>
      }
      await mod.default()
      wasmModule = mod
      return mod
    })()
    wasmLoad = pending
    // A transient chunk/network or wasm-bindgen init failure should not poison
    // this tab forever. Keep the rejection for current callers, but let the
    // next interaction start a fresh load attempt.
    void pending.catch(() => {
      if (wasmLoad === pending) wasmLoad = null
    })
  }
  return wasmLoad
}

/** True once the wasm backend has been loaded (or loading has started). */
export function wasmBackendActive(): boolean {
  return wasmModule !== null
}

/**
 * The whole wasm module (os/db/crypto/compression exports), loaded once.
 * Crypto callers cast it to the narrow interface they need — the local
 * `.d.ts` shim only declares the entry points every transport shares.
 */
export async function wasmModuleExports<T>(): Promise<T> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  return mod as unknown as T
}

/** Dispatch one os/* command through the wasm crate. Throws on transport errors. */
export async function wasmOsDispatch(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const mod = await loadWasm()
  if (!mod) throw new Error('wasm backend unavailable')
  const payload = JSON.stringify({ args: argsToArgList(cmd, args) })
  const raw = mod.os_dispatch(cmd, payload)
  fireOsDispatchHooks()
  try {
    return JSON.parse(raw)
  } catch {
    return raw
  }
}

// Hooks let other composables react to *any* volume mutation without the
// terminal (or `exec`'s internal write/touch/rm) knowing they exist — the
// volume mirror is the only subscriber today.
type OsDispatchHook = () => void
const osDispatchHooks: OsDispatchHook[] = []

export function addOsDispatchHook(fn: OsDispatchHook): () => void {
  osDispatchHooks.push(fn)
  return () => {
    const i = osDispatchHooks.indexOf(fn)
    if (i >= 0) osDispatchHooks.splice(i, 1)
  }
}

function fireOsDispatchHooks() {
  for (const fn of [...osDispatchHooks]) {
    try {
      fn()
    } catch {
      // A hook must never turn a successful command into a failure.
    }
  }
}

/**
 * Nudge volume-mirror subscribers after a direct localStorage volume edit.
 * The static cybsh layer mutates the volume itself for provider↔local moves
 * (the wasm `rm` arm may predate the deployed pkg); the debounced mirror
 * then carries the delta into `volume:*` kv rows as usual.
 */
export function notifyOsDispatch(): void {
  fireOsDispatchHooks()
}

/**
 * Map the os/* REST contract onto the dispatcher's arg-list contract.
 * The wasm dispatcher takes positional string args; the frontend passes
 * named ones.
 */
function argsToArgList(cmd: string, args: Record<string, unknown>): string[] {
  const list: string[] = []
  switch (cmd) {
    case 'exec': list.push(String(args.line ?? '')); break
    case 'complete': list.push(String(args.prefix ?? '')); break
    case 'stat':
    case 'ls':
    case 'du':
    case 'cat':
      if (args.path) list.push(String(args.path))
      break
    case 'compute':
      if (args.job) list.push(String(args.job))
      if (args.path) list.push(String(args.path))
      break
    case 'write':
      if (args.path) list.push(String(args.path))
      list.push(String(args.content ?? ''))
      break
    case 'rm':
      if (args.recursive) list.push('-r')
      for (const p of (args.paths as string[] | undefined) ?? []) list.push(p)
      break
    case 'kill':
      if (args.id !== undefined) list.push(String(args.id))
      break
    case 'search':
      if (args.query) list.push(String(args.query))
      break
    default:
      break
  }
  return list
}

/**
 * Search the wasm volume with BM25-lite — the only data-shape API the wasm
 * pack implements, exposed as a source for `search_files` on Pages.
 * Prefers the scored arm (`search-scored`: real score + snippet + match
 * kind for the cross-device port) and falls back to the legacy path-only
 * `search` output for older packs.
 */
export interface WasmScoredHit {
  path: string
  score: number
  snippet: string
  matchType: string
}

export async function wasmSearchFilesScored(query: string): Promise<WasmScoredHit[]> {
  const mod = await loadWasm()
  if (!mod) return []
  const parse = (raw: string): WasmScoredHit[] | null => {
    try {
      const parsed = JSON.parse(raw) as { ok?: boolean; output?: string }
      if (parsed.ok === false || !parsed.output) return []
      if (parsed.output.startsWith('no matches for')) return []
      return parsed.output
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean)
        .map((line) => {
          const parts = line.split('\t')
          if (parts.length >= 3) {
            const score = Number(parts[0])
            return {
              path: parts[1] || parts[parts.length - 1],
              score: Number.isFinite(score) ? score : 1.0,
              snippet: parts.slice(3).join('\t') || '',
              matchType: parts[2] || 'content',
            }
          }
          return { path: line, score: 1.0, snippet: '', matchType: 'content' }
        })
    } catch {
      return null
    }
  }
  try {
    const raw = mod.os_dispatch('search-scored', JSON.stringify({ args: [query] }))
    const hits = parse(raw)
    if (hits) return hits
  } catch {
    // Fall through to the legacy arm below.
  }
  const raw = mod.os_dispatch('search', JSON.stringify({ args: [query] }))
  return parse(raw) ?? []
}

export async function wasmSearchFiles(query: string): Promise<Array<{ path: string; score: number }>> {
  const hits = await wasmSearchFilesScored(query)
  return hits.map((h) => ({ path: h.path, score: h.score }))
}

// ── Database worker (redb in OPFS) ────────────────────────────────────
// The database runs in a Dedicated Worker because OPFS sync access handles
// — redb's only durable browser primitive — exist solely there. Calls are
// message-passed (the frontend is async end-to-end, so no shared-memory
// ferry is needed). If worker construction fails, we fall back to running
// the same wasm module on the main thread with an in-memory database
// (session-only persistence).

interface DbReply {
  id: number
  ok: boolean
  data?: unknown
  error?: string
}

let dbWorker: Worker | null = null
let dbWorkerFailed = false
let dbSeq = 0
const dbPending = new Map<
  number,
  { resolve: (v: unknown) => void; reject: (e: Error) => void; timer: number }
>()

/** Which storage backs the demo database once known (`opfs`|`memory`|null). */
let dbBackend: string | null = null
export async function wasmDbBackend(): Promise<string | null> {
  if (dbBackend) return dbBackend
  try {
    const s = (await wasmDbDispatch('_status', {}, 30000)) as { backend?: string }
    if (s?.backend) dbBackend = s.backend
  } catch {
    dbBackend = null
  }
  return dbBackend
}

function dbRejectAll(err: Error) {
  for (const [, p] of dbPending) {
    window.clearTimeout(p.timer)
    p.reject(err)
  }
  dbPending.clear()
}

let mainDbOpen: Promise<unknown> | null = null

interface MainDiskState {
  handle: FileSystemFileHandle | null
  name: string
  passphrase: string
  savedAt: number
  savedBytes: number
}

let mainDisk: MainDiskState | null = null
let mainDirty = false

/** Session vault passphrase held after `.cybermanju` unlock (never
 *  persisted by this module). Secrets seal/open on WASM pass it as an
 *  argument — empty string means locked, and the ops answer `unsupported:`. */
export function sessionVaultPassphrase(): string {
  return mainDisk?.passphrase ?? ''
}

function mainDiskStatus() {
  return {
    backend: 'memory' as const,
    file: 'cybermanju.db',
    attached: !!mainDisk,
    name: mainDisk?.name ?? '',
    savedAt: mainDisk?.savedAt ?? 0,
    savedBytes: mainDisk?.savedBytes ?? 0,
    dirty: mainDirty,
    lastError: null,
    disk: {
      attached: !!mainDisk,
      name: mainDisk?.name ?? '',
      savedAt: mainDisk?.savedAt ?? 0,
      savedBytes: mainDisk?.savedBytes ?? 0,
      dirty: mainDirty,
    },
  }
}

function mainIsReadOnly(op: string): boolean {
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

async function ensureMainDb() {
  const mod = await loadWasm()
  if (!mod || typeof (mod as unknown as { db_dispatch?: unknown }).db_dispatch !== 'function') {
    throw new Error('wasm database unavailable')
  }
  const { db_open } = mod as unknown as { db_open: () => Promise<string> }
  if (!mainDbOpen) {
    mainDbOpen = (async () => {
      const raw = await db_open()
      const env = JSON.parse(raw) as { ok: boolean; error?: string }
      if (!env.ok) throw new Error(env.error || 'db_open failed')
      return env
    })()
    mainDbOpen.catch(() => {
      mainDbOpen = null
    })
  }
  await mainDbOpen
  return mod as unknown as {
    db_dispatch: (op: string, argsJson: string) => string
    db_snapshot: () => Uint8Array
    db_restore: (data: Uint8Array) => string
  }
}

async function mainSnapshot(): Promise<Uint8Array> {
  const mod = await ensureMainDb()
  const out = mod.db_snapshot() as unknown as Uint8Array
  return out instanceof Uint8Array ? out : new Uint8Array(out as unknown as ArrayLike<number>)
}

async function mainRestore(image: Uint8Array): Promise<void> {
  const mod = await ensureMainDb()
  const raw = mod.db_restore(image)
  const env = JSON.parse(raw) as { ok: boolean; data?: unknown; error?: string }
  if (!env.ok) throw new Error(String(env.error ?? 'restore failed'))
}

/** Replace the live database with a verified snapshot image (Pages restore). */
export async function wasmDatabaseRestore(image: Uint8Array): Promise<void> {
  await mainRestore(image)
}

async function mainThreadDbDispatch(op: string, args: Record<string, unknown>): Promise<unknown> {
  if (op === '_status') return mainDiskStatus()
  if (op === '_detach') {
    mainDisk = null
    mainDirty = false
    return { attached: false, name: '', savedAt: 0, savedBytes: 0, dirty: false }
  }
  if (op === '_attach') {
    const mod = await ensureMainDb()
    void mod
    const handle = (args?.handle ?? null) as FileSystemFileHandle | null
    const passphrase = String(args?.passphrase ?? '')
    const bytes = (args?.bytes ?? null) as Uint8Array | null
    const name = String(args?.name ?? 'cybermanju.cybermanju')
    if (bytes && bytes.byteLength > 0) {
      const { decodeContainer } = await import('@/utils/container')
      const { image } = await decodeContainer(bytes, passphrase)
      await mainRestore(image)
    }
    mainDisk = {
      handle,
      name,
      passphrase,
      savedAt: Date.now(),
      savedBytes: bytes?.byteLength ?? 0,
    }
    mainDirty = false
    // Mirror the worker: a picked handle binds (attached), a bare import
    // restores into the session database only (attached=false, EXPORT to keep).
    return {
      attached: !!handle,
      name,
      savedAt: mainDisk.savedAt,
      savedBytes: mainDisk.savedBytes,
      dirty: false,
    }
  }
  if (op === '_save') {
    if (!mainDisk?.handle) {
      // Session-only import (or nothing bound): persist via download path.
      // Keep the error actionable — EXPORT still works through _export.
      if (!mainDisk) throw new Error('no .cybermanju file attached — open or create one first')
      throw new Error('session-only import has no file to save back to — use EXPORT to download it')
    }
    const image = await mainSnapshot()
    const { encodeContainer } = await import('@/utils/container')
    const payload = await encodeContainer(image, mainDisk.passphrase)
    const writable = await (mainDisk.handle as unknown as {
      createWritable: () => Promise<{ write: (d: Uint8Array) => Promise<void>; close: () => Promise<void> }>
    }).createWritable()
    await writable.write(payload)
    await writable.close()
    mainDirty = false
    mainDisk.savedAt = Date.now()
    mainDisk.savedBytes = payload.byteLength
    return {
      attached: true,
      name: mainDisk.name,
      savedAt: mainDisk.savedAt,
      savedBytes: mainDisk.savedBytes,
      dirty: false,
    }
  }
  if (op === '_export') {
    const passphrase = String(args?.passphrase ?? mainDisk?.passphrase ?? '')
    const image = await mainSnapshot()
    const { encodeContainer } = await import('@/utils/container')
    const payload = await encodeContainer(image, passphrase)
    return { bytes: payload, name: mainDisk?.name ?? 'cybermanju.cybermanju' }
  }
  const mod = await ensureMainDb()
  const { db_dispatch } = mod
  // The main-thread module needs the same open handshake as the worker —
  // OPFS resolution fails here by design, so this always lands on memory.
  await mainDbOpen
  const raw = db_dispatch(
    op,
    JSON.stringify({ ...args, now: new Date().toISOString() }),
  )
  const env = JSON.parse(raw) as { ok: boolean; data?: unknown; error?: string }
  if (!env.ok) throw new Error(String(env.error ?? 'unknown db error'))
  if (!mainIsReadOnly(op)) mainDirty = true
  return env.data ?? null
}

const MOBILE_NATIVE_KV_COMMANDS: Record<string, string> = {
  'kv.get': 'vault_kv_get',
  'kv.set': 'vault_kv_set',
  'kv.delete': 'vault_kv_delete',
  'kv.list': 'vault_kv_list',
}

/** One database op through the worker (or the main-thread fallback). */
export async function wasmDbDispatch(
  op: string,
  args: Record<string, unknown> = {},
  timeoutMs = 30000,
): Promise<unknown> {
  // Native Tauri mobile has a separate Rust redb from the web WASM worker.
  // Route generic KV calls into that live database so all vault metadata and
  // volume changes are present in the Scoped Storage snapshot.
  const nativeCommand = MOBILE_NATIVE_KV_COMMANDS[op]
  if (nativeCommand) {
    const { isTauriMobile, invoke } = await import('./useTauri')
    if (isTauriMobile()) return await invoke<unknown>(nativeCommand, args)
  }

  if (!dbWorker && !dbWorkerFailed) {
    try {
      dbWorker = new Worker(new URL('../workers/db-worker.ts', import.meta.url), {
        type: 'module',
      })
      dbWorker.onmessage = (ev: MessageEvent<DbReply>) => {
        const pending = dbPending.get(ev.data?.id)
        if (!pending) return
        dbPending.delete(ev.data.id)
        window.clearTimeout(pending.timer)
        if (ev.data.ok) pending.resolve(ev.data.data ?? null)
        else pending.reject(new Error(String(ev.data.error ?? 'unknown db error')))
      }
      dbWorker.onerror = () => {
        dbRejectAll(new Error('database worker error — recreating on next call'))
        // A faulted worker may hold broken wasm state; drop it so the next
        // call boots a fresh one instead of hanging until timeout.
        dbWorker = null
      }
    } catch (e) {
      dbWorkerFailed = true
      dbWorker = null
      void e
    }
  }

  if (dbWorker) {
    const id = ++dbSeq
    return new Promise<unknown>((resolve, reject) => {
      const timer = window.setTimeout(() => {
        dbPending.delete(id)
        reject(new Error(`database worker timed out on '${op}'`))
      }, timeoutMs)
      dbPending.set(id, { resolve, reject, timer })
      dbWorker?.postMessage({ id, op, args })
    })
  }

  // Main-thread fallback: same module, in-memory database.
  return mainThreadDbDispatch(op, args)
}

// ── `.cybermanju` file (File System Access API) ───────────────────────────
// The picked `FileSystemFileHandle` is structured-cloneable, so it is posted
// to the worker once and every save writes back to *that* file. redb keeps
// running on OPFS (the only synchronous browser primitive); the user's file
// is the durable, encrypted, shareable copy.

export interface WasmDiskStatus {
  attached: boolean
  name: string
  savedAt: number
  savedBytes: number
  dirty: boolean
  lastError?: string | null
  backend?: string
  file?: string
}

export interface AttachDiskRequest {
  /** `null` = import without a bound file (session-only, save via EXPORT). */
  handle: FileSystemFileHandle | null
  name: string
  /** Raw container bytes (already read by the caller); null for a new file. */
  bytes: Uint8Array | null
  passphrase: string
}

export async function wasmDiskStatus(): Promise<WasmDiskStatus> {
  const raw = (await wasmDbDispatch('_status', {}, 60000)) as (Partial<WasmDiskStatus> & {
    disk?: Partial<WasmDiskStatus> | null
  }) | null
  // The worker nests the file state under `disk` (`{backend, file, disk}`),
  // the main-thread fallback returns it flat — accept both.
  const d = raw?.disk ?? raw
  return {
    attached: !!d?.attached,
    name: String(d?.name ?? raw?.name ?? ''),
    savedAt: Number(d?.savedAt ?? 0),
    savedBytes: Number(d?.savedBytes ?? 0),
    dirty: !!(d?.dirty ?? (raw as Partial<WasmDiskStatus>)?.dirty),
    lastError: d?.lastError ?? raw?.lastError ?? null,
    backend: raw?.backend,
    file: raw?.file,
  }
}

export async function wasmAttachDisk(req: AttachDiskRequest): Promise<WasmDiskStatus> {
  const raw = (await wasmDbDispatch(
    '_attach',
    { handle: req.handle, name: req.name, bytes: req.bytes, passphrase: req.passphrase },
    60000,
  )) as Partial<WasmDiskStatus> | null
  return {
    attached: !!raw?.attached,
    name: String(raw?.name ?? req.name),
    savedAt: Number(raw?.savedAt ?? 0),
    savedBytes: Number(raw?.savedBytes ?? 0),
    dirty: !!raw?.dirty,
    lastError: raw?.lastError ?? null,
  }
}

export async function wasmSaveDisk(): Promise<WasmDiskStatus> {
  const raw = (await wasmDbDispatch('_save', {}, 120000)) as Partial<WasmDiskStatus> | null
  return {
    attached: !!raw?.attached,
    name: String(raw?.name ?? ''),
    savedAt: Number(raw?.savedAt ?? 0),
    savedBytes: Number(raw?.savedBytes ?? 0),
    dirty: !!raw?.dirty,
    lastError: raw?.lastError ?? null,
  }
}

export async function wasmExportDisk(
  passphrase?: string,
): Promise<{ bytes: Uint8Array; name: string }> {
  const raw = (await wasmDbDispatch('_export', { passphrase: passphrase ?? '' }, 120000)) as {
    bytes: Uint8Array
    name?: string
  } | null
  return {
    bytes: raw?.bytes ?? new Uint8Array(),
    name: String(raw?.name ?? 'cybermanju.cybermanju'),
  }
}

export async function wasmDetachDisk(): Promise<WasmDiskStatus> {
  const raw = (await wasmDbDispatch('_detach', {})) as Partial<WasmDiskStatus> | null
  return {
    attached: !!raw?.attached,
    name: String(raw?.name ?? ''),
    savedAt: Number(raw?.savedAt ?? 0),
    savedBytes: Number(raw?.savedBytes ?? 0),
    dirty: !!raw?.dirty,
  }
}
