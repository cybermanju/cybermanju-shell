// CyberManju OS — Host wasm.test (tests/frontend/host-wasm.test.ts).
// WASM host leg: honest errors with no picker (node), unknown shell verbs
// refuse with `unsupported:`, multi-mount folders list/read/write/copy/move
// through an injected fake handle, mount roots refuse deletes, system junk
// hides, and the native namespace math stays intact where no browser window
// exists (no behavior change for Tauri/desktop).
import { describe, expect, it, beforeEach, afterEach } from 'vitest'
import {
  destFromVmPath,
  isHostId,
  isHostPath,
  shellQuote,
} from '../../src/utils/hostBrowse'
import { HOST_WRITE_LIMIT_BYTES } from '../../src/utils/hostFsSafety'
import {
  HOST_WASM_DIR_KEY,
  cpHostWasm,
  hostWasmSupported,
  isPickerCancel,
  isSystemJunk,
  isWasmMountRootAbs,
  listHostWasmDir,
  listHostWasmMounts,
  mkdirHostWasm,
  mvHostWasm,
  pickHostWasmDir,
  readHostWasmBytes,
  resetHostWasmTestState,
  rmHostWasm,
  runWasmHostLine,
  setHostWasmTestHandle,
  splitWasmAbs,
  statHostWasm,
  writeHostWasmBytes,
  writeHostWasmText,
} from '../../src/utils/hostWasm'
import type { FsDirHandle, FsFileHandle } from '../../src/utils/localDir'

let corruptWrites = false

describe('wasm host support probe', () => {
  it('exposes a stable IndexedDB key and reports unsupported in node', () => {
    expect(HOST_WASM_DIR_KEY).toBe('cybermanju.hostDir')
    expect(hostWasmSupported()).toBe(false)
  })

  it('refuses the picker honestly with no File System Access API', async () => {
    await expect(pickHostWasmDir()).rejects.toThrow(/^unsupported:/)
  })

  it('refuses listings honestly with no remembered home folder', async () => {
    await expect(listHostWasmDir('/host')).rejects.toThrow(/^(unsupported|not_found|auth):/)
  })

  it('refuses unknown shell verbs with unsupported:', async () => {
    await expect(runWasmHostLine('frobnicate -os /x')).rejects.toThrow(/^unsupported:/)
  })
})

describe('native host namespace untouched in node (no window)', () => {
  it('still detects host paths and ids', () => {
    expect(isHostPath('/host')).toBe(true)
    expect(isHostPath('/host/Photos/a.jpg')).toBe(true)
    expect(isHostId('host:/Photos/a.jpg')).toBe(true)
    expect(isHostId('vault-1')).toBe(false)
  })

  it('keeps /host unresolvable without a pwd -os round-trip', () => {
    // Node has no window, so the WASM leg stays off and the native rule
    // (`/host` needs `pwd -os`) is preserved for desktop callers.
    expect(destFromVmPath('/host')).toBeNull()
    expect(destFromVmPath('/host/sdcard/Download')).toEqual({
      kind: 'host',
      absDir: '/sdcard/Download',
    })
  })
})

class FakeFile implements FsFileHandle {
  constructor(
    public name: string,
    public bytes: Uint8Array = new Uint8Array(0),
  ) {}
  async getFile() {
    const bytes = this.bytes
    return { arrayBuffer: async () => bytes.buffer as ArrayBuffer, size: bytes.length }
  }
  async createWritable() {
    const self = this
    let buf = new Uint8Array(0)
    return {
      async write(data: Uint8Array) {
        const next = new Uint8Array(buf.length + data.length)
        next.set(buf, 0)
        next.set(data, buf.length)
        buf = next
      },
      async close() {
        const saved = new Uint8Array(buf)
        if (corruptWrites && saved.length) saved[0] ^= 0xff
        self.bytes = saved
      },
    }
  }
}

class FakeDir implements FsDirHandle {
  files = new Map<string, FakeFile>()
  dirs = new Map<string, FakeDir>()
  constructor(public name: string) {}
  async *keys(): AsyncIterableIterator<string> {
    yield* [...this.dirs.keys(), ...this.files.keys()]
  }
  async getFileHandle(name: string, opts?: { create?: boolean }) {
    const hit = this.files.get(name)
    if (hit) return hit
    if (opts?.create) {
      const f = new FakeFile(name)
      this.files.set(name, f)
      return f
    }
    throw new Error('not found')
  }
  async getDirectoryHandle(name: string, opts?: { create?: boolean }) {
    const hit = this.dirs.get(name)
    if (hit) return hit
    if (opts?.create) {
      const d = new FakeDir(name)
      this.dirs.set(name, d)
      return d
    }
    throw new Error('not found')
  }
  async removeEntry(name: string, opts?: { recursive?: boolean }) {
    if (this.files.delete(name)) return
    const dir = this.dirs.get(name)
    if (!dir) throw new Error('not found')
    if ((dir.files.size > 0 || dir.dirs.size > 0) && !opts?.recursive) {
      throw new Error('needs recursive')
    }
    this.dirs.delete(name)
  }
}

const enc = (s: string) => new TextEncoder().encode(s)
const dec = (b: Uint8Array) => new TextDecoder().decode(b)

describe('wasm multi-mount folders (injected handle)', () => {
  beforeEach(() => {
    corruptWrites = false
    const root = new FakeDir('Documentos')
    root.files.set('foto.jpg', new FakeFile('foto.jpg', enc('jpeg-bytes')))
    root.files.set('Thumbs.db', new FakeFile('Thumbs.db', enc('junk')))
    root.files.set('.secreto', new FakeFile('.secreto', enc('x')))
    const sub = new FakeDir('trabalho')
    sub.files.set('nota.txt', new FakeFile('nota.txt', enc('ola')))
    root.dirs.set('trabalho', sub)
    setHostWasmTestHandle({ id: 'test', label: 'Docs' }, root)
  })

  afterEach(() => {
    corruptWrites = false
    resetHostWasmTestState()
  })

  it('lists the mounts at /host and files inside a mount', async () => {
    const mounts = await listHostWasmMounts()
    expect(mounts).toHaveLength(1)
    expect(mounts[0]).toMatchObject({ id: 'test', label: 'Docs', status: 'ready' })
    const root = await listHostWasmDir('/host')
    expect(root).toHaveLength(1)
    expect(root[0]).toMatchObject({ id: 'host:/Docs', path: '/host/Docs', fileType: 'folder' })
    const rows = await listHostWasmDir('/host/Docs')
    const byName = Object.fromEntries(rows.map((r) => [r.name, r]))
    expect(byName['foto.jpg']).toMatchObject({ id: 'host:/Docs/foto.jpg', parentId: '/host/Docs', fileType: 'file' })
    expect(byName['trabalho']?.fileType).toBe('folder')
  })

  it('marks system junk + dotfiles hidden (FileManager hides them)', async () => {
    const rows = await listHostWasmDir('/host/Docs')
    const byName = Object.fromEntries(rows.map((r) => [r.name, r]))
    expect(byName['Thumbs.db']?.isHidden).toBe(true)
    expect(byName['.secreto']?.isHidden).toBe(true)
    expect(byName['foto.jpg']?.isHidden).toBe(false)
  })

  it('reads, writes, stats, creates and deletes through the mount', async () => {
    expect(dec(await readHostWasmBytes('/Docs/foto.jpg'))).toBe('jpeg-bytes')
    await writeHostWasmBytes('/Docs/trabalho/nova.txt', enc('novo'))
    expect(dec(await readHostWasmBytes('/Docs/trabalho/nova.txt'))).toBe('novo')
    expect(await statHostWasm('/Docs')).toMatchObject({ isDir: true, name: 'Docs' })
    expect(await statHostWasm('/Docs/foto.jpg')).toMatchObject({ isDir: false, sizeBytes: 10 })
    await mkdirHostWasm('/Docs/trabalho/fundo')
    await writeHostWasmBytes('/Docs/trabalho/fundo/f.txt', enc('y'))
    await rmHostWasm('/Docs/trabalho/fundo/f.txt', false)
    await expect(readHostWasmBytes('/Docs/trabalho/fundo/f.txt')).rejects.toThrow(/^not_found:/)
  })

  it('refuses mount-root deletes, root reads and root writes honestly', async () => {
    await expect(rmHostWasm('/Docs', true)).rejects.toThrow(/^invalid:/)
    await expect(rmHostWasm('/', true)).rejects.toThrow(/^invalid:/)
    await expect(readHostWasmBytes('/Docs')).rejects.toThrow(/^invalid:/)
    await expect(writeHostWasmBytes('/', enc('x'))).rejects.toThrow(/^invalid:/)
  })

  it('rejects `..` paths instead of allowing a mount-relative escape', async () => {
    expect(splitWasmAbs('/Docs/../outside.txt')).toBeNull()
    await expect(readHostWasmBytes('/Docs/../outside.txt')).rejects.toThrow(/^invalid:/)
  })

  it('copies and moves trees across the shell shim with verification', async () => {
    await runWasmHostLine('cp -os -r "/Docs/trabalho" "/Docs/copia"')
    expect(dec(await readHostWasmBytes('/Docs/copia/nota.txt'))).toBe('ola')
    await runWasmHostLine('mv -os "/Docs/copia/nota.txt" "/Docs/movida.txt"')
    expect(dec(await readHostWasmBytes('/Docs/movida.txt'))).toBe('ola')
    await expect(readHostWasmBytes('/Docs/copia/nota.txt')).rejects.toThrow(/^not_found:/)
    const ls = JSON.parse(await runWasmHostLine('ls --json -os "/Docs"')) as { entries: { name: string }[] }
    expect(ls.entries.map((e) => e.name).sort()).toContain('movida.txt')
    await expect(runWasmHostLine('cp -os -r "/Docs" "/Docs/x"')).rejects.toThrow(/^invalid:/)
  })

  it('refuses copying or moving a mount path onto itself or into its own subtree', async () => {
    await expect(cpHostWasm('/Docs/trabalho', '/Docs', true)).rejects.toThrow(/^invalid:/)
    await expect(cpHostWasm('/Docs/trabalho', '/Docs/trabalho/sub', true)).rejects.toThrow(/^invalid:/)
    await expect(mvHostWasm('/Docs/trabalho', '/Docs/trabalho/sub')).rejects.toThrow(/^invalid:/)
    await expect(mvHostWasm('/Docs/trabalho/nota.txt', '/Docs/trabalho')).rejects.toThrow(/^invalid:/)
    expect(dec(await readHostWasmBytes('/Docs/trabalho/nota.txt'))).toBe('ola')
  })

  it('detects same-length corruption for file and recursive copies before accepting them', async () => {
    corruptWrites = true
    await expect(cpHostWasm('/Docs/trabalho/nota.txt', '/Docs/broken.txt', false)).rejects.toThrow(/^integrity:/)
    await expect(cpHostWasm('/Docs/trabalho', '/Docs/broken-tree', true)).rejects.toThrow(/^integrity:/)
    await expect(mvHostWasm('/Docs/trabalho/nota.txt', '/Docs/move-broken.txt')).rejects.toThrow(/^integrity:/)
    expect(dec(await readHostWasmBytes('/Docs/trabalho/nota.txt'))).toBe('ola')
  })

  it('round-trips empty and shell-special text, and measures the limit in UTF-8 bytes', async () => {
    const content = '  $HOME `literal` "quoted" \\\\ repeated  spaces  '
    await runWasmHostLine(`write -os ${shellQuote('/Docs/quoted name.txt')} ${shellQuote(content)}`)
    expect(dec(await readHostWasmBytes('/Docs/quoted name.txt'))).toBe(content)
    await runWasmHostLine(`write -os ${shellQuote('/Docs/empty.txt')} ${shellQuote('')}`)
    expect(dec(await readHostWasmBytes('/Docs/empty.txt'))).toBe('')

    const tooLarge = '😀'.repeat(Math.floor(HOST_WRITE_LIMIT_BYTES / 4) + 1)
    await expect(writeHostWasmText('/Docs/too-big.txt', tooLarge)).rejects.toThrow(/^too_large:/)
  })

  it('reports unknown mounts honestly', async () => {
    await expect(listHostWasmDir('/host/Fantasma')).rejects.toThrow(/^not_found:/)
    await expect(readHostWasmBytes('/Fantasma/a.txt')).rejects.toThrow(/^not_found:/)
  })
})

describe('wasm mount path helpers', () => {
  it('splits absolutes into mount label + rel', () => {
    expect(splitWasmAbs('/Docs/a/b.txt')).toEqual({ label: 'Docs', rel: 'a/b.txt' })
    expect(splitWasmAbs('/Docs')).toEqual({ label: 'Docs', rel: '' })
    expect(splitWasmAbs('/')).toBeNull()
    expect(splitWasmAbs('')).toBeNull()
  })

  it('detects mount-root absolutes (never moved/deleted as bytes)', () => {
    expect(isWasmMountRootAbs('/Docs')).toBe(true)
    expect(isWasmMountRootAbs('/Docs/a.txt')).toBe(false)
    expect(isWasmMountRootAbs('/')).toBe(false)
    expect(isWasmMountRootAbs('')).toBe(false)
  })

  it('detects picker cancellation (silent, never an error toast)', () => {
    expect(isPickerCancel({ name: 'AbortError' })).toBe(true)
    expect(isPickerCancel(new Error('auth: x'))).toBe(false)
    expect(isPickerCancel(null)).toBe(false)
  })

  it('flags OS metadata junk for the hidden filter', () => {
    for (const n of ['Thumbs.db', 'desktop.ini', '.DS_Store', '$RECYCLE.BIN', 'System Volume Information', '.Trash-1000', 'lost+found']) {
      expect(isSystemJunk(n), n).toBe(true)
    }
    for (const n of ['foto.jpg', 'nota.txt', 'trabalho']) {
      expect(isSystemJunk(n), n).toBe(false)
    }
  })
})
