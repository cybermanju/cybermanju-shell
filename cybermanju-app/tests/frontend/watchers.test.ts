// CyberManju OS — Watchers.test (tests/frontend/watchers.test.ts).
// ANALYSIS.md §4.6: proactive watcher rule matching.
import { describe, expect, it } from 'vitest'
import {
  createWatcher,
  evaluateWatchers,
  matchWatcher,
  parseWatchersStore,
  serializeWatchersStore,
  type WatchRule,
} from '../../src/utils/watchers'

function rule(over: Partial<WatchRule> = {}): WatchRule {
  return {
    id: 'w1',
    name: 'photos',
    enabled: true,
    match: {},
    actions: { notify: true },
    createdAt: '2026-01-01T00:00:00Z',
    ...over,
  }
}

describe('matchWatcher', () => {
  it('requires every set matcher; disabled never fires', () => {
    const r = rule({ match: { mimePrefix: 'image/', tag: 'trip' } })
    const f = { id: 'f', name: 'a.jpg', mimeType: 'image/jpeg', tags: ['trip'] }
    expect(matchWatcher(r, f)).toBe(true)
    expect(matchWatcher(r, { ...f, tags: [] })).toBe(false)
    expect(matchWatcher({ ...r, enabled: false }, f)).toBe(false)
  })

  it('treats invalid regex as no-match, never throws', () => {
    expect(matchWatcher(rule({ match: { namePattern: '([' } }), { id: 'f', name: 'x' })).toBe(false)
  })

  it('honors size floors', () => {
    const r = rule({ match: { minSizeBytes: 1000 } })
    expect(matchWatcher(r, { id: 'a', name: 'a', sizeBytes: 999 })).toBe(false)
    expect(matchWatcher(r, { id: 'b', name: 'b', sizeBytes: 1000 })).toBe(true)
  })
})

describe('evaluateWatchers', () => {
  it('returns firing pairs in rule order', () => {
    const rules = [rule({ id: 'w1', match: { mimePrefix: 'image/' } }), rule({ id: 'w2', match: {} })]
    const files = [{ id: 'f', name: 'a.jpg', mimeType: 'image/jpeg' }]
    expect(evaluateWatchers(rules, files).map((p) => p.rule.id)).toEqual(['w1', 'w2'])
  })
})

describe('watcher store', () => {
  it('creates, round-trips, rejects garbage', () => {
    const created = createWatcher([], '  R ', { tag: 'x' }, { notify: true })!
    expect(created.rule.name).toBe('R')
    expect(createWatcher([], '  ', {}, { notify: true })).toBeNull()
    expect(parseWatchersStore(serializeWatchersStore(created.rules))).toHaveLength(1)
    expect(parseWatchersStore('junk')).toEqual([])
  })
})
