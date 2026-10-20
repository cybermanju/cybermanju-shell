// CyberManju OS — Local dir.test (tests/frontend/local-dir.test.ts).
// Remembered folder handles — real File System Access I/O through
// in-memory fake handles (node has no picker): list/read/write/mkdir/delete
// round-trip, `..` escapes refuse, missing paths report `not_found:`, and a
// permission-less environment reports `unsupported` without prompting.
import { describe, expect, it } from 'vitest'
import {
  deleteLocalPath,
  ensureLocalDir,
  listLocalDir,
  mkdirLocalDir,
  readLocalFile,
  writeLocalFile,
  type FsDirHandle,
  type FsFileHandle,
} from '../../src/utils/localDir'

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
        self.bytes = buf
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

describe('local dir file I/O', () => {
  it('writes, reads, lists and deletes through nested paths', async () => {
    const root = new FakeDir('vault')
    await writeLocalFile(root, 'files/notes/hi.txt', enc('hello'))
    await writeLocalFile(root, 'files/todo.txt', enc('buy milk'))
    expect(dec(await readLocalFile(root, 'files/notes/hi.txt'))).toBe('hello')
    const top = await listLocalDir(root)
    expect(top.map((e) => e.name)).toEqual(['files'])
    expect(top[0]?.isDir).toBe(true)
    const inner = await listLocalDir(root, 'files')
    expect(inner.map((e) => `${e.isDir ? 'd' : 'f'}:${e.name}`).sort()).toEqual(['d:notes', 'f:todo.txt'])
    await deleteLocalPath(root, 'files/todo.txt')
    expect((await listLocalDir(root, 'files')).map((e) => e.name)).toEqual(['notes'])
    await expect(readLocalFile(root, 'files/todo.txt')).rejects.toThrow(/^not_found:/)
  })

  it('creates directory trees and refuses escapes', async () => {
    const root = new FakeDir('vault')
    await mkdirLocalDir(root, 'a/b/c')
    await writeLocalFile(root, 'a/b/c/f.txt', enc('x'))
    expect(dec(await readLocalFile(root, 'a/b/c/f.txt'))).toBe('x')
    await expect(readLocalFile(root, '../evil.txt')).rejects.toThrow(/^invalid:/)
    await expect(writeLocalFile(root, 'a/../../evil.txt', enc('x'))).rejects.toThrow(/^invalid:/)
  })

  it('reports missing paths honestly', async () => {
    const root = new FakeDir('vault')
    await expect(readLocalFile(root, 'nope.txt')).rejects.toThrow(/^not_found:/)
    await expect(listLocalDir(root, 'ghost')).rejects.toThrow(/^not_found:/)
    await expect(deleteLocalPath(root, 'ghost.txt')).rejects.toThrow(/^not_found:/)
  })
})

describe('ensureLocalDir without a picker', () => {
  it('reports unsupported in node (no File System Access API)', async () => {
    const st = await ensureLocalDir('cybermanju.wizardLocalDir')
    expect(st.status).toBe('unsupported')
  })
})
