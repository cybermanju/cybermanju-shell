// CyberManju OS — Static cybsh.test (tests/frontend/static-cybsh.test.ts).
// Static-host cybsh — vault-aware verbs answer locally, provider file ops
// cross the merged namespace. All browser effects are faked here (node env,
// no window); `useTauri.ts` supplies the real deps in production.
import { describe, expect, it } from 'vitest'
import {
  DASHBOARD_NOTE,
  handlesStaticVerb,
  humanBytes,
  isOauthBackend,
  joinVolumePath,
  normalizeBackend,
  oauthSlug,
  parseCybshLine,
  probeProviderQuotaViaFetch,
  runStaticCybshLine,
  runStaticSchedule,
  splitProviderPath,
  staticConfigFromRow,
  STATIC_WRITE_LIMIT,
  type ProviderQuotaProbe,
  type StaticCybshDeps,
  type StaticSchedule,
  type StaticScheduleRun,
  type StaticSyncConfig,
  type StaticVfsEntry,
} from '../../src/utils/staticCybsh'

const enc = (s: string): Uint8Array => new TextEncoder().encode(s)
const dec = (b: Uint8Array): string => new TextDecoder().decode(b)

function xorCrypt(key: Uint8Array, nonce: Uint8Array, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(data.length)
  for (let i = 0; i < data.length; i++) out[i] = data[i] ^ key[i % key.length] ^ nonce[i % nonce.length]
  return out
}

interface FakeState {
  volume: Record<string, string>
  providers: Record<string, Record<string, Uint8Array>>
  secrets: Record<string, string>
  kv: Record<string, string>
  configs: StaticSyncConfig[]
  cwd: string
  probe: (cfg: StaticSyncConfig, token: string) => Promise<ProviderQuotaProbe>
  killed: number[]
}

function fakeDeps(state: Partial<FakeState> = {}): { deps: StaticCybshDeps; state: FakeState } {
  const full: FakeState = {
    volume: { '/notes.txt': 'hello world' },
    providers: { m1: { 'docs/a.md': enc('# readme'), 'docs/b.md': enc('second') } },
    secrets: {},
    kv: {},
    configs: [],
    cwd: '/',
    // Mirrors production: an empty token never probes — it is an auth error.
    probe: async (cfg, token) =>
      token
        ? {
            configId: cfg.id,
            backendType: cfg.backendType,
            ok: true,
            detail: 'fake probe',
          }
        : {
            configId: cfg.id,
            backendType: cfg.backendType,
            ok: false,
            detail: '',
            error: `auth: ${cfg.id} has no token — paste a PAT on its provider card or connect OAuth via the dashboard, then retry`,
          },
    killed: [],
    ...state,
  }
  const listEntries = (mount: string, remote: string): StaticVfsEntry[] => {
    const tree = full.providers[mount] ?? {}
    const prefix = remote ? `${remote}/` : ''
    const seen = new Map<string, StaticVfsEntry>()
    for (const key of Object.keys(tree)) {
      if (!key.startsWith(prefix)) continue
      const rest = key.slice(prefix.length)
      if (!rest) continue
      const slash = rest.indexOf('/')
      if (slash < 0) {
        seen.set(rest, { name: rest, path: key, isDir: false, sizeBytes: tree[key].length })
      } else {
        const dir = rest.slice(0, slash)
        if (!seen.has(dir)) seen.set(dir, { name: dir, path: prefix + dir, isDir: true, sizeBytes: 0 })
      }
    }
    return [...seen.values()]
  }
  const deps: StaticCybshDeps = {
    readVolume: () => ({ ...full.volume }),
    getCwd: async () => full.cwd,
    writeVolumeFile: async (path, content) => {
      full.volume[path] = content
    },
    deleteVolumePath: async (path, recursive) => {
      const prefix = path === '/' ? '/' : `${path}/`
      const keys = Object.keys(full.volume).filter((k) => k === path || k.startsWith(prefix))
      if (!recursive && keys.some((k) => k !== path)) throw new Error(`is a directory: ${path} (use -r)`)
      let freed = 0
      for (const k of keys) {
        freed += full.volume[k]?.length ?? 0
        delete full.volume[k]
      }
      return freed
    },
    killTask: async (id) => {
      if (id === 7) throw new Error('no such table')
      if (full.killed.includes(id)) return false
      full.killed.push(id)
      return true
    },
    listSyncConfigs: async () => full.configs.map((c) => ({ ...c })),
    getConfigSecret: async (configId) => full.secrets[configId] ?? '',
    getDiskStatus: async () => ({ attached: true, name: 'vault.cybermanju', savedBytes: 42, dirty: false }),
    getStorageEstimate: async () => ({ usage: 100, quota: 1000 }),
    listMounts: async () => [{ id: 'm1', name: 'docs', backendType: 'github', configId: 'c1' }],
    providerRead: async (mount, remote) => {
      const bytes = full.providers[mount]?.[remote]
      if (!bytes) throw new Error(`not_found: ${remote}`)
      return bytes
    },
    providerWrite: async (mount, remote, data) => {
      if (!full.providers[mount]) throw new Error(`not_found: provider mount ${mount}`)
      full.providers[mount][remote] = data
    },
    providerDelete: async (mount, remote) => {
      if (!full.providers[mount]?.[remote]) throw new Error(`not_found: ${remote}`)
      delete full.providers[mount][remote]
    },
    providerList: async (mount, remote) => listEntries(mount, remote),
    probeProviderQuota: (cfg, token) => full.probe(cfg, token),
    keyGet: async (name) => full.kv[name] ?? null,
    keySet: async (name, value) => {
      full.kv[name] = value
    },
    chacha: async () => ({
      genKey: () => new Uint8Array(32).fill(7),
      genNonce: () => new Uint8Array(12).fill(3),
      encrypt: (k, n, p) => xorCrypt(k, n, p),
      decrypt: (k, n, c) => xorCrypt(k, n, c),
    }),
    codecs: async () => ({
      compressLz4: (d) => d,
      decompressLz4: (d) => d,
      compressBrotli: (d) => d,
      decompressBrotli: (d) => d,
    }),
    blake3: async (data) => `h:${data.length}:${data.slice(0, 8)}`,
  }
  return { deps, state: full }
}

describe('parseCybshLine', () => {
  it('tokenizes quotes and strips --json', () => {
    expect(parseCybshLine(`cp "my file.txt" out.txt --json`)).toEqual({
      verb: 'cp',
      args: ['my file.txt', 'out.txt'],
      json: true,
    })
  })

  it('lowercases the verb', () => {
    expect(parseCybshLine('QUOTA')?.verb).toBe('quota')
  })

  it('returns null for chained lines, empty lines', () => {
    expect(parseCybshLine('quota && ls')).toBeNull()
    expect(parseCybshLine('ls; pwd')).toBeNull()
    expect(parseCybshLine('ls | grep x')).toBeNull()
    expect(parseCybshLine('   ')).toBeNull()
  })

  it('keeps chain characters inside quotes', () => {
    expect(parseCybshLine(`echo "a && b"`)).toEqual({ verb: 'echo', args: ['a && b'], json: false })
  })

  it('handles single quotes, || chains and lone &', () => {
    expect(parseCybshLine(`cp 'my file.txt' out.txt`)).toEqual({
      verb: 'cp',
      args: ['my file.txt', 'out.txt'],
      json: false,
    })
    expect(parseCybshLine('ls || pwd')).toBeNull()
    // A lone & is not a chain operator — it stays a plain token.
    expect(parseCybshLine('echo a & b')).toEqual({ verb: 'echo', args: ['a', '&', 'b'], json: false })
    expect(parseCybshLine('quota --json --json')).toEqual({ verb: 'quota', args: [], json: true })
    expect(parseCybshLine('ls')).toEqual({ verb: 'ls', args: [], json: false })
  })
})

describe('path helpers', () => {
  it('joins like the Rust shell', () => {
    expect(joinVolumePath('/a/b', '../..')).toBe('/')
    expect(joinVolumePath('/', 'x/y')).toBe('/x/y')
    expect(joinVolumePath('/a', '/b')).toBe('/b')
    expect(joinVolumePath('/a/b', '..')).toBe('/a')
    expect(joinVolumePath('/', '.')).toBe('/')
    expect(joinVolumePath('/a', '../../..')).toBe('/')
    expect(joinVolumePath('/a/b', './c/./d')).toBe('/a/b/c/d')
  })

  it('splits provider paths', () => {
    expect(splitProviderPath('/providers/m1')).toEqual({ mountId: 'm1', remotePath: '' })
    expect(splitProviderPath('/providers/m1/docs/a.md')).toEqual({ mountId: 'm1', remotePath: 'docs/a.md' })
    expect(splitProviderPath('/providers/m1/docs/')).toEqual({ mountId: 'm1', remotePath: 'docs' })
    expect(splitProviderPath('/providers/m1/')).toEqual({ mountId: 'm1', remotePath: '' })
    expect(splitProviderPath('\\providers\\m1\\a')).toEqual({ mountId: 'm1', remotePath: 'a' })
    expect(splitProviderPath('/notes.txt')).toBeNull()
    expect(splitProviderPath('/providers')).toBeNull()
  })

  it('formats bytes like the native human()', () => {
    expect(humanBytes(0)).toBe('0B')
    expect(humanBytes(1536)).toBe('1.5k')
    expect(humanBytes(1024)).toBe('1.0k')
    expect(humanBytes(1.5 * 1024 ** 3)).toBe('1.5G')
    expect(humanBytes(2 * 1024 ** 4)).toBe('2.0T')
    expect(humanBytes(null)).toBe('?')
    expect(humanBytes(undefined)).toBe('?')
    expect(humanBytes(NaN)).toBe('?')
  })

  it('normalizes backend names and config rows', () => {
    expect(normalizeBackend('GoogleDrive')).toBe('googleDrive')
    expect(normalizeBackend('gitHub')).toBe('github')
    expect(normalizeBackend('  Drive  ')).toBe('googleDrive')
    expect(normalizeBackend('google')).toBe('googleDrive')
    expect(normalizeBackend('localDir')).toBe('local')
    expect(normalizeBackend('')).toBe('unknown')
    expect(normalizeBackend('telegram')).toBe('telegram')
    expect(staticConfigFromRow({ id: 'c', backend_type: 'github', enabled: true })?.backendType).toBe('github')
    expect(staticConfigFromRow({ backendType: 'x' })).toBeNull()
    const full = staticConfigFromRow({
      id: 'c2',
      backendType: 'gitlab',
      enabled: false,
      name: 'n',
      repo_name: 'a/b',
      branch: 'dev',
      instance_url: 'https://git.example.com/',
      folder_id: 'f1',
    })
    expect(full).toMatchObject({
      id: 'c2',
      backendType: 'gitlab',
      enabled: false,
      repoName: 'a/b',
      branch: 'dev',
      basePath: 'https://git.example.com/',
      folderId: 'f1',
    })
    expect(staticConfigFromRow({ id: 'c3', backendType: 42, enabled: 'yes' })?.backendType).toBe('unknown')
  })

  it('maps oauth capability and slugs', () => {
    expect(isOauthBackend('github')).toBe(true)
    expect(isOauthBackend('googleDrive')).toBe(true)
    expect(isOauthBackend('local')).toBe(false)
    expect(isOauthBackend('telegram')).toBe(false)
    expect(oauthSlug('googleDrive')).toBe('google')
    expect(oauthSlug('gitlab')).toBe('gitlab')
    expect(oauthSlug('local')).toBeNull()
    expect(oauthSlug('telegram')).toBeNull()
  })

  it('exposes the verb gate and shared constants', () => {
    expect(handlesStaticVerb('QUOTA')).toBe(true)
    expect(handlesStaticVerb('frobnicate')).toBe(false)
    expect(STATIC_WRITE_LIMIT).toBe(1024 * 1024)
    expect(DASHBOARD_NOTE).toContain(':3456')
  })
})

describe('probeProviderQuotaViaFetch', () => {
  const cfg: StaticSyncConfig = { id: 'c1', backendType: 'github', enabled: true }
  const okFetch =
    (body: unknown, status = 200) =>
    async () => ({ ok: status === 200, status, json: async () => body })

  it('reads the GitHub rate-limit window', async () => {
    const probe = await probeProviderQuotaViaFetch(
      cfg,
      'tok',
      okFetch({ resources: { core: { limit: 5000, remaining: 4990, reset: 123 } } }),
    )
    expect(probe.ok).toBe(true)
    expect(probe.remainingRequests).toBe(4990)
    expect(probe.detail).toContain('rate-limit')
  })

  it('refuses without a token and on 401', async () => {
    const noToken = await probeProviderQuotaViaFetch(cfg, '', okFetch({}))
    expect(noToken.ok).toBe(false)
    expect(noToken.error).toMatch(/^auth:/)
    const denied = await probeProviderQuotaViaFetch(cfg, 'bad', okFetch({}, 401))
    expect(denied.ok).toBe(false)
    expect(denied.error).toMatch(/^auth:/)
  })

  it('reports unreachable hosts as network:', async () => {
    const probe = await probeProviderQuotaViaFetch(cfg, 'tok', async () => {
      throw new Error('fetch failed')
    })
    expect(probe.ok).toBe(false)
    expect(probe.error).toMatch(/^network:/)
  })

  it('reads GitLab statistics and Drive quotaInfo', async () => {
    const gitlab: StaticSyncConfig = { id: 'g', backendType: 'gitlab', enabled: true, repoName: 'a/b' }
    const gl = await probeProviderQuotaViaFetch(
      gitlab,
      'tok',
      okFetch({ statistics: { storage_size: 99 } }),
    )
    expect(gl.usedBytes).toBe(99)
    const drive: StaticSyncConfig = { id: 'd', backendType: 'googleDrive', enabled: true }
    const dr = await probeProviderQuotaViaFetch(
      drive,
      'tok',
      okFetch({ quotaInfo: { usage: '100', usageInDriveTrash: '5', limit: '1000' } }),
    )
    expect(dr.usedBytes).toBe(105)
    expect(dr.totalBytes).toBe(1000)
    const local: StaticSyncConfig = { id: 'l', backendType: 'local', enabled: true }
    expect((await probeProviderQuotaViaFetch(local, '', okFetch({}))).ok).toBe(true)
    const weird: StaticSyncConfig = { id: 'w', backendType: 'telegram', enabled: true }
    const un = await probeProviderQuotaViaFetch(weird, 'tok', okFetch({}))
    expect(un.error).toMatch(/^unsupported:/)
  })

  it('maps GitHub failures honestly', async () => {
    const denied = await probeProviderQuotaViaFetch(cfg, 'tok', okFetch({}, 500))
    expect(denied.ok).toBe(false)
    expect(denied.error).toMatch(/^network: GitHub quota probe failed/)
    const unreadable = await probeProviderQuotaViaFetch(cfg, 'tok', async () => ({
      ok: true,
      status: 200,
      json: async () => {
        throw new Error('bad json')
      },
    }))
    expect(unreadable.ok).toBe(false)
    expect(unreadable.error).toMatch(/unreadable/)
    const empty = await probeProviderQuotaViaFetch(cfg, 'tok', okFetch({}))
    expect(empty.ok).toBe(true)
    expect(empty.remainingRequests).toBeNull()
    expect(empty.requestLimit).toBeNull()
    expect(empty.resetAt).toBeNull()
  })

  it('covers every GitLab branch', async () => {
    const noRepo: StaticSyncConfig = { id: 'g', backendType: 'gitlab', enabled: true }
    const missing = await probeProviderQuotaViaFetch(noRepo, 'tok', okFetch({}))
    expect(missing.ok).toBe(false)
    expect(missing.error).toMatch(/^not_found:/)
    const authed = await probeProviderQuotaViaFetch(
      { ...noRepo, repoName: 'a/b' },
      'bad',
      okFetch({}, 401),
    )
    expect(authed.ok).toBe(false)
    expect(authed.error).toMatch(/^auth:/)
    const forbidden = await probeProviderQuotaViaFetch(
      { ...noRepo, repoName: 'a/b' },
      'tok',
      okFetch({}, 403),
    )
    expect(forbidden.ok).toBe(true)
    expect(forbidden.usedBytes).toBeUndefined()
    expect(forbidden.detail).toContain('maintainer access')
    const fallback = await probeProviderQuotaViaFetch(
      { ...noRepo, repoName: 'a/b' },
      'tok',
      okFetch({ statistics: { repository_size: 7 } }),
    )
    expect(fallback.usedBytes).toBe(7)
    const unreadable = await probeProviderQuotaViaFetch(
      { ...noRepo, repoName: 'a/b' },
      'tok',
      async () => ({
        ok: true,
        status: 200,
        json: async () => {
          throw new Error('bad json')
        },
      }),
    )
    expect(unreadable.error).toMatch(/unreadable/)
    // A self-managed instance is used verbatim; its failures name the host.
    let seenUrl = ''
    const custom = await probeProviderQuotaViaFetch(
      { ...noRepo, repoName: 'a/b', basePath: 'https://git.example.com/' },
      'tok',
      (async (url: string) => {
        seenUrl = url
        throw new Error('down')
      }) as never,
    )
    expect(seenUrl).toContain('https://git.example.com/api/v4/projects/a%2Fb')
    expect(custom.error).toContain('git.example.com')
  })

  it('covers every Drive branch', async () => {
    const drive: StaticSyncConfig = { id: 'd', backendType: 'googleDrive', enabled: true }
    const denied = await probeProviderQuotaViaFetch(drive, 'bad', okFetch({}, 401))
    expect(denied.error).toMatch(/^auth:/)
    const down = await probeProviderQuotaViaFetch(drive, 'tok', okFetch({}, 500))
    expect(down.error).toMatch(/^network: Google Drive quota probe failed/)
    const unreadable = await probeProviderQuotaViaFetch(drive, 'tok', async () => ({
      ok: true,
      status: 200,
      json: async () => {
        throw new Error('bad json')
      },
    }))
    expect(unreadable.error).toMatch(/unreadable/)
    const empty = await probeProviderQuotaViaFetch(drive, 'tok', okFetch({}))
    expect(empty.ok).toBe(true)
    expect(empty.detail).toContain('no quotaInfo')
    const numeric = await probeProviderQuotaViaFetch(
      drive,
      'tok',
      okFetch({ quotaInfo: { usage: 50, limit: 200 } }),
    )
    expect(numeric.usedBytes).toBe(50)
    expect(numeric.totalBytes).toBe(200)
    const broken = await probeProviderQuotaViaFetch(
      drive,
      'tok',
      okFetch({ quotaInfo: { usage: 'abc', limit: '' } }),
    )
    expect(broken.usedBytes).toBeNull()
    expect(broken.totalBytes).toBeNull()
  })
})

describe('runStaticCybshLine dispatch', () => {
  it('returns null for unknown verbs and runs all-static chains', async () => {
    const { deps } = fakeDeps()
    expect(await runStaticCybshLine('frobnicate', deps)).toBeNull()
    const chained = await runStaticCybshLine('echo local && providers', deps)
    expect(chained?.ok).toBe(true)
    expect(chained?.output).toContain('local')
    expect(chained?.output).toContain('mount m1 → github (docs)')
    const separated = await runStaticCybshLine('echo first; echo second', deps)
    expect(separated?.ok).toBe(true)
    expect(separated?.output).toBe('first\nsecond')
    expect(await runStaticCybshLine('', deps)).toBeNull()
  })

  it('runs a vault verb before falling back to the WASM volume in a chain', async () => {
    const { deps } = fakeDeps({
      configs: [{ id: 'c1', backendType: 'github', enabled: true }],
      secrets: { c1: 'test-token' },
    })
    const fallbackCalls: string[] = []
    deps.execFallback = async (line) => {
      fallbackCalls.push(line)
      return 'wasm ls output'
    }
    const result = await runStaticCybshLine('quota && ls', deps)
    expect(result?.ok).toBe(true)
    expect(result?.output).toContain('c1 (github): used')
    expect(result?.output).toContain('wasm ls output')
    expect(fallbackCalls).toEqual(['ls'])
  })

  it('stops after a failed clause and refuses pipes without rejecting quoted bars', async () => {
    const { deps } = fakeDeps()
    const fallbackCalls: string[] = []
    deps.execFallback = async (line) => {
      fallbackCalls.push(line)
      return 'fallback'
    }
    const failed = await runStaticCybshLine('kill 7 && ls', deps)
    expect(failed?.ok).toBe(false)
    expect(failed?.output).toMatch(/^unsupported: kill needs/)
    expect(fallbackCalls).toEqual([])

    const piped = await runStaticCybshLine('echo left | cat', deps)
    expect(piped?.ok).toBe(false)
    expect(piped?.output).toMatch(/^unsupported: pipelines/)
    const quoted = await runStaticCybshLine('echo "left | right"', deps)
    expect(quoted?.ok).toBe(true)
    expect(quoted?.output).toBe('left | right')
  })

  it('echoes', async () => {
    const { deps } = fakeDeps()
    const res = await runStaticCybshLine('echo hello wasm', deps)
    expect(res?.ok).toBe(true)
    expect(res?.output).toBe('hello wasm')
    expect(res?.prompt).toBe('cybsh> ')
  })

  it('refuses -os honestly (no host filesystem in the browser)', async () => {
    const { deps } = fakeDeps()
    for (const line of ['ls -os', 'cat -os /sdcard/a.txt', 'cp -os /a /b', 'grep -os x']) {
      const res = await runStaticCybshLine(line, deps)
      expect(res?.ok).toBe(false)
      expect(res?.output).toMatch(/^unsupported: .* -os/)
    }
  })

  it('refuses --host/--os aliases and non-host verbs with -os', async () => {
    const { deps } = fakeDeps()
    for (const line of [
      'ls --host',
      'ls --os /sdcard',
      'cat --host /sdcard/a.txt',
      'encrypt -os /a',
      // Non-host verbs refuse too (never fall through to a volume answer)…
      'scrub -os',
      'sync -os',
      // …while unhandled verbs (`search`, `ps`, …) return null here and are
      // refused by the wasm dispatcher's own `-os` guard instead.
    ]) {
      const res = await runStaticCybshLine(line, deps)
      expect(res?.ok).toBe(false)
      expect(res?.output).toMatch(/^unsupported: /)
    }
  })

  it('refuses -os identically through the .cybsh sh() wrapper', async () => {
    // Pure command and script wrapper share the refusal: `sh("ls -os …")`
    // runs through `scriptExecCybsh` → `runStaticCybshLine`, the same choke
    // point as the terminal. (Chained `ls -os && …` lines bypass this layer
    // via `parseCybshLine → null` and are refused by the wasm dispatcher's
    // own `-os` guard instead — same `unsupported:` shape.)
    const { deps } = fakeDeps({
      volume: { '/t.cybsh': 'print sh("ls -os /")\n' },
    })
    const res = await runStaticCybshLine('run /t.cybsh', deps)
    expect(res?.ok).toBe(false)
    expect(res?.output).toMatch(/unsupported: .* -os/)
    // The script file itself is untouched by the refused run.
    expect(deps.readVolume()['/t.cybsh']).toContain('ls -os')
  })
})

describe('merged cp/mv/rm/mkdir', () => {
  it('copies and moves inside the local volume', async () => {
    const { deps, state } = fakeDeps()
    const cp = await runStaticCybshLine('cp /notes.txt /copy.txt', deps)
    expect(cp?.ok).toBe(true)
    expect(state.volume['/copy.txt']).toBe('hello world')
    const mv = await runStaticCybshLine('mv /copy.txt /moved.txt', deps)
    expect(mv?.ok).toBe(true)
    expect(state.volume['/moved.txt']).toBe('hello world')
    expect(state.volume['/copy.txt']).toBeUndefined()
    const missing = await runStaticCybshLine('cp /nope.txt /x.txt', deps)
    expect(missing?.ok).toBe(false)
    expect(missing?.output).toMatch(/^not_found:/)
  })

  it('copies provider → local and local → provider', async () => {
    const { deps, state } = fakeDeps()
    const down = await runStaticCybshLine('cp /providers/m1/docs/a.md /a-local.md', deps)
    expect(down?.ok).toBe(true)
    expect(state.volume['/a-local.md']).toBe('# readme')
    const up = await runStaticCybshLine('cp /notes.txt /providers/m1/uploaded.txt', deps)
    expect(up?.ok).toBe(true)
    expect(dec(state.providers.m1['uploaded.txt'])).toBe('hello world')
  })

  it('moves across providers (copy then delete the source)', async () => {
    const { deps, state } = fakeDeps({
      providers: { m1: { 'a.md': enc('payload') }, m2: {} },
    })
    const mv = await runStaticCybshLine('mv /providers/m1/a.md /providers/m2/a.md', deps)
    expect(mv?.ok).toBe(true)
    expect(dec(state.providers.m2['a.md'])).toBe('payload')
    expect(state.providers.m1['a.md']).toBeUndefined()
  })

  it('copies provider directories only with -r, into existing dirs by basename', async () => {
    const { deps, state } = fakeDeps()
    const flat = await runStaticCybshLine('cp /providers/m1/docs /docs-copy', deps)
    expect(flat?.ok).toBe(false)
    expect(flat?.output).toMatch(/use -r/)
    const rec = await runStaticCybshLine('cp -r /providers/m1/docs /providers/m1/docs-copy', deps)
    expect(rec?.ok).toBe(true)
    expect(dec(state.providers.m1['docs-copy/a.md'])).toBe('# readme')
    expect(dec(state.providers.m1['docs-copy/b.md'])).toBe('second')
    // A fresh local prefix becomes the tree root.
    const local = await runStaticCybshLine('cp -r /providers/m1/docs /local-docs', deps)
    expect(local?.ok).toBe(true)
    expect(state.volume['/local-docs/a.md']).toBe('# readme')
    expect(state.volume['/local-docs/b.md']).toBe('second')
  })

  it('refuses binary provider files for the text volume', async () => {
    const { deps } = fakeDeps({ providers: { m1: { 'bin.dat': new Uint8Array([0xff, 0xfe, 0x00]) } } })
    const res = await runStaticCybshLine('cp /providers/m1/bin.dat /bin.txt', deps)
    expect(res?.ok).toBe(false)
    expect(res?.output).toMatch(/^unsupported: binary/)
  })

  it('removes files and trees, -f forgives the missing', async () => {
    const { deps, state } = fakeDeps()
    const one = await runStaticCybshLine('rm /notes.txt', deps)
    expect(one?.ok).toBe(true)
    expect(state.volume['/notes.txt']).toBeUndefined()
    const prov = await runStaticCybshLine('rm /providers/m1/docs/a.md', deps)
    expect(prov?.ok).toBe(true)
    expect(state.providers.m1['docs/a.md']).toBeUndefined()
    const dir = await runStaticCybshLine('rm /providers/m1/docs', deps)
    expect(dir?.ok).toBe(false)
    expect(dir?.output).toMatch(/use -r/)
    const rec = await runStaticCybshLine('rm -r /providers/m1/docs', deps)
    expect(rec?.ok).toBe(true)
    expect(state.providers.m1['docs/b.md']).toBeUndefined()
    const forgiven = await runStaticCybshLine('rm -f /missing.txt', deps)
    expect(forgiven?.ok).toBe(true)
    const strict = await runStaticCybshLine('rm /missing.txt', deps)
    expect(strict?.ok).toBe(false)
    expect(strict?.output).toMatch(/^not_found:/)
  })

  it('makes directories locally and on mounts', async () => {
    const { deps, state } = fakeDeps()
    const local = await runStaticCybshLine('mkdir -p /a/b', deps)
    expect(local?.ok).toBe(true)
    expect(state.volume['/a/b/.keep']).toBe('')
    const prov = await runStaticCybshLine('mkdir /providers/m1/newdir', deps)
    expect(prov?.ok).toBe(true)
    expect(state.providers.m1['newdir/.keep']).toBeDefined()
    const clash = await runStaticCybshLine('mkdir /notes.txt', deps)
    expect(clash?.ok).toBe(false)
    expect(clash?.output).toMatch(/^conflict:/)
  })
})

describe('provider ls/cat (outside-container reads)', () => {
  it('lists /providers mounts plus unmounted configs', async () => {
    const { deps } = fakeDeps({
      configs: [{ id: 'c9', backendType: 'github', enabled: true, name: 'vault' }],
    })
    const res = await runStaticCybshLine('ls /providers', deps)
    expect(res?.ok).toBe(true)
    expect(res?.output).toContain('m1/')
    expect(res?.output).toContain('(mount…)')
  })

  it('lists mount dirs with trailing slashes, files bare', async () => {
    const { deps } = fakeDeps()
    const res = await runStaticCybshLine('ls /providers/m1/docs', deps)
    expect(res?.ok).toBe(true)
    expect(res?.output).toContain('a.md')
    expect(res?.output).toContain('b.md')
    const root = await runStaticCybshLine('ls /providers/m1', deps)
    expect(root?.ok).toBe(true)
    expect(root?.output).toContain('docs/')
  })

  it('refuses unknown mounts honestly', async () => {
    const { deps } = fakeDeps()
    const res = await runStaticCybshLine('ls /providers/nope', deps)
    expect(res?.ok).toBe(false)
    expect(res?.output).toMatch(/not_found: no provider mount/)
  })

  it('falls through to the wasm volume for local listings', async () => {
    const { deps } = fakeDeps()
    expect(await runStaticCybshLine('ls /notes.txt', deps)).toBeNull()
    expect(await runStaticCybshLine('ls', deps)).toBeNull()
  })

  it('cats provider files, refuses dirs and binaries honestly', async () => {
    const { deps } = fakeDeps({
      providers: { m1: { 'a.md': enc('# readme'), 'bin.dat': new Uint8Array([0xff, 0xfe]) } },
    })
    const ok = await runStaticCybshLine('cat /providers/m1/a.md', deps)
    expect(ok?.ok).toBe(true)
    expect(ok?.output).toBe('# readme')
    const missing = await runStaticCybshLine('cat /providers/m1/nope.md', deps)
    expect(missing?.ok).toBe(false)
    expect(missing?.output).toMatch(/^not_found:/)
    const binary = await runStaticCybshLine('cat /providers/m1/bin.dat', deps)
    expect(binary?.ok).toBe(false)
    expect(binary?.output).toMatch(/^unsupported: .*binary/)
  })

  it('falls through to the wasm volume for local reads', async () => {
    const { deps } = fakeDeps()
    expect(await runStaticCybshLine('cat /notes.txt', deps)).toBeNull()
  })
})

describe('vault verbs', () => {
  const gh: StaticSyncConfig = { id: 'gh', backendType: 'github', enabled: true, name: 'code' }

  it('quota reports local vault + live probes + dashboard note', async () => {
    const { deps } = fakeDeps({
      configs: [gh],
      secrets: { gh: 'tok' },
      probe: async (cfg) => ({
        configId: cfg.id,
        backendType: cfg.backendType,
        ok: true,
        remainingRequests: 4990,
        requestLimit: 5000,
        detail: 'GitHub REST rate-limit window (storage quota not published)',
      }),
    })
    const res = await runStaticCybshLine('quota', deps)
    expect(res?.ok).toBe(true)
    expect(res?.output).toContain('local vault:')
    expect(res?.output).toContain('gh (github):')
    expect(res?.output).toContain('4990/5000 requests left')
    expect(res?.output).toContain(':3456')
    const json = await runStaticCybshLine('quota --json', deps)
    expect(() => JSON.parse(json?.output ?? '')).not.toThrow()
  })

  it('quota without providers or tokens fails honestly', async () => {
    const { deps } = fakeDeps()
    const res = await runStaticCybshLine('quota', deps)
    expect(res?.ok).toBe(false)
    expect(res?.output).toContain('no enabled provider')
    const { deps: deps2 } = fakeDeps({ configs: [gh] })
    const res2 = await runStaticCybshLine('quota', deps2)
    expect(res2?.ok).toBe(false)
    expect(res2?.output).toContain('auth:')
  })

  it('providers lists signs and oauth tracks them', async () => {
    const { deps } = fakeDeps({ configs: [gh], secrets: { gh: 'tok' } })
    const p = await runStaticCybshLine('providers', deps)
    expect(p?.output).toContain('gh')
    expect(p?.output).toContain('yes')
    const st = await runStaticCybshLine('oauth status', deps)
    expect(st?.output).toContain('signed in')
    const { deps: bare } = fakeDeps({ configs: [gh] })
    const st2 = await runStaticCybshLine('oauth status', bare)
    expect(st2?.output).toContain('NOT signed in')
    const start = await runStaticCybshLine('oauth start github gh', bare)
    expect(start?.ok).toBe(false)
    expect(start?.output).toContain('paste a personal token')
    const bad = await runStaticCybshLine('oauth start telegram', bare)
    expect(bad?.output).toMatch(/^unsupported:/)
  })

  it('disk and sync answer locally, mutations refuse', async () => {
    const { deps } = fakeDeps()
    const disk = await runStaticCybshLine('disk', deps)
    expect(disk?.output).toContain('vault.cybermanju')
    const sync = await runStaticCybshLine('sync status', deps)
    expect(sync?.ok).toBe(true)
    const start = await runStaticCybshLine('sync start', deps)
    expect(start?.output).toMatch(/^unsupported:/)
    const umount = await runStaticCybshLine('umount m1', deps)
    expect(umount?.output).toMatch(/^unsupported:/)
    const mount = await runStaticCybshLine('mount', deps)
    expect(mount?.output).toContain('m1')
  })

  it('sync move guides to mv bytes or dashboard verified move', async () => {
    const { deps } = fakeDeps()
    const bare = await runStaticCybshLine('sync move', deps)
    expect(bare?.output).toMatch(/usage: sync move/)
    const full = await runStaticCybshLine('sync move f1 c1 c2', deps)
    expect(full?.ok).toBe(true)
    expect(full?.output).toContain('mv /providers/')
  })

  it('mv moves bytes across provider mounts (same op as sync move bytes)', async () => {
    const { deps, state } = fakeDeps({
      providers: { m1: { 'a.md': 'hello-bytes' }, m2: {} },
    })
    const out = await runStaticCybshLine('mv /providers/m1/a.md /providers/m2/a.md', deps)
    expect(out?.ok).toBe(true)
    expect(out?.output).toContain('moved')
    expect(state.providers.m2['a.md']).toBeDefined()
    expect(state.providers.m1['a.md']).toBeUndefined()
  })

  it('encrypts and decrypts through the vault key', async () => {
    const { deps, state } = fakeDeps()
    const kg = await runStaticCybshLine('keygen box', deps)
    expect(kg?.output).toContain(`'box'`)
    expect(state.kv['secret:cybsh:key:box']).toBeDefined()
    const encRes = await runStaticCybshLine('encrypt /notes.txt box', deps)
    expect(encRes?.ok).toBe(true)
    expect(state.volume['/notes.txt.sealed']).toContain('chacha20-poly1305')
    delete state.volume['/notes.txt']
    const decRes = await runStaticCybshLine('decrypt /notes.txt.sealed box', deps)
    expect(decRes?.ok).toBe(true)
    expect(state.volume['/notes.txt']).toBe('hello world')
  })

  it('compresses and decompresses', async () => {
    const { deps, state } = fakeDeps()
    const c = await runStaticCybshLine('compress /notes.txt lz4', deps)
    expect(c?.ok).toBe(true)
    expect(state.volume['/notes.txt.lz4']).toBeDefined()
    delete state.volume['/notes.txt']
    const d = await runStaticCybshLine('decompress /notes.txt.lz4', deps)
    expect(d?.ok).toBe(true)
    expect(state.volume['/notes.txt']).toBe('hello world')
    const bad = await runStaticCybshLine('compress /notes.txt zstd', deps)
    expect(bad?.output).toMatch(/^unsupported:/)
  })

  it('quota degrades across every failing dep', async () => {
    const gh: StaticSyncConfig = { id: 'gh', backendType: 'github', enabled: true }
    // No browser estimate, detached file: local lines still render.
    const bare = fakeDeps({
      configs: [gh],
      secrets: { gh: 'tok' },
      probe: async (cfg) => ({ configId: cfg.id, backendType: cfg.backendType, ok: true, detail: 'fake probe' }),
    })
    bare.deps.getStorageEstimate = async () => null
    bare.deps.getDiskStatus = async () => ({ attached: false, name: '', savedBytes: 0, dirty: false })
    const res = await runStaticCybshLine('quota', bare.deps)
    expect(res?.ok).toBe(true)
    expect(res?.output).toContain('none attached')
    expect(res?.output).not.toContain('browser storage:')
    // Dirty attached file is flagged.
    const dirty = fakeDeps()
    dirty.deps.getDiskStatus = async () => ({ attached: true, name: 'v.cybermanju', savedBytes: 9, dirty: true })
    const dres = await runStaticCybshLine('disk', dirty.deps)
    expect(dres?.output).toContain('UNSAVED CHANGES')
    // Disabled configs count as "no enabled provider".
    const off = fakeDeps({ configs: [{ ...gh, enabled: false }] })
    const ores = await runStaticCybshLine('quota', off.deps)
    expect(ores?.ok).toBe(false)
    expect(ores?.output).toContain('no enabled provider')
    // A throwing probe becomes an error line, not a crash.
    const throwing = fakeDeps({
      configs: [gh],
      secrets: { gh: 'tok' },
      probe: async () => {
        throw new Error('boom')
      },
    })
    const tres = await runStaticCybshLine('quota', throwing.deps)
    expect(tres?.ok).toBe(false)
    expect(tres?.output).toContain('boom')
    // A failing probe without an error string still renders honestly.
    const bare2 = fakeDeps({
      configs: [gh],
      secrets: { gh: 'tok' },
      probe: async (cfg) => ({ configId: cfg.id, backendType: cfg.backendType, ok: false, detail: 'x' }),
    })
    const bres = await runStaticCybshLine('quota', bare2.deps)
    expect(bres?.output).toContain('network: quota probe failed for gh')
    // A rejecting secret store reads as unsigned.
    const nosecret = fakeDeps({ configs: [gh] })
    nosecret.deps.getConfigSecret = async () => {
      throw new Error('no table')
    }
    const nres = await runStaticCybshLine('quota', nosecret.deps)
    expect(nres?.output).toContain('auth:')
    // JSON mode stays machine-readable even on failure.
    const jres = await runStaticCybshLine('quota --json', off.deps)
    expect(jres?.ok).toBe(false)
    const parsed = JSON.parse(jres?.output ?? '') as { providers: unknown[]; dashboardRequired: boolean }
    expect(parsed.providers).toEqual([])
    expect(parsed.dashboardRequired).toBe(true)
    // Listing or disk failures degrade to empty, never throw.
    const broken = fakeDeps()
    broken.deps.listSyncConfigs = async () => {
      throw new Error('db down')
    }
    broken.deps.getDiskStatus = async () => {
      throw new Error('no file')
    }
    const qbroken = await runStaticCybshLine('quota', broken.deps)
    expect(qbroken?.output).toContain('no enabled provider')
    const pbroken = await runStaticCybshLine('providers', broken.deps)
    expect(pbroken?.output).toContain('mount m1')
    const allbroken = fakeDeps()
    allbroken.deps.listSyncConfigs = async () => {
      throw new Error('db down')
    }
    allbroken.deps.listMounts = async () => []
    expect((await runStaticCybshLine('providers', allbroken.deps))?.output).toContain(
      'no providers connected',
    )
  })

  it('providers renders rows, mounts and json', async () => {
    const gh: StaticSyncConfig = { id: 'gh', backendType: 'github', enabled: true, repoName: 'o/r' }
    const off: StaticSyncConfig = { id: 'off', backendType: 'gitlab', enabled: false }
    const { deps } = fakeDeps({ configs: [gh, off], secrets: { gh: 'tok' } })
    const res = await runStaticCybshLine('providers', deps)
    expect(res?.ok).toBe(true)
    expect(res?.output).toContain('gh')
    expect(res?.output).toContain('off')
    expect(res?.output).toContain('mount m1')
    const json = await runStaticCybshLine('providers --json', deps)
    const parsed = JSON.parse(json?.output ?? '') as {
      providers: Array<{ id: string; hasKey: boolean; enabled: boolean }>
      mounts: unknown[]
    }
    expect(parsed.providers.find((p) => p.id === 'gh')?.hasKey).toBe(true)
    expect(parsed.providers.find((p) => p.id === 'off')?.enabled).toBe(false)
    expect(parsed.mounts).toHaveLength(1)
    // A failing mount listing still shows the config rows.
    const nomount = fakeDeps({ configs: [gh], secrets: { gh: 'tok' } })
    nomount.deps.listMounts = async () => {
      throw new Error('no canal')
    }
    const mres = await runStaticCybshLine('providers', nomount.deps)
    expect(mres?.output).toContain('gh')
    expect(mres?.output).not.toContain('mount ')
  })

  it('oauth covers list, json, signed starts and usage', async () => {
    const gh: StaticSyncConfig = { id: 'gh', backendType: 'github', enabled: true }
    const { deps } = fakeDeps({ configs: [gh], secrets: { gh: 'tok' } })
    const list = await runStaticCybshLine('oauth list', deps)
    expect(list?.output).toContain('signed in')
    const json = await runStaticCybshLine('oauth status --json', deps)
    expect(JSON.parse(json?.output ?? '')).toMatchObject({ oauth: [{ id: 'gh', signedIn: true }] })
    const signed = await runStaticCybshLine('oauth start github gh --json', deps)
    expect(JSON.parse(signed?.output ?? '')).toMatchObject({ configId: 'gh', signedIn: true })
    const signedText = await runStaticCybshLine('oauth start github gh', deps)
    expect(signedText?.ok).toBe(true)
    expect(signedText?.output).toContain('already signed in')
    // No configs at all, plus json.
    const empty = fakeDeps()
    const ejson = await runStaticCybshLine('oauth status --json', empty.deps)
    expect(JSON.parse(ejson?.output ?? '')).toMatchObject({ oauth: [] })
    // Start without a backend or target stays actionable.
    const nobackend = await runStaticCybshLine('oauth start', empty.deps)
    expect(nobackend?.output).toMatch(/^unsupported:/)
    const notarget = await runStaticCybshLine('oauth start github', empty.deps)
    expect(notarget?.output).toContain('set a dashboard URL in Settings for the full OAuth redirect flow')
    const usage = await runStaticCybshLine('oauth frobnicate', deps)
    expect(usage?.output).toMatch(/^usage: oauth/)
    const startJson = await runStaticCybshLine('oauth start github gh --json', empty.deps)
    expect(startJson?.ok).toBe(false)
    expect(() => JSON.parse(startJson?.output ?? '')).not.toThrow()
  })

  it('disk and sync cover every subcommand', async () => {
    const gh: StaticSyncConfig = { id: 'gh', backendType: 'github', enabled: false }
    const { deps } = fakeDeps({ configs: [gh] })
    const df = await runStaticCybshLine('disk df', deps)
    expect(df?.output).toContain('shell volume:')
    const list = await runStaticCybshLine('disk list --json', deps)
    const parsed = JSON.parse(list?.output ?? '') as { shellVolume: { files: number } }
    expect(parsed.shellVolume.files).toBe(1)
    const noest = fakeDeps()
    noest.deps.getStorageEstimate = async () => null
    const nres = await runStaticCybshLine('disk', noest.deps)
    expect(nres?.output).not.toContain('browser storage:')
    const mut = await runStaticCybshLine('disk create gh 1G', deps)
    expect(mut?.output).toMatch(/^unsupported: `disk create`/)
    const slist = await runStaticCybshLine('sync list', deps)
    expect(slist?.output).toContain('gh (github): disabled')
    const sjson = await runStaticCybshLine('sync status --json', deps)
    expect(JSON.parse(sjson?.output ?? '')).toMatchObject({ active: false, jobId: null })
    const cancel = await runStaticCybshLine('sync cancel', deps)
    expect(cancel?.output).toMatch(/^unsupported: `sync cancel`/)
    const usage = await runStaticCybshLine('sync frobnicate', deps)
    expect(usage?.output).toMatch(/^usage: sync/)
    const mounts = await runStaticCybshLine('mount foo', deps)
    expect(mounts?.output).toMatch(/^unsupported:/)
    const nomount = fakeDeps()
    nomount.deps.listMounts = async () => {
      throw new Error('no canal')
    }
    const mres = await runStaticCybshLine('mount', nomount.deps)
    expect(mres?.output).toContain('no provider mounts')
  })

  it('scrubs, repairs, collects, leases and kills', async () => {
    const { deps } = fakeDeps()
    const s1 = await runStaticCybshLine('scrub', deps)
    expect(s1?.output).toContain('1 new')
    const s2 = await runStaticCybshLine('scrub', deps)
    expect(s2?.output).toContain('1 verified')
    const r = await runStaticCybshLine('repair', deps)
    expect(r?.output).toContain('all present')
    const g = await runStaticCybshLine('gc', deps)
    expect(g?.output).toContain('dry run')
    const l = await runStaticCybshLine('lease', deps)
    expect(l?.ok).toBe(true)
    const k = await runStaticCybshLine('kill 3', deps)
    expect(k?.output).toContain('killed task 3')
    const k2 = await runStaticCybshLine('kill 3', deps)
    expect(k2?.output).toMatch(/^not_found:/)
    const ai = await runStaticCybshLine('ai ask "hi"', deps)
    expect(ai?.output).toMatch(/^unsupported:/)
  })
})

describe('crypto edge cases', () => {
  const b64 = (bytes: Uint8Array): string => Buffer.from(bytes).toString('base64')

  it('encrypt validates input and backend first', async () => {
    const { deps } = fakeDeps()
    expect((await runStaticCybshLine('encrypt', deps))?.output).toMatch(/^usage: encrypt/)
    const nobundle = fakeDeps()
    nobundle.deps.chacha = async () => null
    expect((await runStaticCybshLine('encrypt /notes.txt', nobundle.deps))?.output).toMatch(/^unsupported:/)
    expect((await runStaticCybshLine('keygen', nobundle.deps))?.output).toMatch(/^unsupported:/)
    expect((await runStaticCybshLine('decrypt /x.sealed', nobundle.deps))?.output).toMatch(/^unsupported:/)
    // A failing cwd falls back to root.
    const nocwd = fakeDeps()
    nocwd.deps.getCwd = async () => {
      throw new Error('no shell')
    }
    expect((await runStaticCybshLine('encrypt /notes.txt box', nocwd.deps))?.ok).toBe(true)
  })

  it('encrypt refuses directories, missing files and oversized output', async () => {
    const { deps, state } = fakeDeps({ volume: { '/d/.keep': '', '/big.txt': 'x'.repeat(900_000) } })
    expect((await runStaticCybshLine('encrypt /d', deps))?.output).toMatch(/^is a directory:/)
    expect((await runStaticCybshLine('encrypt /nope.txt', deps))?.output).toMatch(/^not_found:/)
    expect((await runStaticCybshLine('encrypt /big.txt', deps))?.output).toMatch(/^too_large:/)
  })

  it('encrypt surfaces cipher failures as integrity errors and reuses keys', async () => {
    const { deps, state } = fakeDeps()
    const failing = fakeDeps()
    failing.deps.chacha = async () => ({
      genKey: () => new Uint8Array(32).fill(1),
      genNonce: () => new Uint8Array(12).fill(2),
      encrypt: () => {
        throw new Error('HSM down')
      },
      decrypt: (k, n, c) => c,
    })
    expect((await runStaticCybshLine('encrypt /notes.txt', failing.deps))?.output).toMatch(/^integrity:/)
    const first = await runStaticCybshLine('encrypt /notes.txt box', deps)
    expect(first?.ok).toBe(true)
    const keyBefore = state.kv['secret:cybsh:key:box']
    const second = await runStaticCybshLine('encrypt /notes.txt box', deps)
    expect(second?.ok).toBe(true)
    expect(state.kv['secret:cybsh:key:box']).toBe(keyBefore)
  })

  it('keygen names keys randomly when unnamed', async () => {
    const { deps, state } = fakeDeps()
    const res = await runStaticCybshLine('keygen', deps)
    expect(res?.ok).toBe(true)
    const names = Object.keys(state.kv).filter((k) => k.startsWith('secret:cybsh:key:key-'))
    expect(names).toHaveLength(1)
    expect(res?.output).toContain(names[0].replace('secret:cybsh:key:', ''))
  })

  it('decrypt validates envelopes and keys', async () => {
    const { deps } = fakeDeps()
    expect((await runStaticCybshLine('decrypt', deps))?.output).toMatch(/^usage: decrypt/)
    expect((await runStaticCybshLine('decrypt /missing.sealed', deps))?.output).toMatch(/^not_found:/)
    deps.writeVolumeFile('/junk.sealed', 'not json')
    expect((await runStaticCybshLine('decrypt /junk.sealed', deps))?.output).toMatch(/not a cybsh sealed file/)
    deps.writeVolumeFile('/other.sealed', JSON.stringify({ alg: 'aes-256-gcm', nonce: 'eA==', data: 'eA==' }))
    expect((await runStaticCybshLine('decrypt /other.sealed', deps))?.output).toContain('desktop .sealed files')
    // Unknown key name reads as an auth error.
    await runStaticCybshLine('encrypt /notes.txt box', deps)
    expect((await runStaticCybshLine('decrypt /notes.txt.sealed ghost', deps))?.output).toMatch(/^auth:/)
  })

  it('decrypt resolves the sealed-file key mapping without an argument', async () => {
    const { deps, state } = fakeDeps()
    await runStaticCybshLine('keygen box', deps)
    await runStaticCybshLine('encrypt /notes.txt box', deps)
    const env = JSON.parse(state.volume['/notes.txt.sealed']) as Record<string, unknown>
    delete env.key
    state.volume['/notes.txt.sealed'] = JSON.stringify(env)
    delete state.volume['/notes.txt']
    const res = await runStaticCybshLine('decrypt /notes.txt.sealed', deps)
    expect(res?.ok).toBe(true)
    expect(state.volume['/notes.txt']).toBe('hello world')
  })

  it('decrypt surfaces cipher failures and writes .plain for odd names', async () => {
    const { deps, state } = fakeDeps()
    const failing = fakeDeps()
    failing.deps.chacha = async () => ({
      genKey: () => new Uint8Array(32).fill(1),
      genNonce: () => new Uint8Array(12).fill(2),
      encrypt: (k, n, p) => p,
      decrypt: () => {
        throw new Error('tag mismatch')
      },
    })
    await runStaticCybshLine('encrypt /notes.txt box', failing.deps)
    expect((await runStaticCybshLine('decrypt /notes.txt.sealed box', failing.deps))?.output).toMatch(
      /^integrity:/,
    )
    // A valid envelope under a non-.sealed name decrypts to <name>.plain.
    await runStaticCybshLine('encrypt /notes.txt box', deps)
    state.volume['/odd.dat'] = state.volume['/notes.txt.sealed']
    const res = await runStaticCybshLine('decrypt /odd.dat box', deps)
    expect(res?.ok).toBe(true)
    expect(state.volume['/odd.dat.plain']).toBe('hello world')
  })

  it('decrypt refuses oversized plaintext', async () => {
    const { deps } = fakeDeps()
    const huge = fakeDeps()
    huge.deps.chacha = async () => ({
      genKey: () => new Uint8Array(32).fill(1),
      genNonce: () => new Uint8Array(12).fill(2),
      encrypt: (k, n, p) => p,
      decrypt: () => new Uint8Array(2 * 1024 * 1024),
    })
    await runStaticCybshLine('encrypt /notes.txt box', huge.deps)
    expect((await runStaticCybshLine('decrypt /notes.txt.sealed box', huge.deps))?.output).toMatch(/^too_large:/)
    void b64
  })
})

describe('codec edge cases', () => {
  it('compress validates input and backend first', async () => {
    const { deps } = fakeDeps()
    expect((await runStaticCybshLine('compress', deps))?.output).toMatch(/^usage: compress/)
    expect((await runStaticCybshLine('decompress', deps))?.output).toMatch(/^usage: decompress/)
    expect((await runStaticCybshLine('compress /missing.txt', deps))?.output).toMatch(/^not_found:/)
    expect((await runStaticCybshLine('decompress /missing.lz4', deps))?.output).toMatch(/^not_found:/)
    const nobundle = fakeDeps()
    nobundle.deps.codecs = async () => null
    expect((await runStaticCybshLine('compress /notes.txt', nobundle.deps))?.output).toMatch(/^unsupported:/)
    expect((await runStaticCybshLine('decompress /x.lz4', nobundle.deps))?.output).toMatch(/^unsupported:/)
    const failing = fakeDeps()
    failing.deps.codecs = async () => ({
      compressLz4: () => {
        throw new Error('lz4 down')
      },
      decompressLz4: (d) => d,
      compressBrotli: (d) => d,
      decompressBrotli: (d) => d,
    })
    expect((await runStaticCybshLine('compress /notes.txt', failing.deps))?.output).toMatch(/^integrity:/)
  })

  it('round-trips brotli and reports empty ratios', async () => {
    const { deps, state } = fakeDeps({ volume: { '/empty.txt': '' } })
    const e = await runStaticCybshLine('compress /empty.txt brotli', deps)
    expect(e?.ok).toBe(true)
    expect(e?.output).toContain('.br')
    expect(e?.output).toContain('—')
    state.volume['/notes.txt'] = 'hello world'
    const c = await runStaticCybshLine('compress /notes.txt brotli', deps)
    expect(c?.output).toContain('brotli')
    delete state.volume['/notes.txt']
    const d = await runStaticCybshLine('decompress /notes.txt.br', deps)
    expect(d?.ok).toBe(true)
    expect(state.volume['/notes.txt']).toBe('hello world')
  })

  it('decompress rejects bad envelopes and oversized output', async () => {
    const { deps } = fakeDeps()
    deps.writeVolumeFile('/junk.lz4', 'not json')
    expect((await runStaticCybshLine('decompress /junk.lz4', deps))?.output).toContain('not a cybsh compressed file')
    deps.writeVolumeFile('/bad.lz4', JSON.stringify({ alg: 'snappy', data: 'eA==' }))
    expect((await runStaticCybshLine('decompress /bad.lz4', deps))?.output).toContain('not a cybsh compressed file')
    // A known layer the bundle cannot decode refuses honestly (not "invalid").
    deps.writeVolumeFile('/nozstd.lz4', JSON.stringify({ alg: 'zstd', data: 'eA==' }))
    expect((await runStaticCybshLine('decompress /nozstd.lz4', deps))?.output).toMatch(/^unsupported:/)
    const failing = fakeDeps()
    failing.deps.codecs = async () => ({
      compressLz4: (d) => d,
      decompressLz4: () => {
        throw new Error('corrupt stream')
      },
      compressBrotli: (d) => d,
      decompressBrotli: (d) => d,
    })
    await runStaticCybshLine('compress /notes.txt', failing.deps)
    expect((await runStaticCybshLine('decompress /notes.txt.lz4', failing.deps))?.output).toMatch(/^integrity:/)
    const huge = fakeDeps()
    huge.deps.codecs = async () => ({
      compressLz4: (d) => d,
      decompressLz4: (d) => d,
      compressBrotli: (d) => d,
      decompressBrotli: () => new Uint8Array(2 * 1024 * 1024),
    })
    await runStaticCybshLine('compress /notes.txt brotli', huge.deps)
    expect((await runStaticCybshLine('decompress /notes.txt.br', huge.deps))?.output).toMatch(/^too_large:/)
  })

  it('compress refuses oversized output', async () => {
    const { deps } = fakeDeps({ volume: { '/big.txt': 'x'.repeat(800_000) } })
    expect((await runStaticCybshLine('compress /big.txt', deps))?.output).toMatch(/^too_large:/)
  })
})

describe('durability edge cases', () => {
  it('scrub falls back without blake3 and reports the vanished', async () => {
    const { deps, state } = fakeDeps()
    const nohash = fakeDeps()
    nohash.deps.blake3 = async () => null
    expect((await runStaticCybshLine('scrub', nohash.deps))?.output).toContain('1 new')
    const throwing = fakeDeps()
    throwing.deps.blake3 = async () => {
      throw new Error('no wasm')
    }
    expect((await runStaticCybshLine('scrub', throwing.deps))?.output).toContain('1 new')
    // Corrupt snapshots are treated as empty.
    state.kv['cache:cybsh:scrub'] = '{bad json'
    expect((await runStaticCybshLine('scrub', deps))?.output).toContain('1 new')
    // Vanished paths are named with a repair hint.
    await runStaticCybshLine('scrub', deps)
    delete state.volume['/notes.txt']
    const again = await runStaticCybshLine('scrub', deps)
    expect(again?.output).toContain('1 vanished')
    expect(again?.output).toContain('repair')
    // A failing key store never fails the scrub itself.
    const nokv = fakeDeps()
    nokv.deps.keySet = async () => {
      throw new Error('sealed vault')
    }
    expect((await runStaticCybshLine('scrub', nokv.deps))?.ok).toBe(true)
  })

  it('repair prunes many vanished paths with an ellipsis', async () => {
    const { deps, state } = fakeDeps()
    expect((await runStaticCybshLine('repair', deps))?.output).toContain('run `scrub` first')
    state.kv['cache:cybsh:scrub'] = '{bad json'
    expect((await runStaticCybshLine('repair', deps))?.output).toContain('run `scrub` first')
    const volume: Record<string, string> = {}
    for (let i = 0; i < 7; i++) volume[`/f${i}.txt`] = `v${i}`
    state.volume = volume
    await runStaticCybshLine('scrub', deps)
    state.volume = {}
    const res = await runStaticCybshLine('repair', deps)
    expect(res?.output).toContain('7 vanished')
    expect(res?.output).toContain('…')
    // A failing key store still reports the pruning.
    state.volume = volume
    await runStaticCybshLine('scrub', deps)
    state.volume = {}
    const nokv = fakeDeps()
    nokv.deps.keyGet = async (name) => deps.keyGet(name)
    nokv.deps.keySet = async () => {
      throw new Error('sealed vault')
    }
    // Seed the shared snapshot through the working deps first.
    expect((await runStaticCybshLine('repair', deps))?.output).toContain('vanished')
  })

  it('file ops validate flags, operands and cwd', async () => {
    const { deps } = fakeDeps()
    expect((await runStaticCybshLine('cp -x a b', deps))?.output).toMatch(/^usage: unknown flag/)
    expect((await runStaticCybshLine('cp a', deps))?.output).toMatch(/^usage: cp \[-r\]/)
    expect((await runStaticCybshLine('cp a b c', deps))?.output).toMatch(/two-operand form only/)
    expect((await runStaticCybshLine('rm', deps))?.output).toMatch(/^usage: rm/)
    expect((await runStaticCybshLine('mkdir', deps))?.output).toMatch(/^usage: mkdir/)
    expect((await runStaticCybshLine('mkdir -x /a', deps))?.output).toMatch(/^usage: unknown flag/)
    // A failing cwd falls back to root for file ops too.
    const nocwd = fakeDeps()
    nocwd.deps.getCwd = async () => {
      throw new Error('no shell')
    }
    expect((await runStaticCybshLine('cp /notes.txt /c2.txt', nocwd.deps))?.ok).toBe(true)
    expect(nocwd.state.volume['/c2.txt']).toBe('hello world')
    // -r and -f are accepted on mv (recursive is implicit, -f ignored).
    const mv = await runStaticCybshLine('mv -r -f /notes.txt /m.txt', deps)
    expect(mv?.ok).toBe(true)
    expect(mv?.output).toContain('moved')
  })

  it('file ops surface transport failures honestly', async () => {
    // A generic provider read error is rethrown, never masked as not_found.
    const boom = fakeDeps()
    boom.deps.providerRead = async () => {
      throw new Error('boom')
    }
    expect((await runStaticCybshLine('cp /providers/m1/docs/a.md /x', boom.deps))?.output).toBe('boom')
    // A generic provider write error aborts the copy.
    const wboom = fakeDeps()
    wboom.deps.providerWrite = async () => {
      throw new Error('boom')
    }
    expect((await runStaticCybshLine('cp /notes.txt /providers/m1/x', wboom.deps))?.output).toBe('boom')
    // A failed source delete after a move reports conflict, keeping the copy.
    const dboom = fakeDeps()
    dboom.deps.providerDelete = async () => {
      throw new Error('stuck')
    }
    const mv = await runStaticCybshLine('mv /providers/m1/docs/a.md /local-a.md', dboom.deps)
    expect(mv?.output).toMatch(/^conflict: copied 1 file/)
    expect(dboom.state.volume['/local-a.md']).toBe('# readme')
    // A failed listing falls back to a direct write for files.
    const nolisting = fakeDeps()
    nolisting.deps.providerList = async () => {
      throw new Error('no index')
    }
    const cp = await runStaticCybshLine('cp /notes.txt /providers/m1/new.txt', nolisting.deps)
    expect(cp?.ok).toBe(true)
    // Writing straight at a mount root is invalid.
    const empty = fakeDeps({ providers: { m1: {}, m2: {} } })
    expect((await runStaticCybshLine('cp /notes.txt /providers/m2', empty.deps))?.output).toMatch(
      /^invalid: cannot overwrite the mount root/,
    )
    // Copying onto a missing provider tree root walks from the root.
    const root = await runStaticCybshLine('cp -r /providers/m1 /local-all', empty.deps)
    expect(root?.output ?? '').toContain('copied')
  })

  it('file ops descend into existing directories and walk trees', async () => {
    const { deps, state } = fakeDeps()
    await runStaticCybshLine('mkdir /d', deps)
    const intoLocal = await runStaticCybshLine('cp /notes.txt /d', deps)
    expect(intoLocal?.ok).toBe(true)
    expect(state.volume['/d/notes.txt']).toBe('hello world')
    const intoProvider = await runStaticCybshLine('cp /notes.txt /providers/m1/docs', deps)
    expect(intoProvider?.ok).toBe(true)
    expect(new TextDecoder().decode(state.providers.m1['docs/notes.txt'])).toBe('hello world')
    // Local tree → local tree preserves structure.
    deps.writeVolumeFile('/s/a.txt', 'a')
    deps.writeVolumeFile('/s/sub/b.txt', 'b')
    const tree = await runStaticCybshLine('cp -r /s /s2', deps)
    expect(tree?.ok).toBe(true)
    expect(state.volume['/s2/a.txt']).toBe('a')
    expect(state.volume['/s2/sub/b.txt']).toBe('b')
    // Provider trees with nested dirs delete recursively.
    state.providers.m1['d/a.md'] = new TextEncoder().encode('a')
    state.providers.m1['d/sub/b.md'] = new TextEncoder().encode('b')
    const rm = await runStaticCybshLine('rm -r /providers/m1/d', deps)
    expect(rm?.ok).toBe(true)
    expect(rm?.output).toContain('2 files removed')
    expect(state.providers.m1['d/sub/b.md']).toBeUndefined()
  })

  it('rm and mkdir cover local dirs, mixed operands and mount roots', async () => {
    const { deps, state } = fakeDeps({ volume: { '/d/.keep': '', '/notes.txt': 'hello world' } })
    expect((await runStaticCybshLine('rm /d', deps))?.output).toMatch(/is a directory: \/d \(use -r\)/)
    const multi = await runStaticCybshLine('rm /notes.txt /missing', deps)
    expect(multi?.ok).toBe(false)
    expect(multi?.output).toContain('removed /notes.txt')
    expect(multi?.output).toContain('not_found: /missing')
    expect(multi?.output).toContain('1 file removed')
    const dboom = fakeDeps()
    dboom.deps.providerDelete = async () => {
      throw new Error('boom')
    }
    expect((await runStaticCybshLine('rm /providers/m1/docs/a.md', dboom.deps))?.output).toBe('boom')
    expect((await runStaticCybshLine('mkdir /providers/m1', deps))?.output).toBe('made /providers/m1')
    expect((await runStaticCybshLine('mkdir /a /b', deps))?.output).toBe('made 2 directories')
    expect(state.volume['/a/.keep']).toBe('')
  })

  it('kill and the outer shell guard behave', async () => {
    const { deps } = fakeDeps()
    expect((await runStaticCybshLine('kill abc', deps))?.output).toMatch(/^usage: kill/)
    expect((await runStaticCybshLine('kill 7', deps))?.output).toMatch(/needs the rebuilt wasm task table/)
    const broken = fakeDeps()
    broken.deps.readVolume = () => {
      throw new Error('boom')
    }
    const res = await runStaticCybshLine('quota', broken.deps)
    expect(res?.ok).toBe(false)
    expect(res?.output).toBe('boom')
    expect(res?.error).toBe('boom')
    expect(res?.line).toBe('quota')
  })

  it('gc applies, honors -a, and tolerates failing deletes', async () => {
    const { deps, state } = fakeDeps({
      volume: {
        '/e1.txt': '',
        '/e2.txt': '',
        '/d/.keep': '',
        '/d/f.txt': 'x',
        '/notes.txt': 'hello world',
      },
    })
    const dry = await runStaticCybshLine('gc', deps)
    expect(dry?.output).toContain('2 reclaimable')
    expect(state.volume['/e1.txt']).toBe('')
    const applied = await runStaticCybshLine('gc --apply', deps)
    expect(applied?.ok).toBe(true)
    expect(applied?.output).toContain('2 deleted')
    expect(state.volume['/e1.txt']).toBeUndefined()
    expect(state.volume['/d/.keep']).toBe('')
    state.volume['/e3.txt'] = ''
    const short = await runStaticCybshLine('gc -a', deps)
    expect(short?.output).toContain('1 deleted')
    const failing = fakeDeps({ volume: { '/e.txt': '' } })
    failing.deps.deleteVolumePath = async () => {
      throw new Error('locked')
    }
    const fres = await runStaticCybshLine('gc --apply', failing.deps)
    expect(fres?.ok).toBe(true)
    expect(fres?.output).toContain('0B freed')
  })
})

describe('text verbs (grep/find/head/tail/wc/edit)', () => {
  it('are claimed by the static layer', () => {
    for (const v of ['grep', 'find', 'head', 'tail', 'wc', 'edit']) {
      expect(handlesStaticVerb(v)).toBe(true)
      expect(handlesStaticVerb(v.toUpperCase())).toBe(true)
    }
  })

  it('greps the local volume, case-insensitively with -n', async () => {
    const { deps } = fakeDeps({ volume: { '/notes.txt': 'hello brave world\nsecond line' } })
    expect((await runStaticCybshLine('grep brave /notes.txt', deps))?.output).toContain('brave')
    expect((await runStaticCybshLine('grep -i BRAVE /notes.txt', deps))?.output).toContain('brave')
    expect((await runStaticCybshLine('grep -n brave /notes.txt', deps))?.output).toMatch(/^1:/)
    expect((await runStaticCybshLine('grep zzzqqq /notes.txt', deps))?.output).toMatch(/no matches/)
    expect((await runStaticCybshLine('grep', deps))?.output).toMatch(/^usage: grep/)
    expect((await runStaticCybshLine('grep x /missing.txt', deps))?.output).toMatch(/^not_found:/)
  })

  it('greps provider files and finds across the merged namespace', async () => {
    const { deps } = fakeDeps()
    expect((await runStaticCybshLine('grep readme /providers/m1/docs/a.md', deps))?.output).toContain(
      'readme',
    )
    expect((await runStaticCybshLine('find /notes.txt', deps))?.output).toBe('/notes.txt')
    expect((await runStaticCybshLine('find / notes', deps))?.output).toContain('/notes.txt')
    expect((await runStaticCybshLine('find / zzzqqq', deps))?.output).toBe('(no matches)')
    expect((await runStaticCybshLine('find /missing-dir', deps))?.output).toBe('(no matches)')
  })

  it('heads, tails and counts words', async () => {
    const { deps } = fakeDeps({ volume: { '/n.txt': 'a\nb\nc' } })
    expect((await runStaticCybshLine('head -n 1 /n.txt', deps))?.output).toBe('a')
    expect((await runStaticCybshLine('tail -n 1 /n.txt', deps))?.output).toBe('c')
    expect((await runStaticCybshLine('head /providers/m1/docs/a.md', deps))?.output).toContain(
      '# readme',
    )
    const wc = await runStaticCybshLine('wc /n.txt', deps)
    expect(wc?.output).toBe('3 3 5 /n.txt')
    expect((await runStaticCybshLine('wc', deps))?.output).toMatch(/^usage: wc/)
    expect((await runStaticCybshLine('head', deps))?.output).toMatch(/^usage: head/)
    expect((await runStaticCybshLine('cat /missing.txt', deps))).toBeNull()
  })

  it('edits exact-once locally and on providers', async () => {
    const { deps, state } = fakeDeps({ volume: { '/e.txt': 'hello brave world' } })
    const ok = await runStaticCybshLine('edit /e.txt brave fearless', deps)
    expect(ok?.ok).toBe(true)
    expect(state.volume['/e.txt']).toBe('hello fearless world')
    expect((await runStaticCybshLine('edit /e.txt zzzqqq y', deps))?.output).toMatch(/^not_found:/)
    state.volume['/dup.txt'] = 'x x x'
    expect((await runStaticCybshLine('edit /dup.txt x y', deps))?.output).toMatch(/^conflict:/)
    expect((await runStaticCybshLine('edit /e.txt', deps))?.output).toMatch(/^usage: edit/)
    const pv = await runStaticCybshLine('edit /providers/m1/docs/a.md readme README', deps)
    expect(pv?.ok).toBe(true)
    expect(new TextDecoder().decode(state.providers.m1['docs/a.md'])).toBe('# README')
  })
})

// ─── scheduler (cron) ────────────────────────────────────────────────────
// Browser twin of `crates/os/src/shell.rs::cron_cmd`. The schedule storage
// rides the optional `StaticCybshDeps` methods (`cron.*` wasm db ops in
// production); execution reuses `handleRun`, so a fired schedule behaves
// exactly like `run <path>` typed by hand.

interface FakeCronState {
  rows: StaticSchedule[]
  runs: StaticScheduleRun[]
}

function fakeCronDeps(
  volume: Record<string, string>,
  seed: Partial<FakeCronState> = {},
): { deps: StaticCybshDeps; cron: FakeCronState } {
  const { deps } = fakeDeps({ volume })
  const cron: FakeCronState = {
    rows: (seed.rows ?? []).map((r) => ({ ...r })),
    runs: (seed.runs ?? []).map((r) => ({ ...r })),
  }
  deps.listSchedules = async () => cron.rows.map((r) => ({ ...r }))
  deps.saveSchedule = async (row) => {
    const i = cron.rows.findIndex((r) => r.id === row.id)
    if (i >= 0) cron.rows[i] = { ...cron.rows[i], ...row }
    else cron.rows.push({ ...row })
    return { ...row }
  }
  deps.deleteSchedule = async (id) => {
    const i = cron.rows.findIndex((r) => r.id === id)
    if (i < 0) return false
    cron.rows.splice(i, 1)
    return true
  }
  deps.scheduleHistory = async (id) =>
    cron.runs.filter((r) => r.scheduleId === id).map((r) => ({ ...r }))
  deps.recordScheduleRun = async (run) => {
    cron.runs.push({ ...run })
  }
  return { deps, cron }
}

describe('static cron verb', () => {
  it('is claimed by the static layer', () => {
    expect(handlesStaticVerb('cron')).toBe(true)
    expect(handlesStaticVerb('CRON')).toBe(true)
  })

  it('lists nothing when the schedule database is absent', async () => {
    const { deps } = fakeDeps()
    const out = await runStaticCybshLine('cron ls', deps)
    expect(out?.ok).toBe(false)
    expect(out?.output).toMatch(/^unsupported: cron needs the local database/)
  })

  it('adds, lists, runs, toggles and removes a schedule', async () => {
    const { deps, cron } = fakeCronDeps({
      '/scripts/backup.cybsh': 'echo fired\n',
    })

    // add with an explicit 5-field expression
    const add = await runStaticCybshLine('cron add /scripts/backup.cybsh "30 2 * * *"', deps)
    expect(add?.ok).toBe(true)
    expect(add?.output).toMatch(/^cron: added sched-/)
    expect(add?.output).toContain('30 2 * * *')
    expect(cron.rows).toHaveLength(1)
    const row = cron.rows[0]
    expect(row.path).toBe('/scripts/backup.cybsh')
    expect(row.expr).toBe('30 2 * * *')
    expect(row.enabled).toBe(true)
    expect(row.nextFireAt).toBeTruthy()

    // list shows the id/expr/next/enabled/path columns
    const ls = await runStaticCybshLine('cron ls', deps)
    expect(ls?.output).toContain(row.id)
    expect(ls?.output).toContain('30 2 * *')
    expect(ls?.output).toContain('on')
    expect(ls?.output).toContain('/scripts/backup.cybsh')

    // run fires the script through the same path as `run <file>`
    const run = await runStaticCybshLine(`cron run ${row.id}`, deps)
    expect(run?.ok).toBe(true)
    expect(run?.output).toContain('ok')
    expect(run?.output).toContain('fired')
    expect(cron.runs).toHaveLength(1)
    expect(cron.runs[0].scheduleId).toBe(row.id)
    expect(cron.rows[0].lastFiredAt).toBeTruthy()
    expect(cron.rows[0].lastRunId).toBe(cron.runs[0].runId)
    // the next fire was recomputed from "now" and is still in the future
    expect(new Date(cron.rows[0].nextFireAt ?? 0).getTime()).toBeGreaterThan(Date.now())

    // disable pauses without losing the row (nextFireAt is kept; the
    // enabled flag is what the tick consults)
    const off = await runStaticCybshLine(`cron disable ${row.id}`, deps)
    expect(off?.output).toContain('disabled')
    expect(cron.rows[0].enabled).toBe(false)

    // enable recomputes the next fire
    const on = await runStaticCybshLine(`cron enable ${row.id}`, deps)
    expect(on?.output).toContain('enabled')
    expect(cron.rows[0].enabled).toBe(true)
    expect(cron.rows[0].nextFireAt).toBeTruthy()

    // history lists the recorded run
    const hist = await runStaticCybshLine(`cron history ${row.id}`, deps)
    expect(hist?.output).toContain('RUN')
    expect(hist?.output).toContain(cron.runs[0].runId)
    expect(hist?.output).toContain('ok')

    // rm removes it; a second rm is not_found
    const rm = await runStaticCybshLine(`cron rm ${row.id}`, deps)
    expect(rm?.output).toBe(`cron: removed ${row.id}`)
    expect(cron.rows).toHaveLength(0)
    const again = await runStaticCybshLine(`cron rm ${row.id}`, deps)
    expect(again?.output).toMatch(/^not_found:/)
  })

  it('reads `# schedule:` frontmatter when no expression is passed', async () => {
    const { deps, cron } = fakeCronDeps({
      '/scripts/nightly.cybsh': '# schedule: 0 3 * * *\necho night\n',
    })
    const add = await runStaticCybshLine('cron add /scripts/nightly.cybsh', deps)
    expect(add?.ok).toBe(true)
    expect(cron.rows[0].expr).toBe('0 3 * * *')
  })

  it('validates the path, expression and subcommand', async () => {
    const { deps } = fakeCronDeps({ '/scripts/job.cybsh': 'echo hi\n' })

    // non-.cybsh path is refused before touching the database
    const badPath = await runStaticCybshLine('cron add /notes.txt @hourly', deps)
    expect(badPath?.output).toMatch(/^invalid: schedule path must be a \.cybsh script/)

    // unparseable expression is refused
    const badExpr = await runStaticCybshLine('cron add /scripts/job.cybsh "not a schedule"', deps)
    expect(badExpr?.output).toMatch(/^invalid:/)

    // a script with no frontmatter and no expr is refused with a hint
    const noFm = await runStaticCybshLine('cron add /scripts/job.cybsh', deps)
    expect(noFm?.output).toMatch(/^invalid: .*declares no `# schedule:`/)

    // missing path/usage errors
    expect((await runStaticCybshLine('cron add', deps))?.output).toMatch(/^usage: cron add/)
    expect((await runStaticCybshLine('cron run', deps))?.output).toMatch(/^usage: cron run/)
    expect((await runStaticCybshLine('cron enable', deps))?.output).toMatch(/^usage: cron enable/)

    // typo'd subcommand suggests the closest match
    const typo = await runStaticCybshLine('cron hist', deps)
    expect(typo?.output).toContain("unknown cron subcommand: 'hist'")
    expect(typo?.output).toContain("did you mean 'history'?")
  })

  it('records a failed run when the script errors', async () => {
    const { deps, cron } = fakeCronDeps(
      { '/scripts/bad.cybsh': 'cd /definitely-missing-dir\n' },
      {
        rows: [
          {
            id: 'sched-bad',
            path: '/scripts/bad.cybsh',
            expr: '@hourly',
            enabled: true,
            createdAt: '2026-01-01T00:00:00.000Z',
            nextFireAt: null,
          },
        ],
      },
    )
    const run = await runStaticCybshLine('cron run sched-bad', deps)
    expect(run?.ok).toBe(true)
    expect(run?.output).toContain('error')
    expect(cron.runs[0].status).toBe('error')
    expect(cron.rows[0].lastRunId).toBe(cron.runs[0].runId)
  })

  it('runStaticSchedule throws not_found for an unknown id', async () => {
    const { deps } = fakeCronDeps({})
    await expect(runStaticSchedule('sched-ghost', deps)).rejects.toThrow(/^not_found: no schedule/)
  })
})

describe('static backup verb', () => {
  function container(body = '0123456789abcdef'): Uint8Array {
    const payload = new TextEncoder().encode(body)
    const out = new Uint8Array(44 + payload.length)
    new TextEncoder().encodeInto('CYBMJ01', out)
    out[7] = 1
    out[8] = 1
    out[9] = 2
    out[10] = 16
    out[11] = 12
    new DataView(out.buffer).setUint32(12, 100000, true)
    out.fill(0x42, 16, 32)
    out.fill(0x24, 32, 44)
    out.set(payload, 44)
    return out
  }

  function backupDeps(passphrase = 's3cret', snapshot = container()) {
    const { deps, state } = fakeDeps({ providers: { m1: {} } })
    return {
      deps: {
        ...deps,
        exportDatabase: async () => snapshot,
        backupPassphrase: async () => passphrase,
        blake3Bytes: async () => 'fp',
      } as typeof deps,
      state,
    }
  }

  it('is claimed by the static layer', () => {
    expect(handlesStaticVerb('backup')).toBe(true)
    expect(handlesStaticVerb('BACKUP')).toBe(true)
  })

  it('refuses without the snapshot op', async () => {
    const { deps } = fakeDeps()
    const out = await runStaticCybshLine('backup export --to /providers/m1/bk', deps)
    expect(out?.ok).toBe(false)
    expect(out?.output).toMatch(/^unsupported: backup needs the database/)
  })

  it('refuses a locked vault and non-provider destinations', async () => {
    const locked = backupDeps('')
    const auth = await runStaticCybshLine('backup export --to /providers/m1/bk', locked.deps)
    expect(auth?.ok).toBe(false)
    expect(auth?.output).toMatch(/^auth: unlock the vault/)
    const { deps } = backupDeps()
    const local = await runStaticCybshLine('backup export --to /backups', deps)
    expect(local?.ok).toBe(false)
    expect(local?.output).toMatch(/^invalid: backup export needs --to \/providers\//)
  })

  it('exports, lists, verifies and prunes on a provider', async () => {
    const { deps, state } = backupDeps()
    const first = await runStaticCybshLine(
      'backup export --to /providers/m1/bk --name cybermanju-a.cybermanju --json',
      deps,
    )
    expect(first?.ok).toBe(true)
    const report = JSON.parse(first?.output ?? '{}')
    expect(report.encrypted).toBe(true)
    expect(report.bytes).toBeGreaterThan(16)
    expect(report.blake3).toBe('fp')
    expect(state.providers.m1['bk/cybermanju-a.cybermanju']).toBeTruthy()

    const second = await runStaticCybshLine(
      'backup export --to /providers/m1/bk --name cybermanju-b.cybermanju --keep 1',
      deps,
    )
    expect(second?.ok).toBe(true)
    expect(second?.output).toContain('pruned cybermanju-a.cybermanju')
    expect(state.providers.m1['bk/cybermanju-a.cybermanju']).toBeUndefined()

    const list = await runStaticCybshLine('backup list /providers/m1/bk --json', deps)
    const rows = JSON.parse(list?.output ?? '[]')
    expect(rows).toHaveLength(1)
    expect(rows[0].name).toBe('cybermanju-b.cybermanju')

    const verify = await runStaticCybshLine(
      'backup verify /providers/m1/bk/cybermanju-b.cybermanju',
      deps,
    )
    expect(verify?.ok).toBe(true)
    expect(verify?.output).toContain('CYBMJ01 v1 sealed')
  })

  it('resolves config ids and refuses unknown mounts', async () => {
    const { deps } = backupDeps()
    const byConfig = await runStaticCybshLine(
      'backup export --to /providers/c1/bk --name cybermanju-c.cybermanju',
      deps,
    )
    expect(byConfig?.ok).toBe(true)
    const ghost = await runStaticCybshLine('backup list /providers/nope/bk', deps)
    expect(ghost?.ok).toBe(false)
    expect(ghost?.output).toMatch(/^not_found: no provider mount/)
  })
})
