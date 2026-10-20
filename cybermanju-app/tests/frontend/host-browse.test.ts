// CyberManju OS — Host browse.test (tests/frontend/host-browse.test.ts).
// Host bridge: `/host` namespace math, shell quoting, `ls --json` mapping,
// and the move-anything→anywhere matrix (with fake transports, no Tauri).
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  cpHost,
  destFromVmPath,
  hostAbsFromVm,
  hostEntriesToNodes,
  hostParentVm,
  isHostId,
  isHostPath,
  listHostDir,
  moveAnywhere,
  mvHost,
  rmHost,
  shellQuote,
  sourceFromNode,
  vmForHost,
  writeHostText,
  type MoveDeps,
  type MoveDest,
  type MoveSource,
} from '../../src/utils/hostBrowse'
import { trackShellCwd } from '../../src/utils/shellCwd'
import type { FileNode } from '../../src/types'

const vaultFile = (over: Partial<FileNode> = {}): FileNode => ({
  id: 'vault-1',
  name: 'note.txt',
  fileType: 'file',
  parentId: '/',
  path: '/note.txt',
  sizeBytes: 5,
  encrypted: false,
  compressionLayers: [],
  createdAt: '2026-01-01T00:00:00Z',
  modifiedAt: '2026-01-01T00:00:00Z',
  ...over,
})

interface Call {
  kind: string
  args: unknown[]
}

function fakeDeps(over: Partial<MoveDeps> = {}): { deps: MoveDeps; calls: Call[] } {
  const calls: Call[] = []
  const files = new Map<string, Uint8Array>([
    ['/sdcard/a.txt', new TextEncoder().encode('hello')],
    ['vault:/note.txt', new TextEncoder().encode('vault-bytes')],
  ])
  // The fake provider backend stores what was written, so verify re-reads
  // echo the destination bytes (like the real read-back check).
  let lastProviderWrite = new TextEncoder().encode('provider-bytes')
  const deps: MoveDeps = {
    run: async (line: string) => {
      calls.push({ kind: 'run', args: [line] })
      if (line.startsWith('ls --json -os')) return JSON.stringify({ path: '/x', host: true, entries: [] })
      return ''
    },
    invoke: (async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      calls.push({ kind: `invoke:${cmd}`, args: [args] })
      if (cmd === 'duplicate_file_context') return { ...vaultFile(), id: 'vault-2', parentId: '/' } as unknown as T
      if (cmd === 'read_file_content') return { content: 'editor-text' } as unknown as T
      if (cmd === 'list_files') return [] as unknown as T
      return undefined as unknown as T
    }) as MoveDeps['invoke'],
    canal: {
      readProviderFile: async () => {
        calls.push({ kind: 'canal:read', args: [] })
        return lastProviderWrite
      },
      writeProviderFile: async (_mount: string, _path: string, data: Uint8Array) => {
        calls.push({ kind: 'canal:write', args: [] })
        lastProviderWrite = data
      },
      deleteProviderFile: async () => {
        calls.push({ kind: 'canal:delete', args: [] })
      },
      listProviderDir: async () => {
        calls.push({ kind: 'canal:list', args: [] })
        return []
      },
    },
    listVaultDir: async () => [],
    readHostBytes: async (abs: string) => {
      calls.push({ kind: 'host:read', args: [abs] })
      const hit = files.get(abs) ?? files.get(`vault:${abs}`)
      if (!hit) throw new Error(`not_found: ${abs}`)
      return hit
    },
    writeHostBytes: async (abs: string, _data: Uint8Array) => {
      calls.push({ kind: 'host:write', args: [abs] })
      files.set(abs, _data)
    },
    ...over,
  }
  return { deps, calls }
}

describe('host namespace math', () => {
  it('detects host paths and ids', () => {
    expect(isHostPath('/host')).toBe(true)
    expect(isHostPath('/host/sdcard/x')).toBe(true)
    expect(isHostPath('/providers/m')).toBe(false)
    expect(isHostPath('/')).toBe(false)
    expect(isHostId('host:/sdcard/a')).toBe(true)
    expect(isHostId('vault-1')).toBe(false)
  })

  it('converts between vm paths and host absolutes (posix + windows)', () => {
    expect(hostAbsFromVm('/host')).toBe('')
    expect(hostAbsFromVm('/host/')).toBe('/')
    expect(hostAbsFromVm('/host/sdcard/Download')).toBe('/sdcard/Download')
    expect(hostAbsFromVm('/host/C:/Users/me')).toBe('C:/Users/me')
    expect(vmForHost('')).toBe('/host')
    expect(vmForHost('/')).toBe('/host/')
    expect(vmForHost('/sdcard/x')).toBe('/host/sdcard/x')
    expect(vmForHost('C:/Users/me')).toBe('/host/C:/Users/me')
    // Round-trips.
    for (const abs of ['/sdcard/Download', 'C:/Users/me']) {
      expect(hostAbsFromVm(vmForHost(abs))).toBe(abs)
    }
  })

  it('walks host parents up to the vault', () => {
    expect(hostParentVm('/host/a/b')).toBe('/host/a')
    expect(hostParentVm('/host/a')).toBe('/host')
    expect(hostParentVm('/host')).toBe('/')
    expect(hostParentVm('/')).toBe('/')
  })

  it('quotes shell operands', () => {
    expect(shellQuote('/sdcard/a b')).toBe('"/sdcard/a b"')
    expect(shellQuote('say "hi"')).toBe('"say \\"hi\\""')
    expect(shellQuote('C:\\Users\\A')).toBe('"C:\\\\Users\\\\A"')
  })

  it('maps ls --json entries to file rows with full paths', () => {
    const rows = hostEntriesToNodes(
      [
        { path: '/sdcard/docs', name: 'docs', kind: 'dir', sizeBytes: 0, modifiedMs: 1700000000000, isDir: true },
        { path: '/sdcard/a.txt', name: 'a.txt', kind: 'file', sizeBytes: 5, modifiedMs: 0, isDir: false },
      ],
      '/host/sdcard',
    )
    expect(rows).toHaveLength(2)
    expect(rows[0].id).toBe('host:/sdcard/docs')
    expect(rows[0].fileType).toBe('folder')
    expect(rows[0].path).toBe('/host/sdcard/docs')
    expect(rows[0].parentId).toBe('/host/sdcard')
    expect(rows[1].sizeBytes).toBe(5)
    expect(rows[1].isHidden).toBe(false)
  })
})

describe('source / destination descriptors', () => {
  it('classifies rows by id namespace', () => {
    const host = sourceFromNode(vaultFile({ id: 'host:/sdcard/a.txt', name: 'a.txt' }), '/host/sdcard')
    expect(host?.kind).toBe('host')
    const prov = sourceFromNode(
      vaultFile({ id: 'providers/mnt-1/docs/a.txt', name: 'a.txt' }),
      '/providers/mnt-1/docs',
    )
    expect(prov?.kind).toBe('provider')
    if (prov?.kind === 'provider') {
      expect(prov.mountId).toBe('mnt-1')
      expect(prov.remotePath).toBe('docs/a.txt')
    }
    // The mount row itself is not movable.
    expect(sourceFromNode(vaultFile({ id: 'providers/mnt-1', name: 'm' }), '/providers')).toBeNull()
    const vault = sourceFromNode(vaultFile(), '/')
    expect(vault?.kind).toBe('vault')
  })

  it('destinations follow the current folder namespace', () => {
    expect(destFromVmPath('/photos')).toEqual({ kind: 'vault', vaultPath: '/photos' })
    expect(destFromVmPath('/providers/mnt-1/docs')).toEqual({
      kind: 'provider',
      mountId: 'mnt-1',
      remoteDir: 'docs',
    })
    expect(destFromVmPath('/host/sdcard/Download')).toEqual({ kind: 'host', absDir: '/sdcard/Download' })
    // `/host` needs a `pwd -os` resolve first — never a blind default.
    expect(destFromVmPath('/host')).toBeNull()
    expect(destFromVmPath('/providers')).toBeNull()
  })
})

describe('host browser transport and text safety', () => {
  afterEach(() => {
    vi.unstubAllEnvs()
    vi.unstubAllGlobals()
  })

  it('keeps browser Web/REST host listings on the REST-backed runner', async () => {
    vi.stubEnv('VITE_TRANSPORT', 'rest')
    const run = vi.fn(async () => JSON.stringify({
      entries: [{ path: '/Docs/note.txt', name: 'note.txt', kind: 'file', sizeBytes: 3, modifiedMs: 1, isDir: false }],
    }))
    const rows = await listHostDir('/host/Docs', run)
    expect(run).toHaveBeenCalledWith('ls --json -os "/Docs"')
    expect(rows[0]).toMatchObject({ id: 'host:/Docs/note.txt', path: '/host/Docs/note.txt' })
  })

  it('does not treat the WASM mount list as a move destination', () => {
    vi.stubEnv('VITE_TRANSPORT', 'wasm')
    expect(destFromVmPath('/host')).toBeNull()
    expect(destFromVmPath('/host/')).toBeNull()
    vi.stubEnv('VITE_TRANSPORT', 'rest')
    expect(destFromVmPath('/host/')).toEqual({ kind: 'host', absDir: '/' })
  })

  it('refuses copying or moving a WASM mounted-folder root across namespaces', async () => {
    vi.stubEnv('VITE_TRANSPORT', 'wasm')
    const { deps, calls } = fakeDeps()
    const mountedRoot: MoveSource = { kind: 'host', absPath: '/Docs', name: 'Docs', isDir: true }
    await expect(moveAnywhere(mountedRoot, { kind: 'vault', vaultPath: '/' }, 'move', deps)).rejects.toThrow(/^invalid:/)
    await expect(moveAnywhere(mountedRoot, { kind: 'provider', mountId: 'p', remoteDir: '' }, 'copy', deps)).rejects.toThrow(/^invalid:/)
    expect(calls).toHaveLength(0)
  })

  it('round-trips literal text and applies the 1 MiB limit in UTF-8 bytes', async () => {
    vi.stubEnv('VITE_TRANSPORT', 'rest')
    const run = vi.fn(async () => '')
    const content = '  $HOME `literal` "quotes" \\\\ repeated  spaces  '
    await writeHostText('/Docs/notes.txt', content, run)
    expect(run).toHaveBeenCalledWith(`write -os ${shellQuote('/Docs/notes.txt')} ${shellQuote(content)}`)

    const tooLarge = '😀'.repeat(262_145)
    await expect(writeHostText('/Docs/large.txt', tooLarge, run)).rejects.toThrow(/^too_large:/)
    expect(run).toHaveBeenCalledTimes(1)
  })

  it('refuses deleting, copying or moving a host filesystem root', async () => {
    const run = vi.fn(async () => '')
    await expect(rmHost('/', true, run)).rejects.toThrow(/^invalid:/)
    await expect(rmHost('C:\\', true, run)).rejects.toThrow(/^invalid:/)
    await expect(rmHost('\\\\server\\share\\', true, run)).rejects.toThrow(/^invalid:/)
    await expect(cpHost('/', '/tmp', true, run)).rejects.toThrow(/^invalid:/)
    await expect(mvHost('C:/', '/tmp', run)).rejects.toThrow(/^invalid:/)
    expect(run).not.toHaveBeenCalled()
  })
})

describe('moveAnywhere matrix', () => {
  it('host → host file uses native verbs', async () => {
    const { deps, calls } = fakeDeps()
    const src: MoveSource = { kind: 'host', absPath: '/sdcard/a.txt', name: 'a.txt', isDir: false }
    const msg = await moveAnywhere(src, { kind: 'host', absDir: '/sdcard/b' }, 'move', deps)
    expect(msg).toContain('Moved')
    expect(calls.some((c) => c.kind === 'run' && String(c.args[0]).startsWith('mv -os'))).toBe(true)
    const { deps: d2, calls: c2 } = fakeDeps()
    await moveAnywhere(src, { kind: 'host', absDir: '/sdcard/b' }, 'copy', d2)
    expect(c2.some((c) => c.kind === 'run' && String(c.args[0]).startsWith('cp -os'))).toBe(true)
  })

  it('refuses host-root sources and recursive self/descendant targets before I/O', async () => {
    const { deps, calls } = fakeDeps()
    const root: MoveSource = { kind: 'host', absPath: '/', name: '/', isDir: true }
    await expect(moveAnywhere(root, { kind: 'vault', vaultPath: '/' }, 'move', deps)).rejects.toThrow(/^invalid:/)
    const tree: MoveSource = { kind: 'host', absPath: '/sdcard/pics', name: 'pics', isDir: true }
    await expect(moveAnywhere(tree, { kind: 'host', absDir: '/sdcard/pics/sub' }, 'move', deps)).rejects.toThrow(/^invalid:/)
    await expect(moveAnywhere(tree, { kind: 'host', absDir: '/sdcard' }, 'copy', deps)).rejects.toThrow(/^invalid:/)
    expect(calls).toHaveLength(0)
  })

  it('joins a file destination at `/` without producing a double-slash path', async () => {
    const { deps, calls } = fakeDeps()
    const src: MoveSource = { kind: 'host', absPath: '/sdcard/a.txt', name: 'a.txt', isDir: false }
    await moveAnywhere(src, { kind: 'host', absDir: '/' }, 'copy', deps)
    expect(calls.some((call) => String(call.args[0]).includes('"/a.txt"'))).toBe(true)
    expect(calls.some((call) => String(call.args[0]).includes('//a.txt'))).toBe(false)
  })

  it('vault → vault move re-parents by id (no bytes)', async () => {
    const { deps, calls } = fakeDeps()
    await moveAnywhere({ kind: 'vault', file: vaultFile() }, { kind: 'vault', vaultPath: '/photos' }, 'move', deps)
    expect(calls.some((c) => c.kind === 'invoke:move_file')).toBe(true)
    expect(calls.some((c) => c.kind === 'host:read')).toBe(false)
  })

  it('vault → vault copy duplicates then re-parents', async () => {
    const { deps, calls } = fakeDeps()
    await moveAnywhere({ kind: 'vault', file: vaultFile() }, { kind: 'vault', vaultPath: '/photos' }, 'copy', deps)
    expect(calls.some((c) => c.kind === 'invoke:duplicate_file_context')).toBe(true)
    expect(calls.some((c) => c.kind === 'invoke:move_file')).toBe(true)
  })

  it('provider → vault copies bytes and never deletes on copy', async () => {
    const { deps, calls } = fakeDeps()
    const src: MoveSource = { kind: 'provider', mountId: 'm1', remotePath: 'a.txt', locator: 'a.txt', name: 'a.txt', isDir: false }
    await moveAnywhere(src, { kind: 'vault', vaultPath: '/' }, 'copy', deps)
    expect(calls.some((c) => c.kind === 'canal:read')).toBe(true)
    expect(calls.some((c) => c.kind === 'invoke:upload_file')).toBe(true)
    expect(calls.some((c) => c.kind === 'canal:delete')).toBe(false)
  })

  it('host → provider move verifies then deletes the host source', async () => {
    const { deps, calls } = fakeDeps()
    const src: MoveSource = { kind: 'host', absPath: '/sdcard/a.txt', name: 'a.txt', isDir: false }
    const dest: MoveDest = { kind: 'provider', mountId: 'm1', remoteDir: 'docs' }
    const msg = await moveAnywhere(src, dest, 'move', deps)
    expect(msg).toContain('verified')
    expect(calls.some((c) => c.kind === 'host:read')).toBe(true)
    expect(calls.some((c) => c.kind === 'canal:write')).toBe(true)
    // Verified re-read before the delete.
    expect(calls.filter((c) => c.kind === 'canal:read').length).toBeGreaterThanOrEqual(1)
    expect(calls.some((c) => c.kind === 'run' && String(c.args[0]).startsWith('rm -os'))).toBe(true)
  })

  it('vault dir → vault dir re-parents children by id', async () => {
    const kids = [vaultFile({ id: 'k1', name: 'a.txt', path: '/album/a.txt' })]
    const { deps, calls } = fakeDeps({ listVaultDir: async () => kids })
    const folder = vaultFile({ id: 'f1', name: 'album', fileType: 'folder', path: '/album' })
    const msg = await moveAnywhere({ kind: 'vault', file: folder }, { kind: 'vault', vaultPath: '/photos' }, 'move', deps)
    expect(msg).toContain('Moved')
    expect(calls.some((c) => c.kind === 'invoke:move_file')).toBe(true)
    // No byte copies for a same-namespace folder move.
    expect(calls.some((c) => c.kind === 'invoke:upload_file')).toBe(false)
  })

  it('host tree copy stays on the native recursive verb', async () => {
    const spy = vi.fn()
    const { deps } = fakeDeps({
      run: async (line: string) => {
        spy(line)
        return ''
      },
    })
    const src: MoveSource = { kind: 'host', absPath: '/sdcard/pics', name: 'pics', isDir: true }
    const msg = await moveAnywhere(src, { kind: 'host', absDir: '/sdcard/backup' }, 'copy', deps)
    expect(msg).toContain('pics')
    expect(spy.mock.calls.some(([l]) => String(l).startsWith('cp -os -r'))).toBe(true)
    // Same-filesystem dir moves are one atomic rename, never copy+delete.
    const { deps: d2 } = fakeDeps({
      run: async (line: string) => {
        spy(line)
        return ''
      },
    })
    await moveAnywhere(src, { kind: 'host', absDir: '/sdcard/backup' }, 'move', d2)
    expect(spy.mock.calls.some(([l]) => String(l).startsWith('mv -os'))).toBe(true)
  })

  it('host dir → vault copies the tree file by file', async () => {
    const lines: string[] = []
    const { deps, calls } = fakeDeps({
      run: async (line: string) => {
        lines.push(line)
        if (line.startsWith('ls --json -os')) {
          return JSON.stringify({
            path: '/sdcard/pics',
            host: true,
            entries: [
              { path: '/sdcard/pics/a.txt', name: 'a.txt', kind: 'file', sizeBytes: 5, modifiedMs: 1, isDir: false },
            ],
          })
        }
        return ''
      },
      readHostBytes: async () => new TextEncoder().encode('x'),
    })
    const src: MoveSource = { kind: 'host', absPath: '/sdcard/pics', name: 'pics', isDir: true }
    const msg = await moveAnywhere(src, { kind: 'vault', vaultPath: '/photos' }, 'copy', deps)
    expect(msg).toContain('1 files')
    // Vault leg creates a real folder row (not a host mkdir)…
    expect(calls.some((c) => c.kind === 'invoke:create_folder')).toBe(true)
    expect(lines.some((l) => l.startsWith('mkdir -os'))).toBe(false)
    // …then uploads each verified file into it.
    expect(calls.some((c) => c.kind === 'invoke:upload_file')).toBe(true)
  })
})

describe('shell cwd mirror ignores the host namespace', () => {
  it('cd -os leaves the volume mirror alone', () => {
    expect(trackShellCwd('/vault', 'cd -os /sdcard/Download')).toBe('/vault')
    expect(trackShellCwd('/vault', 'cd -os /sdcard && ls -la -os')).toBe('/vault')
    expect(trackShellCwd('/a', 'cd /b && echo hi')).toBe('/b')
  })
})
