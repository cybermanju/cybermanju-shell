// CyberManju OS — private vault repo provisioning (GitHub + GitLab).
//
// One flow turns a pasted PAT / OAuth token into a fully synced system:
//
//   create private repo → save provider config → seed README + manifest +
//   exported `vault.cybermanju` → verify → provision + attach system disk.
//
// Transports:
//   desktop / Docker  → `create_provider_repo` / `seed_repo_files` (Rust
//                       backends, same error-prefix contract as sync).
//   static Pages host → direct `fetch` to api.github.com / gitlab.com
//                       (both send CORS headers; tokens stay in the form).

import type { SyncBackendType, SyncConfig } from '@/types'

export interface CreatedRepo {
  backend: string
  repoName: string
  fullName: string
  branch: string
  url: string
  projectId?: string | null
}

export interface ProvisionInput {
  backendType: Extract<SyncBackendType, 'github' | 'gitlab'>
  /** Freshly typed PAT (or OAuth token). Never persisted here. */
  token: string
  /** `my-vault` or `owner/my-vault` — only the last segment is sent. */
  name: string
  /** Always true from the UI; kept explicit so tests pin the default. */
  privateRepo?: boolean
  description?: string
  branch?: string
  /** GitLab self-hosted instance (empty = gitlab.com). */
  instanceUrl?: string
  displayName?: string
}

/** Canonical vault layout inside every provisioned repo. */
export const VAULT_FILE_PATH = 'vault.cybermanju'
export const VAULT_README_PATH = 'README.md'
export const VAULT_MANIFEST_PATH = '.cybermanju.json'
export const DEFAULT_VAULT_REPO = 'cybermanju-vault'
export const DEFAULT_VAULT_BRANCH = 'main'
/** Upper bound for one repo-set provisioning run (provider rate limits). */
export const MAX_VAULT_REPOS = 8
/** Seed endpoint cap per file — mirrored vaults bigger than this seed
 *  README + manifest only (the live sync still carries the data). */
export const MAX_SEED_BYTES = 5 * 1024 * 1024

const SEGMENT_RE = /^[A-Za-z0-9._-]{1,100}$/

export function lastSegment(name: string): string {
  return String(name ?? '')
    .trim()
    .replace(/^\/+|\/+$/g, '')
    .split('/')
    .filter(Boolean)
    .pop() ?? ''
}

/** `my-vault` / `owner/my-vault` → error string, or `''` when valid. */
export function validateRepoName(name: string): string {
  const seg = lastSegment(name)
  if (!seg) return 'Give the repo a name — e.g. cybermanju-vault.'
  if (seg === '.' || seg === '..') return 'That name is reserved — pick another.'
  if (!SEGMENT_RE.test(seg)) {
    return 'Letters, numbers, "-", "_" or "." only (max 100 chars).'
  }
  return ''
}

/** Suggest `name`, `name-2`, … given the taken names (case-insensitive). */
export function suggestRepoName(wanted: string, taken: string[] = []): string {
  const base = lastSegment(wanted) || DEFAULT_VAULT_REPO
  const lower = new Set(taken.map((t) => lastSegment(t).toLowerCase()))
  if (!lower.has(base.toLowerCase())) return base
  for (let i = 2; i < 100; i++) {
    const cand = `${base}-${i}`
    if (!lower.has(cand.toLowerCase())) return cand
  }
  return `${base}-${Date.now().toString(36)}`
}

export function defaultVaultRepoName(): string {
  return DEFAULT_VAULT_REPO
}

export function vaultReadme(fullName: string, branch: string, set?: VaultSetInfo): string {
  const setLine = set && set.total > 1
    ? `Vault set \`${set.name}\` — repo ${set.index} of ${set.total} (disks merge into one virtual volume).`
    : null
  return [
    `# ${fullName || 'cybermanju-vault'}`,
    '',
    'Private CyberManju OS vault — created from Accounts → Connections.',
    ...(setLine ? ['', setLine] : []),
    '',
    `- \`vault.cybermanju\` — encrypted + compressed vault container (the file this app opens).`,
    `- \`.cybermanju.json\` — vault manifest (version, branch, updated-at).`,
    '',
    `Branch: \`${branch || DEFAULT_VAULT_BRANCH}\``,
    '',
    '> Do not edit these files by hand — use the app so hashes stay verifiable.',
    '',
  ].join('\n')
}

export interface VaultSetInfo {
  /** Base name of the set (`cybermanju-vault`). */
  name: string
  /** 1-based index of this repo inside the set. */
  index: number
  /** Total repos in the set (merged into one virtual disk). */
  total: number
}

export function vaultManifest(fullName: string, branch: string, set?: VaultSetInfo): string {
  return JSON.stringify(
    {
      app: 'cybermanju-os',
      version: 1,
      repo: fullName,
      branch: branch || DEFAULT_VAULT_BRANCH,
      vaultFile: VAULT_FILE_PATH,
      ...(set ? { set } : {}),
      updatedAt: new Date().toISOString(),
    },
    null,
    2,
  ) + '\n'
}

/** Clamp a requested repo count into the supported range. */
export function validateRepoCount(n: unknown): number {
  const parsed = typeof n === 'number' ? Math.floor(n) : parseInt(String(n ?? ''), 10)
  if (!Number.isFinite(parsed)) return 1
  return Math.min(MAX_VAULT_REPOS, Math.max(1, parsed))
}

/**
 * Expand a base name into per-repo names: `vault` → [`vault`] for one repo,
 * `vault` → [`vault-1`, …, `vault-N`] for a set. Every name stays valid for
 * `validateRepoName` (suffixes are appended to the last segment only).
 */
export function expandRepoNames(base: string, count: number): string[] {
  const n = validateRepoCount(count)
  const seg = lastSegment(base) || DEFAULT_VAULT_REPO
  const prefix = String(base ?? '').trim().replace(/^\/+|\/+$/g, '')
  const head = prefix.includes('/') ? `${prefix.slice(0, prefix.lastIndexOf('/') + 1)}` : ''
  if (n === 1) return [`${head}${seg}`]
  return Array.from({ length: n }, (_, i) => `${head}${seg}-${i + 1}`)
}

export function encodeUtf8Base64(text: string): string {
  const bytes = new TextEncoder().encode(text)
  let bin = ''
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i])
  return btoa(bin)
}

export function encodeBytesBase64(bytes: Uint8Array): string {
  let bin = ''
  const CHUNK = 8192
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return btoa(bin)
}

/** Bootstrap files for a fresh vault repo (README + manifest). */
export function buildSeedFiles(
  fullName: string,
  branch: string,
  set?: VaultSetInfo,
): Array<{ path: string; contentBase64: string }> {
  return [
    { path: VAULT_README_PATH, contentBase64: encodeUtf8Base64(vaultReadme(fullName, branch, set)) },
    { path: VAULT_MANIFEST_PATH, contentBase64: encodeUtf8Base64(vaultManifest(fullName, branch, set)) },
  ]
}

function gitlabBase(instanceUrl?: string): string {
  const raw = String(instanceUrl ?? '').trim().replace(/\/+$/, '')
  return /^https?:\/\//.test(raw) ? raw : 'https://gitlab.com'
}

function classifyCreateError(status: number, body: string, provider: string): Error {
  const snippet = String(body || '').slice(0, 300)
  if (status === 401 || status === 403) {
    return new Error(`auth: ${provider} rejected the token (HTTP ${status}) — check scopes (github: repo, gitlab: api) ${snippet}`)
  }
  if (status === 422 || status === 400) {
    const taken = /already|taken|exists|duplicate/i.test(snippet)
    if (taken) return new Error(`conflict: that repo name is taken on ${provider} — pick another name`)
    return new Error(`unsupported: ${provider} refused the repo name (HTTP ${status}): ${snippet}`)
  }
  if (status === 404) return new Error(`not_found: ${provider} repo endpoint missing (HTTP 404) — check the instance URL`)
  if (status === 429) return new Error(`rate_limited: ${provider} throttled repo creation — wait a minute and retry`)
  return new Error(`network: ${provider} repo creation failed (HTTP ${status}): ${snippet}`)
}

/**
 * Direct browser creation (static Pages host — no dashboard behind the page).
 * Desktop/Docker builds go through `create_provider_repo` (Rust) instead.
 */
export async function createRepoDirect(input: ProvisionInput): Promise<CreatedRepo> {
  const name = lastSegment(input.name)
  const problem = validateRepoName(input.name)
  if (problem) throw new Error(`unsupported: ${problem}`)
  const token = input.token.trim()
  if (!token) throw new Error('auth: paste a token first — repo creation needs one')
  const branch = input.branch?.trim() || DEFAULT_VAULT_BRANCH
  const description = input.description?.trim() || 'Private CyberManju OS vault (.cybermanju)'
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), 20000)
  try {
    if (input.backendType === 'github') {
      const res = await fetch('https://api.github.com/user/repos', {
        method: 'POST',
        headers: {
          Authorization: `token ${token}`,
          Accept: 'application/vnd.github+json',
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ name, private: input.privateRepo !== false, description, auto_init: true, default_branch: branch }),
        signal: ctrl.signal,
      }).catch(() => {
        throw new Error('network: api.github.com is not reachable from this browser (offline or blocked) — use the desktop app or Docker server')
      })
      if (!res.ok) throw classifyCreateError(res.status, await res.text().catch(() => ''), 'GitHub')
      const json = (await res.json().catch(() => ({}))) as Record<string, unknown>
      const full = typeof json.full_name === 'string' && json.full_name ? json.full_name : name
      return {
        backend: 'github',
        repoName: full,
        fullName: full,
        branch: typeof json.default_branch === 'string' && json.default_branch ? json.default_branch : branch,
        url: typeof json.html_url === 'string' && json.html_url ? json.html_url : `https://github.com/${full}`,
        projectId: null,
      }
    }
    const base = gitlabBase(input.instanceUrl)
    const res = await fetch(`${base}/api/v4/projects`, {
      method: 'POST',
      headers: { 'PRIVATE-TOKEN': token, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name,
        path: name.toLowerCase().replace(/[^a-z0-9-_]/g, '-').replace(/-+/g, '-').replace(/^-|-$/g, '') || 'cybermanju-vault',
        visibility: input.privateRepo !== false ? 'private' : 'public',
        description,
        initialize_with_readme: true,
        default_branch: branch,
      }),
      signal: ctrl.signal,
    }).catch(() => {
      throw new Error(`network: ${base} is not reachable from this browser (offline or blocked) — use the desktop app or Docker server`)
    })
    if (!res.ok) throw classifyCreateError(res.status, await res.text().catch(() => ''), 'GitLab')
    const json = (await res.json().catch(() => ({}))) as Record<string, unknown>
    const id = typeof json.id === 'number' ? String(json.id) : typeof json.id === 'string' ? json.id : ''
    const full = typeof json.path_with_namespace === 'string' && json.path_with_namespace ? json.path_with_namespace : name
    return {
      backend: 'gitlab',
      repoName: id,
      fullName: full,
      branch: typeof json.default_branch === 'string' && json.default_branch ? json.default_branch : branch,
      url: typeof json.web_url === 'string' && json.web_url ? json.web_url : `${base}/${full}`,
      projectId: id || null,
    }
  } finally {
    clearTimeout(timer)
  }
}

/** Seed files straight into a fresh repo from the browser (static host). */
export async function seedRepoDirect(
  backendType: 'github' | 'gitlab',
  repo: CreatedRepo,
  token: string,
  files: Array<{ path: string; contentBase64: string }>,
  instanceUrl?: string,
): Promise<string[]> {
  const urls: string[] = []
  if (backendType === 'github') {
    const [owner, name] = repo.fullName.split('/')
    if (!owner || !name) throw new Error('unsupported: GitHub repo has no owner/name yet — create it first')
    const headers = {
      Authorization: `token ${token}`,
      Accept: 'application/vnd.github+json',
      'Content-Type': 'application/json',
    }
    for (const f of files) {
      const put = (sha?: string) =>
        fetch(`https://api.github.com/repos/${owner}/${name}/contents/${f.path}`, {
          method: 'PUT',
          headers,
          body: JSON.stringify({
            message: `CyberManju vault seed: ${f.path}`,
            content: f.contentBase64,
            branch: repo.branch,
            ...(sha ? { sha } : {}),
          }),
        })
      let res = await put()
      if (res.status === 422) {
        // File exists — the Contents API needs the current blob SHA to
        // overwrite (same rule the Rust backend follows). Look it up once.
        const lookup = await fetch(
          `https://api.github.com/repos/${owner}/${name}/contents/${f.path}?ref=${encodeURIComponent(repo.branch || DEFAULT_VAULT_BRANCH)}`,
          { headers },
        )
        if (!lookup.ok) {
          throw new Error(`network: GitHub lookup of '${f.path}' failed (HTTP ${lookup.status})`)
        }
        const current = (await lookup.json().catch(() => ({}))) as { sha?: unknown }
        if (Array.isArray(current) || typeof current.sha !== 'string' || !current.sha) {
          throw new Error(`unsupported: GitHub returned no blob sha for '${f.path}'`)
        }
        res = await put(current.sha)
      }
      if (!res.ok) {
        const body = await res.text().catch(() => '')
        throw new Error(`network: GitHub seed of '${f.path}' failed (HTTP ${res.status}): ${body.slice(0, 200)}`)
      }
      urls.push(`https://github.com/${owner}/${name}/blob/${repo.branch}/${f.path}`)
    }
    return urls
  }
  const base = gitlabBase(instanceUrl)
  const pid = encodeURIComponent(repo.repoName)
  for (const f of files) {
    // Create-or-update: try POST, fall back to PUT when the file exists
    // (fresh repos only have the README, so POST usually wins).
    const body = JSON.stringify({ branch: repo.branch, content: f.contentBase64, encoding: 'base64', commit_message: `CyberManju vault seed: ${f.path}` })
    let res = await fetch(`${base}/api/v4/projects/${pid}/repository/files/${encodeURIComponent(f.path)}`, {
      method: 'POST',
      headers: { 'PRIVATE-TOKEN': token, 'Content-Type': 'application/json' },
      body,
    })
    if (!res.ok) {
      res = await fetch(`${base}/api/v4/projects/${pid}/repository/files/${encodeURIComponent(f.path)}`, {
        method: 'PUT',
        headers: { 'PRIVATE-TOKEN': token, 'Content-Type': 'application/json' },
        body,
      })
    }
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new Error(`network: GitLab seed of '${f.path}' failed (HTTP ${res.status}): ${text.slice(0, 200)}`)
    }
    urls.push(`${base}/${repo.fullName}/-/blob/${repo.branch}/${f.path}`)
  }
  return urls
}

/**
 * Delete one file straight from a provider repo (static-host path; the
 * desktop/Docker builds use `delete_remote_file` through the Rust backend).
 * GitHub needs the blob SHA: one lookup, then the delete. GitLab deletes by
 * path directly. Missing files resolve as success (idempotent).
 */
export async function deleteFileDirect(
  backendType: 'github' | 'gitlab',
  repo: CreatedRepo,
  token: string,
  remotePath: string,
  instanceUrl?: string,
): Promise<void> {
  const rel = String(remotePath ?? '').replace(/^\/+|\/+$/g, '')
  if (!rel) throw new Error('invalid: remotePath is required')
  if (!token.trim()) throw new Error('auth: provider delete needs a token')
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), 20000)
  try {
    if (backendType === 'github') {
      const [owner, name] = repo.fullName.split('/')
      if (!owner || !name) throw new Error('unsupported: GitHub repo has no owner/name yet')
      const headers = {
        Authorization: `token ${token}`,
        Accept: 'application/vnd.github+json',
        'Content-Type': 'application/json',
      }
      const lookup = await fetch(
        `https://api.github.com/repos/${owner}/${name}/contents/${rel}?ref=${encodeURIComponent(repo.branch || DEFAULT_VAULT_BRANCH)}`,
        { headers, signal: ctrl.signal },
      ).catch(() => {
        throw new Error('network: api.github.com is not reachable from this browser')
      })
      if (lookup.status === 404) return
      if (!lookup.ok) throw new Error(`network: GitHub lookup of '${rel}' failed (HTTP ${lookup.status})`)
      const json = (await lookup.json().catch(() => ({}))) as { sha?: unknown }
      if (Array.isArray(json)) return
      const sha = typeof json.sha === 'string' ? json.sha : ''
      if (!sha) throw new Error(`unsupported: GitHub returned no blob sha for '${rel}'`)
      const res = await fetch(`https://api.github.com/repos/${owner}/${name}/contents/${rel}`, {
        method: 'DELETE',
        headers,
        body: JSON.stringify({ message: `CyberManju delete: ${rel}`, sha, branch: repo.branch || DEFAULT_VAULT_BRANCH }),
        signal: ctrl.signal,
      }).catch(() => {
        throw new Error('network: api.github.com is not reachable from this browser')
      })
      if (!res.ok && res.status !== 404) {
        throw new Error(`network: GitHub delete of '${rel}' failed (HTTP ${res.status})`)
      }
      return
    }
    const base = gitlabBase(instanceUrl)
    const res = await fetch(
      `${base}/api/v4/projects/${encodeURIComponent(repo.repoName)}/repository/files/${encodeURIComponent(rel)}?branch=${encodeURIComponent(repo.branch || DEFAULT_VAULT_BRANCH)}`,
      {
        method: 'DELETE',
        headers: { 'PRIVATE-TOKEN': token, 'Content-Type': 'application/json' },
        body: JSON.stringify({ branch: repo.branch || DEFAULT_VAULT_BRANCH, commit_message: `CyberManju delete: ${rel}` }),
        signal: ctrl.signal,
      },
    ).catch(() => {
      throw new Error(`network: ${base} is not reachable from this browser`)
    })
    if (!res.ok && res.status !== 404) {
      throw new Error(`network: GitLab delete of '${rel}' failed (HTTP ${res.status})`)
    }
  } finally {
    clearTimeout(timer)
  }
}

const DRIVE_FILES_URL = 'https://www.googleapis.com/drive/v3/files'
const DRIVE_UPLOAD_URL = 'https://www.googleapis.com/upload/drive/v3/files'
const DRIVE_FOLDER_MIME = 'application/vnd.google-apps.folder'

/** The narrow Drive grant the app needs — also requested via Supabase OAuth. */
export const GOOGLE_DRIVE_FILE_SCOPE = 'https://www.googleapis.com/auth/drive.file'

function driveHeaders(token: string): Record<string, string> {
  return { Authorization: `Bearer ${token}` }
}

/**
 * Granted OAuth scopes behind a Google access token (CORS-open tokeninfo).
 * Returns null when the endpoint is unreachable — callers then keep the
 * generic message instead of claiming a scope state they could not verify.
 */
export async function googleTokenScopes(token: string): Promise<string[] | null> {
  const t = token.trim()
  if (!t) return null
  try {
    const ctrl = new AbortController()
    const timer = setTimeout(() => ctrl.abort(), 8000)
    try {
      const res = await fetch(
        `https://oauth2.googleapis.com/tokeninfo?access_token=${encodeURIComponent(t)}`,
        { signal: ctrl.signal },
      )
      if (!res.ok) return null
      const json = (await res.json().catch(() => null)) as { scope?: unknown } | null
      const raw = typeof json?.scope === 'string' ? json.scope : ''
      if (!raw.trim()) return []
      return raw.split(/\s+/).filter(Boolean)
    } finally {
      clearTimeout(timer)
    }
  } catch {
    return null
  }
}

function googleErrorSnippet(json: Record<string, unknown>): string {
  try {
    const err = (json as { error?: { message?: unknown; status?: unknown; errors?: unknown } }).error
    const msg = typeof err?.message === 'string' ? err.message : ''
    const status = typeof err?.status === 'string' ? err.status : ''
    let reason = ''
    const errs = (err as { errors?: unknown } | undefined)?.errors
    if (Array.isArray(errs)) {
      const first = errs[0] as { reason?: unknown } | undefined
      if (typeof first?.reason === 'string') reason = first.reason
    }
    const combined = [status || reason, msg].filter(Boolean).join(': ').trim()
    if (combined) return combined.slice(0, 260)
  } catch {
    // Fall through to the empty snippet.
  }
  return ''
}

function googleErrorReason(json?: Record<string, unknown>): string {
  try {
    const err = (json as { error?: { errors?: unknown } } | undefined)?.error
    const errs = err?.errors
    if (Array.isArray(errs)) {
      const first = errs[0] as { reason?: unknown } | undefined
      if (typeof first?.reason === 'string') return first.reason
    }
  } catch {
    // No reason present.
  }
  return ''
}

// NOTE: Supabase's Google provider screen has NO extra-scope field (just
// Client IDs + secret + callback URL) — the app already requests
// `drive.file` at runtime via signInWithOAuth `scopes`, so there is nothing
// to paste in Supabase. A fresh-login 403 therefore means the Google Cloud
// side of that same client ID is incomplete, not the Supabase form.
const GOOGLE_SCOPE_FIX =
  `fix (Google Cloud console, same project as your Supabase Client ID): ` +
  `1) APIs & Services → Library → enable "Google Drive API". ` +
  `2) OAuth consent screen → Scopes → add '${GOOGLE_DRIVE_FILE_SCOPE}'` +
  ` (if Testing mode, also add your account under Test users). ` +
  `3) Revoke the stale grant at https://myaccount.google.com/permissions, then sign out + sign in again with consent ` +
  `(or reconnect OAuth on the card). No Supabase scope field exists — the app sends the scope on every login`

const GOOGLE_API_DISABLED_FIX =
  `fix: the Drive API is disabled in the Google Cloud project behind your Supabase Client ID — ` +
  `Google Cloud console → APIs & Services → Library → enable "Google Drive API", wait a few minutes, then retry. ` +
  `If the consent screen is in Testing mode, add your account under Test users too`

/**
 * Actionable `auth:` error for every Google 401/403. A bare "reconnect
 * OAuth" loops forever when the Cloud project behind the Supabase Client ID
 * has the Drive API disabled or the `drive.file` scope unlisted: the
 * Supabase sign-in looks OK while Drive keeps 403ing (Supabase's Google
 * screen has no scope field — the app sends the scope at runtime). This
 * reports Google's own error text/reason plus the granted scopes (via
 * tokeninfo) so the real fix — enable Drive API + list the scope in the
 * Cloud console, then a fresh consent — is visible.
 */
export async function googleDriveAuthError(
  token: string,
  status: number,
  json?: Record<string, unknown>,
): Promise<string> {
  const detail = json ? googleErrorSnippet(json) : ''
  const reason = json ? googleErrorReason(json) : ''
  const suffix = detail ? ` — Google says: ${detail}` : ''
  // Drive API never enabled in this GCP project: re-login can never help.
  if (
    reason === 'accessNotConfigured' ||
    /access not configured|has not been used|is disabled/i.test(detail)
  ) {
    return `auth: Google rejected the token (HTTP ${status})${suffix}. ${GOOGLE_API_DISABLED_FIX}`
  }
  const scopes = await googleTokenScopes(token)
  if (scopes && !scopes.includes(GOOGLE_DRIVE_FILE_SCOPE)) {
    const have = scopes.length ? scopes.join(' ') : '(no scopes reported)'
    return (
      `auth: Google rejected the token (HTTP ${status})${suffix} — the grant lacks '${GOOGLE_DRIVE_FILE_SCOPE}' ` +
      `(granted: ${have}). ${GOOGLE_SCOPE_FIX}`
    )
  }
  if (status === 401) {
    return (
      `auth: Google rejected the token (HTTP 401)${suffix} — expired or revoked. ` +
      `Sign out + sign in again (or reconnect OAuth on the card), then retry`
    )
  }
  return (
    `auth: Google rejected the token (HTTP ${status})${suffix} — even a fresh login 403s when the Cloud project ` +
    `has no Drive API / scope grant. ${GOOGLE_SCOPE_FIX}`
  )
}

/** Sync (non-tokeninfo) variant for paths that already hold the error JSON. */
export function googleDriveAuthErrorSync(status: number, json?: Record<string, unknown>): string {
  const detail = json ? googleErrorSnippet(json) : ''
  const reason = json ? googleErrorReason(json) : ''
  const suffix = detail ? ` — Google says: ${detail}` : ''
  if (
    reason === 'accessNotConfigured' ||
    /access not configured|has not been used|is disabled/i.test(detail)
  ) {
    return `auth: Google rejected the token (HTTP ${status})${suffix}. ${GOOGLE_API_DISABLED_FIX}`
  }
  if (status === 401) {
    return (
      `auth: Google rejected the token (HTTP 401)${suffix} — expired or revoked. ` +
      `Sign out + sign in again (or reconnect OAuth on the card), then retry`
    )
  }
  return (
    `auth: Google rejected the token (HTTP ${status})${suffix} — even a fresh login 403s when the Cloud project ` +
    `has no Drive API / scope grant. ${GOOGLE_SCOPE_FIX}`
  )
}

function decodeBase64Bytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

async function driveJson(
  token: string,
  url: string,
  init?: RequestInit,
  provider = 'Google Drive',
): Promise<{ status: number; json: Record<string, unknown> }> {
  const res = await fetch(url, {
    ...init,
    headers: { ...driveHeaders(token), ...(init?.headers ?? {}) },
  }).catch(() => {
    throw new Error('network: www.googleapis.com is not reachable from this browser (offline or blocked)')
  })
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>
  void provider
  return { status: res.status, json }
}

/** First child id named `name` under `parentId` (files and folders). */
async function driveFindChild(token: string, parentId: string, name: string): Promise<string | null> {
  const q = `name='${name.replace(/'/g, "''")}' and '${parentId}' in parents and trashed=false`
  const { status, json } = await driveJson(
    token,
    `${DRIVE_FILES_URL}?q=${encodeURIComponent(q)}&fields=files(id)&pageSize=10`,
  )
  if (status === 401 || status === 403) throw new Error(await googleDriveAuthError(token, status, json))
  if (status !== 200) throw new Error(`network: Drive lookup failed (HTTP ${status})`)
  const files = Array.isArray(json.files) ? json.files : []
  const first = files[0] as { id?: unknown } | undefined
  return typeof first?.id === 'string' && first.id ? first.id : null
}

/** Walk `parts` below `rootId`, creating missing folders. Returns the parent id. */
async function driveEnsureParents(token: string, rootId: string, parts: string[]): Promise<string> {
  let parent = rootId
  for (const seg of parts) {
    const found = await driveFindChild(token, parent, seg)
    if (found) {
      parent = found
      continue
    }
    const { status, json } = await driveJson(token, DRIVE_FILES_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: seg, mimeType: DRIVE_FOLDER_MIME, parents: [parent] }),
    })
    const id = json.id
    if (status === 401 || status === 403) throw new Error(await googleDriveAuthError(token, status, json))
    if (status !== 200 || typeof id !== 'string' || !id) {
      throw new Error(`network: Drive folder create for '${seg}' failed (HTTP ${status})`)
    }
    parent = id
  }
  return parent
}

/**
 * Write one file to Google Drive straight from the browser (static-host
 * path; desktop/Docker use the Rust backend). Creates parent folders and
 * overwrites an existing same-named child. Returns the Drive file id.
 */
export async function driveWriteDirect(input: {
  token: string
  folderId?: string
  basePath?: string
  remotePath: string
  contentBase64: string
}): Promise<string> {
  const token = input.token.trim()
  if (!token) throw new Error('auth: Drive write needs a token — connect with OAuth first')
  const rel = String(input.remotePath ?? '').replace(/^\/+|\/+$/g, '')
  if (!rel || /(^|\/)\.\.(\/|$)/.test(rel)) throw new Error(`unsupported: Drive path '${input.remotePath}' is invalid`)
  const bytes = decodeBase64Bytes(input.contentBase64)
  if (bytes.length > 5 * 1024 * 1024) throw new Error(`too_large: '${rel}' exceeds the 5 MiB write cap`)
  const root = input.folderId?.trim() || 'root'
  const prefix = String(input.basePath ?? '').trim().replace(/^\/+|\/+$/g, '')
  const parts = [...prefix.split('/'), ...rel.split('/')].map((s) => s.trim()).filter((s) => s && s !== '.')
  const name = parts.pop() ?? ''
  if (!name) throw new Error('unsupported: Drive path must name a file')
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), 30000)
  try {
    const parentId = await driveEnsureParents(token, root, parts)
    const existing = await driveFindChild(token, parentId, name)
    if (existing) {
      const res = await fetch(`${DRIVE_UPLOAD_URL}/${existing}?uploadType=media`, {
        method: 'PATCH',
        headers: { ...driveHeaders(token), 'Content-Type': 'application/octet-stream' },
        body: bytes as unknown as BodyInit,
        signal: ctrl.signal,
      }).catch(() => {
        throw new Error('network: www.googleapis.com is not reachable from this browser')
      })
      if (res.status === 401 || res.status === 403) {
        const json = (await res.json().catch(() => ({}))) as Record<string, unknown>
        throw new Error(await googleDriveAuthError(token, res.status, json))
      }
      if (!res.ok) throw new Error(`network: Drive overwrite of '${rel}' failed (HTTP ${res.status})`)
      return existing
    }
    const boundary = `cybermanju-${Date.now().toString(36)}`
    const meta = new TextEncoder().encode(
      `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n` +
        JSON.stringify({ name, parents: [parentId] }) +
        `\r\n--${boundary}\r\nContent-Type: application/octet-stream\r\n\r\n`,
    )
    const tail = new TextEncoder().encode(`\r\n--${boundary}--`)
    const body = new Uint8Array(meta.length + bytes.length + tail.length)
    body.set(meta, 0)
    body.set(bytes, meta.length)
    body.set(tail, meta.length + bytes.length)
    const res = await fetch(`${DRIVE_UPLOAD_URL}?uploadType=multipart`, {
      method: 'POST',
      headers: { ...driveHeaders(token), 'Content-Type': `multipart/related; boundary=${boundary}` },
      body: body as unknown as BodyInit,
      signal: ctrl.signal,
    }).catch(() => {
      throw new Error('network: www.googleapis.com is not reachable from this browser')
    })
    if (res.status === 401 || res.status === 403) {
      const errJson = (await res.json().catch(() => ({}))) as Record<string, unknown>
      throw new Error(await googleDriveAuthError(token, res.status, errJson))
    }
    if (!res.ok) throw new Error(`network: Drive upload of '${rel}' failed (HTTP ${res.status})`)
    const json = (await res.json().catch(() => ({}))) as { id?: unknown }
    if (typeof json.id !== 'string' || !json.id) throw new Error('network: Drive upload returned no file id')
    return json.id
  } finally {
    clearTimeout(timer)
  }
}

/**
 * Delete one Drive file straight from the browser. Prefers the listing
 * locator (already a file id); otherwise resolves `folderId/basePath/rel`.
 */
export async function driveDeleteDirect(input: {
  token: string
  folderId?: string
  basePath?: string
  remotePath: string
  locator?: string
}): Promise<void> {
  const token = input.token.trim()
  if (!token) throw new Error('auth: Drive delete needs a token — connect with OAuth first')
  let id = String(input.locator ?? '').trim()
  if (!id) {
    const rel = String(input.remotePath ?? '').replace(/^\/+|\/+$/g, '')
    if (!rel) throw new Error('invalid: remotePath is required')
    const root = input.folderId?.trim() || 'root'
    const prefix = String(input.basePath ?? '').trim().replace(/^\/+|\/+$/g, '')
    const parts = [...prefix.split('/'), ...rel.split('/')].map((s) => s.trim()).filter((s) => s && s !== '.')
    const name = parts.pop() ?? ''
    if (!name) throw new Error('invalid: remotePath is required')
    let parent = root
    for (const seg of parts) {
      const next = await driveFindChild(token, parent, seg)
      if (!next) return
      parent = next
    }
    const found = await driveFindChild(token, parent, name)
    if (!found) return
    id = found
  }
  const { status, json } = await driveJson(token, `${DRIVE_FILES_URL}/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (status === 401 || status === 403) throw new Error(await googleDriveAuthError(token, status, json))
  if (status !== 204 && status !== 200 && status !== 404) {
    throw new Error(`network: Drive delete failed (HTTP ${status})`)
  }
}

export interface ProviderRepoChoice {
  /** Value to store in `SyncConfig.repoName` (owner/repo for GitHub, project id for GitLab). */
  value: string
  /** Human label (`owner/repo` or `namespace / name`). */
  label: string
  url: string
  isPrivate: boolean
}

export interface DriveFolderChoice {
  id: string
  name: string
}

function withTimeout(ms = 20000): { ctrl: AbortController; done: () => void } {
  const ctrl = new AbortController()
  const timer = setTimeout(() => ctrl.abort(), ms)
  return { ctrl, done: () => clearTimeout(timer) }
}

/**
 * List the token owner's repos (GitHub) — used by the Accounts picker so an
 * OAuth/token login can *select* an existing vault repo instead of only
 * creating one. Direct browser fetch (CORS-OK); desktop goes through the
 * same token via the Rust probe path.
 */
export async function listGithubRepos(token: string): Promise<ProviderRepoChoice[]> {
  const t = token.trim()
  if (!t) throw new Error('auth: paste a token or connect with OAuth first')
  const { ctrl, done } = withTimeout()
  try {
    const res = await fetch('https://api.github.com/user/repos?per_page=100&sort=updated', {
      headers: { Authorization: `token ${t}`, Accept: 'application/vnd.github+json' },
      signal: ctrl.signal,
    }).catch(() => {
      throw new Error('network: api.github.com is not reachable from this browser (offline or blocked)')
    })
    if (res.status === 401 || res.status === 403) throw new Error('auth: GitHub rejected the token — check scopes (repo) or reconnect with OAuth')
    if (res.status === 429) throw new Error('rate_limited: GitHub throttled the repo list — wait a minute and retry')
    if (!res.ok) throw new Error(`network: GitHub repo list failed (HTTP ${res.status})`)
    const json = (await res.json().catch(() => [])) as Array<Record<string, unknown>>
    if (!Array.isArray(json)) return []
    return json
      .filter((r) => typeof r.full_name === 'string' && r.full_name)
      .map((r) => ({
        value: String(r.full_name),
        label: String(r.full_name),
        url: typeof r.html_url === 'string' ? r.html_url : `https://github.com/${r.full_name}`,
        isPrivate: r.private === true,
      }))
      .slice(0, 100)
  } finally {
    done()
  }
}

/** List GitLab projects the token is a member of (id + path shown). */
export async function listGitlabProjects(token: string, instanceUrl?: string): Promise<ProviderRepoChoice[]> {
  const t = token.trim()
  if (!t) throw new Error('auth: paste a token or connect with OAuth first')
  const base = gitlabBase(instanceUrl)
  const { ctrl, done } = withTimeout()
  try {
    const res = await fetch(`${base}/api/v4/projects?membership=true&per_page=100&order_by=last_activity_at`, {
      headers: { 'PRIVATE-TOKEN': t },
      signal: ctrl.signal,
    }).catch(() => {
      throw new Error(`network: ${base} is not reachable from this browser (offline or blocked)`)
    })
    if (res.status === 401 || res.status === 403) throw new Error('auth: GitLab rejected the token — check scopes (api) or reconnect with OAuth')
    if (res.status === 429) throw new Error('rate_limited: GitLab throttled the project list — wait a minute and retry')
    if (!res.ok) throw new Error(`network: GitLab project list failed (HTTP ${res.status})`)
    const json = (await res.json().catch(() => [])) as Array<Record<string, unknown>>
    if (!Array.isArray(json)) return []
    return json
      .map((p) => ({
        value: typeof p.id === 'number' ? String(p.id) : typeof p.id === 'string' ? p.id : '',
        label: typeof p.path_with_namespace === 'string' && p.path_with_namespace ? p.path_with_namespace : String(p.name ?? p.id ?? ''),
        url: typeof p.web_url === 'string' ? p.web_url : base,
        isPrivate: String((p as Record<string, unknown>).visibility ?? 'private') !== 'public',
      }))
      .filter((p) => p.value)
      .slice(0, 100)
  } finally {
    done()
  }
}

/** List Drive folders (root or under `parentId`) for the folder picker. */
export async function listDriveFolders(token: string, parentId?: string): Promise<DriveFolderChoice[]> {
  const t = token.trim()
  if (!t) throw new Error('auth: Drive needs a token — connect with OAuth first')
  const parent = parentId?.trim() || 'root'
  const q = `'${parent.replace(/'/g, "\\'")}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false`
  const { status, json } = await driveJson(
    t,
    `${DRIVE_FILES_URL}?q=${encodeURIComponent(q)}&fields=files(id,name)&orderBy=name&pageSize=100`,
  )
  if (status === 401 || status === 403) throw new Error(await googleDriveAuthError(t, status, json))
  if (status !== 200) throw new Error(`network: Drive folder list failed (HTTP ${status})`)
  const files = Array.isArray(json.files) ? json.files : []
  return (files as Array<{ id?: unknown; name?: unknown }>)
    .filter((f) => typeof f.id === 'string' && f.id && typeof f.name === 'string')
    .map((f) => ({ id: String(f.id), name: String(f.name) }))
}

/** Create one Drive folder (under `parentId` or root) and return it. */
export async function createDriveFolder(token: string, name: string, parentId?: string): Promise<DriveFolderChoice> {
  const t = token.trim()
  if (!t) throw new Error('auth: Drive needs a token — connect with OAuth first')
  const clean = name.trim()
  if (!clean) throw new Error('Give the folder a name — e.g. cybermanju-vault.')
  const parent = parentId?.trim() || 'root'
  const { status, json } = await driveJson(t, DRIVE_FILES_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: clean, mimeType: DRIVE_FOLDER_MIME, parents: [parent] }),
  })
  const id = json.id
  if (status === 401 || status === 403) throw new Error(await googleDriveAuthError(t, status, json))
  if (status !== 200 || typeof id !== 'string' || !id) {
    throw new Error(`network: Drive folder create failed (HTTP ${status})`)
  }
  return { id, name: clean }
}

/**
 * Full synced-system provisioning after the repo exists:
 * save provider config → seed README/manifest (+ vault bytes when given) →
 * probe → create + attach a system disk. Returns the saved config + disk id.
 */
export async function provisionSyncedSystem(deps: {
  backendType: 'github' | 'gitlab'
  repo: CreatedRepo
  token: string
  displayName: string
  instanceUrl?: string
  diskSizeMb?: number
  diskPassphrase?: string
  /** Compress uploads on future syncs (the container itself is always LZ4). */
  compressBeforeUpload?: boolean
  /** Refuse uploads without a master passphrase (default true for vault sets). */
  requireEncryption?: boolean
  /** Hash basenames into remote locators (default true for vault sets). */
  obfuscateNames?: boolean
  /** Set membership for the manifest when provisioning N repos at once. */
  set?: VaultSetInfo
  vaultBytes?: Uint8Array | null
  saveConfig: (cfg: Omit<SyncConfig, 'id' | 'createdAt' | 'updatedAt'>) => Promise<SyncConfig | null>
  seedViaBackend: (cfg: SyncConfig, files: Array<{ path: string; contentBase64: string }>) => Promise<unknown>
  probe: (cfg: SyncConfig) => Promise<{ ok: boolean; detail: string }>
  createDisk: (configId: string, sizeBytes: number, passphrase: string) => Promise<{ id: string } | null>
  attachDisk: (diskId: string, passphrase: string) => Promise<unknown>
  useDirectSeed: boolean
}): Promise<{ config: SyncConfig; diskId: string | null; vaultSeeded: boolean }> {
  const branch = deps.repo.branch || DEFAULT_VAULT_BRANCH
  const baseCfg: Omit<SyncConfig, 'id' | 'createdAt' | 'updatedAt'> = {
    backendType: deps.backendType,
    enabled: true,
    autoSync: false,
    compressBeforeUpload: deps.compressBeforeUpload !== false,
    createPreviews: false,
    deleteRawAfterSync: false,
    maxConcurrentUploads: 1,
    encryptBeforeUpload: true,
    requireEncryption: deps.requireEncryption !== false,
    obfuscateNames: deps.obfuscateNames !== false,
    conflictPolicy: 'skip',
    placement: 'whole',
    parity: 1,
    name: deps.displayName || `${deps.repo.fullName} vault`,
    repoName: deps.repo.repoName,
    branch,
    token: deps.token.trim() || undefined,
    ...(deps.backendType === 'gitlab' && deps.instanceUrl?.trim()
      ? { basePath: deps.instanceUrl.trim().replace(/\/+$/, '') }
      : {}),
  }
  const saved = await deps.saveConfig(baseCfg)
  if (!saved?.id) throw new Error('network: provider config could not be saved — retry')
  const files = buildSeedFiles(deps.repo.fullName, branch, deps.set)
  // Mirror the encrypted + compressed container into every repo of the set.
  // Oversized vaults skip the file seed (README + manifest still land; the
  // live sync carries the data instead of a 5 MiB-capped seed commit).
  let vaultSeeded = false
  if (deps.vaultBytes && deps.vaultBytes.length > 0 && deps.vaultBytes.length <= MAX_SEED_BYTES) {
    files.push({ path: VAULT_FILE_PATH, contentBase64: encodeBytesBase64(deps.vaultBytes) })
    vaultSeeded = true
  }
  if (deps.useDirectSeed) {
    await seedRepoDirect(deps.backendType, deps.repo, deps.token, files, deps.instanceUrl)
  } else {
    await deps.seedViaBackend(saved, files)
  }
  const probe = await deps.probe({ ...saved, token: deps.token.trim() || saved.token })
  if (!probe.ok) throw new Error(`network: repo created and seeded, but the probe failed — ${probe.detail}`)
  const mb = Math.min(8192, Math.max(64, Math.round(deps.diskSizeMb ?? 512)))
  let diskId: string | null = null
  try {
    const disk = await deps.createDisk(saved.id, mb * 1024 * 1024, deps.diskPassphrase ?? '')
    diskId = disk?.id ?? null
    if (diskId) await deps.attachDisk(diskId, deps.diskPassphrase ?? '').catch(() => undefined)
  } catch (e) {
    // The repo + config + seed are all live — a disk failure must not look
    // like a provisioning failure. Surface it through the returned null.
    console.warn('[vault-provision] system disk step failed (repo is still synced):', e)
  }
  return { config: saved, diskId, vaultSeeded }
}

export interface VaultSetResult {
  repos: CreatedRepo[]
  configs: SyncConfig[]
  diskIds: (string | null)[]
  /** Sum of the requested per-disk sizes (bytes). */
  totalDiskBytes: number
  /** Every repo whose step failed — the rest of the set is still live. */
  failures: Array<{ name: string; error: string }>
}

/**
 * Provision a whole set of private vault repos on one provider (1–8):
 * sequential create → config → seed (mirrored encrypted vault) → probe →
 * encrypted system disk per repo. Disks merge into one virtual volume, so
 * N repos × M MB = N×M of merged space.
 *
 * Partial failure is normal (name taken, rate limit): failures are collected
 * per repo and the run resolves when at least one repo synced; it rejects
 * only when nothing succeeded.
 */
export async function provisionVaultRepoSet(deps: {
  backendType: 'github' | 'gitlab'
  baseName: string
  count: number
  token: string
  description?: string
  branch?: string
  instanceUrl?: string
  displayPrefix?: string
  diskSizeMb?: number
  diskPassphrase?: string
  compressBeforeUpload?: boolean
  requireEncryption?: boolean
  obfuscateNames?: boolean
  vaultBytes?: Uint8Array | null
  createRepo: (input: ProvisionInput) => Promise<CreatedRepo>
  saveConfig: (cfg: Omit<SyncConfig, 'id' | 'createdAt' | 'updatedAt'>) => Promise<SyncConfig | null>
  seedViaBackend: (cfg: SyncConfig, files: Array<{ path: string; contentBase64: string }>) => Promise<unknown>
  probe: (cfg: SyncConfig) => Promise<{ ok: boolean; detail: string }>
  createDisk: (configId: string, sizeBytes: number, passphrase: string) => Promise<{ id: string } | null>
  attachDisk: (diskId: string, passphrase: string) => Promise<unknown>
  useDirectSeed: boolean
  onProgress?: (done: number, total: number, stage: string, name: string) => void
}): Promise<VaultSetResult> {
  const total = validateRepoCount(deps.count)
  const names = expandRepoNames(deps.baseName, total)
  const branch = deps.branch?.trim() || DEFAULT_VAULT_BRANCH
  const setName = lastSegment(deps.baseName) || DEFAULT_VAULT_REPO
  const result: VaultSetResult = { repos: [], configs: [], diskIds: [], totalDiskBytes: 0, failures: [] }
  for (let i = 0; i < names.length; i++) {
    const name = names[i]
    try {
      deps.onProgress?.(i, total, 'creating repo', name)
      const repo = await deps.createRepo({
        backendType: deps.backendType,
        token: deps.token,
        name,
        privateRepo: true,
        description: deps.description,
        branch,
        instanceUrl: deps.instanceUrl,
      })
      deps.onProgress?.(i, total, 'seeding + disk', repo.fullName)
      const { config, diskId } = await provisionSyncedSystem({
        backendType: deps.backendType,
        repo,
        token: deps.token,
        displayName: `${deps.displayPrefix || setName} ${i + 1}/${total}`,
        instanceUrl: deps.instanceUrl,
        diskSizeMb: deps.diskSizeMb,
        diskPassphrase: deps.diskPassphrase,
        compressBeforeUpload: deps.compressBeforeUpload,
        requireEncryption: deps.requireEncryption,
        obfuscateNames: deps.obfuscateNames,
        set: total > 1 ? { name: setName, index: i + 1, total } : undefined,
        vaultBytes: deps.vaultBytes,
        saveConfig: deps.saveConfig,
        seedViaBackend: deps.seedViaBackend,
        probe: deps.probe,
        createDisk: deps.createDisk,
        attachDisk: deps.attachDisk,
        useDirectSeed: deps.useDirectSeed,
      })
      result.repos.push(repo)
      result.configs.push(config)
      result.diskIds.push(diskId)
      deps.onProgress?.(i + 1, total, diskId ? 'synced' : 'synced (disk skipped)', repo.fullName)
    } catch (e) {
      const error = e instanceof Error ? e.message : String(e)
      result.failures.push({ name, error })
      deps.onProgress?.(i + 1, total, `failed: ${error.slice(0, 80)}`, name)
    }
  }
  if (result.repos.length === 0) {
    const first = result.failures[0]
    throw new Error(first ? first.error : 'network: repo set provisioning failed')
  }
  const mb = Math.min(8192, Math.max(64, Math.round(deps.diskSizeMb ?? 512)))
  result.totalDiskBytes = result.diskIds.filter((d): d is string => !!d).length * mb * 1024 * 1024
  return result
}
