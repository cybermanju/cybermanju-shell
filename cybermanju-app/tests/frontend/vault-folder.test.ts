// CyberManju OS — Vault folder.test (tests/frontend/vault-folder.test.ts).
// One-folder vault layout — loop guards (pure, no browser needed).
import { describe, expect, it } from 'vitest'
import {
  SYNC_SUBDIR,
  VAULT_FILENAME,
  baseName,
  isProtectedSyncPath,
  isSyncTempArtifact,
  isVaultContainerPath,
  oneFolderLayout,
  suggestLoopSafeSubdir,
} from '../../src/utils/vaultFolder'

describe('vaultFolder guards', () => {
  it('names the vault + files/ layout', () => {
    expect(VAULT_FILENAME).toBe('vault.cybermanju')
    expect(SYNC_SUBDIR).toBe('files')
    expect(oneFolderLayout('MyVault').syncVirtualPath).toBe('/MyVault/files')
  })

  it('detects vault containers (any case, any sep)', () => {
    expect(isVaultContainerPath('vault.cybermanju')).toBe(true)
    expect(isVaultContainerPath('/a/Vault.CYBERMANJU')).toBe(true)
    expect(isVaultContainerPath('C:\\v\\disk.cybermanju')).toBe(true)
    expect(isVaultContainerPath('/a/notes.txt')).toBe(false)
    expect(isVaultContainerPath('/a/notes.txt.cyb3')).toBe(false)
  })

  it('only skips pipeline-owned .cyb3 temps, not user triple output', () => {
    expect(isSyncTempArtifact('cybermanju-up-1-2-3.cyb3')).toBe(true)
    expect(isSyncTempArtifact('/tmp/cybermanju-up-9.cyb3')).toBe(true)
    expect(isSyncTempArtifact('/notes.txt.cyb3')).toBe(false)
  })

  it('protects vault + secrets + temps from sync', () => {
    expect(isProtectedSyncPath('/v/vault.cybermanju')).toBe(true)
    expect(isProtectedSyncPath('/v/disk.cybermanju')).toBe(true)
    expect(isProtectedSyncPath('/tmp/cybermanju-up-1.cyb3')).toBe(true)
    expect(isProtectedSyncPath('/data/master.passphrase')).toBe(true)
    expect(isProtectedSyncPath('/data/keystore.json')).toBe(true)
    expect(isProtectedSyncPath('/v/.cybermanju/rules.md')).toBe(true)
    expect(isProtectedSyncPath('/v/files/notes.txt')).toBe(false)
    expect(isProtectedSyncPath('/v/files/a.cyb3')).toBe(false)
  })

  it('suggests the files/ subdir for risky manual paths', () => {
    expect(suggestLoopSafeSubdir('/v/files')).toBeNull()
    expect(suggestLoopSafeSubdir('/v/vault.cybermanju')).toBe('/v/files')
    expect(baseName('C:\\v\\notes.txt')).toBe('notes.txt')
  })
})
