// CyberManju OS — Backup.test (tests/frontend/backup.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, @/….
// Contracts: names sort chronologically; prune never touches unmanaged
// files; header errors carry `invalid:`.
import { describe, expect, it } from 'vitest'
import {
  BACKUP_DIR,
  buildBackupName,
  DEFAULT_BACKUP_KEEP,
  isBackupName,
  managedBackupScript,
  MANAGED_SCRIPT_PATH,
  pruneVictims,
  verifyContainerHeader,
} from '@/utils/backup'
import { MAX_CONTAINER_ITERATIONS } from '@/utils/container'

describe('backup names', () => {
  it('shapes chronological file names', () => {
    const name = buildBackupName(new Date('2026-10-11T20:15:00Z'))
    expect(name).toBe('cybermanju-20261011T201500Z.cybermanju')
    expect(isBackupName(name)).toBe(true)
    expect(isBackupName('notes.md')).toBe(false)
    expect(isBackupName('cybermanju-x.txt')).toBe(false)
    expect(isBackupName(`${BACKUP_DIR}/${name}`)).toBe(true)
  })
})

describe('pruneVictims', () => {
  const names = [
    'cybermanju-20261011T200000Z.cybermanju',
    'cybermanju-20261011T210000Z.cybermanju',
    'cybermanju-20261011T220000Z.cybermanju',
  ]
  it('keeps the newest N', () => {
    expect(pruneVictims(names, 2)).toEqual(['cybermanju-20261011T200000Z.cybermanju'])
    expect(pruneVictims(names, 3)).toEqual([])
    expect(pruneVictims(names, 0)).toHaveLength(3)
  })
})

describe('managedBackupScript', () => {
  it('owns the whole job via frontmatter', () => {
    const source = managedBackupScript('30 2 * * *', '/providers/docs/backups', { keep: 5 })
    expect(source).toContain('# schedule: 30 2 * * *')
    expect(source).toContain('backup export --to /providers/docs/backups --keep 5')
    expect(source).not.toContain('--plain')
    expect(MANAGED_SCRIPT_PATH).toBe('/.cybermanju/scheduled-backup.cybsh')
    expect(DEFAULT_BACKUP_KEEP).toBe(7)
  })

  it('quotes destinations with spaces and records --plain', () => {
    const source = managedBackupScript('@daily', '/providers/my drive/bk', { plain: true })
    expect(source).toContain('backup export --to "/providers/my drive/bk"')
    expect(source).toContain('--plain')
  })
})

describe('verifyContainerHeader', () => {
  function container(flags: number, body: Uint8Array): Uint8Array {
    const encrypted = (flags & 2) !== 0
    const saltLen = encrypted ? 16 : 0
    const nonceLen = encrypted ? 12 : 0
    const bodyAt = 16 + saltLen + nonceLen
    const out = new Uint8Array(bodyAt + body.length)
    new TextEncoder().encodeInto('CYBMJ01', out)
    out[7] = 1
    out[8] = encrypted ? 1 : 0
    out[9] = flags
    out[10] = saltLen
    out[11] = nonceLen
    new DataView(out.buffer).setUint32(12, encrypted ? 1000 : 0, true)
    out.set(body, bodyAt)
    return out
  }

  it('parses sealed and plaintext headers', () => {
    const sealed = verifyContainerHeader(container(2, new Uint8Array(16)))
    expect(sealed).toMatchObject({ legacy: false, version: 1, encrypted: true, compressed: false, bodyBytes: 16, totalBytes: 60 })
    const plain = verifyContainerHeader(container(0, new Uint8Array([9])))
    expect(plain.encrypted).toBe(false)
  })

  it('passes legacy images through and refuses damage', () => {
    const legacy = verifyContainerHeader(new TextEncoder().encode('raw redb bytes'))
    expect(legacy.legacy).toBe(true)
    expect(() => verifyContainerHeader(new TextEncoder().encode('CYBMJ01'))).toThrow(/invalid:/)
    const badVersion = container(0, new Uint8Array([1]))
    badVersion[7] = 9
    expect(() => verifyContainerHeader(badVersion)).toThrow(/invalid: container version 9/)
  })

  it('rejects inconsistent encryption metadata, unknown flags, and excessive KDF work', () => {
    const badNonce = container(2, new Uint8Array(16))
    badNonce[11] = 11
    expect(() => verifyContainerHeader(badNonce)).toThrow(/nonce length/)

    const unknownFlags = container(4, new Uint8Array([1]))
    expect(() => verifyContainerHeader(unknownFlags)).toThrow(/unknown container flags/)

    const plaintextWithKdf = container(0, new Uint8Array([1]))
    plaintextWithKdf[8] = 1
    expect(() => verifyContainerHeader(plaintextWithKdf)).toThrow(/plaintext container/)

    const excessiveKdf = container(2, new Uint8Array(16))
    new DataView(excessiveKdf.buffer).setUint32(12, MAX_CONTAINER_ITERATIONS + 1, true)
    expect(() => verifyContainerHeader(excessiveKdf)).toThrow(/iterations/)
  })
})
