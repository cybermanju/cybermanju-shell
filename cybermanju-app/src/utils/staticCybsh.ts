// CyberManju OS — static-host cybsh answers (WASM transport).
//
// The Rust wasm dispatcher (`crates/os-wasm/src/os.rs`) owns the volume
// commands (ls/cat/write/…) but cannot see the vault, the sync configs, or
// the provider network — it runs on the main thread while the database lives
// in the worker. So before an `os_exec` line reaches `wasmOsDispatch`, the
// static transport offers it here: every verb below is answered locally from
// the local-pc vault (OAuth/PAT signs in `sync.secret`, keys under
// `secret:cybsh:key:*`, the `.cybermanju` file behind `wasmDiskStatus`) plus
// live CORS-OK provider probes. Anything that truly needs the server (sync
// push, the OAuth dance, provider scrub) warns that the dashboard is
// required instead of dying with ERR_CONNECTION_REFUSED.
//
// File verbs (`cp`, `mv`, `rm`, `mkdir`) work across the merged namespace:
// plain paths hit the local shell volume, `/providers/<mountId>/…` paths hit
// that mount through the provider canal — same-provider renames, cross-
// provider copies, and provider↔local moves all go through one byte path,
// with `-r` gating every recursive directory walk.
//
// Dependency injection: every browser effect arrives through
// `StaticCybshDeps`, so vitest drives each verb with fakes (node env, no
// window). `useTauri.ts` supplies the real deps; nothing here imports a
// composable, so there are no module cycles.

import type { ScheduleRow, ScheduleRun, ShellResult } from '@/types'
import {
  checkCybshScript,
  cybshFetchKey,
  cybshFingerprint,
  formatCybshScript,
  lintCybshScript,
  parseFrontmatter,
  CYBSH_MAX_RUN_DEPTH,
  CYBSH_SCRIPT_EXT,
  runCybshScript,
  type CybshFetchReq,
} from './cybshScript'
import { googleDriveAuthErrorSync } from './gitProvision'
import { nextFire, parseSchedule } from './schedule'
import { listSkills, parseSkillMd, skillPanels, validateSkill } from './skills'
import { resolutionPayload, syncPlanSteps, type ResolutionChoice } from './oplog'
import { lanSyncSteps, peerBaseUrl } from './lanSync'
import { VOLUME_WRITE_LIMIT } from './volumeCaps'
import { isRelayAddr, parseWanEndpoint, relayDialSteps, wanDialPlan, wanDialSteps, wanWayOut } from './wanDial'
import {
  addTodo,
  clearDone,
  parseTodosStore,
  removeTodo,
  serializeTodosStore,
  toggleTodo,
} from './tasks'

/** The 1 MiB single-write cap — single source of truth lives in volumeCaps
 * (mirrors `MAX_WRITE_BYTES` in crates/os-wasm/src/os.rs). Kept here as a
 * re-export so existing imports keep working. */
export const STATIC_WRITE_LIMIT = VOLUME_WRITE_LIMIT

/** Report footer: provider push always needs the server behind the page. */
export const DASHBOARD_NOTE =
  'note: provider push / sync start needs the CyberManju dashboard (:3456, desktop app or Docker image) — above is the local vault + live provider probes'

export interface StaticSyncConfig {
  id: string
  backendType: string
  enabled: boolean
  name?: string
  repoName?: string
  branch?: string
  basePath?: string
  folderId?: string
}

/** One scheduler row / fire record — shared shapes live in `@/types`
 * (`ScheduleRow`/`ScheduleRun`); the aliases keep this module's deps
 * signatures readable without re-declaring the fields. */
export type StaticSchedule = ScheduleRow
export type StaticScheduleRun = ScheduleRun

export interface StaticDiskStatus {
  attached: boolean
  name: string
  savedBytes: number
  dirty: boolean
}

export interface StaticMount {
  id: string
  name: string
  backendType: string
  configId: string
}

/** One row of a provider directory listing (mirrors `VfsEntry`). */
export interface StaticVfsEntry {
  name: string
  path: string
  isDir: boolean
  sizeBytes: number
}

export interface ProviderQuotaProbe {
  configId: string
  backendType: string
  ok: boolean
  totalBytes?: number | null
  usedBytes?: number | null
  remainingRequests?: number | null
  requestLimit?: number | null
  resetAt?: number | null
  detail: string
  error?: string
}

export interface StaticChacha {
  genKey(): Uint8Array
  genNonce(): Uint8Array
  encrypt(key: Uint8Array, nonce: Uint8Array, plain: Uint8Array): Uint8Array
  decrypt(key: Uint8Array, nonce: Uint8Array, cipher: Uint8Array): Uint8Array
}

export interface StaticCodecs {
  compressLz4(data: Uint8Array): Uint8Array
  decompressLz4(data: Uint8Array): Uint8Array
  compressBrotli(data: Uint8Array): Uint8Array
  decompressBrotli(data: Uint8Array): Uint8Array
  /**
   * zstd via `@dweb-browser/zstd-wasm` (same standard frames as the desktop
   * `zstd` crate). Optional so older bundles / test fakes without it keep
   * answering lz4+brotli — handlers refuse `zstd` honestly when absent.
   * Sync or async return both work (handlers `await` either way).
   */
  compressZstd?(data: Uint8Array, level?: number): Promise<Uint8Array> | Uint8Array
  decompressZstd?(data: Uint8Array): Promise<Uint8Array> | Uint8Array
  /** Triple chain LZ4 → ZSTD → Brotli (the desktop `.cyb3` order). */
  compressTriple?(data: Uint8Array): Promise<Uint8Array> | Uint8Array
  decompressTriple?(data: Uint8Array): Promise<Uint8Array> | Uint8Array
}

export type CompressLayer = 'lz4' | 'zstd' | 'brotli' | 'triple'

/** Envelope algorithm names the shell reads (legacy `zstd` files included). */
export const COMPRESS_ALGORITHMS: CompressLayer[] = ['lz4', 'zstd', 'brotli', 'triple']

export function compressExtension(layer: CompressLayer): string {
  switch (layer) {
    case 'lz4':
      return '.lz4'
    case 'zstd':
      return '.zst'
    case 'brotli':
      return '.br'
    case 'triple':
      return '.cyb3'
  }
}

export interface StaticCybshDeps {
  readVolume(): Record<string, string>
  getCwd(): Promise<string>
  writeVolumeFile(path: string, content: string): Promise<void>
  deleteVolumePath(path: string, recursive: boolean): Promise<number>
  killTask(id: number): Promise<boolean>
  listSyncConfigs(): Promise<StaticSyncConfig[]>
  getConfigSecret(configId: string): Promise<string>
  getDiskStatus(): Promise<StaticDiskStatus>
  getStorageEstimate(): Promise<{ usage?: number; quota?: number } | null>
  listMounts(): Promise<StaticMount[]>
  providerRead(mountId: string, remotePath: string): Promise<Uint8Array>
  providerWrite(mountId: string, remotePath: string, data: Uint8Array): Promise<void>
  providerDelete(mountId: string, remotePath: string): Promise<void>
  providerList(mountId: string, remotePath: string): Promise<StaticVfsEntry[]>
  probeProviderQuota(cfg: StaticSyncConfig, token: string): Promise<ProviderQuotaProbe>
  keyGet(name: string): Promise<string | null>
  keySet(name: string, value: string): Promise<void>
  chacha(): Promise<StaticChacha | null>
  codecs(): Promise<StaticCodecs | null>
  blake3(data: string): Promise<string | null>
  /** Full-transport fallback for inline `sh` lines (wasm dispatcher on Pages). */
  execFallback?: (line: string) => Promise<string>
  // Scheduler (cron) — the browser has no daemon thread, so the store ticks
  // due rows and the `cron` verb runs them here. Optional: a fixture (or a
  // stale deps object) without them answers `unsupported:` honestly.
  listSchedules?(): Promise<StaticSchedule[]>
  saveSchedule?(row: StaticSchedule): Promise<StaticSchedule>
  deleteSchedule?(id: string): Promise<boolean>
  scheduleHistory?(id: string): Promise<StaticScheduleRun[]>
  recordScheduleRun?(run: StaticScheduleRun): Promise<unknown>
  // Vault backup (`backup` verb) — sealed snapshot bytes + the vault
  // passphrase, both optional so fixtures stay honest when absent.
  exportDatabase?(passphrase: string): Promise<Uint8Array>
  backupPassphrase?(): Promise<string>
  blake3Bytes?(data: Uint8Array): Promise<string | null>
}

export interface ParsedLine {
  verb: string
  args: string[]
  json: boolean
}

/** Verbs answered here. Everything else (and every chained line) falls
 *  through to the wasm dispatcher. `ls`/`cat` only intercept provider
 *  operands (`/providers/…`) — local paths fall through to the wasm volume
 *  so its listing format stays the single source of truth. */
const HANDLED_VERBS = new Set([
  'echo', 'ls', 'cat', 'cp', 'mv', 'rm', 'mkdir', 'kill',
  'grep', 'find', 'head', 'tail', 'wc', 'edit',
  'quota', 'providers', 'oauth', 'disk', 'sync',
  'encrypt', 'decrypt', 'keygen', 'compress', 'decompress',
  'scrub', 'repair', 'gc', 'lease', 'mount', 'umount', 'ai',
  'run', 'theme', 'ui', 'cron', 'backup', 'skill', 'task', 'oplog', 'lan', 'wan',
  'pex', 'relay', 'pair', 'swarm', 'room',
])

export function handlesStaticVerb(verb: string): boolean {
  return HANDLED_VERBS.has(verb.toLowerCase())
}

/**
 * Quote-aware first-word parse. Returns `null` for chain operators outside
 * quotes; the line runner dispatches `&&`/`;` clauses across static + WASM
 * backends and refuses `|` honestly.
 */
export function parseCybshLine(line: string): ParsedLine | null {
  const tokens: string[] = []
  let current = ''
  let quote: string | null = null
  let hasToken = false
  const push = () => {
    if (hasToken) {
      tokens.push(current)
      current = ''
      hasToken = false
    }
  }
  for (let i = 0; i < line.length; i++) {
    const c = line[i]
    if (quote) {
      if (c === quote) quote = null
      else {
        current += c
        hasToken = true
      }
      continue
    }
    if (c === "'" || c === '"') {
      quote = c
      hasToken = true
      continue
    }
    if (c === '|' || c === ';') return null
    if (c === '&' && line[i + 1] === '&') return null
    if (/\s/.test(c)) {
      push()
      continue
    }
    current += c
    hasToken = true
  }
  push()
  if (tokens.length === 0) return null
  const args = tokens.slice(1).filter((a) => a !== '--json')
  const json = tokens.slice(1).some((a) => a === '--json')
  return { verb: tokens[0].toLowerCase(), args, json }
}

interface StaticCybshChain {
  commands: string[]
  separators: string[]
  hasPipe: boolean
}

/** Split only unquoted shell operators so static verbs can share a chain with WASM commands. */
function splitStaticCybshChain(line: string): StaticCybshChain {
  const commands: string[] = []
  const separators: string[] = []
  let quote: string | null = null
  let start = 0
  let hasPipe = false
  for (let i = 0; i < line.length; i++) {
    const c = line[i]
    if (quote) {
      if (c === quote) quote = null
      continue
    }
    if (c === "'" || c === '"') {
      quote = c
      continue
    }
    if (c === '|') {
      hasPipe = true
      continue
    }
    if (c === ';') {
      commands.push(line.slice(start, i))
      separators.push(';')
      start = i + 1
      continue
    }
    if (c === '&' && line[i + 1] === '&') {
      commands.push(line.slice(start, i))
      separators.push('&&')
      i++
      start = i + 1
    }
  }
  commands.push(line.slice(start))
  return { commands, separators, hasPipe }
}

/** Mirror of `join()` in crates/os-wasm/src/os.rs (cwd + relative → absolute). */
export function joinVolumePath(cwd: string, path: string): string {
  const parts: string[] = []
  if (!path.startsWith('/')) {
    for (const part of cwd.replace(/^\/+/, '').split('/')) {
      if (part !== '' && part !== '.') parts.push(part)
    }
  }
  for (const part of path.split('/')) {
    if (part === '' || part === '.') continue
    if (part === '..') parts.pop()
    else parts.push(part)
  }
  return parts.length === 0 ? '/' : `/${parts.join('/')}`
}

/**
 * Split an absolute shell path into a provider mount reference.
 * `/providers/<mountId>` is the mount root, `/providers/<mountId>/a/b` a
 * path inside it. Anything else returns `null` (a local volume path). The
 * shape mirrors `parseProviderPath` in useProviderCanal.
 */
export function splitProviderPath(absPath: string): { mountId: string; remotePath: string } | null {
  const norm = absPath.replace(/\\/g, '/')
  const segs = norm.split('/').filter((s) => s !== '')
  if (segs.length < 2 || segs[0] !== 'providers') return null
  return { mountId: segs[1], remotePath: segs.slice(2).join('/').replace(/^\/+|\/+$/g, '') }
}

function baseName(path: string): string {
  const clean = path.replace(/\/+$/g, '')
  const i = clean.lastIndexOf('/')
  return i < 0 ? clean : clean.slice(i + 1)
}

function utf8DecodeStrict(bytes: Uint8Array): string | null {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
  } catch {
    return null
  }
}

/** Human byte size in the native shell's `1.5G` style (`human()` in shell.rs). */
export function humanBytes(n: number | null | undefined): string {
  if (n === null || n === undefined || Number.isNaN(n)) return '?'
  const units = ['B', 'k', 'M', 'G', 'T']
  let value = n
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  if (unit === 0) return `${Math.round(value)}B`
  return `${value.toFixed(1)}${units[unit]}`
}

// Node-only fallback for non-DOM runtimes (browsers and modern Node take
// the btoa/atob path above): declared locally so the web build needs no
// @types/node dependency.
declare const Buffer: {
  from(input: Uint8Array | string, encoding?: string): Uint8Array & { toString(encoding?: string): string }
}

function b64Encode(bytes: Uint8Array): string {
  if (typeof btoa === 'function') {
    let bin = ''
    const CHUNK = 0x8000
    for (let i = 0; i < bytes.length; i += CHUNK) {
      bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
    }
    return btoa(bin)
  }
  return Buffer.from(bytes).toString('base64')
}

function b64Decode(b64: string): Uint8Array {
  if (typeof atob === 'function') {
    const bin = atob(b64)
    const out = new Uint8Array(bin.length)
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
    return out
  }
  return new Uint8Array(Buffer.from(b64, 'base64'))
}

function utf8Encode(text: string): Uint8Array {
  return new TextEncoder().encode(text)
}

function utf8Decode(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes)
}

/** Canonical backend name — tolerant of whatever casing a config carries. */
export function normalizeBackend(raw: string): string {
  const b = (raw ?? '').trim().toLowerCase()
  if (b === 'googledrive' || b === 'google_drive' || b === 'drive' || b === 'google') return 'googleDrive'
  if (b === 'github') return 'github'
  if (b === 'gitlab') return 'gitlab'
  if (b === 'local' || b === 'localdir' || b === 'local_dir') return 'local'
  return b || 'unknown'
}

/**
 * Normalize one `sync.list` row (wasm db or REST, camelCase or snake_case)
 * into the shape the static handlers probe. Returns `null` for id-less rows.
 */
export function staticConfigFromRow(row: Record<string, unknown>): StaticSyncConfig | null {
  const id = String(row.id ?? '')
  if (!id) return null
  const str = (v: unknown): string | undefined => (typeof v === 'string' ? v : undefined)
  return {
    id,
    backendType: str(row.backendType) ?? str(row.backend_type) ?? str(row.backend) ?? 'unknown',
    enabled: row.enabled !== false,
    name: str(row.name),
    repoName: str(row.repoName) ?? str(row.repo_name),
    branch: str(row.branch),
    basePath: str(row.basePath) ?? str(row.base_path) ?? str(row.instanceUrl) ?? str(row.instance_url),
    folderId: str(row.folderId) ?? str(row.folder_id),
  }
}

export function isOauthBackend(backend: string): boolean {
  const b = normalizeBackend(backend)
  return b === 'github' || b === 'gitlab' || b === 'googleDrive'
}

export function oauthSlug(backend: string): string | null {
  const b = normalizeBackend(backend)
  if (b === 'googleDrive') return 'google'
  if (b === 'github' || b === 'gitlab') return b
  return null
}

type FetchFn = (url: string, init?: Record<string, unknown>) => Promise<{
  ok: boolean
  status: number
  json(): Promise<unknown>
}>

/** A body that is not JSON is a transport failure, never a probe throw —
 *  `probeProviderQuotaViaFetch` keeps its never-throws contract either way. */
async function safeJson(res: {
  json(): Promise<unknown>
}): Promise<{ ok: true; value: unknown } | { ok: false }> {
  try {
    return { ok: true, value: await res.json() }
  } catch {
    return { ok: false }
  }
}

/**
 * Live provider quota over browser fetch — the same endpoints
 * `crates/sync/src/quota.rs::usage()` probes (Drive quotaInfo, GitHub
 * rate-limit, GitLab project statistics), so a number here means the same
 * thing as on the desktop. `fetchFn` is injected (browser `fetch` in prod,
 * a fake in tests).
 */
export async function probeProviderQuotaViaFetch(
  cfg: StaticSyncConfig,
  token: string,
  fetchFn: FetchFn,
  timeoutMs = 10000,
): Promise<ProviderQuotaProbe> {
  const backend = normalizeBackend(cfg.backendType)
  const base = { configId: cfg.id, backendType: cfg.backendType }
  if (backend === 'local') {
    return {
      ...base,
      ok: true,
      detail: 'local filesystem quota is not tracked (best-effort: unknown)',
    }
  }
  if (!token) {
    return {
      ...base,
      ok: false,
      detail: '',
      error:
        `auth: ${cfg.id} has no token — paste a PAT on its provider card or connect OAuth via the dashboard, then retry`,
    }
  }
  const ctrl =
    typeof AbortController !== 'undefined' ? new AbortController() : null
  const timer =
    ctrl !== null ? setTimeout(() => ctrl.abort(), timeoutMs) : 0
  const blocked = (host: string): ProviderQuotaProbe => ({
    ...base,
    ok: false,
    detail: '',
    error:
      `network: ${host} is not reachable from this browser (offline, or the provider sends no CORS headers) — the token stays saved in the local vault; live quota needs the desktop app, Docker image or dashboard server`,
  })
  try {
    if (backend === 'github') {
      let res
      try {
        res = await fetchFn('https://api.github.com/rate_limit', {
          headers: {
            Authorization: `token ${token}`,
            Accept: 'application/vnd.github+json',
          },
          signal: ctrl?.signal,
        })
      } catch {
        return blocked('api.github.com')
      }
      if (!res.ok) {
        return {
          ...base,
          ok: false,
          detail: '',
          error:
            res.status === 401 || res.status === 403
              ? `auth: GitHub rejected the token for ${cfg.id} (HTTP ${res.status}) — reseal it on the provider card`
              : `network: GitHub quota probe failed for ${cfg.id} (HTTP ${res.status})`,
        }
      }
      const body = await safeJson(res)
      if (!body.ok) {
        return {
          ...base,
          ok: false,
          detail: '',
          error: `network: GitHub quota response unreadable for ${cfg.id}`,
        }
      }
      const json = body.value as { resources?: { core?: { limit?: number; remaining?: number; reset?: number } } }
      const core = json?.resources?.core ?? {}
      return {
        ...base,
        ok: true,
        requestLimit: typeof core.limit === 'number' ? core.limit : null,
        remainingRequests: typeof core.remaining === 'number' ? core.remaining : null,
        resetAt: typeof core.reset === 'number' ? core.reset : null,
        detail: 'GitHub REST rate-limit window (storage quota not published)',
      }
    }
    if (backend === 'gitlab') {
      const repo = (cfg.repoName ?? '').trim()
      if (!repo) {
        return {
          ...base,
          ok: false,
          detail: '',
          error: 'not_found: GitLab backend requires project_id (use repo_name field)',
        }
      }
      const rawBase = (cfg.basePath ?? '').trim()
      const host =
        /^https?:\/\//.test(rawBase) && rawBase
          ? rawBase.replace(/\/+$/, '')
          : 'https://gitlab.com'
      const hostName = host.replace(/^https?:\/\//, '').split('/')[0]
      let res
      try {
        res = await fetchFn(
          `${host}/api/v4/projects/${encodeURIComponent(repo)}?statistics=true`,
          { headers: { 'PRIVATE-TOKEN': token }, signal: ctrl?.signal },
        )
      } catch {
        return blocked(hostName)
      }
      if (res.status === 401) {
        return {
          ...base,
          ok: false,
          detail: '',
          error: `auth: GitLab rejected the token for ${cfg.id} (HTTP 401) — reseal it on the provider card`,
        }
      }
      if (res.status !== 200) {
        return {
          ...base,
          ok: true,
          detail:
            `project statistics need maintainer access (HTTP ${res.status}) — best-effort: unknown`,
        }
      }
      const body = await safeJson(res)
      if (!body.ok) {
        return {
          ...base,
          ok: false,
          detail: '',
          error: `network: GitLab quota response unreadable for ${cfg.id}`,
        }
      }
      const json = body.value as { statistics?: { storage_size?: number; repository_size?: number } }
      const stats = json?.statistics ?? {}
      return {
        ...base,
        ok: true,
        usedBytes:
          typeof stats.storage_size === 'number'
            ? stats.storage_size
            : typeof stats.repository_size === 'number'
              ? stats.repository_size
              : null,
        detail: 'project statistics.storage_size (project storage has no provider quota)',
      }
    }
    if (backend === 'googleDrive') {
      let res
      try {
        res = await fetchFn(
          'https://www.googleapis.com/drive/v3/about?fields=user,quotaInfo',
          { headers: { Authorization: `Bearer ${token}` }, signal: ctrl?.signal },
        )
      } catch {
        return blocked('www.googleapis.com')
      }
      if (!res.ok) {
        if (res.status === 401 || res.status === 403) {
          const body = await safeJson(res)
          const errJson = body.ok && typeof body.value === 'object' && body.value !== null
            ? (body.value as Record<string, unknown>)
            : undefined
          return {
            ...base,
            ok: false,
            detail: '',
            error: `${googleDriveAuthErrorSync(res.status, errJson)} (config ${cfg.id})`,
          }
        }
        return {
          ...base,
          ok: false,
          detail: '',
          error: `network: Google Drive quota probe failed for ${cfg.id} (HTTP ${res.status})`,
        }
      }
      const body = await safeJson(res)
      if (!body.ok) {
        return {
          ...base,
          ok: false,
          detail: '',
          error: `network: Google Drive quota response unreadable for ${cfg.id}`,
        }
      }
      const json = body.value as {
        quotaInfo?: { usage?: number | string; usageInDriveTrash?: number | string; limit?: number | string }
      }
      const info = json?.quotaInfo ?? {}
      const num = (v: unknown): number | null => {
        if (typeof v === 'number') return v
        if (typeof v === 'string' && v !== '') {
          const n = parseInt(v, 10)
          return Number.isNaN(n) ? null : n
        }
        return null
      }
      const used = num(info.usage)
      const trash = num(info.usageInDriveTrash) ?? 0
      const total = num(info.limit)
      const out: ProviderQuotaProbe = {
        ...base,
        ok: true,
        usedBytes: used === null ? null : used + trash,
        totalBytes: total,
        detail: 'Drive v3 about/quotaInfo',
      }
      if (out.usedBytes === null && out.totalBytes === null) {
        out.detail = 'Drive returned no quotaInfo for this account'
      }
      return out
    }
    return {
      ...base,
      ok: false,
      detail: '',
      error: `unsupported: quota is not supported for '${cfg.backendType}' (github + gitlab + googleDrive + local only)`,
    }
  } finally {
    if (timer) clearTimeout(timer)
  }
}

interface VerbOut {
  ok: boolean
  text: string
}

function shellOk(text: string): VerbOut {
  return { ok: true, text }
}

function shellErr(text: string): VerbOut {
  return { ok: false, text }
}

function fmtProbeLine(probe: ProviderQuotaProbe, cfgId: string, backend: string): string {
  if (!probe.ok) return probe.error ?? `network: quota probe failed for ${cfgId}`
  const used = humanBytes(probe.usedBytes)
  const total = humanBytes(probe.totalBytes)
  let extra = ''
  if (probe.remainingRequests !== null && probe.remainingRequests !== undefined) {
    extra = ` · ${probe.remainingRequests}/${probe.requestLimit ?? '?'} requests left`
  }
  return `${cfgId} (${backend}): used ${used} of ${total}${extra} · source: ${probe.detail}`
}

async function handleQuota(_args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const volume = deps.readVolume()
  const names = Object.keys(volume)
  const localBytes = names.reduce((n, k) => n + (volume[k]?.length ?? 0), 0)
  const [disk, est, configs] = await Promise.all([
    deps.getDiskStatus().catch((): StaticDiskStatus => ({ attached: false, name: '', savedBytes: 0, dirty: false })),
    deps.getStorageEstimate().catch(() => null),
    deps.listSyncConfigs().catch((): StaticSyncConfig[] => []),
  ])
  const lines: string[] = []
  lines.push(`local vault: ${names.length} files · ${humanBytes(localBytes)} in the shell volume`)
  if (est && (est.usage !== undefined || est.quota !== undefined)) {
    const pct =
      est.quota && est.quota > 0 && est.usage !== undefined
        ? ` (${Math.round((est.usage / est.quota) * 100)}%)`
        : ''
    lines.push(
      `browser storage: ${humanBytes(est.usage)} of ${humanBytes(est.quota)}${pct} · persisted by the OS when granted`,
    )
  }
  lines.push(
    disk.attached
      ? `.cybermanju file: ${disk.name || 'attached'} · ${humanBytes(disk.savedBytes)} saved${disk.dirty ? ' · UNSAVED CHANGES' : ''}`
      : `.cybermanju file: none attached — open or create one to take the vault home (local-pc file)`,
  )
  const enabled = configs.filter((c) => c.enabled)
  const probes: ProviderQuotaProbe[] = []
  let failed = false
  if (enabled.length === 0) {
    lines.push('not_found: no enabled provider to query — add one on its provider card')
    failed = true
  }
  for (const cfg of enabled) {
    const token = await deps.getConfigSecret(cfg.id).catch(() => '')
    const probe = await deps
      .probeProviderQuota(cfg, token)
      .catch(
        (e): ProviderQuotaProbe => ({
          configId: cfg.id,
          backendType: cfg.backendType,
          ok: false,
          detail: '',
          error: e instanceof Error ? e.message : String(e),
        }),
      )
    probes.push(probe)
    lines.push(fmtProbeLine(probe, cfg.id, cfg.backendType))
    if (!probe.ok) failed = true
  }
  lines.push(DASHBOARD_NOTE)
  if (json) {
    return (failed ? shellErr : shellOk)(
      JSON.stringify({
        local: { files: names.length, bytes: localBytes },
        browser: est,
        disk,
        providers: probes,
        dashboardRequired: true,
      }),
    )
  }
  return (failed ? shellErr : shellOk)(lines.join('\n'))
}

async function handleProviders(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  void args
  const [configs, mounts, disk] = await Promise.all([
    deps.listSyncConfigs().catch((): StaticSyncConfig[] => []),
    deps.listMounts().catch((): StaticMount[] => []),
    deps.getDiskStatus().catch((): StaticDiskStatus => ({ attached: false, name: '', savedBytes: 0, dirty: false })),
  ])
  const rows: Array<Record<string, unknown>> = []
  for (const c of configs) {
    const secret = await deps.getConfigSecret(c.id).catch(() => '')
    rows.push({
      id: c.id,
      backend: c.backendType,
      enabled: c.enabled,
      hasKey: secret !== '',
      repo: c.repoName ?? null,
      disk: disk.attached ? disk.name : null,
    })
  }
  if (json) return shellOk(JSON.stringify({ providers: rows, mounts, dashboardRequired: true }))
  if (rows.length === 0 && mounts.length === 0) {
    return shellOk(
      `no providers connected — add one in Settings (provider card), then \`oauth status\` to check its sign-in\n${DASHBOARD_NOTE}`,
    )
  }
  const out = ['ID                   BACKEND       ENABLED   SIGNED-IN   REPO']
  for (const r of rows) {
    out.push(
      `${String(r.id).padEnd(20)} ${String(r.backend).padEnd(13)} ${(r.enabled ? 'yes' : 'no').padEnd(9)} ${(r.hasKey ? 'yes' : 'no').padEnd(11)} ${String(r.repo ?? '—')}`,
    )
  }
  for (const m of mounts) {
    out.push(`  mount ${m.id} → ${m.backendType} (${m.name})`)
  }
  out.push(DASHBOARD_NOTE)
  return shellOk(out.join('\n'))
}

async function handleOauth(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'status').toLowerCase()
  const configs = await deps.listSyncConfigs().catch((): StaticSyncConfig[] => [])
  if (sub === 'status' || sub === 'list') {
    const rows = []
    for (const c of configs) {
      const slug = oauthSlug(c.backendType)
      const secret = await deps.getConfigSecret(c.id).catch(() => '')
      rows.push({ id: c.id, backend: c.backendType, oauth: slug, signedIn: secret !== '' })
    }
    if (json) return shellOk(JSON.stringify({ oauth: rows, dashboardRequired: true }))
    if (rows.length === 0) return shellOk('no provider configs yet — add one on its provider card first')
    return shellOk(
      rows
        .map((r) =>
          r.oauth
            ? `${r.id} (${r.backend}): ${r.signedIn ? 'signed in (token sealed in the local vault)' : 'NOT signed in — `oauth start ' + r.backend + ' ' + r.id + '`'}`
            : `${r.id} (${r.backend}): local backend, no OAuth flow — nothing to sign`,
        )
        .join('\n'),
    )
  }
  if (sub === 'start') {
    const backend = normalizeBackend(args[1] ?? '')
    const configId = args[2] ?? ''
    const slug = oauthSlug(backend)
    if (!slug) {
      return shellErr(
        `unsupported: no OAuth flow for '${args[1] ?? ''}' (oauth-capable: github, gitlab, googleDrive)`,
      )
    }
    const target =
      configs.find((c) => c.id === configId) ??
      configs.find((c) => normalizeBackend(c.backendType) === backend && c.enabled) ??
      configs.find((c) => normalizeBackend(c.backendType) === backend)
    if (target) {
      const secret = await deps.getConfigSecret(target.id).catch(() => '')
      if (secret) {
        const msg = `${target.id} is already signed in (token sealed in the local vault) — \`quota\` probes it live`
        return json
          ? shellOk(JSON.stringify({ configId: target.id, signedIn: true }))
          : shellOk(msg)
      }
    }
    const msg = [
      `static build: the ${slug} OAuth dance needs the dashboard — this page cannot receive the provider callback`,
      'sign in instead with one of:',
      '  1. paste a personal token on the provider card (Settings → provider) — it seals into the local vault',
      target
        ? `  2. set a dashboard URL in Settings, then \`oauth start\` runs the full flow for ${target.id}`
        : '  2. set a dashboard URL in Settings for the full OAuth redirect flow',
    ].join('\n')
    return json
      ? shellErr(JSON.stringify({ signedIn: false, dashboardRequired: true, hint: msg }))
      : shellErr(`auth: ${msg}`)
  }
  return shellErr(`usage: oauth [status|start <github|gitlab|googleDrive> [configId]]`)
}

async function handleDisk(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'status').toLowerCase()
  if (sub === 'status' || sub === 'list' || sub === 'df') {
    const [disk, est] = await Promise.all([
      deps.getDiskStatus().catch((): StaticDiskStatus => ({ attached: false, name: '', savedBytes: 0, dirty: false })),
      deps.getStorageEstimate().catch(() => null),
    ])
    const volume = deps.readVolume()
    const bytes = Object.values(volume).reduce((n, v) => n + v.length, 0)
    const payload = {
      file: disk,
      shellVolume: { files: Object.keys(volume).length, bytes },
      browser: est,
      dashboardRequired: true,
    }
    if (json) return shellOk(JSON.stringify(payload))
    const lines = [
      disk.attached
        ? `local-pc vault file: ${disk.name} · ${humanBytes(disk.savedBytes)} saved${disk.dirty ? ' · UNSAVED CHANGES (`save` in the file menu writes it back)' : ''}`
        : 'local-pc vault file: none attached — open or create a .cybermanju file to persist the vault',
      `shell volume: ${payload.shellVolume.files} files · ${humanBytes(bytes)} (mirrored into the vault as volume:*)`,
    ]
    if (est?.quota) lines.push(`browser storage: ${humanBytes(est.usage)} of ${humanBytes(est.quota)}`)
    lines.push(DASHBOARD_NOTE)
    return shellOk(lines.join('\n'))
  }
  return shellErr(
    `unsupported: \`disk ${sub}\` mutates provider disks on the server — open/attach .cybermanju files from the file menu on this build (${DASHBOARD_NOTE})`,
  )
}

async function handleSync(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'status').toLowerCase()
  if (sub === 'status' || sub === 'list') {
    const configs = await deps.listSyncConfigs().catch((): StaticSyncConfig[] => [])
    const payload = {
      active: false,
      jobId: null,
      configs: configs.map((c) => ({ id: c.id, backend: c.backendType, enabled: c.enabled })),
      detail: 'static build keeps no run registry — start/cancel run on the dashboard (202 jobs)',
    }
    if (json) return shellOk(JSON.stringify(payload))
    if (configs.length === 0) return shellOk('no sync configs yet — add a provider first')
    return shellOk(
      [
        ...configs.map(
          (c) => `${c.id} (${c.backendType}): ${c.enabled ? 'enabled' : 'disabled'} · browse offline via its provider mount, push via the dashboard`,
        ),
        DASHBOARD_NOTE,
      ].join('\n'),
    )
  }
  if (sub === 'start' || sub === 'cancel') {
    return shellErr(
      `unsupported: \`sync ${sub}\` runs detached 202 jobs on the server — this static build answers \`sync status\` locally; seed/browse providers offline, push from the desktop app, Docker image or dashboard server`,
    )
  }
  if (sub === 'move') {
    // Static hosts hold no sync-record table — bytes move via `mv`, which
    // already copies across mounts (`/providers/<a>/x` → `/providers/<b>/y`)
    // then deletes the source. Record-aware home moves run on the dashboard.
    if (args.length === 4) {
      return shellOk(
        `move ${args[1]} ${args[2]} → ${args[3]} via bytes: run \`mv /providers/<from-mount>/${args[1]} /providers/<to-mount>/${args[1]}\` here, or \`sync move ${args[1]} ${args[2]} ${args[3]}\` on the dashboard for a verified record-aware move`,
      )
    }
    return shellErr('usage: sync move <fileId> <fromConfig> <toConfig> (bytes: mv /providers/<a>/x /providers/<b>/y)')
  }
  return shellErr(`usage: sync [status|list|start|cancel|move] (only status|list answer locally — the rest need the dashboard)`)
}

async function handleMount(args: string[], _json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  if (args.length === 0) {
    const mounts = await deps.listMounts().catch((): StaticMount[] => [])
    if (mounts.length === 0) {
      return shellOk('no provider mounts — create one from a provider card (Accounts), then browse it under /providers/<id>')
    }
    return shellOk(mounts.map((m) => `${m.id} → ${m.backendType} (${m.name})`).join('\n'))
  }
  return shellErr(
    'unsupported: mounts are created from the provider cards on this build (tokens stay sealed in the local vault) — `mount` with no args lists them',
  )
}

async function handleEncrypt(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  const [rawPath, keyNameArg] = args
  if (!rawPath) return shellErr('usage: encrypt <path> [key-name]')
  const chacha = await deps.chacha().catch(() => null)
  if (!chacha) {
    return shellErr('unsupported: encrypt needs the rebuilt wasm crypto bundle — reconnect after the next Pages deploy')
  }
  const cwd = await deps.getCwd().catch(() => '/')
  const path = joinVolumePath(cwd, rawPath)
  const volume = deps.readVolume()
  const text = volume[path]
  if (text === undefined) {
    const prefix = `${path}/`
    if (Object.keys(volume).some((k) => k.startsWith(prefix))) {
      return shellErr(`is a directory: ${rawPath} (encrypt seals files only)`)
    }
    return shellErr(`not_found: ${rawPath}`)
  }
  const keyName = keyNameArg || 'default'
  let keyB64 = await deps.keyGet(`secret:cybsh:key:${keyName}`).catch(() => null)
  if (!keyB64) {
    keyB64 = b64Encode(chacha.genKey())
    await deps.keySet(`secret:cybsh:key:${keyName}`, keyB64)
  }
  const key = b64Decode(keyB64)
  const nonce = chacha.genNonce()
  let cipher: Uint8Array
  try {
    cipher = chacha.encrypt(key, nonce, utf8Encode(text))
  } catch (e) {
    return shellErr(`integrity: encryption failed (${e instanceof Error ? e.message : String(e)})`)
  }
  const sealedPath = `${path}.sealed`
  const sealed = JSON.stringify({
    alg: 'chacha20-poly1305',
    key: keyName,
    nonce: b64Encode(nonce),
    data: b64Encode(cipher),
  })
  if (sealed.length > STATIC_WRITE_LIMIT) {
    return shellErr(`too_large: sealed output is ${sealed.length} bytes, wasm write limit is ${STATIC_WRITE_LIMIT}`)
  }
  await deps.writeVolumeFile(sealedPath, sealed)
  await deps.keySet(`secret:cybsh:seal:${sealedPath}`, keyName).catch(() => undefined)
  return shellOk(`${path} -> ${sealedPath} (${text.length} bytes sealed, ChaCha20-Poly1305, key '${keyName}' in the local vault)`)
}

async function handleDecrypt(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  const [rawPath, keyNameArg] = args
  if (!rawPath) return shellErr('usage: decrypt <path.sealed> [key-name]')
  const chacha = await deps.chacha().catch(() => null)
  if (!chacha) {
    return shellErr('unsupported: decrypt needs the rebuilt wasm crypto bundle — reconnect after the next Pages deploy')
  }
  const cwd = await deps.getCwd().catch(() => '/')
  const path = joinVolumePath(cwd, rawPath)
  const volume = deps.readVolume()
  const sealed = volume[path]
  if (sealed === undefined) return shellErr(`not_found: ${rawPath}`)
  let env: { alg?: string; key?: string; nonce?: string; data?: string }
  try {
    env = JSON.parse(sealed) as typeof env
  } catch {
    return shellErr(`invalid: ${rawPath} is not a cybsh sealed file (no JSON envelope)`)
  }
  if (env.alg !== 'chacha20-poly1305' || !env.nonce || !env.data) {
    return shellErr(`invalid: ${rawPath} is not a cybsh sealed file (desktop .sealed files open on the desktop)`)
  }
  const keyName =
    keyNameArg ?? env.key ?? (await deps.keyGet(`secret:cybsh:seal:${path}`).catch(() => null)) ?? 'default'
  const keyB64 = await deps.keyGet(`secret:cybsh:key:${keyName}`).catch(() => null)
  if (!keyB64) return shellErr(`auth: no key '${keyName}' in the local vault — unlock or re-encrypt first`)
  let plain: Uint8Array
  try {
    plain = chacha.decrypt(b64Decode(keyB64), b64Decode(env.nonce), b64Decode(env.data))
  } catch {
    return shellErr('integrity: sealed bytes failed authentication — wrong key or tampered file')
  }
  const outPath = path.endsWith('.sealed') ? path.slice(0, -'.sealed'.length) : `${path}.plain`
  const text = utf8Decode(plain)
  if (text.length > STATIC_WRITE_LIMIT) {
    return shellErr(`too_large: plaintext is ${text.length} bytes, wasm write limit is ${STATIC_WRITE_LIMIT}`)
  }
  await deps.writeVolumeFile(outPath, text)
  return shellOk(`${path} -> ${outPath} (${text.length} bytes)`)
}

async function handleKeygen(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  const name = args[0] || `key-${Math.random().toString(36).slice(2, 10)}`
  const chacha = await deps.chacha().catch(() => null)
  if (!chacha) {
    return shellErr('unsupported: keygen needs the rebuilt wasm crypto bundle — reconnect after the next Pages deploy')
  }
  const keyB64 = b64Encode(chacha.genKey())
  await deps.keySet(`secret:cybsh:key:${name}`, keyB64)
  return shellOk(`key '${name}' ready (32 bytes, ChaCha20-Poly1305, sealed in the local vault)`)
}

async function handleCompress(args: string[], deps: StaticCybshDeps, layer: 'lz4' | 'brotli'): Promise<VerbOut> {
  const [rawPath, layerArg] = args
  const want = (layerArg ?? layer).toLowerCase() as CompressLayer
  if (!rawPath) return shellErr(`usage: compress <path> [lz4|zstd|brotli|triple] (decompress: decompress <path.(lz4|zst|br|cyb3)>)`)
  if (want !== 'lz4' && want !== 'zstd' && want !== 'brotli' && want !== 'triple') {
    return shellErr(`unsupported: compress layer '${layerArg}' (lz4|zstd|brotli|triple)`)
  }
  const codecs = await deps.codecs().catch(() => null)
  if (!codecs) {
    return shellErr('unsupported: compress needs the rebuilt wasm bundle — reconnect after the next Pages deploy')
  }
  if (want === 'zstd' && !codecs.compressZstd) {
    return shellErr('unsupported: zstd needs the rebuilt wasm bundle with @dweb-browser/zstd-wasm — reconnect after the next Pages deploy')
  }
  if (want === 'triple' && !codecs.compressTriple) {
    return shellErr('unsupported: triple needs the rebuilt wasm bundle with @dweb-browser/zstd-wasm — reconnect after the next Pages deploy')
  }
  const cwd = await deps.getCwd().catch(() => '/')
  const path = joinVolumePath(cwd, rawPath)
  const volume = deps.readVolume()
  const text = volume[path]
  if (text === undefined) return shellErr(`not_found: ${rawPath}`)
  const input = utf8Encode(text)
  let out: Uint8Array
  try {
    if (want === 'lz4') out = await codecs.compressLz4(input)
    else if (want === 'brotli') out = await codecs.compressBrotli(input)
    else if (want === 'zstd') out = await codecs.compressZstd!(input)
    else out = await codecs.compressTriple!(input)
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    // A missing zstd module stays `unsupported:` (stale bundle) — only a
    // leg that actually ran and failed is an integrity problem.
    if (/^unsupported:/.test(detail)) return shellErr(detail)
    return shellErr(`integrity: compression failed (${detail})`)
  }
  const ext = compressExtension(want)
  const stored = JSON.stringify({ alg: want, data: b64Encode(out) })
  if (stored.length > STATIC_WRITE_LIMIT) {
    return shellErr(`too_large: compressed output is ${stored.length} bytes, wasm write limit is ${STATIC_WRITE_LIMIT}`)
  }
  await deps.writeVolumeFile(`${path}${ext}`, stored)
  const ratio = input.length === 0 ? '—' : `${Math.round((out.length / input.length) * 100)}%`
  return shellOk(`${path} -> ${path}${ext} (${input.length}B → ${out.length}B, ${ratio}, ${want} in the local vault volume)`)
}

async function handleDecompress(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  const [rawPath] = args
  if (!rawPath) return shellErr('usage: decompress <path.(lz4|zst|br|cyb3)>')
  const codecs = await deps.codecs().catch(() => null)
  if (!codecs) {
    return shellErr('unsupported: decompress needs the rebuilt wasm bundle — reconnect after the next Pages deploy')
  }
  const cwd = await deps.getCwd().catch(() => '/')
  const path = joinVolumePath(cwd, rawPath)
  const volume = deps.readVolume()
  const stored = volume[path]
  if (stored === undefined) return shellErr(`not_found: ${rawPath}`)
  let env: { alg?: string; data?: string }
  try {
    env = JSON.parse(stored) as typeof env
  } catch {
    return shellErr(`invalid: ${rawPath} is not a cybsh compressed file`)
  }
  if (!env.data || (env.alg !== 'lz4' && env.alg !== 'zstd' && env.alg !== 'brotli' && env.alg !== 'triple')) {
    return shellErr(`invalid: ${rawPath} is not a cybsh compressed file`)
  }
  if (env.alg === 'zstd' && !codecs.decompressZstd) {
    return shellErr('unsupported: zstd needs the rebuilt wasm bundle with @dweb-browser/zstd-wasm — reconnect after the next Pages deploy')
  }
  if (env.alg === 'triple' && !codecs.decompressTriple) {
    return shellErr('unsupported: triple needs the rebuilt wasm bundle with @dweb-browser/zstd-wasm — reconnect after the next Pages deploy')
  }
  let plain: Uint8Array
  try {
    if (env.alg === 'lz4') plain = await codecs.decompressLz4(b64Decode(env.data))
    else if (env.alg === 'brotli') plain = await codecs.decompressBrotli(b64Decode(env.data))
    else if (env.alg === 'zstd') plain = await codecs.decompressZstd!(b64Decode(env.data))
    else plain = await codecs.decompressTriple!(b64Decode(env.data))
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    if (/^unsupported:/.test(detail)) return shellErr(detail)
    return shellErr('integrity: compressed bytes failed to decode — tampered file')
  }
  const text = utf8Decode(plain)
  const outPath = path.replace(/\.(lz4|zst|br|cyb3)$/, '') || `${path}.plain`
  if (text.length > STATIC_WRITE_LIMIT) {
    return shellErr(`too_large: plaintext is ${text.length} bytes, wasm write limit is ${STATIC_WRITE_LIMIT}`)
  }
  await deps.writeVolumeFile(outPath, text)
  return shellOk(`${path} -> ${outPath} (${text.length} bytes)`)
}

const SCRUB_CACHE_KEY = 'cache:cybsh:scrub'

async function handleScrub(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  void args
  const volume = deps.readVolume()
  const names = Object.keys(volume)
  const prevRaw = await deps.keyGet(SCRUB_CACHE_KEY).catch(() => null)
  let prev: Record<string, string> = {}
  try {
    if (prevRaw) prev = JSON.parse(prevRaw) as Record<string, string>
  } catch {
    prev = {}
  }
  const next: Record<string, string> = {}
  let verified = 0
  let changed = 0
  let fresh = 0
  for (const name of names) {
    const hash = await deps.blake3(volume[name] ?? '').catch(() => null)
    const digest = hash ?? `fnv:${fnv1a(volume[name] ?? '')}`
    next[name] = digest
    if (!(name in prev)) fresh++
    else if (prev[name] === digest) verified++
    else changed++
  }
  const missing = Object.keys(prev).filter((k) => !(k in volume))
  await deps.keySet(SCRUB_CACHE_KEY, JSON.stringify(next)).catch(() => undefined)
  const bytes = names.reduce((n, k) => n + (volume[k]?.length ?? 0), 0)
  const suffix =
    missing.length > 0
      ? ` · ${missing.length} vanished since last scrub (run \`repair\` to prune the record)`
      : ''
  return shellOk(
    `scrub done · ${names.length} files · ${humanBytes(bytes)} hashed (${verified} verified, ${changed} changed, ${fresh} new)${suffix} · provider chunk scrub needs the dashboard`,
  )
}

function fnv1a(text: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16).padStart(8, '0')
}

async function handleRepair(_args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  const prevRaw = await deps.keyGet(SCRUB_CACHE_KEY).catch(() => null)
  if (!prevRaw) {
    return shellOk('nothing queued — run `scrub` first to snapshot the volume (provider repair needs the dashboard)')
  }
  let prev: Record<string, string> = {}
  try {
    prev = JSON.parse(prevRaw) as Record<string, string>
  } catch {
    return shellOk('nothing queued — run `scrub` first to snapshot the volume (provider repair needs the dashboard)')
  }
  const volume = deps.readVolume()
  const missing = Object.keys(prev).filter((k) => !(k in volume))
  if (missing.length === 0) {
    return shellOk(`nothing queued — ${Object.keys(prev).length} scrubbed paths all present (provider repair needs the dashboard)`)
  }
  const next: Record<string, string> = {}
  for (const [k, v] of Object.entries(prev)) {
    if (k in volume) next[k] = v
  }
  await deps.keySet(SCRUB_CACHE_KEY, JSON.stringify(next)).catch(() => undefined)
  return shellOk(
    `repaired scrub record · ${missing.length} vanished path(s) pruned (${missing.slice(0, 5).join(', ')}${missing.length > 5 ? '…' : ''}) · provider rebuild needs the dashboard`,
  )
}

async function handleGc(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  const apply = args.includes('--apply') || args.includes('-a')
  const volume = deps.readVolume()
  const names = Object.keys(volume)
  const empty = names.filter((k) => (volume[k] ?? '').length === 0 && !k.endsWith('/.keep'))
  const keepMarkers = names.filter((k) => k.endsWith('/.keep'))
  // A `.keep` whose directory holds nothing else is reclaimable.
  const lonelyKeeps = keepMarkers.filter((k) => {
    const dir = k.slice(0, -'/.keep'.length)
    const prefix = dir === '' ? '/' : `${dir}/`
    return !names.some((n) => n !== k && (n === dir || n.startsWith(prefix)))
  })
  const reclaimable = [...new Set([...empty, ...lonelyKeeps])]
  const bytes = reclaimable.reduce((n, k) => n + (volume[k]?.length ?? 0), 0)
  if (!apply) {
    return shellOk(
      `gc (dry run) · ${names.length} checked · ${reclaimable.length} reclaimable (${humanBytes(bytes)}: ${empty.length} empty files, ${lonelyKeeps.length} lonely .keep markers) · re-run with --apply to delete · provider GC needs the dashboard`,
    )
  }
  let freed = 0
  for (const k of reclaimable) {
    try {
      freed += await deps.deleteVolumePath(k, false)
    } catch {
      // Best-effort per path; the summary reports what moved.
    }
  }
  return shellOk(`gc · ${names.length} checked · ${reclaimable.length} deleted · ${humanBytes(freed)} freed · provider GC needs the dashboard`)
}

type Endpoint =
  | { kind: 'local'; path: string }
  | { kind: 'provider'; mountId: string; remotePath: string; label: string }

function resolveEndpoint(cwd: string, raw: string): Endpoint {
  const abs = joinVolumePath(cwd, raw)
  const split = splitProviderPath(abs)
  if (split) {
    const label = `/providers/${split.mountId}${split.remotePath ? `/${split.remotePath}` : ''}`
    return { kind: 'provider', mountId: split.mountId, remotePath: split.remotePath, label }
  }
  return { kind: 'local', path: abs }
}

/** Split leading `-r`/`-f`-style flags off an operand list. */
function splitFlags(args: string[], allowed: string): { flags: Set<string>; rest: string[] } {
  const flags = new Set<string>()
  const rest: string[] = []
  for (const a of args) {
    if (a.startsWith('-') && a.length > 1 && !a.startsWith('--')) {
      const letters = a.slice(1)
      if ([...letters].every((ch) => allowed.includes(ch))) {
        for (const ch of letters) flags.add(ch)
        continue
      }
      throw new Error(`usage: unknown flag -${letters} (allowed: -${allowed.split('').join('/')})`)
    }
    rest.push(a)
  }
  return { flags, rest }
}

function localDirExists(volume: Record<string, string>, path: string): boolean {
  if (path === '/') return true
  const prefix = `${path}/`
  return Object.keys(volume).some((k) => k.startsWith(prefix))
}

function localChildren(volume: Record<string, string>, dir: string): string[] {
  const prefix = dir === '/' ? '/' : `${dir}/`
  return Object.keys(volume).filter((k) => k.startsWith(prefix))
}

async function readEndpointFile(ep: Endpoint, deps: StaticCybshDeps): Promise<Uint8Array> {
  if (ep.kind === 'local') {
    const text = deps.readVolume()[ep.path]
    if (text === undefined) throw new Error('__missing__')
    return utf8Encode(text)
  }
  try {
    return await deps.providerRead(ep.mountId, ep.remotePath)
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    if (/not_found|404|does not exist|no such/i.test(detail)) throw new Error('__missing__')
    throw e
  }
}

async function writeEndpointFile(ep: Endpoint, data: Uint8Array, deps: StaticCybshDeps): Promise<void> {
  if (ep.kind === 'local') {
    if (data.length > STATIC_WRITE_LIMIT) {
      throw new Error(`too_large: content is ${data.length} bytes, wasm write limit is ${STATIC_WRITE_LIMIT}`)
    }
    const text = utf8DecodeStrict(data)
    if (text === null) {
      throw new Error('unsupported: binary content cannot live in the text shell volume — keep it on the provider or use the dashboard')
    }
    await deps.writeVolumeFile(ep.path, text)
    return
  }
  if (!ep.remotePath) throw new Error(`invalid: cannot overwrite the mount root ${ep.label}`)
  await deps.providerWrite(ep.mountId, ep.remotePath, data)
}

/** Classify an endpoint: file (with bytes), directory, or missing. */
async function classifyEndpoint(
  ep: Endpoint,
  deps: StaticCybshDeps,
): Promise<{ kind: 'file'; bytes: Uint8Array } | { kind: 'dir' } | { kind: 'missing' }> {
  if (ep.kind === 'local') {
    const volume = deps.readVolume()
    if (volume[ep.path] !== undefined) return { kind: 'file', bytes: utf8Encode(volume[ep.path]) }
    return localDirExists(volume, ep.path) ? { kind: 'dir' } : { kind: 'missing' }
  }
  if (!ep.remotePath) return { kind: 'dir' }
  try {
    const bytes = await readEndpointFile(ep, deps)
    return { kind: 'file', bytes }
  } catch (e) {
    // Genuine transport failures surface; only `__missing__` falls through
    // to the listing, which decides dir vs missing. Git-backed mounts
    // cannot hold empty directories, so an empty listing is `missing`.
    if (e instanceof Error && e.message !== '__missing__') throw e
    const entries = await deps.providerList(ep.mountId, ep.remotePath).catch(() => null)
    if (entries && entries.length > 0) return { kind: 'dir' }
    return { kind: 'missing' }
  }
}

async function copyTree(
  src: Endpoint,
  dst: Endpoint,
  recursive: boolean,
  deps: StaticCybshDeps,
  tally: { files: number; bytes: number },
): Promise<void> {
  const cls = await classifyEndpoint(src, deps)
  if (cls.kind === 'missing') throw new Error('__missing__')
  if (cls.kind === 'dir' && !recursive) {
    throw new Error('__isdir__')
  }
  if (cls.kind === 'file') {
    let target = dst
    if (dst.kind === 'local' && localDirExists(deps.readVolume(), dst.path)) {
      target = { kind: 'local', path: `${dst.path === '/' ? '' : dst.path}/${baseName(srcLabel(src))}` }
    } else if (dst.kind === 'provider') {
      const listed = await deps.providerList(dst.mountId, dst.remotePath).catch(() => null)
      if (listed && listed.length > 0) {
        target = {
          kind: 'provider',
          mountId: dst.mountId,
          remotePath: `${dst.remotePath}/${baseName(srcLabel(src))}`.replace(/^\/+/, ''),
          label: `${dst.label}/${baseName(srcLabel(src))}`,
        }
      }
    }
    await writeEndpointFile(target, cls.bytes, deps)
    tally.files++
    tally.bytes += cls.bytes.length
    return
  }
  // Directory walk. Destination becomes a directory prefix on both sides.
  if (src.kind === 'local') {
    const volume = deps.readVolume()
    const strip = src.path === '/' ? 1 : src.path.length + 1
    for (const key of localChildren(volume, src.path)) {
      const rel = key.slice(strip)
      const childDst: Endpoint =
        dst.kind === 'local'
          ? { kind: 'local', path: `${dst.path === '/' ? '' : dst.path}/${rel}` }
          : {
              kind: 'provider',
              mountId: dst.mountId,
              remotePath: `${dst.remotePath}/${rel}`.replace(/^\/+/, ''),
              label: `${dst.label}/${rel}`,
            }
      await writeEndpointFile(childDst, utf8Encode(volume[key] ?? ''), deps)
      tally.files++
      tally.bytes += (volume[key] ?? '').length
    }
    return
  }
  const walk = async (remoteDir: string, dstPrefix: Endpoint): Promise<void> => {
    const entries = await deps.providerList(src.mountId, remoteDir)
    for (const e of entries) {
      const childRemote = remoteDir ? `${remoteDir}/${e.name}` : e.name
      if (e.isDir) {
        await walk(childRemote, dstPrefix)
        continue
      }
      const bytes = await readEndpointFile(
        {
          kind: 'provider',
          mountId: src.mountId,
          remotePath: childRemote,
          label: `/providers/${src.mountId}/${childRemote}`,
        },
        deps,
      )
      const rel = childRemote.slice(src.remotePath ? src.remotePath.length + 1 : 0)
      const childDst: Endpoint =
        dstPrefix.kind === 'local'
          ? { kind: 'local', path: `${dstPrefix.path === '/' ? '' : dstPrefix.path}/${rel}` }
          : {
              kind: 'provider',
              mountId: dstPrefix.mountId,
              remotePath: `${dstPrefix.remotePath}/${rel}`.replace(/^\/+/, ''),
              label: `${dstPrefix.label}/${rel}`,
            }
      await writeEndpointFile(childDst, bytes, deps)
      tally.files++
      tally.bytes += bytes.length
    }
  }
  await walk(src.remotePath, dst)
}

function srcLabel(ep: Endpoint): string {
  return ep.kind === 'local' ? ep.path : ep.label
}

/**
 * `ls` for outside-container repos. Returns `null` when no operand touches
 * `/providers/…` so the wasm volume keeps serving local listings.
 * `/providers` itself lists every mount plus enabled-but-unmounted configs.
 */
async function handleLs(args: string[], deps: StaticCybshDeps): Promise<VerbOut | null> {
  let flags: Set<string>
  let rest: string[]
  try {
    ;({ flags, rest } = splitFlags(args, 'l'))
  } catch (e) {
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  const long = flags.has('l')
  const cwd = await deps.getCwd().catch(() => '/')
  const targets = rest.length ? rest : ['.']
  const eps = targets.map((t) => resolveEndpoint(cwd, t))
  if (!eps.some((ep) => ep.kind === 'provider' || srcLabel(ep) === '/providers')) {
    const bare = joinVolumePath(cwd, targets[0])
    if (bare !== '/providers') return null
  }
  const blocks: string[] = []
  for (const ep of eps) {
    if (ep.kind === 'local') {
      const bare = ep.path.replace(/\/+$/, '') || '/'
      if (bare !== '/providers') return null
      const [mounts, configs] = await Promise.all([
        deps.listMounts().catch((): StaticMount[] => []),
        deps.listSyncConfigs().catch((): StaticSyncConfig[] => []),
      ])
      const mounted = new Set(mounts.map((m) => m.configId))
      const rows = [
        ...mounts.map((m) => `${m.id}/`),
        ...configs
          .filter((c) => c.enabled && !mounted.has(c.id))
          .map((c) => `${c.backendType} · ${(c.repoName || c.basePath || c.id).slice(0, 32)} (mount…)/`),
      ].sort()
      blocks.push(rows.join(long ? '\n' : '  ') || '(no providers connected)')
      continue
    }
    if (!ep.remotePath) {
      // Mount roots list through the canal; a config id without a mount
      // names nothing listable yet (mounts come from the provider cards).
      const mounts = await deps.listMounts().catch((): StaticMount[] => [])
      if (!mounts.some((m) => m.id === ep.mountId)) {
        return shellErr(`not_found: no provider mount ${ep.mountId} — mount it from its provider card first (ls /providers shows mounts)`)
      }
    }
    let entries: StaticVfsEntry[]
    try {
      entries = await deps.providerList(ep.mountId, ep.remotePath)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
    if (!entries.length) {
      blocks.push('(empty)')
      continue
    }
    blocks.push(
      entries
        .map((e) => (e.isDir ? `${e.name}/` : long ? `${String(e.sizeBytes ?? 0).padStart(10)} ${e.name}` : e.name))
        .join(long ? '\n' : '  '),
    )
  }
  return shellOk(blocks.join('\n'))
}

/**
 * `cat` for outside-container files. Returns `null` when no operand touches
 * `/providers/…` so the wasm volume keeps serving local reads.
 */
async function handleCat(args: string[], deps: StaticCybshDeps): Promise<VerbOut | null> {
  if (!args.length) return null
  const cwd = await deps.getCwd().catch(() => '/')
  const eps = args.map((t) => resolveEndpoint(cwd, t))
  if (!eps.some((ep) => ep.kind === 'provider')) return null
  const parts: string[] = []
  for (let i = 0; i < eps.length; i++) {
    const ep = eps[i]
    try {
      if (ep.kind === 'local') {
        const text = deps.readVolume()[ep.path]
        if (text === undefined) return shellErr(`not_found: ${args[i]}`)
        parts.push(text)
        continue
      }
      if (!ep.remotePath) return shellErr(`is a directory: ${args[i]}`)
      const bytes = await deps.providerRead(ep.mountId, ep.remotePath)
      const text = utf8DecodeStrict(bytes)
      if (text === null) {
        return shellErr(`unsupported: ${args[i]} is binary — cp it to the volume first`)
      }
      parts.push(text)
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e)
      if (/not_found|404|does not exist|no such/i.test(detail)) return shellErr(`not_found: ${args[i]}`)
      return shellErr(detail)
    }
  }
  return shellOk(parts.join(''))
}

async function deleteTree(ep: Endpoint, recursive: boolean, deps: StaticCybshDeps): Promise<{ files: number; bytes: number }> {
  const cls = await classifyEndpoint(ep, deps)
  if (cls.kind === 'missing') throw new Error('__missing__')
  if (cls.kind === 'file') {
    if (ep.kind === 'local') {
      const freed = await deps.deleteVolumePath(ep.path, false)
      return { files: 1, bytes: freed }
    }
    await deps.providerDelete(ep.mountId, ep.remotePath)
    return { files: 1, bytes: cls.bytes.length }
  }
  if (!recursive) throw new Error('__isdir__')
  if (ep.kind === 'local') {
    const volume = deps.readVolume()
    const keys = localChildren(volume, ep.path)
    let files = 0
    let freed = 0
    for (const key of keys) {
      try {
        freed += await deps.deleteVolumePath(key, false)
        files++
      } catch {
        // Best-effort per key; the summary reports what actually moved.
      }
    }
    return { files, bytes: freed }
  }
  let files = 0
  let bytes = 0
  const walk = async (remoteDir: string): Promise<void> => {
    const entries = await deps.providerList(ep.mountId, remoteDir)
    for (const e of entries) {
      const childRemote = remoteDir ? `${remoteDir}/${e.name}` : e.name
      if (e.isDir) {
        await walk(childRemote)
        continue
      }
      await deps.providerDelete(ep.mountId, childRemote)
      files++
      bytes += e.sizeBytes ?? 0
    }
  }
  await walk(ep.remotePath)
  return { files, bytes }
}

async function handleFileOp(
  verb: 'cp' | 'mv',
  args: string[],
  deps: StaticCybshDeps,
): Promise<VerbOut> {
  let flags: Set<string>
  let rest: string[]
  try {
    // `mv` is always recursive (like the native shell); `-r` is accepted
    // and ignored so `mv -r a b` muscle memory keeps working.
    ;({ flags, rest } = splitFlags(args, 'rRf'))
  } catch (e) {
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  const recursive = flags.has('r') || flags.has('R')
  if (rest.length < 2) return shellErr(`usage: ${verb} [-r] <src> <dst>`)
  const cwd = await deps.getCwd().catch(() => '/')
  // `mv a b c` (multi-source) is dashboard territory; two-operand form only.
  if (rest.length > 2) {
    return shellErr(`unsupported: \`${verb}\` with ${rest.length} operands needs the dashboard shell (two-operand form only here)`)
  }
  const src = resolveEndpoint(cwd, rest[0])
  const dst = resolveEndpoint(cwd, rest[1])
  const tally = { files: 0, bytes: 0 }
  try {
    await copyTree(src, dst, verb === 'mv' ? true : recursive, deps, tally)
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    if (detail === '__missing__') return shellErr(`not_found: ${rest[0]}`)
    if (detail === '__isdir__') return shellErr(`is a directory: ${rest[0]} (use -r)`)
    return shellErr(detail)
  }
  if (verb === 'mv') {
    try {
      await deleteTree(src, true, deps)
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e)
      return shellErr(`conflict: copied ${tally.files} file(s) but the source would not delete (${detail})`)
    }
  }
  const what = `${tally.files} file${tally.files === 1 ? '' : 's'} · ${humanBytes(tally.bytes)}`
  return shellOk(`${srcLabel(src)} -> ${srcLabel(dst)} (${what}${verb === 'mv' ? ' moved' : ' copied'})`)
}

async function handleRm(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  let flags: Set<string>
  let rest: string[]
  try {
    ;({ flags, rest } = splitFlags(args, 'rRf'))
  } catch (e) {
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  if (rest.length === 0) return shellErr('usage: rm [-r] [-f] <path>…')
  const recursive = flags.has('r') || flags.has('R')
  const force = flags.has('f')
  const cwd = await deps.getCwd().catch(() => '/')
  const lines: string[] = []
  let failed = 0
  let files = 0
  let bytes = 0
  for (const raw of rest) {
    const ep = resolveEndpoint(cwd, raw)
    try {
      const done = await deleteTree(ep, recursive, deps)
      files += done.files
      bytes += done.bytes
      lines.push(`removed ${srcLabel(ep)}`)
    } catch (e) {
      const detail = e instanceof Error ? e.message : String(e)
      if (detail === '__missing__') {
        if (force) continue
        lines.push(`not_found: ${raw}`)
        failed++
        continue
      }
      if (detail === '__isdir__') {
        lines.push(`is a directory: ${raw} (use -r)`)
        failed++
        continue
      }
      lines.push(detail)
      failed++
    }
  }
  if (files > 0) lines.push(`${files} file${files === 1 ? '' : 's'} removed · ${humanBytes(bytes)} freed`)
  return (failed > 0 ? shellErr : shellOk)(lines.join('\n') || 'nothing removed')
}

async function handleMkdir(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  let rest: string[]
  try {
    ;({ rest } = splitFlags(args, 'p'))
  } catch (e) {
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  if (rest.length === 0) return shellErr('usage: mkdir [-p] <dir>…')
  const cwd = await deps.getCwd().catch(() => '/')
  for (const raw of rest) {
    const ep = resolveEndpoint(cwd, raw)
    if (ep.kind === 'local') {
      if (ep.path === '/') continue
      const volume = deps.readVolume()
      if (volume[ep.path] !== undefined) {
        return shellErr(`conflict: ${raw} exists and is a file`)
      }
      // Directories are implied by paths; the `.keep` marker makes the
      // empty directory visible to `ls`/`cd` (same as the wasm `mkdir` arm).
      await deps.writeVolumeFile(`${ep.path}/.keep`, '')
    } else {
      if (!ep.remotePath) continue
      // Git-backed mounts cannot hold empty directories — the `.keep`
      // marker is the portable equivalent (and `rm -r` cleans it).
      await deps.providerWrite(ep.mountId, `${ep.remotePath}/.keep`, new Uint8Array())
    }
  }
  return shellOk(rest.length === 1 ? `made ${rest[0]}` : `made ${rest.length} directories`)
}

/** `*`-only glob match (mirrors `glob_match` in shell.rs / `wasm_glob` in os.rs). */
function staticGlob(pattern: string, text: string): boolean {
  if (pattern === '*' || pattern === '') return true
  if (!pattern.includes('*')) return text.includes(pattern)
  const parts = pattern.split('*')
  let rest = text
  let first = true
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i]
    if (!part) continue
    const last = i === parts.length - 1
    if (first && !pattern.startsWith('*')) {
      if (!rest.startsWith(part)) return false
      rest = rest.slice(part.length)
      first = false
      continue
    }
    first = false
    const pos = rest.indexOf(part)
    if (pos < 0) return false
    if (last && !pattern.endsWith('*') && pos + part.length !== rest.length) return false
    rest = rest.slice(pos + part.length)
  }
  return true
}

/** List every file under a local dir (cap 5000, sorted). */
function staticLocalWalk(volume: Record<string, string>, dir: string): string[] {
  const out = Object.keys(volume).filter((k) => {
    if (dir === '/') return true
    return k === dir || k.startsWith(`${dir}/`)
  })
  out.sort()
  return out.slice(0, 5000)
}

async function handleGrep(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  let insensitive = false
  let showLine = false
  const rest: string[] = []
  for (const a of args) {
    if (a === '-i' || a === '--ignore-case') insensitive = true
    else if (a === '-n' || a === '--line-number') showLine = true
    else if (/^-[in]+$/.test(a)) {
      if (a.includes('i')) insensitive = true
      if (a.includes('n')) showLine = true
    } else rest.push(a)
  }
  if (rest.length === 0) return shellErr('usage: grep [-i] [-n] <pattern> [paths…]')
  const pattern = rest[0]
  const needle = insensitive ? pattern.toLowerCase() : pattern
  const hitsLine = (line: string): boolean =>
    insensitive ? line.toLowerCase().includes(needle) : line.includes(needle)
  const cwd = await deps.getCwd().catch(() => '/')
  // No paths: grep the whole local volume (no stdin on this transport).
  const targets = rest.length === 1 ? ['/'] : rest.slice(1)
  const files: string[] = []
  for (const raw of targets) {
    const ep = resolveEndpoint(cwd, raw)
    const cls = await classifyEndpoint(ep, deps).catch(() => ({ kind: 'missing' }) as const)
    if (cls.kind === 'missing') return shellErr(`not_found: ${raw}`)
    if (cls.kind === 'file') {
      files.push(srcLabel(ep))
      continue
    }
    if (ep.kind === 'local') {
      files.push(...staticLocalWalk(deps.readVolume(), ep.path).filter((k) => !k.endsWith('/.keep')))
    } else {
      const entries = await deps.providerList(ep.mountId, ep.remotePath).catch(() => null)
      if (!entries) return shellErr(`not_found: ${raw}`)
      for (const e of entries) {
        if (!e.isDir) files.push(`${ep.label}/${e.name}`)
      }
    }
  }
  const uniq = [...new Set(files)].sort()
  const prefixFile = uniq.length > 1
  const hits: string[] = []
  for (const label of uniq) {
    const ep = label.startsWith('/providers/') ? resolveEndpoint('/', label) : resolveEndpoint(cwd, label)
    let bytes: Uint8Array
    try {
      bytes = await readEndpointFile(ep, deps)
    } catch {
      continue
    }
    const text = utf8Decode(bytes)
    const lines = text.split('\n')
    for (let i = 0; i < lines.length; i++) {
      if (hitsLine(lines[i])) {
        const body = showLine ? `${i + 1}:${lines[i]}` : lines[i]
        hits.push(prefixFile ? `${label}:${body}` : body)
        if (hits.length >= 400) break
      }
    }
    if (hits.length >= 400) break
  }
  if (json) return shellOk(JSON.stringify({ pattern, matches: hits }))
  if (hits.length === 0) return shellOk(`no matches for \`${pattern}\``)
  return shellOk(hits.join('\n'))
}

async function handleFind(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const operands = args.filter((a) => !a.startsWith('-'))
  const cwd = await deps.getCwd().catch(() => '/')
  let rootRaw = '/'
  let pattern: string | undefined
  if (operands.length === 1) {
    const ep = resolveEndpoint(cwd, operands[0])
    const cls = await classifyEndpoint(ep, deps).catch(() => ({ kind: 'missing' }) as const)
    if (cls.kind === 'dir' || (cls.kind === 'file' && operands[0].endsWith('/'))) {
      rootRaw = operands[0]
    } else if (cls.kind === 'file') {
      const abs = joinVolumePath(cwd, operands[0])
      if (json) return shellOk(JSON.stringify({ root: abs, pattern: null, matches: [abs] }))
      return shellOk(abs)
    } else {
      pattern = operands[0]
    }
  } else if (operands.length >= 2) {
    rootRaw = operands[0]
    pattern = operands[1]
  }
  const ep = resolveEndpoint(cwd, rootRaw)
  const cls = await classifyEndpoint(ep, deps).catch(() => ({ kind: 'missing' }) as const)
  if (cls.kind === 'missing') return shellErr(`not_found: ${rootRaw}`)
  if (cls.kind === 'file') {
    const label = srcLabel(ep)
    if (json) return shellOk(JSON.stringify({ root: label, pattern: pattern ?? null, matches: [label] }))
    return shellOk(label)
  }
  let all: string[] = []
  if (ep.kind === 'local') {
    all = staticLocalWalk(deps.readVolume(), ep.path)
  } else {
    const entries = await deps.providerList(ep.mountId, ep.remotePath).catch(() => null)
    if (!entries) return shellErr(`not_found: ${rootRaw}`)
    all = entries.map((e) => `${ep.label}/${e.name}${e.isDir ? '/' : ''}`)
  }
  const hits = all.filter((f) => {
    if (!pattern) return true
    const base = f.split('/').filter(Boolean).pop() ?? f
    return staticGlob(pattern, base) || staticGlob(pattern, f)
  })
  if (json) return shellOk(JSON.stringify({ root: srcLabel(ep), pattern: pattern ?? null, matches: hits }))
  if (hits.length === 0) return shellOk('(no matches)')
  return shellOk(hits.join('\n'))
}

async function handleHeadTail(
  head: boolean,
  args: string[],
  deps: StaticCybshDeps,
): Promise<VerbOut> {
  let n = 10
  const rest: string[] = []
  for (let i = 0; i < args.length; i++) {
    if ((args[i] === '-n' || args[i] === '--lines') && i + 1 < args.length) {
      n = Math.max(1, parseInt(args[i + 1], 10) || 10)
      i++
      continue
    }
    const m = args[i].match(/^-n(\d+)$/)
    if (m) {
      n = Math.max(1, parseInt(m[1], 10) || 10)
      continue
    }
    rest.push(args[i])
  }
  if (rest.length === 0) return shellErr(`usage: ${head ? 'head' : 'tail'} [-n N] <path>`)
  const cwd = await deps.getCwd().catch(() => '/')
  const ep = resolveEndpoint(cwd, rest[0])
  let bytes: Uint8Array
  try {
    bytes = await readEndpointFile(ep, deps)
  } catch (e) {
    if (e instanceof Error && e.message === '__missing__') return shellErr(`not_found: ${rest[0]}`)
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  const text = utf8Decode(bytes)
  const lines = text.split('\n')
  const picked = head ? lines.slice(0, n) : lines.slice(Math.max(0, lines.length - n))
  return shellOk(picked.join('\n'))
}

async function handleWc(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const operands = args.filter((a) => !a.startsWith('-'))
  if (operands.length === 0) return shellErr('usage: wc [paths…]')
  const cwd = await deps.getCwd().catch(() => '/')
  const rows: Array<{ path: string; lines: number; words: number; bytes: number }> = []
  for (const raw of operands) {
    const ep = resolveEndpoint(cwd, raw)
    let bytes: Uint8Array
    try {
      bytes = await readEndpointFile(ep, deps)
    } catch (e) {
      if (e instanceof Error && e.message === '__missing__') return shellErr(`not_found: ${raw}`)
      return shellErr(e instanceof Error ? e.message : String(e))
    }
    const text = utf8Decode(bytes)
    rows.push({
      path: srcLabel(ep),
      lines: text === '' ? 0 : text.split('\n').length,
      words: text.split(/\s+/).filter(Boolean).length,
      bytes: bytes.length,
    })
  }
  if (json) {
    const total = rows.reduce(
      (t, r) => ({ lines: t.lines + r.lines, words: t.words + r.words, bytes: t.bytes + r.bytes }),
      { lines: 0, words: 0, bytes: 0 },
    )
    return shellOk(JSON.stringify({ files: rows, total }))
  }
  const lines = rows.map((r) => `${r.lines} ${r.words} ${r.bytes} ${r.path}`)
  if (rows.length > 1) {
    const t = rows.reduce(
      (a, r) => ({ lines: a.lines + r.lines, words: a.words + r.words, bytes: a.bytes + r.bytes }),
      { lines: 0, words: 0, bytes: 0 },
    )
    lines.push(`${t.lines} ${t.words} ${t.bytes} total`)
  }
  return shellOk(lines.join('\n'))
}

async function handleEdit(args: string[], deps: StaticCybshDeps): Promise<VerbOut> {
  if (args.length < 3) return shellErr('usage: edit <path> <old> <new>')
  const [rawPath, oldText, ...newParts] = args
  const newText = newParts.join(' ')
  if (!oldText) return shellErr('integrity: refusing empty anchor (old text must be ≥1 char)')
  const cwd = await deps.getCwd().catch(() => '/')
  const ep = resolveEndpoint(cwd, rawPath)
  let bytes: Uint8Array
  try {
    bytes = await readEndpointFile(ep, deps)
  } catch (e) {
    if (e instanceof Error && e.message === '__missing__') return shellErr(`not_found: ${rawPath}`)
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  const text = utf8DecodeStrict(bytes)
  if (text === null) return shellErr('invalid: file is not UTF-8 text')
  const count = text.split(oldText).length - 1
  if (count === 0) return shellErr(`not_found: anchor occurs 0 times in ${srcLabel(ep)}`)
  if (count > 1) return shellErr(`conflict: anchor occurs ${count} times in ${srcLabel(ep)} — refine it to exactly one`)
  const updated = text.replace(oldText, newText)
  try {
    await writeEndpointFile(ep, utf8Encode(updated), deps)
  } catch (e) {
    return shellErr(e instanceof Error ? e.message : String(e))
  }
  return shellOk(`edited ${srcLabel(ep)} (1 replacement, ${updated.length} bytes)`)
}

/**
 * Run one terminal line against the local vault. Returns `null` when the
 * line is not ours (chained line, unknown verb) so the caller falls through
 * to the wasm dispatcher. Never throws — handler failures become
 * `{ ok: false }` shell answers with the house `prefix: detail` contract.
 */
// ─── `.cybsh` scripts + `theme`/`ui` (mirrors crates/os/src/script.rs ──────
// and the `run`/`theme`/`ui` verbs in crates/os/src/shell.rs). The native
// shell interprets the same grammar; only transport-owned bits differ:
// `fetch` really runs here (browser fetch, 10 s, 64 KiB), provider verbs
// stay local-vault answers, and theme writes land in both the volume mirror
// (`/.cybermanju/theme.json`) and the live store (`cybermanju_theme_v1`).

/** Nesting guard for scripts calling scripts (same 4-deep budget as native). */
let scriptDepth = 0

const STATIC_THEME_FILE = '/.cybermanju/theme.json'
const STATIC_THEME_IDS = [
  'os-dark',
  'os-light',
  'os-graphite',
  'plasma-dark',
  'plasma-light',
]
const STATIC_THEME_ALIASES: Record<string, string> = {
  // Pre-remake 17-theme ids migrate by mode.
  'mac-light': 'os-light',
  'mac-graphite-light': 'os-light',
  'ocean-light': 'os-light',
  'sunset-light': 'os-light',
  'forest-light': 'os-light',
  'lavender-light': 'os-light',
  'rose-light': 'os-light',
  daylight: 'os-light',
  'mac-dark': 'os-dark',
  'mac-graphite-dark': 'os-graphite',
  'mac-midnight': 'os-dark',
  'ocean-night': 'os-dark',
  'forest-night': 'os-dark',
  'ember-night': 'os-dark',
  'nebula-night': 'os-dark',
  'cyber-night': 'os-dark',
  'matrix-night': 'os-dark',
  'cyberpunk-night': 'os-dark',
  midnight: 'os-dark',
  nebula: 'os-dark',
  ember: 'os-dark',
  ghostline: 'os-dark',
  dark: 'os-dark',
  light: 'os-light',
  graphite: 'os-graphite',
  plasma: 'plasma-dark',
  auto: 'os-dark',
}

function canonicalStaticTheme(id: string): string | null {
  const lower = id.toLowerCase()
  if ((STATIC_THEME_IDS as string[]).includes(lower)) return lower
  return STATIC_THEME_ALIASES[lower] ?? null
}

function validStaticAccent(raw: string): boolean {
  if (!raw) return false
  const hex = raw.startsWith('#') ? raw.slice(1) : raw
  return (hex.length === 3 || hex.length === 6) && /^[0-9a-fA-F]+$/.test(hex)
}

/** Full interface settings mirror (`/.cybermanju/theme.json`) — every `ui`
 *  verb reads/writes this shape on all three transports, so the terminal
 *  customizes the whole interface, not just the palette. */
export interface StaticUiSettings {
  theme: string
  accent: string | null
  accents: Record<string, string>
  density: 'compact' | 'comfortable'
  glass: 0 | 1 | 2 | 3
  motion: 'auto' | 'full' | 'reduced'
  glow: boolean
}

const STATIC_UI_DEFAULTS: StaticUiSettings = {
  theme: 'os-dark',
  accent: null,
  accents: {},
  density: 'comfortable',
  glass: 2,
  motion: 'auto',
  glow: false,
}

const STATIC_GLASS_NAMES: Record<string, 0 | 1 | 2 | 3> = {
  // Flat remake: solid (0) or translucent (everything else → 2).
  '0': 0, solid: 0, off: 0,
  '1': 2, light: 2,
  '2': 2, default: 2, translucent: 2, on: 2,
  '3': 2, rich: 2,
}

function readStaticUi(deps: StaticCybshDeps): StaticUiSettings {
  try {
    const raw = deps.readVolume()[STATIC_THEME_FILE]
    if (!raw) return { ...STATIC_UI_DEFAULTS, accents: {} }
    const value = JSON.parse(raw) as Record<string, unknown>
    const theme =
      typeof value.theme === 'string' && canonicalStaticTheme(value.theme)
        ? (canonicalStaticTheme(value.theme) as string)
        : STATIC_UI_DEFAULTS.theme
    const accent =
      typeof value.accent === 'string' && validStaticAccent(value.accent) ? value.accent : null
    const accents: Record<string, string> = {}
    if (value.accents && typeof value.accents === 'object') {
      for (const [k, v] of Object.entries(value.accents as Record<string, unknown>)) {
        if (canonicalStaticTheme(k) && typeof v === 'string' && validStaticAccent(v)) accents[k] = v
      }
    }
    const density = value.density === 'compact' ? 'compact' : 'comfortable'
    const glassRaw = value.glass
    // Collapse legacy vibrancy levels onto solid (0) / translucent (2).
    const glass: 0 | 1 | 2 | 3 =
      glassRaw === 0 ? 0 : 2
    const motion =
      value.motion === 'full' || value.motion === 'reduced' ? value.motion : 'auto'
    const glow = typeof value.glow === 'boolean' ? value.glow : false
    return { theme, accent, accents, density, glass, motion, glow }
  } catch {
    return { ...STATIC_UI_DEFAULTS, accents: {} }
  }
}

/** Best-effort live apply: localStorage (survives reload) + document. The
 *  Terminal panel applies the same intent reactively from the `ui:` effect
 *  lines every verb prints, so all transports converge live. */
function applyLiveUi(s: StaticUiSettings): void {
  try {
    if (typeof localStorage !== 'undefined') {
      const raw = localStorage.getItem('cybermanju_theme_v1')
      const cur = raw ? (JSON.parse(raw) as Record<string, unknown>) : {}
      localStorage.setItem('cybermanju_theme_v1', JSON.stringify({ ...cur, ...s }))
    }
  } catch {
    // Private mode / quota — the volume mirror still holds the intent.
  }
  try {
    if (typeof document !== 'undefined') {
      const root = document.documentElement
      root.dataset.uiTheme = s.theme
      if (s.accent) root.style.setProperty('--ui-accent', s.accent)
      else root.style.removeProperty('--ui-accent')
    }
  } catch {
    // Node/test env — no document to restyle.
  }
}

async function writeStaticUi(deps: StaticCybshDeps, s: StaticUiSettings): Promise<void> {
  await deps.writeVolumeFile(STATIC_THEME_FILE, JSON.stringify(s))
  applyLiveUi(s)
}

function staticUiLine(s: StaticUiSettings): string {
  const accent = s.accent ?? 'system'
  const perTheme = Object.entries(s.accents)
    .map(([k, v]) => `${k}=${v}`)
    .join(' ')
  const head = `theme: ${s.theme} · accent: ${accent}`
  const body =
    `density: ${s.density} · glass: ${s.glass} · motion: ${s.motion} · glow: ${s.glow ? 'on' : 'off'}` +
    (perTheme ? `\naccents: ${perTheme}` : '')
  const effects =
    `ui: theme=${s.theme}\nui: accent=${s.accent ?? 'system'}\n` +
    `ui: density=${s.density}\nui: glass=${s.glass}\nui: motion=${s.motion}\nui: glow=${s.glow ? 'on' : 'off'}` +
    Object.entries(s.accents)
      .map(([k, v]) => `\nui: accent-for=${k}:${v}`)
      .join('')
  return `${head}\n${body}\n${effects}`
}

async function handleTheme(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const want = args.find((a) => !a.startsWith('-'))
  const s = readStaticUi(deps)
  if (!want || want === 'get') {
    if (json) return shellOk(JSON.stringify(s))
    return shellOk(staticUiLine(s))
  }
  const canonical = canonicalStaticTheme(want)
  if (!canonical) {
    return shellErr(`invalid: unknown theme '${want}' (try: ${STATIC_THEME_IDS.join(', ')})`)
  }
  const next = { ...s, theme: canonical }
  await writeStaticUi(deps, next)
  if (json) return shellOk(JSON.stringify(next))
  return shellOk(staticUiLine(next))
}

async function handleUi(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = args[0] ?? 'get'
  const rest = args.slice(1).filter((a) => !a.startsWith('-'))
  const s = readStaticUi(deps)
  if (sub === 'get') {
    if (json) return shellOk(JSON.stringify(s))
    return shellOk(staticUiLine(s))
  }
  if (sub === 'theme') {
    const id = rest[0]
    if (!id) return shellErr(`usage: ui theme <id> (try: ${STATIC_THEME_IDS.join(', ')})`)
    const canonical = canonicalStaticTheme(id)
    if (!canonical) {
      return shellErr(`invalid: unknown theme '${id}' (try: ${STATIC_THEME_IDS.join(', ')})`)
    }
    const next = { ...s, theme: canonical }
    await writeStaticUi(deps, next)
    if (json) return shellOk(JSON.stringify(next))
    return shellOk(staticUiLine(next))
  }
  if (sub === 'accent') {
    const raw = rest[0]
    if (!raw) return shellErr('usage: ui accent <#rrggbb|#rgb|default> [--for <theme>]')
    const flagArgs = args.slice(1)
    const forIdx = flagArgs.findIndex((a) => a === '--for' || a === '-for')
    const forTheme = forIdx >= 0 ? flagArgs[forIdx + 1] : undefined
    if (forIdx >= 0 && (!forTheme || !canonicalStaticTheme(forTheme))) {
      return shellErr(`invalid: unknown theme '${forTheme ?? ''}' (try: ${STATIC_THEME_IDS.join(', ')})`)
    }
    const next: string | null =
      raw === 'default' || raw === 'system' || raw === 'none'
        ? null
        : (() => {
            const hex = raw.startsWith('#') ? raw : `#${raw}`
            return validStaticAccent(hex) ? hex : null
          })()
    if (raw !== 'default' && raw !== 'system' && raw !== 'none' && next === null) {
      return shellErr(`invalid: bad accent '${raw}' (use #rrggbb, #rgb, or \`default\`)`)
    }
    if (forTheme) {
      const canonical = canonicalStaticTheme(forTheme) as string
      const accents = { ...s.accents }
      if (next) accents[canonical] = next
      else delete accents[canonical]
      const updated = { ...s, accents }
      await writeStaticUi(deps, updated)
      const shown = next ?? 'system'
      if (json) return shellOk(JSON.stringify(updated))
      return shellOk(`accent: ${canonical} → ${shown}\nui: accent-for=${canonical}:${shown}`)
    }
    const updated = { ...s, accent: next }
    await writeStaticUi(deps, updated)
    if (json) return shellOk(JSON.stringify(updated))
    return shellOk(staticUiLine(updated))
  }
  if (sub === 'density') {
    const want = rest[0]
    if (!want || want === 'get') {
      if (json) return shellOk(JSON.stringify({ density: s.density }))
      return shellOk(`density: ${s.density}\nui: density=${s.density}`)
    }
    if (want !== 'compact' && want !== 'comfortable') {
      return shellErr(`invalid: bad density '${want}' (use compact|comfortable)`)
    }
    // Validated above — the `as` keeps the literal type `writeStaticUi` needs.
    const updated = { ...s, density: want as 'compact' | 'comfortable' }
    await writeStaticUi(deps, updated)
    if (json) return shellOk(JSON.stringify({ density: want }))
    return shellOk(`density: ${want}\nui: density=${want}`)
  }
  if (sub === 'glass') {
    const want = rest[0]
    if (!want || want === 'get') {
      if (json) return shellOk(JSON.stringify({ glass: s.glass }))
      return shellOk(`glass: ${s.glass}\nui: glass=${s.glass}`)
    }
    const level = STATIC_GLASS_NAMES[want.toLowerCase()]
    if (level === undefined) {
      return shellErr(`invalid: bad glass '${want}' (use 0|solid|off, 2|translucent|default|on)`)
    }
    const updated = { ...s, glass: level }
    await writeStaticUi(deps, updated)
    if (json) return shellOk(JSON.stringify({ glass: level }))
    return shellOk(`glass: ${level}\nui: glass=${level}`)
  }
  if (sub === 'motion') {
    const want = rest[0]
    if (!want || want === 'get') {
      if (json) return shellOk(JSON.stringify({ motion: s.motion }))
      return shellOk(`motion: ${s.motion}\nui: motion=${s.motion}`)
    }
    if (want !== 'auto' && want !== 'full' && want !== 'reduced') {
      return shellErr(`invalid: bad motion '${want}' (use auto|full|reduced)`)
    }
    // Validated above — the `as` keeps the literal type `writeStaticUi` needs.
    const updated = { ...s, motion: want as 'auto' | 'full' | 'reduced' }
    await writeStaticUi(deps, updated)
    if (json) return shellOk(JSON.stringify({ motion: want }))
    return shellOk(`motion: ${want}\nui: motion=${want}`)
  }
  if (sub === 'glow') {
    const want = rest[0]
    if (!want || want === 'get') {
      if (json) return shellOk(JSON.stringify({ glow: s.glow }))
      return shellOk(`glow: ${s.glow ? 'on' : 'off'}\nui: glow=${s.glow ? 'on' : 'off'}`)
    }
    const lower = want.toLowerCase()
    if (!['on', 'off', 'true', 'false', '1', '0'].includes(lower)) {
      return shellErr(`invalid: bad glow '${want}' (use on|off)`)
    }
    const on = lower === 'on' || lower === 'true' || lower === '1'
    const updated = { ...s, glow: on }
    await writeStaticUi(deps, updated)
    if (json) return shellOk(JSON.stringify({ glow: on }))
    return shellOk(`glow: ${on ? 'on' : 'off'}\nui: glow=${on ? 'on' : 'off'}`)
  }
  return shellErr(
    'usage: ui theme <os-dark|os-light|os-graphite|plasma-dark|plasma-light>|accent <#hex|default> [--for <theme>]|density <compact|comfortable>|glass <0|solid|2|translucent>|motion <auto|full|reduced>|glow <on|off>|get ' +
      `(got \`${sub}\`)`,
  )
}

/** Inline `sh` for scripts: static verbs first, wasm volume second. */
async function scriptExecCybsh(line: string, deps: StaticCybshDeps): Promise<string> {
  const handled = await runStaticCybshLine(line, deps)
  if (handled) {
    if (handled.ok) return handled.output
    throw new Error(handled.output || handled.error || 'command failed')
  }
  if (deps.execFallback) return deps.execFallback(line)
  const verb = line.trim().split(/\s+/, 1)[0] ?? line
  throw new Error(
    `unsupported: \`${verb}\` needs the CyberManju dashboard (:3456) on this build — the wasm volume answers single-command lines, provider push stays server-side`,
  )
}

async function staticFetchText(req: CybshFetchReq): Promise<string> {
  const url = req.url
  if (typeof fetch === 'undefined') {
    throw new Error(
      `unsupported: \`fetch ${url}\` has no HTTP client in this context — run the same \`.cybsh\` in the browser build`,
    )
  }
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), 10_000)
  try {
    const res = await fetch(url, { signal: ctrl.signal, method: req.method, headers: req.headers })
    if (!res.ok) throw new Error(`network: fetch ${url} → HTTP ${res.status}`)
    return (await res.text()).slice(0, 64 * 1024)
  } catch (e) {
    if (e instanceof Error && /abort/i.test(e.message)) {
      throw new Error(`network: fetch ${url} timed out after 10 s`)
    }
    throw e
  } finally {
    clearTimeout(timer)
  }
}

interface ReplayJournal {
  fingerprint: string
  args: string[] | null
  sh: Record<string, { ok: boolean; output: string }>
  fetch: Record<string, { ok: boolean; output: string }>
}

function parseReplayJournal(raw: string, source: string, arg: string, args: string[]): ReplayJournal {
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch {
    throw new Error(`invalid: \`${arg}\` is not a replay journal`)
  }
  const doc = parsed as { cybsh?: unknown; fingerprint?: unknown; calls?: unknown; args?: unknown }
  if (doc.cybsh !== 1) throw new Error('invalid: replay journal is not a cybsh v1 journal')
  if (doc.fingerprint !== cybshFingerprint(source)) {
    throw new Error(
      'integrity: replay journal fingerprint mismatch — the script changed since `--record` (re-record, don\'t replay stale inputs)',
    )
  }
  // Journals recorded with argv bind them: replaying with different args is
  // an `integrity:` refusal, never a silent lie. Journals without `args`
  // (v1) replay regardless of argv (back-compat).
  const recordedArgs = Array.isArray(doc.args) ? (doc.args as unknown[]).map(String) : null
  if (recordedArgs && (recordedArgs.length !== args.length || recordedArgs.some((a, i) => a !== args[i]))) {
    throw new Error(
      'integrity: replay journal argv mismatch — the script ran with different `--` args since `--record` (re-record, don\'t replay stale inputs)',
    )
  }
  const calls = (doc.calls ?? {}) as { sh?: unknown; fetch?: unknown }
  const table = (v: unknown): ReplayJournal['sh'] => {
    if (!v || typeof v !== 'object') return {}
    const out: ReplayJournal['sh'] = {}
    for (const [k, rec] of Object.entries(v as Record<string, unknown>)) {
      const r = rec as { ok?: unknown; output?: unknown }
      out[k] = { ok: r.ok === true, output: typeof r.output === 'string' ? r.output : '' }
    }
    return out
  }
  return {
    fingerprint: String(doc.fingerprint ?? ''),
    args: recordedArgs,
    sh: table(calls.sh),
    fetch: table(calls.fetch),
  }
}

async function handleRun(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  // Script argv: everything after a bare `--` belongs to the script
  // (`run job.cybsh -- /inbox weekly`), bound as `args`.
  const dash = args.indexOf('--')
  const scriptArgs = dash >= 0 ? args.slice(dash + 1) : []
  const flags = dash >= 0 ? args.slice(0, dash) : args
  const dry = flags.some((a) => a === '--dry' || a === '--check')
  const lint = flags.some((a) => a === '--lint')
  const fmt = flags.some((a) => a === '--fmt')
  if (flags.some((a) => a === '--help' || a === '-h') || flags.length === 0) {
    return shellErr('usage: run <file.cybsh> [--dry] [--lint] [--fmt] [--json] [--record <journal.json>] [--replay <journal.json>] [-- <args…>]')
  }
  // Value flags consume the next arg, so positionals skip both.
  const positional = flags.filter((a, i) => {
    if (a.startsWith('-')) return false
    const prev = flags[i - 1]
    return prev !== '--record' && prev !== '--replay'
  })
  let recordArg: string | null = null
  let replayArg: string | null = null
  for (let i = 0; i < flags.length; i++) {
    if (flags[i] === '--record' || flags[i] === '--replay') {
      const value = flags[i + 1]
      if (!value || value.startsWith('-')) {
        return shellErr(`usage: run <file.cybsh> [${flags[i]} <journal.json>]`)
      }
      if (flags[i] === '--record') recordArg = value
      else replayArg = value
    }
  }
  if (recordArg && replayArg) return shellErr('invalid: `--record` and `--replay` are exclusive')
  const pathArg = positional[0]
  if (!pathArg) return shellErr('usage: run <file.cybsh> [--dry] [--json] [-- <args…>]')
  if (!pathArg.toLowerCase().endsWith(CYBSH_SCRIPT_EXT)) {
    return shellErr(
      `invalid: \`run\` needs a ${CYBSH_SCRIPT_EXT} file (got \`${pathArg}\`) — scripts are interpreted, no build step`,
    )
  }
  const cwd = await deps.getCwd().catch(() => '/')
  const path = joinVolumePath(cwd, pathArg)
  if (path.startsWith('/providers/')) {
    return shellErr(
      'unsupported: `run` reads scripts from the shell volume (provider mounts are data, not code) — `cp` it locally first',
    )
  }
  const source = deps.readVolume()[path]
  if (source === undefined) return shellErr(`not_found: no script at ${path}`)
  if (lint) {
    try {
      return shellOk(lintCybshScript(source).join('\n'))
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (fmt) {
    try {
      return shellOk(formatCybshScript(source).trimEnd())
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (dry) {
    try {
      return shellOk(`${path}: ${checkCybshScript(source)}`)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (scriptDepth >= CYBSH_MAX_RUN_DEPTH) {
    return shellErr('too_large: `run` nesting exceeds 4 (script calling script calling …)')
  }
  // Replay journal: same source fingerprint (and argv, when recorded) or an
  // `integrity:` refusal.
  let journal: ReplayJournal | null = null
  if (replayArg) {
    const journalPath = joinVolumePath(cwd, replayArg)
    const raw = deps.readVolume()[journalPath]
    if (raw === undefined) {
      return shellErr(`not_found: no replay journal at ${journalPath} (\`--record\` one first)`)
    }
    try {
      journal = parseReplayJournal(raw, source, replayArg, scriptArgs)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  const recording = recordArg !== null
  const logSh: Record<string, { ok: boolean; output: string }> = {}
  const logFetch: Record<string, { ok: boolean; output: string }> = {}
  const execJournal = async (line: string): Promise<string> => {
    if (journal) {
      const rec = journal.sh[line]
      if (!rec) throw new Error(`not_found: replay journal has no \`sh "${line}"\` (re-record with \`--record\`)`)
      if (!rec.ok) throw new Error(rec.output)
      return rec.output
    }
    try {
      const text = await scriptExecCybsh(line, deps)
      if (recording) logSh[line] = { ok: true, output: text }
      return text
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (recording) logSh[line] = { ok: false, output: msg }
      throw e
    }
  }
  const fetchJournal = async (req: CybshFetchReq): Promise<string> => {
    const key = cybshFetchKey(req)
    if (journal) {
      const rec = journal.fetch[key] ?? journal.fetch[req.url]
      if (!rec) throw new Error(`not_found: replay journal has no \`fetch ${key}\` (re-record with \`--record\`)`)
      if (!rec.ok) throw new Error(rec.output)
      return rec.output
    }
    try {
      const body = await staticFetchText(req)
      if (recording) logFetch[key] = { ok: true, output: body }
      return body
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (recording) logFetch[key] = { ok: false, output: msg }
      throw e
    }
  }
  scriptDepth += 1
  try {
    const out = await runCybshScript(
      source,
      {
        execCybsh: execJournal,
        fetchText: fetchJournal,
      },
      scriptArgs,
    )
    if (recording) {
      const recordPath = joinVolumePath(cwd, recordArg as string)
      await deps.writeVolumeFile(
        recordPath,
        JSON.stringify(
          {
            cybsh: 1,
            algo: 'fnv1a64',
            fingerprint: cybshFingerprint(source),
            script: path,
            args: scriptArgs,
            calls: { sh: logSh, fetch: logFetch },
          },
          null,
          2,
        ),
      )
    }
    if (json) {
      const fm = parseFrontmatter(source)
      return shellOk(
        JSON.stringify({
          path,
          vars: out.vars,
          effects: out.effects,
          caps: out.caps,
          calls: { sh: out.shCalls, fetch: out.fetchCalls },
          journal: journal ? 'replay' : recording ? 'record' : null,
          args: scriptArgs,
          schedule: fm.schedule,
          triggers: fm.triggers,
          output: out.output,
        }),
      )
    }
    return shellOk(out.output)
  } catch (e) {
    return shellErr(e instanceof Error ? e.message : String(e))
  } finally {
    scriptDepth = Math.max(0, scriptDepth - 1)
  }
}

// ─── scheduler (cron) ───────────────────────────────────────────────────
// Browser twin of `crates/os/src/shell.rs::cron_cmd`. Persistence rides the
// `cron.*` db ops (supplied through `StaticCybshDeps` by `useTauri.ts`) and
// execution reuses `handleRun`, so a fired schedule behaves exactly like
// `run <path>` typed by hand. The daemon thread exists only on desktop/server.

const CRON_SUBCOMMANDS = ['ls', 'add', 'rm', 'run', 'enable', 'disable', 'history']

/** Output kept per run — mirrors `OUTPUT_TAIL_LIMIT` in `scheduler.rs`. */
const CRON_TAIL_LIMIT = 8 * 1024

function cronPad(s: string, n: number): string {
  return s.length >= n ? s : s + ' '.repeat(n - s.length)
}

function cronTruncate(s: string, max: number): string {
  if (s.length <= max) return s
  return `${s.slice(0, Math.max(1, max - 1))}…`
}

/** `sched-<uuid>` when the platform has it, a unique fallback otherwise. */
function schedId(): string {
  const c = globalThis.crypto as Crypto | undefined
  if (c && typeof c.randomUUID === 'function') return `sched-${c.randomUUID()}`
  return `sched-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

/** Resolve a `/providers/< head>/…` destination to a live mount id. */
async function resolveBackupMount(
  head: string,
  deps: StaticCybshDeps,
): Promise<{ mountId: string; label: string }> {
  const mounts = await deps.listMounts().catch(() => [])
  const exact = mounts.find((m) => m.id === head)
  if (exact) return { mountId: exact.id, label: exact.name || exact.id }
  const byConfig = mounts.find((m) => m.configId === head)
  if (byConfig) return { mountId: byConfig.id, label: byConfig.name || byConfig.id }
  const byName = mounts.find((m) => m.name.toLowerCase() === head.toLowerCase())
  if (byName) return { mountId: byName.id, label: byName.name || byName.id }
  throw new Error(
    `not_found: no provider mount \`${head}\` (mount id, config id or name — see \`providers\`)`,
  )
}

interface BackupFlags {
  to: string | null
  name: string | null
  keep: number | null
  plain: boolean
}

/** Shared `--to/--name/--keep/--plain` parse (mirrors the native verb). */
function parseBackupFlags(args: string[]): { flags: BackupFlags; rest: string[] } {
  const flags: BackupFlags = { to: null, name: null, keep: null, plain: false }
  const rest: string[] = []
  for (let i = 0; i < args.length; i++) {
    const a = args[i]
    if (a === '--to') flags.to = args[++i] ?? null
    else if (a === '--name') flags.name = args[++i] ?? null
    else if (a === '--keep') {
      const n = Number(args[++i])
      if (!Number.isInteger(n) || n < 0) throw new Error('invalid: --keep needs a number')
      flags.keep = n
    } else if (a === '--plain') flags.plain = true
    else if (a === '--json') continue
    else if (a.startsWith('--')) throw new Error(`usage: unknown flag ${a}`)
    else rest.push(a)
  }
  return { flags, rest }
}

async function backupFingerprint(
  deps: StaticCybshDeps,
  data: Uint8Array,
): Promise<string> {
  try {
    const fp = await deps.blake3Bytes?.(data)
    if (typeof fp === 'string' && fp) return fp
  } catch {
    // Fall through to the honest placeholder.
  }
  return 'unavailable'
}

/**
 * Vault snapshot backups on static hosts: seal via the wasm `_export` op,
 * land on a provider (the browser volume is text-only, so a local `--to`
 * refuses honestly), prune beyond `--keep`. Same shapes as the native
 * `backup` verb, `--json` included.
 */
async function handleBackup(
  args: string[],
  json: boolean,
  deps: StaticCybshDeps,
): Promise<VerbOut> {
  const lazyBackup = await import('./backup')
  const sub = args[0] ?? 'export'
  const { flags, rest } = parseBackupFlags(args.slice(1))
  const cwd = await deps.getCwd().catch(() => '/')
  switch (sub) {
    case 'export': {
      if (!deps.exportDatabase || !deps.backupPassphrase) {
        return shellErr('unsupported: backup needs the database snapshot op on this build')
      }
      const dest = flags.to ?? rest[0] ?? ''
      if (!dest) return shellErr('usage: backup export --to /providers/<mount>[/dir] [--name F] [--keep 7]')
      const ep = resolveEndpoint(cwd, dest)
      if (ep.kind !== 'provider') {
        return shellErr(
          'invalid: backup export needs --to /providers/<mount>[/dir] (the browser volume is text-only — pick a provider destination)',
        )
      }
      const { mountId } = await resolveBackupMount(ep.mountId, deps)
      const passphrase = flags.plain ? '' : await deps.backupPassphrase()
      if (!flags.plain && !passphrase) {
        return shellErr('auth: unlock the vault first (or pass --plain to record a plaintext export)')
      }
      const sealed = await deps.exportDatabase(passphrase)
      const file = flags.name?.trim() || lazyBackup.buildBackupName()
      if (file.includes('/') || file.includes('\0')) {
        return shellErr('invalid: backup file name must be a plain file name')
      }
      const remote = ep.remotePath ? `${ep.remotePath}/${file}` : file
      await deps.providerWrite(mountId, remote, sealed)
      const keep = flags.keep ?? lazyBackup.DEFAULT_BACKUP_KEEP
      const pruned: string[] = []
      if (keep > 0) {
        const dir = ep.remotePath
        const entries = await deps.providerList(mountId, dir).catch(() => null)
        if (entries) {
          const victims = lazyBackup.pruneVictims(
            entries.filter((e) => !e.isDir && lazyBackup.isBackupName(e.name) && e.name !== file).map((e) => e.name),
            Math.max(0, keep - 1),
          )
          for (const v of victims) {
            const victimRemote = dir ? `${dir}/${v}` : v
            try {
              await deps.providerDelete(mountId, victimRemote)
              pruned.push(v)
            } catch {
              // A failed prune never fails the export that just landed.
            }
          }
        }
      }
      const path = `/providers/${ep.mountId}/${remote}`
      const digest = await backupFingerprint(deps, sealed)
      const encrypted = passphrase.length > 0
      if (json) {
        return shellOk(JSON.stringify({ path, bytes: sealed.length, blake3: digest, encrypted, pruned }))
      }
      return shellOk(
        `backup: exported ${sealed.length} bytes → ${path} (blake3 ${digest})${encrypted ? ' [sealed]' : ' [PLAINTEXT — --plain]'}${pruned.length ? ` · pruned ${pruned.join(', ')}` : ''}`,
      )
    }
    case 'ls':
    case 'list': {
      const dest = flags.to ?? rest[0] ?? ''
      if (!dest) return shellErr('usage: backup list /providers/<mount>[/dir]')
      const ep = resolveEndpoint(cwd, dest)
      if (ep.kind !== 'provider') return shellErr('invalid: backup list needs a /providers/… destination')
      const { mountId } = await resolveBackupMount(ep.mountId, deps)
      const entries = await deps.providerList(mountId, ep.remotePath)
      const rows = entries
        .filter((e) => !e.isDir && lazyBackup.isBackupName(e.name))
        .sort((a, b) => (a.name < b.name ? 1 : -1))
      if (json) {
        return shellOk(JSON.stringify(rows.map((e) => ({ name: e.name, bytes: e.sizeBytes }))))
      }
      if (!rows.length) return shellOk(`no snapshots in ${dest}`)
      let out = `${'NAME'.padEnd(44)} ${'BYTES'.padStart(12)}`
      for (const e of rows) out += `\n${e.name.padEnd(44)} ${String(e.sizeBytes).padStart(12)}`
      return shellOk(out)
    }
    case 'verify': {
      const target = flags.to ?? rest[0] ?? ''
      if (!target) return shellErr('usage: backup verify /providers/<mount>[/dir]/<file>')
      const ep = resolveEndpoint(cwd, target)
      if (ep.kind !== 'provider' || !ep.remotePath) {
        return shellErr('invalid: backup verify needs a /providers/<mount>/… file path')
      }
      const { mountId } = await resolveBackupMount(ep.mountId, deps)
      const bytes = await deps.providerRead(mountId, ep.remotePath)
      const header = lazyBackup.verifyContainerHeader(bytes)
      const digest = await backupFingerprint(deps, bytes)
      const path = `/providers/${ep.mountId}/${ep.remotePath}`
      if (json) {
        return shellOk(
          JSON.stringify({
            path,
            legacy: header.legacy,
            version: header.version,
            encrypted: header.encrypted,
            compressed: header.compressed,
            body_bytes: header.bodyBytes,
            total_bytes: header.totalBytes,
            blake3: digest,
            image_bytes: null,
          }),
        )
      }
      if (header.legacy) {
        return shellOk(`backup: ${path} legacy raw image (${header.totalBytes} bytes, blake3 ${digest})`)
      }
      return shellOk(
        `backup: ${path} CYBMJ01 v${header.version} ${header.encrypted ? 'sealed' : 'plaintext'}${header.compressed ? '+lz4' : ''} (${header.totalBytes} bytes, blake3 ${digest}) · header only (trial-decrypt runs at restore)`,
      )
    }
    default:
      return shellErr('usage: backup export|list|verify [--to …] [--json]')
  }
}

async function cronList(deps: StaticCybshDeps): Promise<StaticSchedule[]> {
  if (!deps.listSchedules) throw new Error('unsupported: cron needs the local database')
  return deps.listSchedules()
}

/**
 * Fire one schedule now: run its script through `handleRun`, record the run
 * and stamp the row. `cron run <id>`, the store's browser tick and
 * `invoke('cron_run')` on Pages all land here — one execution path.
 */
export async function runStaticSchedule(
  id: string,
  deps: StaticCybshDeps,
): Promise<StaticScheduleRun> {
  const rows = await cronList(deps)
  const row = rows.find(r => r.id === id)
  if (!row) throw new Error(`not_found: no schedule \`${id}\``)
  if (!deps.saveSchedule || !deps.recordScheduleRun) {
    throw new Error('unsupported: cron needs the local database')
  }
  const startedAt = new Date().toISOString()
  let status = 'ok'
  let tail = ''
  try {
    const out = await handleRun([row.path], false, deps)
    status = out.ok ? 'ok' : 'error'
    tail = out.text.trim().slice(0, CRON_TAIL_LIMIT)
  } catch (e) {
    status = 'error'
    tail = (e instanceof Error ? e.message : String(e)).slice(0, CRON_TAIL_LIMIT)
  }
  const now = new Date()
  const run: StaticScheduleRun = {
    runId: schedId(),
    scheduleId: row.id,
    startedAt,
    finishedAt: now.toISOString(),
    status,
    outputTail: tail || null,
  }
  const next = nextFire(row.expr, now)
  await deps.saveSchedule({
    ...row,
    lastFiredAt: startedAt,
    runOnBoot: false,
    nextFireAt: next ? next.toISOString() : null,
    lastRunId: run.runId,
  })
  await deps.recordScheduleRun(run)
  return run
}

async function handleCron(
  args: string[],
  json: boolean,
  deps: StaticCybshDeps,
): Promise<VerbOut> {
  const sub = args[0] ?? 'ls'
  const rest = args.slice(1)
  switch (sub) {
    case 'ls':
    case 'list': {
      const rows = await cronList(deps)
      if (json) return shellOk(JSON.stringify(rows))
      if (!rows.length) return shellOk('no schedules — `cron add <path.cybsh> [expr]`')
      let out = `${cronPad('ID', 22)} ${cronPad('EXPR', 14)} ${cronPad('NEXT', 22)} ${cronPad('ENABLED', 8)} PATH`
      for (const r of rows) {
        out += `\n${cronPad(r.id, 22)} ${cronPad(cronTruncate(r.expr, 14), 14)} ${cronPad(r.nextFireAt ?? '-', 22)} ${cronPad(r.enabled ? 'on' : 'off', 8)} ${r.path}`
      }
      return shellOk(out)
    }
    case 'add': {
      const path = rest[0]
      if (!path) return shellErr('usage: cron add <path.cybsh> [expr]')
      if (!path.toLowerCase().endsWith(CYBSH_SCRIPT_EXT)) {
        return shellErr(
          `invalid: schedule path must be a ${CYBSH_SCRIPT_EXT} script (got \`${path}\`)`,
        )
      }
      if (!deps.saveSchedule) return shellErr('unsupported: cron needs the local database')
      let expr = rest[1]
      if (!expr) {
        // No expr → the script's own `# schedule:` frontmatter.
        const cwd = await deps.getCwd().catch(() => '/')
        const abs = joinVolumePath(cwd, path)
        const source = deps.readVolume()[abs]
        if (source === undefined) return shellErr(`not_found: ${abs}`)
        expr = parseFrontmatter(source).schedule ?? ''
        if (!expr) {
          return shellErr(
            `invalid: \`${path}\` declares no \`# schedule:\` — pass an expr: cron add ${path} "30 2 * * *"`,
          )
        }
      }
      try {
        parseSchedule(expr)
      } catch (e) {
        return shellErr(e instanceof Error ? e.message : String(e))
      }
      const now = new Date()
      const next = nextFire(expr, now)
      const saved = await deps.saveSchedule({
        id: schedId(),
        path,
        expr,
        enabled: true,
        createdAt: now.toISOString(),
        nextFireAt: next ? next.toISOString() : null,
        runOnBoot: false,
      })
      if (json) return shellOk(JSON.stringify(saved))
      return shellOk(`cron: added ${saved.id} → ${saved.expr} (next ${saved.nextFireAt ?? '-'})`)
    }
    case 'rm':
    case 'remove':
    case 'delete': {
      const id = rest[0]
      if (!id) return shellErr('usage: cron rm <id>')
      if (!deps.deleteSchedule) return shellErr('unsupported: cron needs the local database')
      const removed = await deps.deleteSchedule(id)
      if (!removed) return shellErr(`not_found: no schedule \`${id}\``)
      if (json) return shellOk(JSON.stringify({ removed: id }))
      return shellOk(`cron: removed ${id}`)
    }
    case 'run': {
      const id = rest[0]
      if (!id) return shellErr('usage: cron run <id>')
      const run = await runStaticSchedule(id, deps)
      if (json) return shellOk(JSON.stringify(run))
      return shellOk(
        `cron: ${run.scheduleId} ${run.status} (${run.finishedAt})${run.outputTail ? `\n${run.outputTail}` : ''}`,
      )
    }
    case 'enable':
    case 'disable': {
      const id = rest[0]
      if (!id) return shellErr(`usage: cron ${sub} <id>`)
      const rows = await cronList(deps)
      const row = rows.find(r => r.id === id)
      if (!row) return shellErr(`not_found: no schedule \`${id}\``)
      if (!deps.saveSchedule) return shellErr('unsupported: cron needs the local database')
      const enabled = sub === 'enable'
      let nextFireAt = row.nextFireAt ?? null
      if (enabled) {
        try {
          parseSchedule(row.expr)
        } catch (e) {
          return shellErr(e instanceof Error ? e.message : String(e))
        }
        const next = nextFire(row.expr, new Date())
        nextFireAt = next ? next.toISOString() : null
      }
      const saved = await deps.saveSchedule({ ...row, enabled, nextFireAt })
      if (json) return shellOk(JSON.stringify(saved))
      return shellOk(
        `cron: ${id} ${saved.enabled ? 'enabled' : 'disabled'} (next ${saved.nextFireAt ?? '-'})`,
      )
    }
    case 'history': {
      const id = rest[0]
      if (!id) return shellErr('usage: cron history <id>')
      if (!deps.scheduleHistory) return shellErr('unsupported: cron needs the local database')
      const runs = await deps.scheduleHistory(id)
      if (json) return shellOk(JSON.stringify(runs))
      if (!runs.length) return shellOk(`cron: no runs recorded for ${id}`)
      let out = `${cronPad('RUN', 26)} ${cronPad('STATUS', 8)} FINISHED`
      for (const r of runs) {
        out += `\n${cronPad(r.runId, 26)} ${cronPad(r.status, 8)} ${r.finishedAt}`
      }
      return shellOk(out)
    }
    default: {
      const suggestion = CRON_SUBCOMMANDS.find(c => c.startsWith(sub) || sub.startsWith(c))
      return shellErr(
        suggestion
          ? `unknown cron subcommand: '${sub}' — did you mean '${suggestion}'?`
          : `unknown cron subcommand: '${sub}'`,
      )
    }
  }
}

const SKILL_SUBCOMMANDS = ['list', 'show', 'run', 'install', 'panel'] as const

/** Skills as shareable apps, day one (ANALYSIS.md §4.8): `.cybermanju/skills/*`
 * folders carried by sync; SKILL.md is the manifest, `run` names the entry
 * `.cybsh`. `install <url>` fetches a SKILL.md (+ its run script) into place. */
async function handleSkill(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'list').toLowerCase()
  const rest = args.slice(1)
  const volume = deps.readVolume()
  const skillFiles = Object.entries(volume)
    .filter(([path]) => path.includes('.cybermanju/skills/'))
    .map(([path, content]) => ({ path, content }))
  if (sub === 'list') {
    const skills = listSkills(skillFiles)
    if (json) return shellOk(JSON.stringify(skills))
    if (!skills.length) {
      return shellOk('skill: no skills installed under .cybermanju/skills/ — `skill install <url>` copies one in')
    }
    return shellOk(skills.map((s) => `${s.slug} ${s.version} — ${s.description}`).join('\n'))
  }
  if (sub === 'show') {
    const slug = rest[0]
    if (!slug) return shellErr('usage: skill show <slug>')
    const skills = listSkills(skillFiles)
    const found = skills.find((s) => s.slug === slug)
    if (!found) return shellErr(`not_found: no skill '${slug}'`)
    const problems = validateSkill(
      found,
      skillFiles.map((f) => f.path),
    )
    if (json) return shellOk(JSON.stringify({ ...found, problems }))
    let out = `${found.name} ${found.version}\n${found.description}\nrun: ${found.run}`
    if (problems.length) out += `\nproblems:\n- ${problems.join('\n- ')}`
    return shellOk(out)
  }
  if (sub === 'run') {
    const slug = rest[0]
    if (!slug) return shellErr('usage: skill run <slug>')
    const skills = listSkills(skillFiles)
    const found = skills.find((s) => s.slug === slug)
    if (!found) return shellErr(`not_found: no skill '${slug}'`)
    if (!deps.execFallback) {
      return shellErr('unsupported: skill run needs the wasm shell fallback — run the same line in the terminal')
    }
    const dir = found.path.split('/').slice(0, -1).join('/')
    const scriptPath = `${dir}/${found.run}`
    const script = volume[scriptPath]
    if (typeof script !== 'string') return shellErr(`not_found: missing run script ${scriptPath}`)
    const lines = script.split(/\r?\n/).map((l) => l.trim()).filter((l) => l && !l.startsWith('#'))
    if (!lines.length) return shellErr(`invalid: ${scriptPath} has no runnable lines`)
    const out: string[] = []
    for (const skillLine of lines) {
      try {
        out.push(await deps.execFallback(skillLine))
      } catch (e) {
        return shellErr(`skill '${slug}' stopped at \`${skillLine}\`: ${e instanceof Error ? e.message : String(e)}`)
      }
    }
    if (json) return shellOk(JSON.stringify({ slug, lines: out }))
    return shellOk(out.join('\n'))
  }
  if (sub === 'install') {    const url = rest[0]
    if (!url || !/^https?:\/\//i.test(url)) return shellErr('usage: skill install <https-url-to-SKILL.md>')
    if (typeof fetch === 'undefined') {
      return shellErr('unsupported: skill install has no HTTP client in this context — copy the folder into .cybermanju/skills/ by hand')
    }
    let text: string
    try {
      const res = await fetch(url)
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      text = await res.text()
    } catch (e) {
      return shellErr(`network: skill install fetch failed (${e instanceof Error ? e.message : String(e)})`)
    }
    const manifest = parseSkillMd('.cybermanju/skills/install/SKILL.md', text)
    if (!manifest) return shellErr('invalid: fetched SKILL.md has no usable name/description')
    const base = url.split('/').slice(0, -1).join('/')
    const dir = `.cybermanju/skills/${manifest.slug}`
    await deps.writeVolumeFile(`${dir}/SKILL.md`, text)
    // Best-effort sibling fetch for the run script (same directory as the URL).
    if (manifest.run && base) {
      try {
        const res = await fetch(`${base}/${manifest.run}`)
        if (res.ok) await deps.writeVolumeFile(`${dir}/${manifest.run}`, await res.text())
      } catch {
        /* manifest without its script still lists; `skill show` reports it */
      }
    }
    const problems = validateSkill(manifest, [...skillFiles.map((f) => f.path), `${dir}/SKILL.md`])
    if (json) return shellOk(JSON.stringify({ slug: manifest.slug, problems }))
    let out = `skill: installed '${manifest.slug}' under ${dir}/`
    if (problems.length) out += `\nproblems:\n- ${problems.join('\n- ')}`
    return shellOk(out)
  }
  if (sub === 'panel') {
    const slug = rest[0]
    if (!slug) return shellErr('usage: skill panel <slug>')
    const skills = listSkills(skillFiles)
    const found = skills.find((s) => s.slug === slug)
    if (!found) return shellErr(`not_found: no skill '${slug}'`)
    const targets = skillPanels(skills)
    const target = targets.find((t) => t.slug === slug)
    if (!target) return shellErr(`not_found: skill '${slug}' ships no panel (add \`panel: <id>\` to its SKILL.md frontmatter)`)
    if (json) return shellOk(JSON.stringify(target))
    const tab = target.tab ? ` tab ${target.tab}` : ''
    return shellOk(`skill:${slug} → open panel '${target.panel}'${tab} — run \`ui open ${target.panel}\` or click the skill card`)
  }
  const suggestion = SKILL_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: skill <${SKILL_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: skill <${SKILL_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const OPLOG_SUBCOMMANDS = ['plan', 'conflicts', 'resolve'] as const

/** Op-log exchange from the shell (ANALYSIS.md §2C): `plan` prints the
 * pull→push→render steps (works offline); `conflicts`/`resolve` need the
 * dashboard, so without a transport fallback they answer `unsupported:`
 * with the exact REST calls instead of guessing. */
async function handleOplog(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'plan').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'plan') {
    const sinceRaw = rest.find((a) => !a.startsWith('-')) ?? '0'
    const since = Math.max(0, parseInt(sinceRaw, 10) || 0)
    const steps = syncPlanSteps(since)
    if (json) return shellOk(JSON.stringify({ since, steps }))
    return shellOk(steps.join('\n'))
  }
  if (sub === 'conflicts') {
    if (!deps.execFallback) {
      return shellErr('unsupported: `oplog conflicts` needs the dashboard — GET /api/oplog/conflicts (Sync → Conflicts shows the same pairs)')
    }
    try {
      return shellOk(await deps.execFallback('oplog conflicts'))
    } catch (e) {
      return shellErr(`unsupported: oplog conflicts via fallback failed (${e instanceof Error ? e.message : String(e)})`)
    }
  }
  if (sub === 'resolve') {
    const [opA, opB, choiceRaw] = rest.filter((a) => !a.startsWith('-'))
    if (!opA || !opB) return shellErr('usage: oplog resolve <op-a> <op-b> <keep-a|keep-b|merge>')
    const choice = (['keep-a', 'keep-b', 'merge'].includes(choiceRaw ?? '') ? choiceRaw : 'merge') as ResolutionChoice
    const payload = resolutionPayload(
      { a: { seq: 0, opId: opA, deviceId: '', createdAt: '', kind: '' }, b: { seq: 0, opId: opB, deviceId: '', createdAt: '', kind: '' } },
      choice,
      '',
    )
    if (json) return shellOk(JSON.stringify(payload))
    return shellOk(`resolve ${opA} × ${opB} as ${choice} — POST /api/oplog/resolve ${JSON.stringify(payload)} (Sync → Conflicts resolves with one click)`)
  }
  const suggestion = OPLOG_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: oplog <${OPLOG_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: oplog <${OPLOG_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const LAN_SUBCOMMANDS = ['plan', 'peers', 'sync'] as const

/** Direct LAN sync from the shell (ANALYSIS.md §2B.2 follow-up): `plan`
 * prints the pull→push→merge→render steps for a peer; `peers`/`sync` need
 * the dashboard transport, so they forward via the fallback or refuse
 * honestly with the exact REST calls. */
async function handleLan(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'plan').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'plan') {
    const rawTarget = rest.find((a) => !a.startsWith('-')) ?? '127.0.0.1:3456'
    // Accept `host:port`, `[v6]:port`, bare hosts, and `device@addr` (WAN grammar).
    let deviceId = 'peer'
    let target = rawTarget
    const at = rawTarget.indexOf('@')
    if (at >= 0 && rawTarget.slice(0, at).trim() && rawTarget.slice(at + 1).trim()) {
      deviceId = rawTarget.slice(0, at).trim()
      target = rawTarget.slice(at + 1).trim()
    } else if (rest.filter((a) => !a.startsWith('-'))[1]) {
      deviceId = rest.filter((a) => !a.startsWith('-'))[1]
    }
    let host = '127.0.0.1'
    let port = 3456
    const bracketed = target.match(/^\[([^\]]+)\](?::(\d+))?$/)
    if (bracketed) {
      host = bracketed[1] || host
      port = parseInt(bracketed[2] ?? '', 10) || 3456
    } else if (target.startsWith('[')) {
      host = target
    } else {
      const lastColon = target.lastIndexOf(':')
      // One colon = host:port; more = bare IPv6 (no port).
      if (lastColon >= 0 && target.indexOf(':') === lastColon) {
        host = target.slice(0, lastColon).trim() || host
        port = parseInt(target.slice(lastColon + 1), 10) || 3456
      } else if (lastColon < 0) {
        host = target.trim() || host
      } else {
        host = target.trim()
      }
    }
    const peer = { deviceId, name: deviceId, host, port }
    const steps = lanSyncSteps(peer, 0)
    if (json) return shellOk(JSON.stringify({ base: peerBaseUrl(peer), steps }))
    return shellOk([`peer: ${peerBaseUrl(peer)}`, ...steps].join('\n'))
  }
  if (sub === 'peers' || sub === 'sync') {
    if (!deps.execFallback) {
      const hint = sub === 'peers'
        ? 'GET /api/peers (Devices → LAN peers shows the same scan)'
        : 'POST /api/peers/{id}/sync (Devices → LAN peers → SYNC does the authenticated exchange)'
      return shellErr(`unsupported: \`lan ${sub}\` needs the dashboard — ${hint}`)
    }
    try {
      return shellOk(await deps.execFallback(`lan ${rest.join(' ')}`.trim()))
    } catch (e) {
      return shellErr(`unsupported: lan ${sub} via fallback failed (${e instanceof Error ? e.message : String(e)})`)
    }
  }
  const suggestion = LAN_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: lan <${LAN_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: lan <${LAN_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const WAN_SUBCOMMANDS = ['plan', 'dial'] as const

/** WAN dial seam from the shell (ANALYSIS.md §2B.5): `wan plan
 * <device>@<addr>` prints the LIVE dial plan (direct HTTP, or the relay
 * envelope when `--relay <url>` is set and the addr is a relay ticket —
 * `unsupported:` + LAN/bundle way out only with no relay configured);
 * `wan dial <device>@<addr> [since]` prints the REAL dial steps (readyz
 * probe + op-log pull/push/merge, same as Devices → WAN) — works offline on
 * every transport, never dials from the static shell, never guesses. */
async function handleWan(args: string[], json: boolean): Promise<VerbOut> {
  const sub = (args[0] ?? 'plan').toLowerCase()
  const rest = args.slice(1)
  const relayFlag = rest.find((a, i) => a === '--relay' && rest[i + 1]) as string | undefined
  const relayIdx = rest.indexOf('--relay')
  const relayUrl = relayIdx >= 0 ? (rest[relayIdx + 1] ?? '') : ''
  void relayFlag
  if (sub === 'plan' || sub === 'dial') {
    const raw = rest.find((a) => !a.startsWith('-'))
    if (!raw) return shellErr(`usage: wan ${sub} <device-id>@<addr> [since] [--relay <url>]`)
    let endpoint
    try {
      endpoint = parseWanEndpoint(raw)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
    if (sub === 'dial') {
      const sinceRaw = rest.filter((a) => !a.startsWith('-'))[1]
      const since = sinceRaw == null ? 0 : Math.max(0, Math.floor(Number(sinceRaw) || 0))
      try {
        const steps = isRelayAddr(endpoint.addr)
          ? relayDialSteps(endpoint, relayUrl, since)
          : wanDialSteps(endpoint, since)
        if (json) return shellOk(JSON.stringify({ endpoint, steps }))
        return shellOk(steps.join('\n'))
      } catch (e) {
        return shellErr(e instanceof Error ? e.message : String(e))
      }
    }
    let plan: string
    try {
      plan = wanDialPlan(endpoint, 15, relayUrl)
    } catch (e) {
      plan = e instanceof Error ? e.message : String(e)
    }
    const steps = (() => {
      try {
        return isRelayAddr(endpoint.addr) ? relayDialSteps(endpoint, relayUrl, 0) : wanDialSteps(endpoint, 0)
      } catch {
        return [] as string[]
      }
    })()
    const wayOut = plan.startsWith('unsupported:') ? wanWayOut(endpoint) : []
    if (json) return shellOk(JSON.stringify({ endpoint, plan, steps, wayOut }))
    return shellOk([plan, ...steps, ...wayOut].join('\n'))
  }
  const suggestion = WAN_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: wan <${WAN_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: wan <${WAN_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const PEX_SUBCOMMANDS = ['announce', 'known'] as const

/** Server-assisted presence from the shell: `pex announce
 * <device> <host> [port]` + `pex known` print the exact REST calls (they run
 * where the dashboard is reachable — Devices → Presence does the clicks).
 * Offline-safe: `announce` validates + prints the body without sending. */
async function handlePex(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'known').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'announce') {
    const [deviceId, host, portRaw] = rest.filter((a) => !a.startsWith('-'))
    if (!deviceId || !host) return shellErr('usage: pex announce <device-id> <host> [port]')
    const port = portRaw == null ? 3456 : Math.floor(Number(portRaw) || 0)
    try {
      const { buildAnnounceBody } = await import('./p2pPresence')
      const body = buildAnnounceBody(deviceId, host, port)
      if (!deps.execFallback) {
        const line = `POST /api/peers/announce ${JSON.stringify(body)} (Devices → Presence announces where the dashboard is reachable)`
        if (json) return shellOk(JSON.stringify({ body }))
        return shellOk(line)
      }
      return shellOk(await deps.execFallback(`pex announce ${deviceId} ${host} ${port}`.trim()))
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (sub === 'known') {
    if (!deps.execFallback) {
      if (json) return shellOk(JSON.stringify({ url: 'GET /api/peers/known' }))
      return shellOk('GET /api/peers/known (Devices → Presence shows the same heartbeats)')
    }
    try {
      return shellOk(await deps.execFallback('pex known'))
    } catch (e) {
      return shellErr(`unsupported: pex known via fallback failed (${e instanceof Error ? e.message : String(e)})`)
    }
  }
  const suggestion = PEX_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: pex <${PEX_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: pex <${PEX_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const RELAY_SUBCOMMANDS = ['drop', 'poll', 'ack'] as const

/** Relay mailbox from the shell: `relay drop <to> <kind> [json]` builds the
 * envelope offline; `poll`/`ack` need the dashboard (fallback or the exact
 * REST calls). Never dials from the static shell. */
async function handleRelay(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'drop').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'drop') {
    const [to, kind, payloadRaw] = rest.filter((a) => !a.startsWith('-'))
    const from = rest.find((a) => a.startsWith('--from='))?.slice('--from='.length) ?? 'here'
    if (!to || !kind) return shellErr('usage: relay drop <to-device> <kind> [payload-json] [--from=<device>]')
    let payload: unknown = null
    if (payloadRaw) {
      try {
        payload = JSON.parse(payloadRaw)
      } catch {
        payload = payloadRaw
      }
    }
    try {
      const { buildRelayEnvelope } = await import('./wanDial')
      const envelope = buildRelayEnvelope(to, kind, payload)
      void from
      if (json) return shellOk(JSON.stringify({ envelope: JSON.parse(envelope) }))
      return shellOk(`POST /api/relay/drop ${envelope}`)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (sub === 'poll' || sub === 'ack') {
    if (!deps.execFallback) {
      const hint = sub === 'poll'
        ? 'POST /api/relay/poll { for: <device-id> } (non-destructive — ack clears)'
        : 'POST /api/relay/ack { for: <device-id>, ids: [...] }'
      return shellErr(`unsupported: \`relay ${sub}\` needs the dashboard — ${hint}`)
    }
    try {
      return shellOk(await deps.execFallback(`relay ${rest.join(' ')}`.trim()))
    } catch (e) {
      return shellErr(`unsupported: relay ${sub} via fallback failed (${e instanceof Error ? e.message : String(e)})`)
    }
  }
  const suggestion = RELAY_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: relay <${RELAY_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: relay <${RELAY_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const PAIR_SUBCOMMANDS = ['make', 'drop', 'claim'] as const

/** Human pairing codes from the shell: `pair make` mints a code offline
 * (WebCrypto entropy); `pair drop <code> <kind> [json]` builds the mailbox
 * envelope offline; `pair claim <code>` needs the dashboard (Devices →
 * pair card polls the same mailbox). */
async function handlePair(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  const sub = (args[0] ?? 'make').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'make') {
    try {
      const { makePairingCode, pairingTarget } = await import('./pairing')
      const code = makePairingCode()
      if (json) return shellOk(JSON.stringify({ code, target: pairingTarget(code) }))
      return shellOk(`code: ${code} (read it aloud; the other side runs \`pair claim ${code}\`)`)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (sub === 'drop') {
    const [code, kind, payloadRaw] = rest.filter((a) => !a.startsWith('-'))
    const from = rest.find((a) => a.startsWith('--from='))?.slice('--from='.length) ?? 'here'
    if (!code || !kind) return shellErr('usage: pair drop <code> <kind> [payload-json] [--from=<device>]')
    let payload: unknown = null
    if (payloadRaw) {
      try {
        payload = JSON.parse(payloadRaw)
      } catch {
        payload = payloadRaw
      }
    }
    try {
      const { pairingTarget } = await import('./pairing')
      const { buildRelayEnvelope } = await import('./wanDial')
      const target = pairingTarget(code)
      const envelope = buildRelayEnvelope(target, kind, payload)
      void from
      if (json) return shellOk(JSON.stringify({ target, envelope: JSON.parse(envelope) }))
      return shellOk(`POST /api/relay/drop ${envelope}`)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (sub === 'claim') {
    if (!deps.execFallback) {
      return shellErr('unsupported: `pair claim` needs the dashboard — POST /api/relay/poll { for: "pair:<code>" } then ack (Devices → pair card does the clicks)')
    }
    try {
      return shellOk(await deps.execFallback(`pair ${rest.join(' ')}`.trim()))
    } catch (e) {
      return shellErr(`unsupported: pair claim via fallback failed (${e instanceof Error ? e.message : String(e)})`)
    }
  }
  const suggestion = PAIR_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: pair <${PAIR_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: pair <${PAIR_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const SWARM_SUBCOMMANDS = ['plan', 'fountain'] as const

/** Erasure-coded swarm planner: `swarm plan <bytes> [k] [m]` prints the RS
 * layout + spray/collect/decode steps offline (pure math, no codec needed);
 * `swarm fountain <bytes> [mtu] [repair]` plans the RaptorQ spray instead.
 * The actual coding runs natively (`swarm_*`/`fountain_*` IPC) or on Pages
 * (wasm `*_json` exports); bulk sends ride `swarm-symbol`/`fountain-symbol`
 * relay messages. */
async function handleSwarm(args: string[], json: boolean): Promise<VerbOut> {
  const sub = (args[0] ?? 'plan').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'plan') {
    const [bytesRaw, kRaw, mRaw] = rest.filter((a) => !a.startsWith('-'))
    if (bytesRaw == null) return shellErr('usage: swarm plan <bytes> [k] [m]')
    try {
      const { buildSwarmPlan } = await import('./p2pSwarm')
      const steps = buildSwarmPlan(
        Number(bytesRaw),
        kRaw == null ? undefined : Math.floor(Number(kRaw)),
        mRaw == null ? undefined : Math.floor(Number(mRaw)),
      )
      if (json) return shellOk(JSON.stringify({ steps }))
      return shellOk(steps.join('\n'))
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  if (sub === 'fountain') {
    const [bytesRaw, mtuRaw, repairRaw] = rest.filter((a) => !a.startsWith('-'))
    if (bytesRaw == null) return shellErr('usage: swarm fountain <bytes> [mtu] [repair]')
    try {
      const { buildFountainPlan } = await import('./p2pSwarm')
      const steps = buildFountainPlan(
        Number(bytesRaw),
        mtuRaw == null ? 32768 : Math.floor(Number(mtuRaw)),
        repairRaw == null ? 64 : Math.floor(Number(repairRaw)),
      )
      if (json) return shellOk(JSON.stringify({ steps }))
      return shellOk(steps.join('\n'))
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  const suggestion = SWARM_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: swarm <${SWARM_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: swarm <${SWARM_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const ROOM_SUBCOMMANDS = ['join', 'say'] as const

/** Relay rooms from the shell: `room join <app> <name>` + `room say
 * <app> <name> <action> [json]` print the exact mailbox calls (sending
 * needs the dashboard or the Devices → room demo; reading lives there too).
 * Offline-safe: both subcommands validate + print without sending. */
async function handleRoom(args: string[], json: boolean, deps: StaticCybshDeps): Promise<VerbOut> {
  void deps
  const sub = (args[0] ?? 'join').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'join' || sub === 'say') {
    const [app, name, action, bodyRaw] = rest.filter((a) => !a.startsWith('-'))
    if (!app || !name || (sub === 'say' && !action)) {
      return shellErr(`usage: room ${sub} <app> <room>${sub === 'say' ? ' <action> [body-json]' : ''}`)
    }
    try {
      const { roomTopic } = await import('./p2pRoom')
      const topic = roomTopic(app, name)
      if (sub === 'join') {
        const line = `room ${topic} (POST /api/relay/poll { for: "${topic}" } reads; hello via kind room-hello)`
        if (json) return shellOk(JSON.stringify({ topic }))
        return shellOk(line)
      }
      let body: unknown = null
      if (bodyRaw) {
        try {
          body = JSON.parse(bodyRaw)
        } catch {
          body = bodyRaw
        }
      }
      const line = `POST /api/relay/drop { to: "${topic}", kind: room-action, payload: ${JSON.stringify({ action, body })} }`
      if (json) return shellOk(JSON.stringify({ topic, action, body }))
      return shellOk(line)
    } catch (e) {
      return shellErr(e instanceof Error ? e.message : String(e))
    }
  }
  const suggestion = ROOM_SUBCOMMANDS.find((c) => c.startsWith(sub) || sub.startsWith(c))
  return shellErr(
    suggestion
      ? `usage: room <${ROOM_SUBCOMMANDS.join('|')}> (did you mean \`${suggestion}\`?)`
      : `usage: room <${ROOM_SUBCOMMANDS.join('|')}> (got \`${sub}\`)`,
  )
}

const TODOS_KEY = 'cybermanju.todos.v1'

function readTaskList(): ReturnType<typeof parseTodosStore> {
  try {
    if (typeof localStorage === 'undefined') return []
    return parseTodosStore(localStorage.getItem(TODOS_KEY))
  } catch {
    return []
  }
}

function writeTaskList(list: ReturnType<typeof parseTodosStore>): void {
  try {
    if (typeof localStorage === 'undefined') return
    localStorage.setItem(TODOS_KEY, serializeTodosStore(list))
  } catch {
    /* private mode — session-only */
  }
}

/** Round-out daily use in the shell (ANALYSIS.md §4.9): todos share the
 * Archive TASKS store, so shell and panel stay one list. */
function handleTask(args: string[], json: boolean): VerbOut {
  const sub = (args[0] ?? 'list').toLowerCase()
  const rest = args.slice(1)
  if (sub === 'list') {
    const list = readTaskList()
    if (json) return shellOk(JSON.stringify(list))
    if (!list.length) return shellOk('task: no todos — `task add <title>`')
    return shellOk(
      list.map((item) => `${item.done ? '[x]' : '[ ]'} ${item.id} ${item.title}`).join('\n'),
    )
  }
  if (sub === 'add') {
    const title = rest.join(' ').trim()
    const created = addTodo(readTaskList(), title)
    if (!created) return shellErr('usage: task add <title>')
    writeTaskList(created.list)
    if (json) return shellOk(JSON.stringify(created.item))
    return shellOk(`task: added ${created.item.id}`)
  }
  if (sub === 'done' || sub === 'undone') {
    const prefix = rest[0]
    if (!prefix) return shellErr(`usage: task ${sub} <id-prefix>`)
    const list = readTaskList()
    const found = list.find((item) => item.id.startsWith(prefix))
    if (!found) return shellErr(`not_found: no todo starting with '${prefix}'`)
    const wantDone = sub === 'done'
    const next = wantDone === found.done ? list : toggleTodo(list, found.id)
    writeTaskList(next)
    return shellOk(`task: ${found.id} ${wantDone ? 'done' : 'reopened'}`)
  }
  if (sub === 'rm') {
    const prefix = rest[0]
    if (!prefix) return shellErr('usage: task rm <id-prefix>')
    const list = readTaskList()
    const found = list.find((item) => item.id.startsWith(prefix))
    if (!found) return shellErr(`not_found: no todo starting with '${prefix}'`)
    writeTaskList(removeTodo(list, found.id))
    return shellOk(`task: removed ${found.id}`)
  }
  if (sub === 'clear') {
    const { list, cleared } = clearDone(readTaskList())
    writeTaskList(list)
    return shellOk(`task: cleared ${cleared} done`)
  }
  return shellErr('usage: task <list|add|done|undone|rm|clear> (got `' + sub + '`)')
}

export async function runStaticCybshLine(
  line: string,
  deps: StaticCybshDeps,
): Promise<ShellResult | null> {
  const done = (out: VerbOut): ShellResult => ({
    ok: out.ok,
    line,
    output: out.text,
    ...(out.ok ? {} : { error: out.text }),
    prompt: 'cybsh> ',
  })
  const chain = splitStaticCybshChain(line)
  if (chain.hasPipe) {
    return done(shellErr('unsupported: pipelines are not available in the wasm sandbox'))
  }
  if (chain.separators.length > 0) {
    // Keep mixed-operator semantics with the Rust dispatcher until both paths
    // share a parser. Homogeneous `&&` and `;` chains can safely mix local and
    // volume commands here.
    if (new Set(chain.separators).size > 1) return null
    const commands = chain.commands.map((command) => command.trim()).filter(Boolean)
    const includesStaticVerb = commands.some((command) => {
      const commandParsed = parseCybshLine(command)
      return !!commandParsed && handlesStaticVerb(commandParsed.verb)
    })
    if (!includesStaticVerb) return null

    const output: string[] = []
    for (const command of commands) {
      let result: VerbOut
      try {
        const handled = await runStaticCybshLine(command, deps)
        if (handled) {
          result = { ok: handled.ok, text: handled.output }
        } else if (deps.execFallback) {
          try {
            result = shellOk(await deps.execFallback(command))
          } catch (e) {
            result = shellErr(e instanceof Error ? e.message : String(e))
          }
        } else {
          const commandParsed = parseCybshLine(command)
          const verb = commandParsed?.verb ?? command.split(/\s+/, 1)[0] ?? command
          result = shellErr(`unsupported: \`${verb}\` needs the WASM shell fallback in this static build`)
        }
      } catch (e) {
        result = shellErr(e instanceof Error ? e.message : String(e))
      }
      if (result.text) output.push(result.text)
      // Match the WASM runner: stop at the first unsuccessful command.
      if (!result.ok) return done(shellErr(output.join('\n')))
    }
    return done(shellOk(output.join('\n')))
  }

  let parsed: ParsedLine | null
  try {
    parsed = parseCybshLine(line)
  } catch {
    return null
  }
  if (!parsed || !handlesStaticVerb(parsed.verb)) return null
  try {
    const { verb, args, json } = parsed
    // Host filesystem (`-os`) only exists in the native shell (desktop app /
    // Android): this static build has no host paths to transpose onto.
    // Refuse loudly — the flag would otherwise be swallowed as an unknown
    // switch and the volume answer would masquerade as a host listing.
    // (Single-command lines only: chained `&&`/`;` lines return null above
    // and direct `wasm` dispatch calls never reach this layer — both are
    // refused by the wasm dispatcher's own `-os` guard with the same shape.
    // `.cybsh` `sh("… -os …")` lines land here via `scriptExecCybsh`, so the
    // wrapper agrees with the terminal on every transport.)
    if (args.some((a) => a === '-os' || a === '--host' || a === '--os')) {
      return done(
        shellErr(
          `unsupported: '${verb} -os' addresses the host filesystem (sdcard on Android) — run it in the desktop app or the native Android build`,
        ),
      )
    }
    switch (verb) {
      case 'echo':
        return done(shellOk(args.join(' ')))
      case 'ls': {
        const out = await handleLs(args, deps)
        // `null` = no provider operand — the wasm volume owns this listing.
        if (out === null) return null
        return done(out)
      }
      case 'cat': {
        const out = await handleCat(args, deps)
        // `null` = no provider operand — the wasm volume owns this read.
        if (out === null) return null
        return done(out)
      }
      case 'cp':
      case 'mv':
        return done(await handleFileOp(verb, args, deps))
      case 'rm':
        return done(await handleRm(args, deps))
      case 'mkdir':
        return done(await handleMkdir(args, deps))
      case 'grep':
        return done(await handleGrep(args, json, deps))
      case 'find':
        return done(await handleFind(args, json, deps))
      case 'head':
        return done(await handleHeadTail(true, args, deps))
      case 'tail':
        return done(await handleHeadTail(false, args, deps))
      case 'wc':
        return done(await handleWc(args, json, deps))
      case 'edit':
        return done(await handleEdit(args, deps))
      case 'kill': {
        const id = parseInt(args[0] ?? '', 10)
        if (!Number.isInteger(id)) return done(shellErr('usage: kill <id>'))
        try {
          const removed = await deps.killTask(id)
          return done(removed ? shellOk(`killed task ${id}`) : shellErr(`not_found: no task ${id}`))
        } catch (e) {
          return done(
            shellErr(
              `unsupported: kill needs the rebuilt wasm task table (${e instanceof Error ? e.message : String(e)}) — \`ps\` still lists tasks`,
            ),
          )
        }
      }
      case 'quota':
        return done(await handleQuota(args, json, deps))
      case 'providers':
        return done(await handleProviders(args, json, deps))
      case 'oauth':
        return done(await handleOauth(args, json, deps))
      case 'disk':
        return done(await handleDisk(args, json, deps))
      case 'sync':
        return done(await handleSync(args, json, deps))
      case 'mount':
        return done(await handleMount(args, json, deps))
      case 'umount':
        return done(
          shellErr('unsupported: `umount` detaches provider mounts managed by the dashboard — remove the mount from its provider card on this build'),
        )
      case 'encrypt':
        return done(await handleEncrypt(args, deps))
      case 'decrypt':
        return done(await handleDecrypt(args, deps))
      case 'keygen':
        return done(await handleKeygen(args, deps))
      case 'compress':
        return done(await handleCompress(args, deps, 'lz4'))
      case 'decompress':
        return done(await handleDecompress(args, deps))
      case 'scrub':
        return done(await handleScrub(args, deps))
      case 'repair':
        return done(await handleRepair(args, deps))
      case 'gc':
        return done(await handleGc(args, deps))
      case 'lease':
        return done(
          shellOk('no lease held for browser scope (this tab owns the local vault volume) · provider single-writer leases need the dashboard'),
        )
      case 'ai':
        return done(
          shellErr('unsupported: `ai ask` needs a detached worker — run it from the Agent panel or POST /api/os/exec on the dashboard; see docs/OPERATIONS.md'),
        )
      case 'run':
        return done(await handleRun(args, json, deps))
      case 'theme':
        return done(await handleTheme(args, json, deps))
      case 'ui':
        return done(await handleUi(args, json, deps))
      case 'cron':
        return done(await handleCron(args, json, deps))
      case 'backup':
        return done(await handleBackup(args, json, deps))
      case 'skill':
        return done(await handleSkill(args, json, deps))
      case 'oplog':
        return done(await handleOplog(args, json, deps))
      case 'lan':
        return done(await handleLan(args, json, deps))
      case 'wan':
        return done(await handleWan(args, json))
      case 'pex':
        return done(await handlePex(args, json, deps))
      case 'relay':
        return done(await handleRelay(args, json, deps))
      case 'pair':
        return done(await handlePair(args, json, deps))
      case 'swarm':
        return done(await handleSwarm(args, json))
      case 'room':
        return done(await handleRoom(args, json, deps))
      case 'task':
        return done(handleTask(args, json))
      default:
        return null
    }
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    return done(shellErr(detail))
  }
}
