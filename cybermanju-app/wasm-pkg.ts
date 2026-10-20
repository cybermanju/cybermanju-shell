// CyberManju OS — Wasm pkg (wasm-pkg.ts).
// Freshness gate for the wasm-pack output (`crates/os-wasm/pkg`).
//
// pkg/ is a gitignored artifact: CI rebuilds it in the wasm job, but a local
// copy outlives Rust edits and would otherwise be bundled silently — which
// is how a build shipped with a frontend calling `agent_catalog`/
// `agent_prompt` while the wasm only exported the older surface. Compare the
// artifact's mtime against the newest source it is built from and refuse to
// use a stale one: fall back to the stub instead of shipping dead exports.
import { existsSync, readdirSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

const PKG_ENTRY = 'crates/os-wasm/pkg/cybermanju_os_wasm.js'

// Everything under crates/ that can change the artifact: Rust sources plus
// the Cargo manifests (a dependency edit rebuilds the wasm too).
const SOURCE_RE = /\.(rs|toml)$/
// Build/cache dirs never feed the artifact (an in-crate `target/` holds no
// sources anyway, but a stray one must not mask a genuinely stale pkg).
const SKIP_DIRS = new Set(['target', 'pkg', 'node_modules'])

let warned = false

/** Newest mtime over the Rust sources and Cargo manifests under crates/; 0
 *  when the tree is missing. */
function newestSourceMtime(root: string): number {
  let newest = 0
  const walk = (dir: string): void => {
    let entries
    try {
      entries = readdirSync(dir, { withFileTypes: true })
    } catch {
      return
    }
    for (const entry of entries) {
      const path = join(dir, entry.name)
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name)) walk(path)
      } else if (entry.isFile() && SOURCE_RE.test(entry.name)) {
        const mtime = statSync(path).mtimeMs
        if (mtime > newest) newest = mtime
      }
    }
  }
  walk(join(root, 'crates'))
  return newest
}

/**
 * Path to the wasm-pack entry file, or `null` when there is no usable pkg/
 * — absent (Docker/frontend-only stage) or older than `crates/**` (rebuild
 * it, don't bundle it). Callers then resolve `cybermanju-os-wasm` to the
 * wasm stub.
 */
export function wasmPkgEntry(root: string): string | null {
  const entry = resolve(root, PKG_ENTRY)
  if (!existsSync(entry)) return null
  if (newestSourceMtime(root) > statSync(entry).mtimeMs) {
    if (!warned) {
      warned = true
      console.warn(
        `[wasm] ${PKG_ENTRY} is older than the Rust sources in crates/ — ` +
          'ignoring it and bundling the wasm stub instead. Rebuild with:\n' +
          '  wasm-pack build crates/os-wasm --target web --out-dir pkg',
      )
    }
    return null
  }
  return entry
}
