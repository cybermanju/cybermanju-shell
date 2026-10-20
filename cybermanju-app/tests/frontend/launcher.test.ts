// CyberManju OS — Launcher.test (tests/frontend/launcher.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  countUnread,
  displayIcon,
  displayLabel,
  filterOrderedApps,
  formatMessageTime,
  isSwipeUpGesture,
  launcherKeyForApp,
  launcherKeyForPanel,
  nextOrder,
  orderAndroidApps,
} from '@/utils/launcher'
import type { AndroidApp } from '@/types'

function app(pkg: string, label: string): AndroidApp {
  return { packageName: pkg, label, activityClass: `${pkg}.Main`, systemApp: false, socialApp: false, iconBase64: '' }
}

describe('launcher keys', () => {
  it('namespaces android vs home tiles', () => {
    expect(launcherKeyForApp('com.example.a')).toBe('android:com.example.a')
    expect(launcherKeyForPanel('files')).toBe('os:files')
  })
})

describe('display overrides', () => {
  it('prefers alias and custom icons', () => {
    expect(displayLabel('Browser', { alias: 'Web', order: -1, customIcon: '', iconPack: '', hidden: false })).toBe('Web')
    expect(displayLabel('Browser', undefined)).toBe('Browser')
    const img = displayIcon('', 'Browser', { alias: '', order: -1, customIcon: 'data:x', iconPack: '', hidden: false })
    expect(img).toEqual({ kind: 'image', src: 'data:x' })
    const letter = displayIcon('', 'browser', undefined)
    expect(letter).toEqual({ kind: 'letter', src: 'B' })
  })
})

describe('orderAndroidApps', () => {
  it('drops hidden, ordered-first, then label sort', () => {
    const rows = orderAndroidApps(
      [app('c.z', 'Telegram'), app('a.y', 'Browser'), app('b.x', 'Camera')],
      {
        'android:c.z': { alias: '', order: 0, customIcon: '', iconPack: '', hidden: false },
        'android:b.x': { alias: '', order: -1, customIcon: '', iconPack: '', hidden: true },
      },
    )
    expect(rows.map((r) => r.app.packageName)).toEqual(['c.z', 'a.y'])
  })

  it('renamed rows filter by alias', () => {
    const rows = orderAndroidApps([app('a.y', 'Browser')], {
      'android:a.y': { alias: 'Web', order: -1, customIcon: '', iconPack: '', hidden: false },
    })
    expect(filterOrderedApps(rows, 'web')).toHaveLength(1)
    expect(filterOrderedApps(rows, 'brows')).toHaveLength(0)
  })
})

describe('nextOrder', () => {
  it('appends after the max slot', () => {
    expect(nextOrder({})).toBe(0)
    expect(
      nextOrder({ a: { alias: '', order: 3, customIcon: '', iconPack: '', hidden: false } }),
    ).toBe(4)
  })
})

describe('isSwipeUpGesture', () => {
  it('needs the minimum travel', () => {
    expect(isSwipeUpGesture(700, 600)).toBe(true)
    expect(isSwipeUpGesture(700, 650)).toBe(false)
    expect(isSwipeUpGesture(600, 700)).toBe(false)
  })
})

describe('formatMessageTime', () => {
  const now = new Date('2026-10-09T12:00:00Z').getTime()
  it('renders relative buckets', () => {
    expect(formatMessageTime(now - 10_000, now)).toBe('now')
    expect(formatMessageTime(now - 5 * 60_000, now)).toBe('5m')
    expect(formatMessageTime(now - 3 * 3_600_000, now)).toBe('3h')
  })
  it('degrades honestly on bad input', () => {
    expect(formatMessageTime(0, now)).toBe('')
    expect(formatMessageTime(Number.NaN, now)).toBe('')
  })
})

describe('countUnread', () => {
  it('counts only newer than the mark', () => {
    expect(countUnread([100, 200, 300], 200)).toBe(1)
    expect(countUnread([], 0)).toBe(0)
    expect(countUnread([50], 99)).toBe(0)
  })
})
