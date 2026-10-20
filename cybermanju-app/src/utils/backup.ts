// CyberManju OS — Backup (src/utils/backup.ts).
// Role: Pure backup helpers shared by the shell verb, the store and the UI.
// Exports: BACKUP_DIR, MANAGED_SCRIPT_PATH, DEFAULT_BACKUP_KEEP,
//   buildBackupName, isBackupName, pruneVictims, managedBackupScript,
//   verifyContainerHeader, ContainerHeader.
// Integrates: container.ts format constants only (no crypto or WASM load).
// Contracts: names (`cybermanju-<UTC stamp>.cybermanju`) sort chronologically;
//   prune never touches non-managed files; header errors carry AGENT-1
//   prefixes (`invalid:`); deep trial-decrypt stays with the caller
//   (`decodeContainer`), which maps failures to `auth:` at restore time.

import {
  CONTAINER_VERSION,
  FLAG_COMPRESSED,
  FLAG_ENCRYPTED,
  KDF_NONE,
  KDF_PBKDF2_SHA512,
  MAX_CONTAINER_FILE_BYTES,
  MAX_CONTAINER_IMAGE_BYTES,
  MAX_CONTAINER_ITERATIONS,
} from './container'

/** Default vault directory for local snapshot exports (native volume). */
export const BACKUP_DIR = '/backups'
/** The UI-managed scheduled-backup script (frontmatter carries the expr). */
export const MANAGED_SCRIPT_PATH = '/.cybermanju/scheduled-backup.cybsh'
/** Default prune depth (mirrors `scripts/backup.sh --keep` + Rust). */
export const DEFAULT_BACKUP_KEEP = 7

const PREFIX = 'cybermanju-'
const EXT = '.cybermanju'
const MAGIC = 'CYBMJ01'
const HEADER_FIXED = 16
const SALT_LEN = 16
const NONCE_LEN = 12
const AEAD_TAG_LEN = 16

function stamp(d: Date): string {
  const p = (n: number, w = 2) => String(n).padStart(w, '0')
  return (
    `${d.getUTCFullYear()}${p(d.getUTCMonth() + 1)}${p(d.getUTCDate())}` +
    `T${p(d.getUTCHours())}${p(d.getUTCMinutes())}${p(d.getUTCSeconds())}Z`
  )
}

/** Fresh snapshot file name (`cybermanju-<stamp>.cybermanju`). */
export function buildBackupName(now = new Date()): string {
  return `${PREFIX}${stamp(now)}${EXT}`
}

/** True for files this feature manages (prune never touches anything else). */
export function isBackupName(name: string): boolean {
  const base = name.split('/').pop() ?? name
  return base.startsWith(PREFIX) && base.endsWith(EXT)
}

/** Names beyond `keep` (oldest first) — the prune victims. */
export function pruneVictims(names: string[], keep: number): string[] {
  const sorted = [...names].sort()
  if (sorted.length <= keep) return []
  return sorted.slice(0, sorted.length - keep)
}

/** Shell-quote one operand (the managed script embeds the destination). */
function quote(s: string): string {
  return /[\s"']/u.test(s) ? `"${s.replace(/"/g, '\\"')}"` : s
}

export interface ManagedBackupOptions {
  keep?: number
  plain?: boolean
}

/**
 * The UI-managed scheduled-backup script. The `# schedule:` frontmatter
 * means `cron add <path>` needs no expr — one file owns the whole job.
 */
export function managedBackupScript(
  expr: string,
  dest: string,
  options: ManagedBackupOptions = {},
): string {
  const keep = options.keep ?? DEFAULT_BACKUP_KEEP
  const plain = options.plain ? ' --plain' : ''
  return (
    `# schedule: ${expr}\n` +
    `# desc: Nightly vault snapshot (managed by the Backup panel — edit here, then \`cron add\` again).\n` +
    `backup export --to ${quote(dest)} --keep ${keep}${plain}\n`
  )
}

export interface ContainerHeader {
  legacy: boolean
  version: number
  encrypted: boolean
  compressed: boolean
  bodyBytes: number
  totalBytes: number
}

/**
 * Header-only verification of snapshot bytes (no key needed). Throws
 * `invalid:` on structural damage; legacy raw images report `legacy: true`.
 */
export function verifyContainerHeader(bytes: Uint8Array): ContainerHeader {
  if (bytes.byteLength === 0) throw new Error('invalid: backup image is empty')
  if (bytes.byteLength > MAX_CONTAINER_FILE_BYTES) {
    throw new Error('too_large: backup container exceeds the 256 MiB image cap')
  }
  const magic = new TextDecoder().decode(bytes.subarray(0, MAGIC.length))
  if (magic !== MAGIC) {
    if (bytes.byteLength > MAX_CONTAINER_IMAGE_BYTES) {
      throw new Error('too_large: legacy backup image exceeds the 256 MiB cap')
    }
    return {
      legacy: true,
      version: 0,
      encrypted: false,
      compressed: false,
      bodyBytes: bytes.length,
      totalBytes: bytes.length,
    }
  }
  if (bytes.length < HEADER_FIXED) throw new Error('invalid: container header truncated')
  const version = bytes[7]
  if (version !== CONTAINER_VERSION) throw new Error(`invalid: container version ${version}`)
  const kdf = bytes[8]
  const flags = bytes[9]
  const saltLen = bytes[10]
  const nonceLen = bytes[11]
  const encrypted = (flags & FLAG_ENCRYPTED) !== 0
  const compressed = (flags & FLAG_COMPRESSED) !== 0
  if ((flags & ~(FLAG_COMPRESSED | FLAG_ENCRYPTED)) !== 0) {
    throw new Error(`invalid: unknown container flags ${flags}`)
  }
  const iterations = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(12, true)
  if (encrypted) {
    if (kdf !== KDF_PBKDF2_SHA512) throw new Error(`invalid: container kdf ${kdf}`)
    if (saltLen !== SALT_LEN || nonceLen !== NONCE_LEN) {
      throw new Error('invalid: encrypted container has invalid salt or nonce length')
    }
    if (iterations < 1 || iterations > MAX_CONTAINER_ITERATIONS) {
      throw new Error(`invalid: PBKDF2 iterations must be between 1 and ${MAX_CONTAINER_ITERATIONS}`)
    }
  } else if (kdf !== KDF_NONE || saltLen !== 0 || nonceLen !== 0 || iterations !== 0) {
    throw new Error('invalid: plaintext container has encrypted metadata')
  }
  const bodyAt = HEADER_FIXED + saltLen + nonceLen
  if (bytes.length < bodyAt) throw new Error('invalid: container header truncated')
  const bodyBytes = bytes.length - bodyAt
  const minimumBodyBytes = (encrypted ? AEAD_TAG_LEN : 0) + (compressed ? 4 : 0)
  if (bodyBytes < minimumBodyBytes) throw new Error('invalid: container body truncated')
  return {
    legacy: false,
    version,
    encrypted,
    compressed,
    bodyBytes,
    totalBytes: bytes.length,
  }
}
