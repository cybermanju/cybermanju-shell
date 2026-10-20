// CyberManju OS — Provider canal.test (tests/frontend/provider-canal.test.ts).
// Provider canal orchestration — path mapping, cache TTL, mount validation.
import { describe, expect, it, vi } from 'vitest'
import {
  cacheKey,
  isCacheFresh,
  mountKey,
  parseProviderPath,
  providerPathFor,
  shouldCacheVfsListing,
  VFS_CACHE_TTL_MS,
} from '../../src/composables/useProviderCanal'

describe('parseProviderPath', () => {
  it('splits mount id from the remote path', () => {
    expect(parseProviderPath('/providers/mnt-1/a/b')).toEqual({ mountId: 'mnt-1', remotePath: 'a/b' })
  })

  it('accepts a bare mount root', () => {
    expect(parseProviderPath('/providers/mnt-1')).toEqual({ mountId: 'mnt-1', remotePath: '' })
  })

  it('rejects paths outside the namespace', () => {
    expect(parseProviderPath('/files/x')).toBeNull()
    expect(parseProviderPath('/providers')).toBeNull()
    expect(parseProviderPath('')).toBeNull()
  })

  it('normalizes backslashes and leading slashes', () => {
    expect(parseProviderPath('\\providers\\m\\a')).toEqual({ mountId: 'm', remotePath: 'a' })
  })
})

describe('providerPathFor', () => {
  it('round-trips through parseProviderPath', () => {
    const p = providerPathFor('mnt-9', 'docs/a.md')
    expect(parseProviderPath(p)).toEqual({ mountId: 'mnt-9', remotePath: 'docs/a.md' })
  })

  it('handles the mount root', () => {
    expect(providerPathFor('mnt-9', '')).toBe('/providers/mnt-9')
  })
})

describe('cache keys and TTL', () => {
  it('keeps ordinary provider cache but always refreshes a native SAF folder', () => {
    expect(shouldCacheVfsListing('github')).toBe(true)
    expect(shouldCacheVfsListing('googleDrive')).toBe(true)
    expect(shouldCacheVfsListing('scopedStorage')).toBe(false)
  })

  it('namespaces cache rows per mount + path', () => {
    expect(cacheKey('m', 'a/b')).toBe('vfs:cache:m:a/b')
    expect(cacheKey('m', '/a/b')).toBe('vfs:cache:m:a/b')
    expect(mountKey('m')).toBe('vfs:mount:m')
  })

  it('treats fresh rows as hits and old rows as misses', () => {
    const now = 1_000_000
    expect(isCacheFresh(now - 1_000, now)).toBe(true)
    expect(isCacheFresh(now - VFS_CACHE_TTL_MS - 1, now)).toBe(false)
    expect(isCacheFresh(NaN, now)).toBe(false)
  })
})

describe('saveVfsMount validation', () => {
  it('refuses mounts without a config', async () => {
    const mod = await import('../../src/composables/useProviderCanal')
    await expect(
      mod.saveVfsMount({ configId: '  ', name: 'x', backendType: 'github' }),
    ).rejects.toThrow('invalid: configId is required')
    void vi
  })
})

describe('vfs write-through validation', () => {
  it('refuses empty paths before touching any backend', async () => {
    const mod = await import('../../src/composables/useProviderCanal')
    await expect(mod.writeVfsFile('m', '', new Uint8Array([1]))).rejects.toThrow('invalid: remotePath is required')
    await expect(mod.deleteVfsFile('m', '')).rejects.toThrow('invalid: remotePath is required')
  })

  it('refuses path escapes out of the mount', async () => {
    const mod = await import('../../src/composables/useProviderCanal')
    await expect(mod.writeVfsFile('m', '../evil.txt', 'x')).rejects.toThrow('unsupported:')
  })

  it('refuses oversized writes up front (5 MiB cap)', async () => {
    const mod = await import('../../src/composables/useProviderCanal')
    await expect(
      mod.writeVfsFile('m', 'big.bin', new Uint8Array(6 * 1024 * 1024)),
    ).rejects.toThrow('too_large:')
  })

  it('reports unknown mounts as not_found', async () => {
    const mod = await import('../../src/composables/useProviderCanal')
    await expect(mod.writeVfsFile('mnt-missing', 'a.txt', 'hi')).rejects.toThrow('not_found:')
    await expect(mod.deleteVfsFile('mnt-missing', 'a.txt')).rejects.toThrow('not_found:')
  })
})
