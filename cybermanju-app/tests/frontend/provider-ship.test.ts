// CyberManju OS — Provider ship.test (tests/frontend/provider-ship.test.ts).
// CONTROL 5.8 — `.cybermanju` ship test: mounts + cache + master passphrase
// live in the redb `kv` table, so `_save`/`_attach` of the container carries
// the whole provider namespace. This pins the key-naming contract (pure,
// no worker needed): every VFS row the canal writes must sit under a `kv`
// key, never beside the container.
import { describe, expect, it } from 'vitest'
import {
  cacheKey,
  mountKey,
  parseProviderPath,
  providerPathFor,
  VFS_CACHE_PREFIX,
  VFS_MOUNT_INDEX_KEY,
  VFS_SECRET_MASTER_KEY,
} from '../../src/composables/useProviderCanal'

describe('vfs ship contract (kv carries the namespace)', () => {
  it('mount index + rows + cache + secret all live under kv keys', () => {
    expect(VFS_MOUNT_INDEX_KEY).toBe('vfs:mount:index')
    expect(mountKey('m1')).toBe('vfs:mount:m1')
    expect(cacheKey('m1', 'docs/a.md')).toBe(`${VFS_CACHE_PREFIX}m1:docs/a.md`)
    expect(VFS_SECRET_MASTER_KEY).toBe('vfs:secret:master')
    for (const k of [VFS_MOUNT_INDEX_KEY, mountKey('m1'), cacheKey('m1', 'x'), VFS_SECRET_MASTER_KEY]) {
      expect(k.startsWith('vfs:')).toBe(true)
    }
  })

  it('cache keys normalize the same remote path one way', () => {
    expect(cacheKey('m', '/a/b')).toBe(cacheKey('m', 'a/b'))
  })

  it('provider paths round-trip, so FileGrid browse hits the same rows', () => {
    const p = providerPathFor('mnt-1', 'docs/a.md')
    expect(parseProviderPath(p)).toEqual({ mountId: 'mnt-1', remotePath: 'docs/a.md' })
    expect(parseProviderPath('/providers/mnt-1')).toEqual({ mountId: 'mnt-1', remotePath: '' })
  })
})
