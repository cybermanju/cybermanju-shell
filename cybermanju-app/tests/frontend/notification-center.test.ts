// CyberManju OS — Notification-center.test (tests/frontend/notification-center.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { beforeEach, describe, expect, it, vi } from 'vitest'
import fs from 'node:fs'

function memStorage(seed: Record<string, string> = {}): Storage {
  const jar = new Map(Object.entries(seed))
  return {
    get length() { return jar.size },
    clear: () => jar.clear(),
    getItem: (k: string) => (jar.has(k) ? jar.get(k)! : null),
    key: (i: number) => [...jar.keys()][i] ?? null,
    removeItem: (k: string) => { jar.delete(k) },
    setItem: (k: string, v: string) => { jar.set(k, String(v)) },
  } as Storage
}

const KEY = 'cybermanju.notifications.history.v1'

describe('notification announcements', () => {
  it('announces in-app toasts politely rather than as assertive alerts', () => {
    const component = fs.readFileSync('src/components/NotificationStack.vue', 'utf8')
    expect(component).toMatch(/class="notification-stack" role="status" aria-live="polite"/)
    expect(component).not.toMatch(/class="notification-stack" role="alert"/)
  })
})

describe('notification history', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.stubGlobal('localStorage', memStorage())
  })

  async function fresh() {
    return await import('@/composables/useNotifications')
  }

  it('persists every notify to history, newest first, unread', async () => {
    const { useNotifications } = await fresh()
    const n = useNotifications()
    n.notify('error', 'disk full', 0, 'Sync')
    n.notify('info', 'hello', 0)
    expect(n.history.value).toHaveLength(2)
    expect(n.history.value[0].message).toBe('hello')
    expect(n.history.value[0].read).toBe(false)
    expect(n.unreadCount.value).toBe(2)
    const stored = JSON.parse(localStorage.getItem(KEY)!) as unknown[]
    expect(stored).toHaveLength(2)
    expect(n.notifications.value).toHaveLength(2)
  })

  it('dismissing the toast keeps the history row', async () => {
    const { useNotifications } = await fresh()
    const n = useNotifications()
    const id = n.notify('warning', 'low space', 0)
    n.dismiss(id)
    expect(n.notifications.value).toHaveLength(0)
    expect(n.history.value).toHaveLength(1)
    expect(n.unreadCount.value).toBe(1)
  })

  it('markRead / markAllRead clear the badge; clearHistory empties', async () => {
    const { useNotifications } = await fresh()
    const n = useNotifications()
    const a = n.notify('info', 'a', 0)
    n.notify('info', 'b', 0)
    n.markRead(a)
    expect(n.unreadCount.value).toBe(1)
    n.markAllRead()
    expect(n.unreadCount.value).toBe(0)
    n.clearHistory()
    expect(n.history.value).toHaveLength(0)
    expect(localStorage.getItem(KEY)).toBe('[]')
  })

  it('removeFromHistory drops one row', async () => {
    const { useNotifications } = await fresh()
    const n = useNotifications()
    const a = n.notify('info', 'a', 0)
    n.notify('info', 'b', 0)
    n.removeFromHistory(a)
    expect(n.history.value.map((h) => h.message)).toEqual(['b'])
  })

  it('caps history at 100 rows', async () => {
    const { useNotifications } = await fresh()
    const n = useNotifications()
    for (let i = 0; i < 105; i++) n.notify('info', `m${i}`, 0)
    expect(n.history.value).toHaveLength(100)
    expect(n.history.value[0].message).toBe('m104')
  })

  it('restores seeded rows on load', async () => {
    vi.resetModules()
    vi.stubGlobal(
      'localStorage',
      memStorage({
        [KEY]: JSON.stringify([
          { id: 'x', type: 'error', title: 'T', message: 'boom', createdAt: 42, read: true },
        ]),
      }),
    )
    const { useNotifications } = await import('@/composables/useNotifications')
    const n = useNotifications()
    expect(n.history.value).toHaveLength(1)
    expect(n.history.value[0].message).toBe('boom')
    expect(n.unreadCount.value).toBe(0)
  })

  it('drops corrupt or malformed payloads', async () => {
    for (const bad of ['not-json{{{', '{"a":1}', '[42, null, {"id":1}]']) {
      vi.resetModules()
      vi.stubGlobal('localStorage', memStorage({ [KEY]: bad }))
      const { useNotifications } = await import('@/composables/useNotifications')
      expect(useNotifications().history.value).toEqual([])
    }
  })
})
