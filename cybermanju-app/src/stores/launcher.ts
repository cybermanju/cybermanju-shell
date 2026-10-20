// CyberManju OS — Launcher (src/stores/launcher.ts).
// CyberManju launcher customization store — rename / reorder / custom
// icons / hide for BOTH Android apps (`android:<package>`) and home tiles
// (`os:<panel>`). Persisted in localStorage (instant, offline, per-device);
// the shape is JSON-safe so it can ride the vault export later.

import { defineStore } from 'pinia'
import { ref, watch } from 'vue'
import { defaultLauncherOverride, type LauncherOverride } from '@/types'
import { nextOrder } from '@/utils/launcher'
import type { OverrideMap } from '@/utils/launcher'

const LS_KEY = 'cybermanju.launcher.overrides.v1'
const LS_SEEN_KEY = 'cybermanju.launcher.messagesSeen.v1'

function readStored(): OverrideMap {
  try {
    const raw = localStorage.getItem(LS_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as Record<string, Partial<LauncherOverride>>
    const out: OverrideMap = {}
    for (const [k, v] of Object.entries(parsed)) {
      if (!v || typeof v !== 'object') continue
      out[k] = {
        alias: typeof v.alias === 'string' ? v.alias.slice(0, 48) : '',
        order: typeof v.order === 'number' && Number.isFinite(v.order) ? Math.floor(v.order) : -1,
        customIcon: typeof v.customIcon === 'string' ? v.customIcon.slice(0, 200_000) : '',
        iconPack: typeof v.iconPack === 'string' ? v.iconPack.slice(0, 256) : '',
        hidden: v.hidden === true,
      }
    }
    return out
  } catch {
    return {}
  }
}

export const useLauncherStore = defineStore('launcher', () => {
  const overrides = ref<OverrideMap>(readStored())
  /** Drawer open state — home swipe-up and the handle button share it. */
  const drawerOpen = ref(false)
  /** Notification-center open state — the bell icons toggle it. */
  const messagesOpen = ref(false)
  /** Which center tab opens: system history vs Android messages hub. */
  const centerTab = ref<'system' | 'messages'>('messages')
  /** Last mark-read time for the hub badge (unix millis, per-device). */
  const messagesSeenAt = ref(0)

  watch(
    overrides,
    (v) => {
      try {
        localStorage.setItem(LS_KEY, JSON.stringify(v))
      } catch {
        // Quota (many custom icons): keep the session copy; the oldest
        // custom icons could be pruned here in a follow-up.
      }
    },
    { deep: true },
  )

  function get(key: string): LauncherOverride {
    return overrides.value[key] ?? defaultLauncherOverride()
  }

  function patch(key: string, p: Partial<LauncherOverride>) {
    overrides.value = { ...overrides.value, [key]: { ...get(key), ...p } }
  }

  function rename(key: string, alias: string) {
    patch(key, { alias: alias.trim().slice(0, 48) })
  }

  function setIcon(key: string, dataUrl: string) {
    patch(key, { customIcon: dataUrl })
  }

  function setIconPack(key: string, pack: string) {
    patch(key, { iconPack: pack })
  }

  function hide(key: string, hidden = true) {
    patch(key, { hidden })
  }

  function pinToEnd(key: string) {
    if ((overrides.value[key]?.order ?? -1) >= 0) return
    patch(key, { order: nextOrder(overrides.value) })
  }

  function move(key: string, dir: -1 | 1) {
    // Directional move swaps with the neighbour in resolved order. The
    // caller passes the resolved key list; here we only bump the slot and
    // let the sort re-render — neighbours without slots sort by label.
    const cur = get(key).order
    const base = cur >= 0 ? cur : nextOrder(overrides.value)
    patch(key, { order: Math.max(0, base + dir) })
  }

  function swap(keyA: string, keyB: string) {
    const a = get(keyA).order
    const b = get(keyB).order
    patch(keyA, { order: b >= 0 ? b : nextOrder(overrides.value) })
    patch(keyB, { order: a >= 0 ? a : nextOrder(overrides.value) + 1 })
  }

  function reset(key: string) {
    const next = { ...overrides.value }
    delete next[key]
    overrides.value = next
  }

  function setDrawer(open: boolean) {
    drawerOpen.value = open
  }

  function setMessagesOpen(open: boolean, tab?: 'system' | 'messages') {
    if (tab) centerTab.value = tab
    messagesOpen.value = open
  }

  function markMessagesSeen(atMs = Date.now()) {
    messagesSeenAt.value = atMs
    try {
      localStorage.setItem(LS_SEEN_KEY, String(atMs))
    } catch {
      // Session-only then.
    }
  }

  // Restore the badge mark (separate key so override edits never wipe it).
  try {
    const raw = localStorage.getItem(LS_SEEN_KEY)
    if (raw) messagesSeenAt.value = Number(raw) || 0
  } catch {
    // Session-only.
  }

  return { overrides, drawerOpen, messagesOpen, centerTab, messagesSeenAt, get, patch, rename, setIcon, setIconPack, hide, pinToEnd, move, swap, reset, setDrawer, setMessagesOpen, markMessagesSeen }
})
