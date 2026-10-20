// Virtual module for `cybermanju-os-wasm` in builds that don't have the
// wasm-pack output (`crates/os-wasm/pkg`):
// - desktop/Tauri builds (vite.config.ts): the OS layer goes through Tauri
//   IPC / same-origin REST, so the stub throws only if ever loaded — which
//   never happens (isStaticHost() is false there).
// - Docker frontend stage (vite.config.wasm.ts without pkg): the image serves
//   the dashboard REST API on :3456, so the frontend never imports the wasm
//   backend either.
// When a usable pkg EXISTS (wasm/GH-Pages build), the alias in
// vite.config.wasm.ts resolves first and this plugin is never consulted —
// "usable" meaning present *and* not older than crates/ (see wasm-pkg.ts).
import { wasmPkgEntry } from './wasm-pkg'

const STUB_SOURCE = `
function unavailable() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}
export const os_dispatch = unavailable
export const db_open = unavailable
export const db_dispatch = unavailable
export const db_snapshot = unavailable
export const db_restore = unavailable
export const agent_catalog = unavailable
export const agent_prompt = unavailable
export default function init() {
  return Promise.resolve()
}
`

export default function wasmStubPlugin() {
  return {
    name: 'cybermanju-os-wasm-stub',
    resolveId(id: string) {
      if (id !== 'cybermanju-os-wasm') return null
      // Usable pkg? Let the alias handle it (plugins run after aliases, so
      // returning null here defers to the alias — and if the alias target is
      // missing or stale we stub instead of failing the build).
      if (wasmPkgEntry(process.cwd())) return null
      return '\0cybermanju-os-wasm-stub'
    },
    load(id: string) {
      if (id === '\0cybermanju-os-wasm-stub') return STUB_SOURCE
      return null
    },
  }
}
