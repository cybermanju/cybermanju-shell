// CyberManju OS — external opener (src/utils/openExternal.ts).
// Role: one seam that hands links and device files to the platform opener, so
//   every runtime gets the path it can actually execute.
// Exports: openExternalUrl, openLocalFile, externalOpenRoute.
// Integrates: useTauri (isAndroidApp / isTauri / invoke), @tauri-apps/plugin-shell
//   (desktop/macOS/Windows), window.open (plain browser tabs).
// Contracts: Android routes to `launcher_open_url` / `launcher_open_file` — the
//   Intent bridge in `cybermanju-launcher` — because `plugin-shell open` spawns
//   `xdg-open`, which Android does not ship; that failure surfaced on every
//   Google/GitHub/GitLab button as `Scoped shell IO error: No such file or
//   directory (os error 2)`. Scheme/file allow-lists and the `invalid:` /
//   `not_found:` / `unsupported:` prefixes live in the Rust command.

import { invoke, isAndroidApp, isTauri } from '@/composables/useTauri'

/** Which opener one target gets on the current runtime. */
export type ExternalOpenRoute = 'android-intent' | 'shell' | 'browser-tab'

/**
 * Pure routing decision (unit-tested on its own): the Android WebView has no
 * opener of its own, every other Tauri build keeps the proven shell opener,
 * and a browser tabs out.
 */
export function externalOpenRoute(runtime: { android: boolean; tauri: boolean }): ExternalOpenRoute {
  if (runtime.android) return 'android-intent'
  if (runtime.tauri) return 'shell'
  return 'browser-tab'
}

/** Current-runtime route — the seam the opener functions share. */
function currentRoute(): ExternalOpenRoute {
  return externalOpenRoute({ android: isAndroidApp(), tauri: isTauri() })
}

/**
 * Open an external link (https/mailto/tel plus the `cybermanju://` OAuth deep
 * link) in the app that owns it. Native failures reject with the backend's
 * `prefix: detail` string so callers can show it verbatim.
 */
export async function openExternalUrl(url: string): Promise<void> {
  const target = String(url ?? '').trim()
  if (!target) throw new Error('unsupported: nothing to open')
  const route = currentRoute()
  if (route === 'android-intent') {
    await invoke('launcher_open_url', { url: target })
    return
  }
  if (route === 'shell') {
    const { open } = await import('@tauri-apps/plugin-shell')
    await open(target)
    return
  }
  if (typeof window !== 'undefined') window.open(target, '_blank', 'noopener,noreferrer')
}

/**
 * Hand one local device file to a system viewer (`OPEN WITH SYSTEM`). A
 * browser has no system opener, so it refuses honestly instead of pretending:
 * the File Manager downloads the bytes on that leg itself.
 */
export async function openLocalFile(path: string): Promise<void> {
  const target = String(path ?? '').trim()
  if (!target) throw new Error('unsupported: nothing to open')
  const route = currentRoute()
  if (route === 'android-intent') {
    await invoke('launcher_open_file', { path: target })
    return
  }
  if (route === 'shell') {
    const { open } = await import('@tauri-apps/plugin-shell')
    await open(target)
    return
  }
  throw new Error('unsupported: opening a device file needs the native app')
}
