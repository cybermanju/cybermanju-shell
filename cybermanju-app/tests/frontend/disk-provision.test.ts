// CyberManju OS — Disk provision.test (tests/frontend/disk-provision.test.ts).
// Disk ↔ remote provisioning: per-disk dirs, Drive folder creation via
// upload, GitHub private-repo creation + file seed, local short-circuit.
import { describe, expect, it, vi } from 'vitest'
import {
  DISK_MANIFEST_NAME,
  DISK_VAULT_NAME,
  buildDiskSeedFiles,
  diskRemoteDir,
  ensureRemoteForDisk,
  provisionDiskWithRemote,
  slugifySegment,
  type DiskProvisionDeps,
} from '@/utils/diskProvision'
import type { SyncConfig } from '@/types'

function cfg(over: Partial<SyncConfig> = {}): SyncConfig {
  return {
    id: 'cfg-1',
    backendType: 'googleDrive',
    name: 'Drive',
    enabled: true,
    ...over,
  } as unknown as SyncConfig
}

function deps(over: Partial<DiskProvisionDeps> = {}): DiskProvisionDeps {
  return {
    config: cfg(),
    disk: { id: 'disk-abcdef123456', name: 'photos' },
    token: 'tok',
    sizeBytes: 512 * 1024 * 1024,
    useDirectSeed: false,
    uploadFile: vi.fn(async () => 'url'),
    seedFiles: vi.fn(async () => ['url']),
    createRepo: vi.fn(async () => null),
    saveConfig: vi.fn(async (c: SyncConfig) => c),
    ...over,
  }
}

describe('diskRemoteDir', () => {
  it('namespaces every disk under cybermanju-disks', () => {
    const dir = diskRemoteDir({ id: 'disk-abcdef123456', name: 'photos' })
    expect(dir.startsWith('cybermanju-disks/')).toBe(true)
    expect(dir).toContain('photos')
    expect(dir).toContain('abcdef12')
  })

  it('falls back to the disk id when unnamed', () => {
    expect(diskRemoteDir({ id: 'disk-xyz' })).toContain('cybermanju-disks/')
  })

  it('rejects bad segments', () => {
    expect(slugifySegment('ok-name_1', 'disk')).toBe('ok-name_1')
    expect(slugifySegment('bad name!', 'disk-abc')).toBe('bad-name')
    expect(slugifySegment('!!!', 'disk-abc')).toBe('disk-abc')
    expect(slugifySegment('', 'disk-abc')).toBe('disk-abc')
  })
})

describe('buildDiskSeedFiles', () => {
  it('seeds README + manifest naming the disk', () => {
    const files = buildDiskSeedFiles('cybermanju-disks/photos-ab12', { id: 'disk-ab12', name: 'photos' }, 1024, 'googleDrive')
    const paths = files.map((f) => f.path)
    expect(paths).toContain('cybermanju-disks/photos-ab12/README.md')
    expect(paths).toContain(`cybermanju-disks/photos-ab12/${DISK_MANIFEST_NAME}`)
    for (const f of files) expect(() => atob(f.contentBase64)).not.toThrow()
  })
})

describe('ensureRemoteForDisk', () => {
  it('uploads manifest + vault file for Drive (folders created server-side)', async () => {
    const d = deps()
    const out = await ensureRemoteForDisk(d)
    expect(out.remoteDir.startsWith('cybermanju-disks/')).toBe(true)
    expect(out.files).toContain(`${out.remoteDir}/${DISK_MANIFEST_NAME}`)
    expect(out.files).toContain(`${out.remoteDir}/${DISK_VAULT_NAME}`)
    expect(out.repoCreated).toBe(false)
    expect(d.uploadFile).toHaveBeenCalledTimes(3)
  })

  it('creates a private repo first when a git config has none, then seeds', async () => {
    const createRepo = vi.fn(async () => ({ repoName: 'owner/photos-ab12', fullName: 'owner/photos-ab12', branch: 'main' }))
    const seedFiles = vi.fn(async () => ['u'])
    const saved = cfg({ backendType: 'github', repoName: 'owner/photos-ab12', branch: 'main' })
    const saveConfig = vi.fn(async () => saved)
    const d = deps({
      config: cfg({ backendType: 'github', repoName: '' }),
      createRepo, seedFiles, saveConfig,
    })
    const out = await ensureRemoteForDisk(d)
    expect(createRepo).toHaveBeenCalledOnce()
    expect(createRepo.mock.calls[0][0].privateRepo).toBe(true)
    expect(saveConfig).toHaveBeenCalledOnce()
    expect(seedFiles).toHaveBeenCalledOnce()
    expect(out.repoCreated).toBe(true)
    expect(out.config.repoName).toBe('owner/photos-ab12')
  })

  it('refuses local backends and missing tokens', async () => {
    await expect(ensureRemoteForDisk(deps({ config: cfg({ backendType: 'local' }) }))).rejects.toThrow('unsupported:')
    await expect(ensureRemoteForDisk(deps({ token: '' }))).rejects.toThrow('auth:')
  })
})

describe('provisionDiskWithRemote', () => {
  function createDeps(over: Record<string, unknown> = {}) {
    const provisionDeps = {
      uploadFile: vi.fn(async () => 'url'),
      seedFiles: vi.fn(async () => ['url']),
      createRepo: vi.fn(async () => null),
      saveConfig: vi.fn(async (c: SyncConfig) => c),
    }
    return {
      provisionDeps,
      args: {
        config: cfg(),
        sizeMb: 512,
        passphrase: 'pw',
        token: 'tok',
        useDirectSeed: false,
        createDisk: vi.fn(async () => ({ id: 'disk-new', name: 'Disk new' })),
        attachDisk: vi.fn(async () => undefined),
        provisionDeps,
        ...over,
      },
    }
  }

  it('creates + attaches the disk, then seeds the remote', async () => {
    const { args } = createDeps()
    const out = await provisionDiskWithRemote(args as unknown as Parameters<typeof provisionDiskWithRemote>[0])
    expect(out.disk.id).toBe('disk-new')
    expect(args.createDisk).toHaveBeenCalledOnce()
    expect(args.attachDisk).toHaveBeenCalledOnce()
    expect(out.remote?.files.length).toBe(3)
    expect(out.remoteWarning).toBe('')
  })

  it('keeps the disk when the remote step fails (warning, not a throw)', async () => {
    const { args } = createDeps({ token: '' })
    const out = await provisionDiskWithRemote(args as unknown as Parameters<typeof provisionDiskWithRemote>[0])
    expect(out.disk.id).toBe('disk-new')
    expect(out.remote).toBeNull()
    expect(out.remoteWarning).toContain('auth:')
  })

  it('skips the remote step for local disks', async () => {
    const { args } = createDeps({ config: cfg({ backendType: 'local' }) })
    const out = await provisionDiskWithRemote(args as unknown as Parameters<typeof provisionDiskWithRemote>[0])
    expect(out.remote).toBeNull()
    expect(out.remoteWarning).toBe('')
  })
})
