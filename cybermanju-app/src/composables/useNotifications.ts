// CyberManju OS — UseNotifications (src/composables/useNotifications.ts).
// Role: Vue shared state: in-app toasts + persisted notification history.
// Exports: NotificationType, Notification, HistoryEntry, useNotifications.
// Integrates: vue, @vueuse.
// Contracts: history is per-device localStorage (cap 100, best-effort); toasts
// stay ephemeral (auto-dismiss removes the toast, never the history row).
import { computed, ref } from 'vue'
import { useWebNotification } from '@vueuse/core'

export type NotificationType = 'success' | 'error' | 'warning' | 'info'

export interface Notification {
  id: string
  type: NotificationType
  /** Banner title (defaults to the type label when omitted). */
  title?: string
  message: string
  duration: number
  createdAt: number
}

/** One persisted history row — the notification center's "while you were away". */
export interface HistoryEntry {
  id: string
  type: NotificationType
  title?: string
  message: string
  createdAt: number
  read: boolean
}

const HISTORY_KEY = 'cybermanju.notifications.history.v1'
const HISTORY_CAP = 100

function readHistory(): HistoryEntry[] {
  try {
    if (typeof localStorage === 'undefined') return []
    const raw = localStorage.getItem(HISTORY_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    const out: HistoryEntry[] = []
    for (const row of parsed) {
      if (!row || typeof row !== 'object') continue
      const r = row as Record<string, unknown>
      if (typeof r.id !== 'string' || typeof r.message !== 'string') continue
      const type = r.type as NotificationType
      if (type !== 'success' && type !== 'error' && type !== 'warning' && type !== 'info') continue
      out.push({
        id: r.id,
        type,
        title: typeof r.title === 'string' ? r.title.slice(0, 120) : undefined,
        message: (r.message as string).slice(0, 2000),
        createdAt: typeof r.createdAt === 'number' && Number.isFinite(r.createdAt) ? r.createdAt : 0,
        read: r.read === true,
      })
      if (out.length >= HISTORY_CAP) break
    }
    return out
  } catch {
    return []
  }
}

const notifications = ref<Notification[]>([])
const history = ref<HistoryEntry[]>(readHistory())
let counter = 0

function persistHistory() {
  try {
    if (typeof localStorage === 'undefined') return
    localStorage.setItem(HISTORY_KEY, JSON.stringify(history.value.slice(0, HISTORY_CAP)))
  } catch {
    // Quota or private mode: the session copy is the truth.
  }
}

export function useNotifications() {
  // OS-level mirror: if the user already granted notification permission,
  // errors/warnings also surface outside the window (Web Notification API
  // via VueUse). Never prompts on its own — permission is requested from
  // the Devices panel.
  let showWeb: ((opts: object) => void) | null = null
  try {
    const web = useWebNotification({ title: 'CyberManju OS' })
    if (web.isSupported.value && web.permissionGranted.value) showWeb = web.show as (opts: object) => void
  } catch { showWeb = null }

  function notify(type: NotificationType, message: string, duration = 4000, title?: string) {
    const id = `notify-${++counter}`
    const at = Date.now()
    const n: Notification = { id, type, title, message, duration, createdAt: at }
    notifications.value.push(n)
    // Every toast also lands in the persisted center history (newest first,
    // unread until marked read). Dismissing the toast never drops the row.
    history.value.unshift({ id, type, title, message, createdAt: at, read: false })
    if (history.value.length > HISTORY_CAP) history.value.length = HISTORY_CAP
    persistHistory()
    if (type === 'error' || type === 'warning') {
      try { showWeb?.({ body: message, tag: id }) } catch { /* in-app toast is enough */ }
    }
    if (duration > 0) {
      setTimeout(() => dismiss(id), duration)
    }
    return id
  }

  function success(message: string) { return notify('success', message) }
  function error(message: string) { return notify('error', message) }
  function warning(message: string) { return notify('warning', message) }
  function info(message: string) { return notify('info', message) }

  function dismiss(id: string) {
    const idx = notifications.value.findIndex((n: Notification) => n.id === id)
    if (idx !== -1) notifications.value.splice(idx, 1)
  }

  function clearAll() {
    notifications.value.splice(0)
  }

  /** Unread history rows — the center badge ("what happened while away"). */
  const unreadCount = computed(() => history.value.filter((h) => !h.read).length)

  function markRead(id: string) {
    const row = history.value.find((h) => h.id === id)
    if (row && !row.read) {
      row.read = true
      persistHistory()
    }
  }

  function markAllRead() {
    if (history.value.some((h) => !h.read)) {
      for (const h of history.value) h.read = true
      persistHistory()
    }
  }

  function removeFromHistory(id: string) {
    const idx = history.value.findIndex((h) => h.id === id)
    if (idx !== -1) {
      history.value.splice(idx, 1)
      persistHistory()
    }
  }

  function clearHistory() {
    if (history.value.length) {
      history.value.splice(0)
      persistHistory()
    }
  }

  return { notifications, history, unreadCount, notify, success, error, warning, info, dismiss, clearAll, markRead, markAllRead, removeFromHistory, clearHistory }
}
