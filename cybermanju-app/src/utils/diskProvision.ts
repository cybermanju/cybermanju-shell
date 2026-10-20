// CyberManju OS — disk ↔ remote provisioning (Drive folder + .cybermanju file,
// GitHub/GitLab private repo + file).
//
// `create_disk` alone only writes the local sealed container (desktop data dir
// / OPFS) — the provider never sees a folder, repo or file. Every plain
// "create disk" surface must funnel through `provisionDiskWithRemote` so a
// disk bound to a cloud provider visibly lands there:
//
//   googleDrive      → `<dir>/.cybermanju.json` + `<dir>/vault.cybermanju`
//                      uploaded through the backend (Rust `upload_file`
//                      creates the folder chain; static `driveWriteDirect`
//                      runs `driveEnsureParents`).
//   github / gitlab  → private repo created first when the config has none,
//                      then README + manifest + `vault.cybermanju` seeded
//                      (Rust `seed_repo_files` / static `seedRepoDirect`).
//
// Fully dependency-injected (no store/invoke imports) so vitest can pin it.

import type { SyncConfig } from '@/types'

export const DISK_SEED_DIR = 'cybermanju-disks'
export const DISK_MANIFEST_NAME = '.cybermanju.json'
export const DISK_VAULT_NAME = 'vault.cybermanju'
export const DISK_README_NAME = 'README.md'

export interface DiskRef {
  id: string
  name?: string | null
}

export interface RemoteSeedFile {
  path: string
  contentBase64: string
}

export interface DiskProvisionDeps {
  config: SyncConfig
  disk: DiskRef
  /** Explicit paste, else the saved secret. Never persisted here. */
  token: string
  sizeBytes: number
  /** Encrypted container mirror (empty = placeholder note instead). */
  vaultBytes?: Uint8Array | null
  useDirectSeed: boolean
  branch?: string
  instanceUrl?: string
  displayName?: string
  uploadFile: (cfg: SyncConfig, remotePath: string, contentBase64: string) => Promise<unknown>
  seedFiles: (cfg: SyncConfig, files: RemoteSeedFile[]) => Promise<unknown>
  createRepo: (input: {
    backendType: string
    token: string
    name: string
    privateRepo: boolean
    description?: string
    branch?: string
    instanceUrl?: string
  }) => Promise<{ repoName: string; fullName: string; branch: string } | null>
  saveConfig: (cfg: SyncConfig) => Promise<SyncConfig | null>
  driveWriteDirect?: (input: {
    token: string
    folderId?: string
    remotePath: string
    contentBase64: string
  }) => Promise<unknown>
}

export interface DiskProvisionResult {
  /** Remote dir holding the files (`cybermanju-disks/<slug>`). */
  remoteDir: string
  files: string[]
  /** True when a fresh private repo was created for this disk. */
  repoCreated: boolean
  config: SyncConfig
  vaultSeeded: boolean
}

const SEGMENT_RE = /^[A-Za-z0-9._-]{1,100}$/

export function slugifySegment(raw: string, fallback: string): string {
  const seg = String(raw ?? '')
    .trim()
    .replace(/^\/+|\/+$/g, '')
    .split('/')
    .filter(Boolean)
    .pop() ?? ''
  let out = ''
  for (const c of seg || fallback) {
    if (/[A-Za-z0-9-_]/.test(c)) out += c.toLowerCase()
    else if (c === ' ' || c === '.') out += '-'
  }
  out = out.replace(/-+/g, '-').replace(/^-|-$/g, '').slice(0, 100)
  if (!out || !SEGMENT_RE.test(out)) {
    out = fallback.toLowerCase().replace(/[^a-z0-9-_]/g, '-').slice(0, 100) || 'disk'
  }
  return out
}

/** Per-disk remote dir — stable for a disk id, collision-free for N disks. */
export function diskRemoteDir(disk: DiskRef): string {
  const short = String(disk.id ?? 'disk').replace(/^disk-/, '').slice(0, 8) || 'disk'
  return `${DISK_SEED_DIR}/${slugifySegment(disk.name ?? '', `disk-${short}`)}-${short}`
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

/** README + manifest for one disk dir (vault bytes appended by the caller). */
export function buildDiskSeedFiles(
  dir: string,
  disk: DiskRef,
  sizeBytes: number,
  providerLabel: string,
): RemoteSeedFile[] {
  const name = disk.name || disk.id
  const readme = [
    `# ${name}`,
    '',
    `CyberManju OS disk \`${disk.id}\` on ${providerLabel}.`,
    '',
    `- \`${DISK_VAULT_NAME}\` — encrypted vault mirror for this disk (the file this app opens).`,
    `- \`${DISK_MANIFEST_NAME}\` — disk manifest (disk id, size, updated-at).`,
    '',
    '> Do not edit these files by hand — use the app so hashes stay verifiable.',
    '',
  ].join('\n')
  const manifest = JSON.stringify(
    {
      app: 'cybermanju-os',
      version: 1,
      kind: 'disk',
      diskId: disk.id,
      diskName: disk.name ?? disk.id,
      sizeBytes,
      vaultFile: `${dir}/${DISK_VAULT_NAME}`,
      updatedAt: new Date().toISOString(),
    },
    null,
    2,
  ) + '\n'
  return [
    { path: `${dir}/${DISK_README_NAME}`, contentBase64: encodeUtf8Base64(readme) },
    { path: `${dir}/${DISK_MANIFEST_NAME}`, contentBase64: encodeUtf8Base64(manifest) },
  ]
}

/**
 * Ensure the remote side of one disk exists and visibly holds its
 * `.cybermanju` files. Never deletes anything; missing files are uploaded,
 * existing ones overwritten. Resolves with the seeded paths.
 */
export async function ensureRemoteForDisk(deps: DiskProvisionDeps): Promise<DiskProvisionResult> {
  const backend = String(deps.config.backendType ?? '')
  if (backend !== 'googleDrive' && backend !== 'github' && backend !== 'gitlab') {
    throw new Error(`unsupported: remote provisioning is for cloud providers (got '${backend || '(none)'}')`)
  }
  const token = deps.token.trim()
  if (!token) throw new Error('auth: remote provisioning needs a token — connect with OAuth or paste one first')
  const dir = diskRemoteDir(deps.disk)
  const branch = deps.branch?.trim() || deps.config.branch || 'main'
  let config = deps.config
  let repoCreated = false

  // Git providers need a repo before any file can land. When the config has
  // none yet, create one private repo named after the disk and persist it.
  if ((backend === 'github' || backend === 'gitlab') && !String(config.repoName ?? '').trim()) {
    const slug = dir.split('/').pop() ?? 'cybermanju-vault'
    const created = await deps.createRepo({
      backendType: backend,
      token,
      name: slug,
      privateRepo: true,
      description: `Private CyberManju OS disk ${deps.disk.id} (.cybermanju, encrypted)`,
      branch,
      instanceUrl: deps.instanceUrl,
    })
    if (!created?.repoName) throw new Error('network: private repo creation failed — see the toast for the error prefix')
    const saved = await deps.saveConfig({
      ...config,
      repoName: created.repoName,
      branch: created.branch || branch,
      // Keep the token that just created the repo when the row has none —
      // otherwise the seed + every later sync runs tokenless on desktop.
      ...(!String(config.token ?? '').trim() && token ? { token } : {}),
    })
    if (!saved?.id) throw new Error('network: provider config could not be saved after repo creation — retry')
    config = saved
    repoCreated = true
  }

  const files = buildDiskSeedFiles(dir, deps.disk, deps.sizeBytes, backend)
  let vaultSeeded = false
  if (deps.vaultBytes && deps.vaultBytes.length > 0 && deps.vaultBytes.length <= 5 * 1024 * 1024) {
    files.push({ path: `${dir}/${DISK_VAULT_NAME}`, contentBase64: encodeBytesBase64(deps.vaultBytes) })
    vaultSeeded = true
  } else {
    files.push({
      path: `${dir}/${DISK_VAULT_NAME}`,
      contentBase64: encodeUtf8Base64(
        `# CyberManju OS disk placeholder — ${deps.disk.id}\n# The live sync carries block data; this file marks the disk's folder.\n`,
      ),
    })
  }

  if (deps.useDirectSeed) {
    if (backend === 'googleDrive') {
      if (!deps.driveWriteDirect) throw new Error('unsupported: Drive direct write is unavailable here')
      for (const f of files) {
        await deps.driveWriteDirect({
          token,
          folderId: config.folderId || undefined,
          remotePath: f.path,
          contentBase64: f.contentBase64,
        })
      }
    } else {
      // Static git path mirrors `seedRepoDirect` (same layout the Rust seeder writes).
      const { seedRepoDirect } = await import('@/utils/gitProvision')
      const fullName = backend === 'github'
        ? String(config.repoName ?? '')
        : String(config.repoName ?? '')
      await seedRepoDirect(
        backend as 'github' | 'gitlab',
        {
          backend,
          repoName: String(config.repoName ?? ''),
          fullName,
          branch,
          url: '',
          projectId: backend === 'gitlab' ? String(config.repoName ?? '') : null,
        },
        token,
        files,
        deps.instanceUrl,
      )
    }
  } else if (backend === 'googleDrive') {
    // One `upload_remote_file` per file: the Rust Drive backend resolves
    // `parent_for(remote, create=true)`, so the folder chain is created
    // server-side on first upload. The backend reads the token out of the
    // config — merge the resolved session/draft token in when the row has
    // none, or desktop Drive seeds 401 while static direct writes succeed.
    const backendConfig = token && !String(config.token ?? '').trim() ? { ...config, token } : config
    for (const f of files) {
      await deps.uploadFile(backendConfig, f.path, f.contentBase64)
    }
  } else {
    const backendConfig = token && !String(config.token ?? '').trim() ? { ...config, token } : config
    await deps.seedFiles(backendConfig, files)
  }
  return { remoteDir: dir, files: files.map((f) => f.path), repoCreated, config, vaultSeeded }
}

export interface DiskCreateDeps {
  config: SyncConfig
  sizeMb: number
  passphrase: string
  diskName?: string
  token: string
  vaultBytes?: Uint8Array | null
  useDirectSeed: boolean
  branch?: string
  instanceUrl?: string
  createDisk: (configId: string, sizeBytes: number, passphrase: string) => Promise<DiskRef | null>
  attachDisk: (diskId: string, passphrase: string) => Promise<unknown>
  provisionDeps: Omit<DiskProvisionDeps, 'config' | 'disk' | 'token' | 'sizeBytes' | 'vaultBytes' | 'useDirectSeed' | 'branch' | 'instanceUrl'>
}

/**
 * Full create-disk flow: local sealed container first (so the disk row always
 * exists), then the remote folder/repo + `.cybermanju` files. A remote
 * failure never deletes the disk — it comes back as `remoteWarning` while
 * the disk stays created + attached.
 */
export async function provisionDiskWithRemote(deps: DiskCreateDeps): Promise<{
  disk: DiskRef
  remote: DiskProvisionResult | null
  remoteWarning: string
}> {
  const mb = Math.min(8192, Math.max(64, Math.round(deps.sizeMb || 512)))
  const sizeBytes = mb * 1024 * 1024
  if (!deps.passphrase) throw new Error('auth: a disk passphrase must not be empty — type one on the disk card first')
  const row = await deps.createDisk(deps.config.id, sizeBytes, deps.passphrase)
  if (!row?.id) throw new Error('network: disk row could not be created — retry')
  const disk: DiskRef = { id: row.id, name: deps.diskName || row.name || row.id }
  await deps.attachDisk(disk.id, deps.passphrase).catch(() => undefined)
  const backend = String(deps.config.backendType ?? '')
  if (backend === 'local') return { disk, remote: null, remoteWarning: '' }
  try {
    const remote = await ensureRemoteForDisk({
      ...deps.provisionDeps,
      config: deps.config,
      disk,
      token: deps.token,
      sizeBytes,
      vaultBytes: deps.vaultBytes,
      useDirectSeed: deps.useDirectSeed,
      branch: deps.branch,
      instanceUrl: deps.instanceUrl,
    })
    return { disk, remote, remoteWarning: '' }
  } catch (e) {
    return { disk, remote: null, remoteWarning: e instanceof Error ? e.message : String(e) }
  }
}
