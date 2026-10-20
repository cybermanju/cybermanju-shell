// CyberManju OS — Git provision.test (tests/frontend/git-provision.test.ts).
// Private vault repo provisioning — validation, naming, seed layout.
import { describe, expect, it, vi } from 'vitest'
import {
  MAX_VAULT_REPOS,
  VAULT_FILE_PATH,
  VAULT_MANIFEST_PATH,
  VAULT_README_PATH,
  buildSeedFiles,
  defaultVaultRepoName,
  encodeUtf8Base64,
  expandRepoNames,
  lastSegment,
  provisionSyncedSystem,
  provisionVaultRepoSet,
  suggestRepoName,
  validateRepoCount,
  validateRepoName,
  vaultManifest,
  vaultReadme,
} from '@/utils/gitProvision'

describe('validateRepoName', () => {
  it('accepts plain and owner/repo forms', () => {
    expect(validateRepoName('cybermanju-vault')).toBe('')
    expect(validateRepoName('owner/cybermanju-vault')).toBe('')
  })

  it('rejects empties, reserved names and bad characters', () => {
    expect(validateRepoName('')).not.toBe('')
    expect(validateRepoName('///')).not.toBe('')
    expect(validateRepoName('bad name!')).not.toBe('')
    expect(validateRepoName('.')).not.toBe('')
    // A trailing slash still resolves to the last segment.
    expect(validateRepoName('owner/my-vault/')).toBe('')
  })
})

describe('lastSegment / suggestRepoName', () => {
  it('takes the last path segment', () => {
    expect(lastSegment('owner/my-vault/')).toBe('my-vault')
  })

  it('bumps taken names', () => {
    expect(suggestRepoName('vault', ['vault'])).toBe('vault-2')
    expect(suggestRepoName('vault', [])).toBe('vault')
    expect(suggestRepoName('', [])).toBe(defaultVaultRepoName())
  })
})

describe('seed layout', () => {
  it('builds README + manifest with the vault file named', () => {
    const files = buildSeedFiles('owner/vault', 'main')
    const paths = files.map((f) => f.path)
    expect(paths).toContain(VAULT_README_PATH)
    expect(paths).toContain(VAULT_MANIFEST_PATH)
    expect(vaultReadme('owner/vault', 'main')).toContain('vault.cybermanju')
    expect(vaultReadme('owner/vault', 'main')).toContain(VAULT_FILE_PATH)
    const manifest = JSON.parse(vaultManifest('owner/vault', 'main')) as Record<string, unknown>
    expect(manifest.vaultFile).toBe(VAULT_FILE_PATH)
    // README + manifest decode from base64 cleanly.
    for (const f of files) {
      expect(() => atob(f.contentBase64)).not.toThrow()
    }
    expect(encodeUtf8Base64('hi')).toBe(btoa('hi'))
  })
})

describe('provisionSyncedSystem', () => {
  it('saves config, seeds, probes, then creates + attaches a disk', async () => {
    const calls: string[] = []
    const saved = { id: 'cfg-1', backendType: 'github', repoName: 'o/v', branch: 'main' }
    const out = await provisionSyncedSystem({
      backendType: 'github',
      repo: { backend: 'github', repoName: 'o/v', fullName: 'o/v', branch: 'main', url: 'https://x', projectId: null },
      token: 'tok',
      displayName: 'o/v vault',
      diskSizeMb: 128,
      diskPassphrase: '',
      vaultBytes: new Uint8Array([1, 2, 3]),
      saveConfig: async (c) => {
        calls.push('save')
        expect(c.repoName).toBe('o/v')
        // Vault sets are secure by default: fail-closed uploads, hidden names.
        expect(c.requireEncryption).toBe(true)
        expect(c.obfuscateNames).toBe(true)
        return saved as never
      },
      seedViaBackend: async (_c, files) => {
        calls.push(`seed:${files.length}`)
        expect(files.some((f) => f.path === 'vault.cybermanju')).toBe(true)
        return []
      },
      probe: async () => {
        calls.push('probe')
        return { ok: true, detail: 'ok' }
      },
      createDisk: async () => {
        calls.push('disk')
        return { id: 'disk-1' }
      },
      attachDisk: async () => {
        calls.push('attach')
      },
      useDirectSeed: false,
    })
    expect(out.config.id).toBe('cfg-1')
    expect(out.diskId).toBe('disk-1')
    expect(calls).toEqual(['save', 'seed:3', 'probe', 'disk', 'attach'])
  })

  it('fails loudly when the probe fails (repo still exists remotely)', async () => {
    const saveConfig = vi.fn(async () => ({ id: 'cfg-9' }) as never)
    const seedViaBackend = vi.fn(async () => [])
    await expect(
      provisionSyncedSystem({
        backendType: 'gitlab',
        repo: { backend: 'gitlab', repoName: '42', fullName: 'g/v', branch: 'main', url: 'https://x', projectId: '42' },
        token: 'tok',
        displayName: 'g/v vault',
        saveConfig,
        seedViaBackend,
        probe: async () => ({ ok: false, detail: 'auth: bad token' }),
        createDisk: async () => ({ id: 'd' }),
        attachDisk: async () => undefined,
        useDirectSeed: false,
      }),
    ).rejects.toThrow(/probe failed/)
    expect(seedViaBackend).toHaveBeenCalledOnce()
  })
})

describe('repo sets (multi-repo merged disks)', () => {
  it('clamps counts and expands suffixed names', () => {
    expect(validateRepoCount(0)).toBe(1)
    expect(validateRepoCount(3)).toBe(3)
    expect(validateRepoCount(99)).toBe(MAX_VAULT_REPOS)
    expect(validateRepoCount('nope')).toBe(1)
    expect(expandRepoNames('vault', 1)).toEqual(['vault'])
    expect(expandRepoNames('vault', 3)).toEqual(['vault-1', 'vault-2', 'vault-3'])
    // Owner prefix survives; suffix lands on the last segment only.
    expect(expandRepoNames('owner/vault', 2)).toEqual(['owner/vault-1', 'owner/vault-2'])
    for (const n of expandRepoNames('vault', 8)) expect(validateRepoName(n)).toBe('')
  })

  it('stamps set membership into the manifest', () => {
    const solo = JSON.parse(vaultManifest('o/v', 'main')) as Record<string, unknown>
    expect(solo.set).toBeUndefined()
    const m = JSON.parse(
      vaultManifest('o/v-1', 'main', { name: 'v', index: 1, total: 3 }),
    ) as Record<string, { index: number; total: number }>
    expect(m.set.index).toBe(1)
    expect(m.set.total).toBe(3)
    expect(vaultReadme('o/v-1', 'main', { name: 'v', index: 1, total: 3 })).toContain('1 of 3')
  })

  it('provisions N repos with mirrored vault + one disk each', async () => {
    const created: string[] = []
    const stages: string[] = []
    const set = await provisionVaultRepoSet({
      backendType: 'github',
      baseName: 'vault',
      count: 3,
      token: 'tok',
      diskSizeMb: 100,
      diskPassphrase: 's3cret',
      vaultBytes: new Uint8Array([9, 9]),
      createRepo: async (input) => {
        created.push(input.name)
        const seg = input.name.split('/').pop() ?? input.name
        return { backend: 'github', repoName: `o/${seg}`, fullName: `o/${seg}`, branch: 'main', url: `https://x/o/${seg}`, projectId: null }
      },
      saveConfig: async (c) => ({ id: `cfg-${c.repoName}`, ...c }) as never,
      seedViaBackend: async (_c, files) => {
        // Mirrored encrypted vault in every repo of the set.
        expect(files.some((f) => f.path === 'vault.cybermanju')).toBe(true)
        return []
      },
      probe: async () => ({ ok: true, detail: 'ok' }),
      createDisk: async (configId) => ({ id: `disk-${configId}` }),
      attachDisk: async () => undefined,
      useDirectSeed: false,
      onProgress: (_d, _t, stage) => {
        stages.push(stage)
      },
    })
    expect(created).toEqual(['vault-1', 'vault-2', 'vault-3'])
    expect(set.repos).toHaveLength(3)
    expect(set.diskIds).toEqual(['disk-cfg-o/vault-1', 'disk-cfg-o/vault-2', 'disk-cfg-o/vault-3'])
    expect(set.totalDiskBytes).toBe(3 * 100 * 1024 * 1024)
    expect(set.failures).toEqual([])
    expect(stages).toContain('synced')
  })

  it('collects per-repo failures and still resolves the survivors', async () => {
    const set = await provisionVaultRepoSet({
      backendType: 'gitlab',
      baseName: 'vault',
      count: 3,
      token: 'tok',
      createRepo: async (input) => {
        if (input.name.endsWith('-2')) throw new Error('conflict: that repo name is taken on GitLab')
        return { backend: 'gitlab', repoName: '42', fullName: input.name, branch: 'main', url: 'https://x', projectId: '42' }
      },
      saveConfig: async (c) => ({ id: `cfg-${c.repoName}`, ...c }) as never,
      seedViaBackend: async () => [],
      probe: async () => ({ ok: true, detail: 'ok' }),
      createDisk: async () => null,
      attachDisk: async () => undefined,
      useDirectSeed: false,
    })
    expect(set.repos).toHaveLength(2)
    expect(set.failures).toHaveLength(1)
    expect(set.failures[0].name).toBe('vault-2')
    expect(set.totalDiskBytes).toBe(0)
  })

  it('rejects only when every repo failed', async () => {
    await expect(
      provisionVaultRepoSet({
        backendType: 'github',
        baseName: 'vault',
        count: 2,
        token: 'bad',
        createRepo: async () => {
          throw new Error('auth: GitHub rejected the token (HTTP 401)')
        },
        saveConfig: async () => null,
        seedViaBackend: async () => [],
        probe: async () => ({ ok: true, detail: 'ok' }),
        createDisk: async () => null,
        attachDisk: async () => undefined,
        useDirectSeed: false,
      }),
    ).rejects.toThrow(/auth:/)
  })

  it('retries GitHub overwrites with the looked-up blob SHA', async () => {
    const { seedRepoDirect } = await import('@/utils/gitProvision')
    const bodies: string[] = []
    const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
      bodies.push(String(init?.body ?? ''))
      const method = init?.method ?? 'GET'
      if (method === 'PUT' && !bodies[bodies.length - 1].includes('"sha"')) {
        return new Response('{"message": "already exists"}', { status: 422 })
      }
      if (method === 'GET') return new Response('{"sha": "abc123sha"}', { status: 200 })
      return new Response('{"content": {}}', { status: 200 })
    })
    vi.stubGlobal('fetch', fakeFetch)
    try {
      const urls = await seedRepoDirect(
        'github',
        { backend: 'github', repoName: 'o/v', fullName: 'o/v', branch: 'main', url: '', projectId: null },
        'tok',
        [{ path: 'vault.cybermanju', contentBase64: 'aGk=' }],
      )
      expect(urls).toHaveLength(1)
      // Second PUT carried the SHA from the lookup — a real overwrite.
      expect(bodies.some((b) => b.includes('"sha":"abc123sha"'))).toBe(true)
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('writes new Drive files with parent folders + overwrites by id', async () => {
    const { driveWriteDirect, driveDeleteDirect } = await import('@/utils/gitProvision')
    const calls: Array<{ url: string; method: string }> = []
    const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      calls.push({ url, method })
      if (url.includes('/drive/v3/files?q=')) return new Response('{"files":[]}', { status: 200 })
      if (url.includes('upload/drive/v3/files?uploadType=multipart')) {
        return new Response('{"id":"new-file-id"}', { status: 200 })
      }
      if (url === 'https://www.googleapis.com/drive/v3/files' && method === 'POST') {
        return new Response('{"id":"folder-docs"}', { status: 200 })
      }
      if (method === 'DELETE') return new Response(null, { status: 204 })
      return new Response('{}', { status: 200 })
    })
    vi.stubGlobal('fetch', fakeFetch)
    try {
      const id = await driveWriteDirect({
        token: 'tok',
        remotePath: 'docs/note.txt',
        contentBase64: 'aGk=',
      })
      expect(id).toBe('new-file-id')
      // One folder create (docs/) + one multipart upload.
      expect(calls.filter((c) => c.method === 'POST')).toHaveLength(2)
      expect(calls.some((c) => c.url.includes('uploadType=multipart'))).toBe(true)
      await driveDeleteDirect({ token: 'tok', remotePath: 'docs/note.txt', locator: 'new-file-id' })
      const del = calls.find((c) => c.method === 'DELETE')
      expect(del?.url).toContain('/drive/v3/files/new-file-id')
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('overwrites an existing Drive child instead of duplicating it', async () => {
    const { driveWriteDirect } = await import('@/utils/gitProvision')
    const patched: string[] = []
    const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (url.includes('/drive/vfs') || url.includes('/drive/v3/files?q=')) {
        return new Response('{"files":[{"id":"child-1"}]}', { status: 200 })
      }
      if (url.includes('/upload/drive/v3/files/child-1')) {
        patched.push(url)
        return new Response('{}', { status: 200 })
      }
      return new Response('{"files":[{"id":"child-1"}]}', { status: 200 })
    })
    vi.stubGlobal('fetch', fakeFetch)
    try {
      const id = await driveWriteDirect({ token: 'tok', remotePath: 'note.txt', contentBase64: 'aGk=' })
      expect(id).toBe('child-1')
      expect(patched).toHaveLength(1)
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('skips the vault file seed when the container exceeds the seed cap', async () => {
    let seeded = 0
    const out = await provisionSyncedSystem({
      backendType: 'github',
      repo: { backend: 'github', repoName: 'o/v', fullName: 'o/v', branch: 'main', url: 'https://x', projectId: null },
      token: 'tok',
      displayName: 'o/v vault',
      vaultBytes: new Uint8Array(6 * 1024 * 1024),
      saveConfig: async (c) => ({ id: 'cfg-1', ...c }) as never,
      seedViaBackend: async (_c, files) => {
        seeded = files.length
        expect(files.some((f) => f.path === 'vault.cybermanju')).toBe(false)
        return []
      },
      probe: async () => ({ ok: true, detail: 'ok' }),
      createDisk: async () => null,
      attachDisk: async () => undefined,
      useDirectSeed: false,
    })
    expect(out.vaultSeeded).toBe(false)
    expect(seeded).toBe(2)
  })
})
