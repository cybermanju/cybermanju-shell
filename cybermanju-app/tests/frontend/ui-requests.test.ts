// CyberManju OS — Ui requests.test (tests/frontend/ui-requests.test.ts).
// One-shot UI requests from the agent (`ui_open_panel` / `ui_notify`) —
// pure apply + dedupe. A replayed snapshot must never re-open a panel or
// re-fire a toast; an unknown panel id never opens a ghost window.
import { describe, expect, it } from 'vitest'
import { applyUiRequest, createUiRequestDedupe, isKnownPanel, type UiRequest } from '@/utils/uiRequests'
import { MODULE_METADATA } from '@/types'

const noop = { open: () => {}, notify: () => {} }

describe('isKnownPanel', () => {
  it('accepts canonical panel ids', () => {
    for (const id of ['files', 'terminal', 'agent', 'settings', 'disks', 'processes', 'secrets'] as const) {
      if (id === 'secrets') continue // Phase 3 adds it — guarded there
      expect(isKnownPanel(id), id).toBe(true)
    }
  })

  it('accepts aliases that resolve onto known windows', () => {
    expect(isKnownPanel('cron')).toBe(true)
    expect(isKnownPanel('storage')).toBe(true)
    expect(isKnownPanel('compression')).toBe(true)
  })

  it('refuses unknown or empty ids', () => {
    expect(isKnownPanel('nope')).toBe(false)
    expect(isKnownPanel('')).toBe(false)
    expect(isKnownPanel('   ')).toBe(false)
    // `landing` is a shell state, not an openable window.
    expect(isKnownPanel('landing')).toBe(false)
  })

  it('MODULE_METADATA is the parity source (every canonical id is known)', () => {
    for (const id of Object.keys(MODULE_METADATA)) {
      if (id === 'landing') continue
      expect(isKnownPanel(id), id).toBe(true)
    }
  })
})

describe('applyUiRequest', () => {
  it('opens a panel with tab and path props', () => {
    const opened: Array<{ panel: string; props?: Record<string, unknown> }> = []
    const req: UiRequest = { op: 'open', panel: 'cron', tab: 'schedules', path: '/inbox', seq: 1 }
    const note = applyUiRequest(req, {
      open: (panel, props) => opened.push({ panel, props }),
      notify: () => {},
    })
    expect(note).toBe('opened cron')
    expect(opened).toEqual([{ panel: 'processes', props: { tab: 'schedules', path: '/inbox' } }])
  })

  it('refuses unknown panels without opening anything', () => {
    const opened: string[] = []
    const note = applyUiRequest(
      { op: 'open', panel: 'nope', seq: 1 },
      { open: (p) => opened.push(p), notify: () => {} },
    )
    expect(note).toBeNull()
    expect(opened).toEqual([])
  })

  it('notifies with a validated level, defaulting bad levels to info', () => {
    const fired: Array<[string, string]> = []
    const handlers = { open: () => {}, notify: (l: string, m: string) => fired.push([l, m]) }
    expect(applyUiRequest({ op: 'notify', level: 'warning', msg: 'careful', seq: 1 }, handlers)).toBe('notified (warning)')
    expect(applyUiRequest({ op: 'notify', level: 'bogus', msg: 'x', seq: 2 }, handlers)).toBe('notified (info)')
    expect(fired).toEqual([
      ['warning', 'careful'],
      ['info', 'x'],
    ])
  })

  it('rejects malformed requests without throwing', () => {
    expect(applyUiRequest(null, noop)).toBeNull()
    expect(applyUiRequest(undefined, noop)).toBeNull()
    expect(applyUiRequest({ op: 'open', panel: '', seq: 1 }, noop)).toBeNull()
    expect(applyUiRequest({ op: 'notify', msg: '', seq: 1 }, noop)).toBeNull()
    expect(applyUiRequest({ op: 'notify', level: 'info', msg: '   ', seq: 1 }, noop)).toBeNull()
    expect(applyUiRequest({ op: 'unknown' as never, seq: 1 }, noop)).toBeNull()
  })
})

describe('createUiRequestDedupe', () => {
  it('applies each seq exactly once', () => {
    const fired: string[] = []
    const dedupe = createUiRequestDedupe()
    const handlers = {
      open: (p: string) => fired.push(`open:${p}`),
      notify: (_l: string, m: string) => fired.push(`notify:${m}`),
    }
    expect(dedupe.apply({ op: 'notify', level: 'info', msg: 'a', seq: 1 }, handlers)).toBe('notified (info)')
    // Replayed snapshot with the same seq — no-op.
    expect(dedupe.apply({ op: 'notify', level: 'info', msg: 'a', seq: 1 }, handlers)).toBeNull()
    expect(dedupe.apply({ op: 'open', panel: 'files', seq: 2 }, handlers)).toBe('opened files')
    expect(dedupe.apply({ op: 'open', panel: 'files', seq: 2 }, handlers)).toBeNull()
    // Older seq never re-applies.
    expect(dedupe.apply({ op: 'notify', level: 'info', msg: 'old', seq: 1 }, handlers)).toBeNull()
    expect(fired).toEqual(['notify:a', 'open:files'])
    expect(dedupe.seq).toBe(2)
  })
})
