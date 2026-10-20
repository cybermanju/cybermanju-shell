// CyberManju OS — one-folder vault layout (pure, unit-tested).
//
// One directory holds both things without letting the sync engine eat itself:
//
//   <picked>/
//     vault.cybermanju   <- the vault DB container, NEVER synced
//     files/             <- the `local` provider root (plain mirrored copies)
//
// The pipeline treats these as poison paths and skips them quietly:
// `*.cybermanju`, `cybermanju-up-*.cyb3` temps, `master.passphrase`,
// `keystore.json`. The vault changes hash on every save, so without the
// exclusion it would re-sync forever.

export const VAULT_FILENAME = 'vault.cybermanju'
export const SYNC_SUBDIR = 'files'

const VAULT_EXT = '.cybermanju'
const TEMP_PREFIX = 'cybermanju-up-'
const TEMP_EXT = '.cyb3'

/** Basename of a path (both `/` and `\` separators). */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? ''
}

/** True for the vault DB container itself (`vault.cybermanju`, any `*.cybermanju`). */
export function isVaultContainerPath(path: string): boolean {
  const base = baseName(path.trim()).toLowerCase()
  if (!base) return false
  return base.endsWith(VAULT_EXT)
}

/** True for in-flight sync artifacts (`<orig>.cyb3`, `cybermanju-up-*.cyb3`). */
export function isSyncTempArtifact(path: string): boolean {
  const base = baseName(path.trim()).toLowerCase()
  if (!base) return false
  if (base.startsWith(TEMP_PREFIX) && base.endsWith(TEMP_EXT)) return true
  // A bare `.cyb3` next to its original is a travel bag, not a source file.
  // Only the pipeline-owned `cybermanju-up-` temps are auto-skipped; user
  // `.cyb3` files sync normally so `compress triple` output still backs up.
  return false
}

const SECRET_BASENAMES = new Set(['master.passphrase', 'keystore.json'])

/** True for any path the sync engine must never upload (quiet skip, not error). */
export function isProtectedSyncPath(path: string): boolean {
  const clean = path.trim().replace(/\\/g, '/')
  if (!clean) return false
  const lower = clean.toLowerCase()
  if (lower.includes('/.cybermanju/') || lower.startsWith('.cybermanju/')) return true
  const base = baseName(lower)
  if (base.endsWith(VAULT_EXT)) return true
  if (base.startsWith(TEMP_PREFIX) && base.endsWith(TEMP_EXT)) return true
  if (SECRET_BASENAMES.has(base)) return true
  return false
}

/** Layout for a one-folder pick: vault file + `files/` sync root. */
export interface OneFolderLayout {
  /** Display label of the picked folder (`MyVault`). */
  folderName: string
  /** Vault file inside it (`vault.cybermanju`). */
  vaultName: string
  /** Sync subdir inside it (`files`). */
  syncSubdir: string
  /** Browser-style virtual path for the sync root (`/MyVault/files`). */
  syncVirtualPath: string
}

export function oneFolderLayout(folderName: string): OneFolderLayout {
  const clean = folderName.trim().replace(/^\/+|\/+$/g, '') || 'vault'
  return {
    folderName: clean,
    vaultName: VAULT_FILENAME,
    syncSubdir: SYNC_SUBDIR,
    syncVirtualPath: `/${clean}/${SYNC_SUBDIR}`,
  }
}

/**
 * Given a manually typed `basePath`, suggest the loop-safe variant.
 * Returns `null` when the path already looks safe.
 */
export function suggestLoopSafeSubdir(basePath: string): string | null {
  const clean = basePath.trim().replace(/\\/g, '/').replace(/\/+$/g, '')
  if (!clean) return null
  const lower = clean.toLowerCase()
  if (lower.endsWith(`/${SYNC_SUBDIR}`)) return null
  if (lower.endsWith(VAULT_EXT)) {
    return `${clean.replace(/\/[^/]*$/, '')}/${SYNC_SUBDIR}`
  }
  // A bare folder pick is fine as-is only when the vault is NOT inside it.
  // We cannot know that from the string alone, so nudge toward `files/`.
  if (!lower.includes('/')) return null
  return `${clean}/${SYNC_SUBDIR}`
}
