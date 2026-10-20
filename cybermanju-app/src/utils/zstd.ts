// CyberManju OS — zstd in the browser (WASM transport).
//
// The native pipeline (`crates/compression`) encodes zstd through `zstd-sys`
// (C via `cc`), which can never build for `wasm32-unknown-unknown` — that is
// why the `cybermanju-os-wasm` crate historically shipped lz4 + brotli only
// and refused `zstd`/`triple` honestly. Instead of fighting the C toolchain,
// the browser encodes/decodes zstd through `@dweb-browser/zstd-wasm`
// (wasm-bindgen glue around `zstd-rs`, same `28 B5 2F FD` frames the desktop
// reads and writes):
//
//   - `compress(source: Uint8Array, level: number): Uint8Array`
//   - `decompress(source: Uint8Array): Uint8Array`
//
// Insights applied from the three referenced sources:
//
//   - `@dweb-browser/zstd-wasm` over `@bokuweb/zstd-wasm`: the former is
//     wasm-bindgen output (the same toolchain as our own `os-wasm` crate),
//     needs no bundler rules, and offers `initSync` over an embedded
//     base64 binary. The bokuweb build is Emscripten output and needs
//     `file-loader` (webpack 4) / `asset/resource` (webpack 5) rules plus a
//     deployed `zstd.wasm` next to the bundle — framework-specific wiring we
//     deliberately avoid on Pages.
//   - `ssojet.com/compression/compress-files-with-zstd-in-wasm`: `compress`
//     takes `Uint8Array`/`ArrayBuffer` directly, `decompress` throws on
//     non-zstd input (callers map that to `integrity:`), and large inputs
//     should stay off the critical path — the callers keep the 1 MiB
//     `STATIC_WRITE_LIMIT` / vault caps, and init is lazy (first zstd use)
//     so the ~890 KB embedded binary never joins the initial bundle.
//   - `bokuweb/zstd-wasm` API shape (`init` once, then sync
//     `compress(buf, level)` / `decompress(buf)`, default level 3): mirrored
//     here, except the default level is 15 to match `TripleCompressor`
//     (`crates/compression/src/triple.rs`), clamped to the 1–22 range.
//
// Init strategy is deliberately fetch-free: `initSync` over the embedded
// base64 binary (`zstd_wasm_bg_wasm`, dynamically imported so it stays out
// of the main chunk). No `?url` asset, no relative-URL fetch that 404s when
// the host does not deploy the `.wasm` next to the glue — offline-friendly
// and identical in dev, Pages and vitest (node). The db-worker never loads
// this module (compression runs on the main thread only).
//
// The triple chain (`LZ4 → ZSTD → Brotli`, the exact order of
// `TripleCompressor::compress_triple`) is composed here from the caller's
// lz4/brotli closures (Rust wasm exports) plus this module's zstd, so every
// browser entry point (cybsh verbs, file pipeline) shares one definition.

export const ZSTD_DEFAULT_LEVEL = 15
export const ZSTD_MIN_LEVEL = 1
export const ZSTD_MAX_LEVEL = 22

/** Lz4/brotli closures supplied by the caller (Rust wasm exports). */
export interface TripleCodecs {
  compressLz4(data: Uint8Array): Uint8Array
  decompressLz4(data: Uint8Array): Uint8Array
  compressBrotli(data: Uint8Array): Uint8Array
  decompressBrotli(data: Uint8Array): Uint8Array
}

interface ZstdGlue {
  compress(source: Uint8Array, level: number): Uint8Array
  decompress(source: Uint8Array): Uint8Array
}

let glue: ZstdGlue | null = null
let initPromise: Promise<ZstdGlue> | null = null

/** Test/DI helper: replace the loader (pass `null` to restore it). */
export function setZstdImpl(impl: ZstdGlue | null): void {
  glue = impl
  initPromise = impl ? Promise.resolve(impl) : null
}

function clampLevel(level: number): number {
  if (!Number.isFinite(level)) return ZSTD_DEFAULT_LEVEL
  return Math.min(ZSTD_MAX_LEVEL, Math.max(ZSTD_MIN_LEVEL, Math.round(level)))
}

/**
 * Load (once) and init the zstd wasm module. Resolves `null` when the
 * dependency cannot load — callers answer `unsupported:` then, never a
 * fake success. Never throws for missing-module reasons; a loaded module
 * that fails to instantiate still resolves `null`.
 */
export async function ensureZstd(): Promise<ZstdGlue | null> {
  if (glue) return glue
  if (!initPromise) {
    initPromise = (async (): Promise<ZstdGlue> => {
      const mod = (await import('@dweb-browser/zstd-wasm')) as unknown as {
        default?: (input?: unknown) => Promise<unknown>
        initSync?: (input?: unknown) => unknown
        compress?: (source: Uint8Array, level: number) => Uint8Array
        decompress?: (source: Uint8Array) => Uint8Array
      }
      if (typeof mod.compress !== 'function' || typeof mod.decompress !== 'function') {
        throw new Error('zstd glue has no compress/decompress exports')
      }
      // Fetch-free init: the embedded base64 binary keeps working even when
      // the host never deploys a standalone `.wasm` asset (Pages, workers).
      // `didInit` guards the one path that would otherwise hand back an
      // uninitialised module (no bytes AND no default init): that resolves
      // `null` below so callers stay honest instead of throwing wasm glue
      // errors at compress time.
      let didInit = false
      if (typeof mod.initSync === 'function') {
        try {
          const bin = (await import('@dweb-browser/zstd-wasm/zstd_wasm_bg_wasm')) as unknown as {
            default?: () => ArrayBuffer
          }
          const bytes = typeof bin.default === 'function' ? bin.default() : null
          if (bytes) {
            mod.initSync({ module: bytes })
            didInit = true
          } else if (typeof mod.default === 'function') {
            await mod.default()
            didInit = true
          }
        } catch {
          // A deployed `.wasm` next to the glue is the fallback (vite emits
          // it for `new URL(..., import.meta.url)` when the host serves it).
          if (typeof mod.default === 'function') {
            await mod.default()
            didInit = true
          }
        }
      } else if (typeof mod.default === 'function') {
        await mod.default()
        didInit = true
      }
      if (!didInit) throw new Error('zstd wasm init produced no instance')
      return {
        compress: mod.compress as ZstdGlue['compress'],
        decompress: mod.decompress as ZstdGlue['decompress'],
      }
    })()
    initPromise.catch(() => {
      initPromise = null
    })
  }
  try {
    glue = await initPromise
    return glue
  } catch {
    initPromise = null
    return null
  }
}

/** True once the module has loaded (no I/O — for UI gating only). */
export function isZstdReady(): boolean {
  return glue !== null
}

/** One zstd frame, level clamped to 1–22 (default 15, the desktop level). */
export async function compressZstd(data: Uint8Array, level = ZSTD_DEFAULT_LEVEL): Promise<Uint8Array> {
  const z = await ensureZstd()
  if (!z) throw new Error('unsupported: zstd needs the browser wasm bundle — reconnect after the next deploy')
  return z.compress(data, clampLevel(level))
}

/** Inverse of {@link compressZstd} — throws on non-zstd input. */
export async function decompressZstd(data: Uint8Array): Promise<Uint8Array> {
  const z = await ensureZstd()
  if (!z) throw new Error('unsupported: zstd needs the browser wasm bundle — reconnect after the next deploy')
  return z.decompress(data)
}

/**
 * Triple-layer compression: LZ4 → ZSTD → Brotli (byte-identical order to
 * `TripleCompressor::compress_triple`; any standard decoder chain —
 * desktop `zstd::decode_all`, `ruzstd`, this module — opens it).
 */
export async function compressTriple(
  codecs: TripleCodecs,
  data: Uint8Array,
  zstdLevel = ZSTD_DEFAULT_LEVEL,
): Promise<{ bytes: Uint8Array; sizes: { lz4: number; zstd: number; brotli: number } }> {
  // Fail fast on a missing module before doing any leg of the chain.
  const z = await ensureZstd()
  if (!z) throw new Error('unsupported: triple needs zstd in the browser wasm bundle — reconnect after the next deploy')
  const lz4 = codecs.compressLz4(data)
  const zstd = z.compress(lz4, clampLevel(zstdLevel))
  const brotli = codecs.compressBrotli(zstd)
  return { bytes: brotli, sizes: { lz4: lz4.length, zstd: zstd.length, brotli: brotli.length } }
}

/** Triple-layer decompression: Brotli → ZSTD → LZ4. */
export async function decompressTriple(codecs: TripleCodecs, data: Uint8Array): Promise<Uint8Array> {
  if (data.length === 0) return new Uint8Array(0)
  const brotli = codecs.decompressBrotli(data)
  const z = await ensureZstd()
  if (!z) throw new Error('unsupported: triple needs zstd in the browser wasm bundle — reconnect after the next deploy')
  const zstd = z.decompress(brotli)
  return codecs.decompressLz4(zstd)
}
