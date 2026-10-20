// CyberManju OS — UseAndroidApps (src/composables/useAndroidApps.ts).
// CyberManju Android apps bridge — device-local IPC, never REST.
// Off-device the Rust layer refuses with `unsupported:` (no mock rows).

import { ref } from 'vue'
import { invoke, isAndroidApp, isTauri } from '@/composables/useTauri'
import type { AndroidApp, IconPack } from '@/types'

export type LauncherStatus = 'idle' | 'loading' | 'ready' | 'error'

const apps = ref<AndroidApp[]>([])
const iconPacks = ref<IconPack[]>([])
const status = ref<LauncherStatus>('idle')
const lastError = ref<string | null>(null)
let inFlight: Promise<AndroidApp[]> | null = null

export function useAndroidApps() {
  /** True inside the native Android WebView (the only transport that lists). */
  const supported = isTauri() && isAndroidApp()

  async function refresh(force = false): Promise<AndroidApp[]> {
    if (inFlight) return inFlight
    if (!force && status.value === 'ready' && apps.value.length > 0) return apps.value
    status.value = 'loading'
    lastError.value = null
    const run = (async () => {
      try {
        const rows = await invoke<AndroidApp[]>('launcher_list_apps')
        apps.value = Array.isArray(rows) ? rows : []
        status.value = 'ready'
        return apps.value
      } catch (e) {
        status.value = 'error'
        lastError.value = e instanceof Error ? e.message : String(e)
        throw e
      } finally {
        inFlight = null
      }
    })()
    inFlight = run
    return run
  }

  async function openApp(packageName: string): Promise<void> {
    const pkg = packageName.trim()
    if (!pkg) throw new Error('invalid: packageName is required')
    try {
      ;(navigator as Navigator & { vibrate?: (p: number) => boolean }).vibrate?.(8)
    } catch { /* haptics optional */ }
    await invoke('launcher_open_app', { packageName: pkg })
  }

  async function uninstallApp(packageName: string): Promise<void> {
    await invoke('launcher_uninstall_app', { packageName: packageName.trim() })
  }

  async function refreshIconPacks(): Promise<IconPack[]> {
    try {
      const rows = await invoke<IconPack[]>('launcher_list_icon_packs')
      iconPacks.value = Array.isArray(rows) ? rows : []
    } catch (e) {
      // Silent UX fallback (no packs), but never invisible: a real
      // `network:`/`unsupported:` refusal still lands in the console.
      console.debug('launcher_list_icon_packs failed:', e instanceof Error ? e.message : String(e))
      iconPacks.value = []
    }
    return iconPacks.value
  }

  async function packIcon(packPackage: string, appPackage: string): Promise<string> {
    return invoke<string>('launcher_pack_icon', {
      packPackage: packPackage.trim(),
      appPackage: appPackage.trim(),
    })
  }

  return { apps, iconPacks, status, lastError, supported, refresh, openApp, uninstallApp, refreshIconPacks, packIcon }
}
