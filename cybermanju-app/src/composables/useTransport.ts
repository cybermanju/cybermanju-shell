// CyberManju OS — shared transport helper (P1-2).
//
// Single 3-state classification for the three runtimes:
//   tauri — desktop shell, Tauri IPC (+ REST-first os/disk via :3456)
//   wasm  — static host (GitHub Pages / WASM pack), local OPFS backend
//   rest  — web dashboard origin (:3456) or an explicit server URL
//
// Ordering mirrors the LandingPage boot probe
// (`isTauri → isStaticHost → rest`): `isStaticHost()` is the synchronous
// first-paint signal. `wasmBackendActive()` only flips after the async wasm
// load resolves, so labels must NOT branch on it (else a Web→WASM flicker,
// P1-4). `wasmBackendActive()` remains the right check for "backend ready",
// never for "which transport".
//
// `VITE_TRANSPORT` build override (documented here and in Settings):
//   'tauri' | 'wasm' | 'rest' (case-insensitive;
//   aliases: 'desktop'→tauri, 'static'/'pages'→wasm, 'web'→rest).
//   Takes precedence when set; unknown values are ignored.

import { isStaticHost, isTauri } from './useTauri'

export type TransportBackend = 'tauri' | 'wasm' | 'rest'
export type TransportTone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'

export interface TransportInfo {
  backend: TransportBackend
  /** Full human label, e.g. 'WASM local (Pages)'. */
  label: string
  /** Compact uppercase tag for badges / status bar, e.g. 'WASM'. */
  short: 'TAURI' | 'WASM' | 'WEB'
  tone: TransportTone
}

const OVERRIDES: Record<string, TransportBackend> = {
  tauri: 'tauri',
  desktop: 'tauri',
  wasm: 'wasm',
  static: 'wasm',
  pages: 'wasm',
  rest: 'rest',
  web: 'rest',
}

function infoFor(backend: TransportBackend): TransportInfo {
  switch (backend) {
    case 'tauri':
      return {
        backend,
        label: 'Tauri desktop (REST-first: os/disk via :3456)',
        short: 'TAURI',
        tone: 'accent',
      }
    case 'wasm':
      return { backend, label: 'WASM local (Pages)', short: 'WASM', tone: 'info' }
    case 'rest':
      return { backend, label: 'Web / REST (:3456)', short: 'WEB', tone: 'neutral' }
  }
}

/**
 * Shared transport classification. Pure + synchronous — safe to call inside
 * a `computed()` for a reactive label without first-paint flicker.
 */
export function useTransport(): TransportInfo {
  const raw = import.meta.env?.VITE_TRANSPORT as string | undefined
  if (raw) {
    const forced = OVERRIDES[raw.trim().toLowerCase()]
    if (forced) return infoFor(forced)
  }
  if (isTauri()) return infoFor('tauri')
  if (isStaticHost()) return infoFor('wasm')
  return infoFor('rest')
}
