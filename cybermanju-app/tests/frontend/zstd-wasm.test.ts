// CyberManju OS — Zstd wasm.test (tests/frontend/zstd-wasm.test.ts).
// zstd in the browser — `@dweb-browser/zstd-wasm` (wasm-bindgen around
// zstd-rs) round-trips standard frames, and the cybsh shell + triple chain
// compose it with lz4/brotli in the desktop order (LZ4 → ZSTD → Brotli).
//
// The triple legs here use identity lz4/brotli stand-ins: they prove the
// chain mechanics (order, envelope, error mapping), while the zstd leg is
// the real module — the leg that was missing before this change.
import { beforeAll, describe, expect, it } from 'vitest'
import { brotliCompressSync, brotliDecompressSync } from 'node:zlib'
import {
  compressTriple,
  compressZstd,
  decompressTriple,
  decompressZstd,
  ensureZstd,
  ZSTD_DEFAULT_LEVEL,
} from '../../src/utils/zstd'
import { compressExtension, runStaticCybshLine, type StaticCybshDeps } from '../../src/utils/staticCybsh'

const enc = (s: string): Uint8Array => new TextEncoder().encode(s)
const dec = (b: Uint8Array): string => new TextDecoder().decode(b)

// Identity stand-ins for the Rust wasm lz4/brotli exports (node has no
// wasm-pack bundle; the real legs are exercised by the Rust tests and the
// browser build).
const identityCodecs = {
  compressLz4: (d: Uint8Array) => d,
  decompressLz4: (d: Uint8Array) => d,
  compressBrotli: (d: Uint8Array) => d,
  decompressBrotli: (d: Uint8Array) => d,
}

// Production-faithful brotli leg: node:zlib speaks the same RFC 7932 frames
// as the Rust `brotli` crate, so this triple crosses implementation
// boundaries exactly like a browser→desktop `.cyb3` does. (Only the test
// file imports node:zlib — production stays browser-safe.)
const nodeBrotliCodecs = {
  ...identityCodecs,
  compressBrotli: (d: Uint8Array) => brotliCompressSync(d),
  decompressBrotli: (d: Uint8Array) => brotliDecompressSync(d),
}

function shellDeps(volume: Record<string, string>, zstd: 'real' | 'missing'): StaticCybshDeps {
  // 'missing' omits the optional legs entirely — the stale-bundle shape the
  // handlers must refuse with `unsupported:`, never `invalid:`/`integrity:`.
  // 'real' uses the true zstd module plus a genuine brotli leg
  // (node:zlib) — the closest node gets to the production wasm pair.
  const zstdLegs =
    zstd === 'real'
      ? {
          compressZstd: (d: Uint8Array) => compressZstd(d),
          decompressZstd: (d: Uint8Array) => decompressZstd(d),
          compressTriple: async (d: Uint8Array) => (await compressTriple(nodeBrotliCodecs, d)).bytes,
          decompressTriple: (d: Uint8Array) => decompressTriple(nodeBrotliCodecs, d),
        }
      : {}
  return {
    readVolume: () => ({ ...volume }),
    getCwd: async () => '/',
    writeVolumeFile: async (path, content) => {
      volume[path] = content
    },
    deleteVolumePath: async () => 0,
    killTask: async () => false,
    listSyncConfigs: async () => [],
    getConfigSecret: async () => '',
    getDiskStatus: async () => ({ attached: false, name: '', savedBytes: 0, dirty: false }),
    getStorageEstimate: async () => null,
    listMounts: async () => [],
    providerRead: async () => new Uint8Array(0),
    providerWrite: async () => undefined,
    providerDelete: async () => undefined,
    providerList: async () => [],
    probeProviderQuota: async (cfg) => ({ configId: cfg.id, backendType: cfg.backendType, ok: false, detail: '', error: 'unsupported: probe' }),
    keyGet: async () => null,
    keySet: async () => undefined,
    chacha: async () => null,
    codecs: async () => ({
      ...identityCodecs,
      ...zstdLegs,
    }),
    blake3: async () => null,
  }
}

describe('zstd module (@dweb-browser/zstd-wasm)', () => {
  // First touch compiles the ~890 KB embedded binary — absorb that once
  // with a generous budget so the assertions below measure codec behavior,
  // not worker cold-start under a loaded suite.
  beforeAll(async () => {
    expect(await ensureZstd()).not.toBeNull()
  }, 60000)

  it('loads and reports the default desktop level', async () => {
    expect(ZSTD_DEFAULT_LEVEL).toBe(15)
    const z = await ensureZstd()
    expect(z).not.toBeNull()
  })

  it('round-trips compressible bytes with a standard zstd frame', async () => {
    const data = enc('hello world '.repeat(200))
    const compressed = await compressZstd(data)
    // Standard zstd magic `28 B5 2F FD` — the desktop `zstd::decode_all`
    // and the crate's `ruzstd` decoder both accept these frames.
    expect([...compressed.slice(0, 4)]).toEqual([0x28, 0xb5, 0x2f, 0xfd])
    expect(compressed.length).toBeLessThan(data.length)
    expect(dec(await decompressZstd(compressed))).toBe(dec(data))
  })

  it('clamps out-of-range levels instead of failing', async () => {
    const data = enc('level clamp probe '.repeat(100))
    for (const level of [-5, 22, 99]) {
      const out = await compressZstd(data, level)
      expect(dec(await decompressZstd(out))).toBe(dec(data))
    }
  })

  it('rejects non-zstd input on decompress', async () => {
    await expect(decompressZstd(enc('not a zstd frame at all'))).rejects.toThrow()
  })

  it('round-trips the triple chain (LZ4 → ZSTD → Brotli)', async () => {
    const data = enc('triple chain probe '.repeat(150))
    const { bytes, sizes } = await compressTriple(identityCodecs, data)
    expect(sizes.lz4).toBe(data.length)
    expect(sizes.zstd).toBeLessThan(data.length)
    expect(bytes.length).toBe(sizes.brotli)
    expect(dec(await decompressTriple(identityCodecs, bytes))).toBe(dec(data))
  })

  it('triple chain interops with a foreign brotli leg (node:zlib)', async () => {
    const data = enc('cross-implementation triple '.repeat(150))
    const { bytes } = await compressTriple(nodeBrotliCodecs, data)
    // Outer layer is a genuine brotli stream a non-Rust decoder opens…
    const middle = brotliDecompressSync(bytes)
    // …whose payload is a standard zstd frame…
    expect([...middle.slice(0, 4)]).toEqual([0x28, 0xb5, 0x2f, 0xfd])
    expect(dec(await decompressZstd(middle))).toBe(dec(data))
    // …and the whole chain unwinds through the foreign leg.
    expect(dec(await decompressTriple(nodeBrotliCodecs, bytes))).toBe(dec(data))
  })

  it('maps extensions per layer', () => {
    expect(compressExtension('lz4')).toBe('.lz4')
    expect(compressExtension('zstd')).toBe('.zst')
    expect(compressExtension('brotli')).toBe('.br')
    expect(compressExtension('triple')).toBe('.cyb3')
  })
})

describe('cybsh compress/decompress with zstd legs', () => {
  it('compresses and decompresses zstd (.zst envelope)', async () => {
    const volume: Record<string, string> = { '/notes.txt': 'hello world' }
    const deps = shellDeps(volume, 'real')
    const c = await runStaticCybshLine('compress /notes.txt zstd', deps)
    expect(c?.ok).toBe(true)
    expect(volume['/notes.txt.zst']).toBeDefined()
    expect(JSON.parse(volume['/notes.txt.zst']).alg).toBe('zstd')
    delete volume['/notes.txt']
    const d = await runStaticCybshLine('decompress /notes.txt.zst', deps)
    expect(d?.ok).toBe(true)
    expect(volume['/notes.txt']).toBe('hello world')
  })

  it('compresses and decompresses triple (.cyb3 envelope)', async () => {
    const volume: Record<string, string> = { '/notes.txt': 'hello world' }
    const deps = shellDeps(volume, 'real')
    const c = await runStaticCybshLine('compress /notes.txt triple', deps)
    expect(c?.ok).toBe(true)
    expect(volume['/notes.txt.cyb3']).toBeDefined()
    expect(JSON.parse(volume['/notes.txt.cyb3']).alg).toBe('triple')
    delete volume['/notes.txt']
    const d = await runStaticCybshLine('decompress /notes.txt.cyb3', deps)
    expect(d?.ok).toBe(true)
    expect(volume['/notes.txt']).toBe('hello world')
  })

  it('still compresses lz4 and brotli', async () => {
    const volume: Record<string, string> = { '/a.txt': 'hello', '/b.txt': 'world' }
    const deps = shellDeps(volume, 'real')
    expect((await runStaticCybshLine('compress /a.txt lz4', deps))?.ok).toBe(true)
    expect(volume['/a.txt.lz4']).toBeDefined()
    expect((await runStaticCybshLine('compress /b.txt brotli', deps))?.ok).toBe(true)
    expect(volume['/b.txt.br']).toBeDefined()
  })

  it('refuses unknown layers', async () => {
    const volume: Record<string, string> = { '/notes.txt': 'hello world' }
    const deps = shellDeps(volume, 'real')
    const bad = await runStaticCybshLine('compress /notes.txt snappy', deps)
    expect(bad?.output).toMatch(/^unsupported:/)
  })

  it('refuses zstd/triple honestly when the bundle predates the legs', async () => {
    const volume: Record<string, string> = { '/notes.txt': 'hello world' }
    const deps = shellDeps(volume, 'missing')
    // lz4 still works — only the missing legs refuse.
    expect((await runStaticCybshLine('compress /notes.txt lz4', deps))?.ok).toBe(true)
    const zst = await runStaticCybshLine('compress /notes.txt zstd', deps)
    expect(zst?.output).toMatch(/^unsupported:/)
    const tri = await runStaticCybshLine('compress /notes.txt triple', deps)
    expect(tri?.output).toMatch(/^unsupported:/)
    // …and so does the read path for envelopes naming those legs.
    deps.writeVolumeFile('/a.zst', JSON.stringify({ alg: 'zstd', data: 'eA==' }))
    expect((await runStaticCybshLine('decompress /a.zst', deps))?.output).toMatch(/^unsupported:/)
    deps.writeVolumeFile('/a.cyb3', JSON.stringify({ alg: 'triple', data: 'eA==' }))
    expect((await runStaticCybshLine('decompress /a.cyb3', deps))?.output).toMatch(/^unsupported:/)
  })
})
