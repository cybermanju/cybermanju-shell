// Stub for `cybermanju-os-wasm` in builds without the wasm-pack output.
// Resolved via the `cybermanju-os-wasm` alias in vite.config.ts (desktop
// builds) — a real file, so worker bundles resolve it deterministically
// (virtual-module `resolveId` does not reliably fire for worker chunks).
// Every export throws if ever loaded; the static-host code paths that need
// the real module never run in these builds.
export function os_dispatch() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export async function db_open() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export function db_dispatch() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export function db_snapshot() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export function db_restore() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export function agent_catalog() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export async function agent_prompt() {
  throw new Error('cybermanju-os-wasm is not bundled in this build')
}

export default function init() {
  return Promise.resolve()
}
