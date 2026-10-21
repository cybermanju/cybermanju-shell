// CyberManju OS — Home workspace model tests.
// Role: exercise persistence, limits, corruption, movement and gesture thresholds.
import { describe, expect, it } from 'vitest'
import {
  addHomeItem, classifyHomeGesture, createHomeState, ensureTrailingWorkspace,
  MAX_ITEMS_PER_WORKSPACE, MAX_WORKSPACES, moveHomeItem, normalizeHomeState,
  removeHomeItem, type HomeWorkspaceState,
} from '../src/utils/homeWorkspaces'

describe('home workspaces', () => {
  it('creates first page with apps and an empty trailing page', () => {
    const state = createHomeState(['com.example.one', 'com.example.two'])
    expect(state.pages).toHaveLength(2)
    expect(state.pages[0].items.map(item => item.ref)).toEqual(['com.example.one', 'com.example.two'])
    expect(state.pages[1].items).toEqual([])
    expect(state.activeId).toBe(state.pages[0].id)
  })

  it('normalizes corruption, duplicate IDs, malformed records and invalid active/theme', () => {
    const state = normalizeHomeState({
      version: 1, activeId: 'missing', theme: 'nope', pages: [null,
        { id: 'p', title: 9, items: [
          { id: 'same', kind: 'app', ref: 'com.android.settings', data: { ok: true } },
          { id: 'same', kind: 'app', ref: 'duplicate' },
          { id: 'bad', kind: 'nope', ref: 'bad' },
          { id: 'unsafe', kind: 'widget', ref: 'widget', data: { nope: null } },
        ] }, { id: 'p', title: 'duplicate page', items: [] },
      ],
    }, [])
    expect(state.activeId).toBe('p')
    expect(state.theme).toBe('aurora')
    expect(state.pages[0].items).toEqual([{ id: 'same', kind: 'app', ref: 'com.android.settings', data: { ok: true } }])
    expect(state.pages.at(-1)?.items).toEqual([])
  })

  it('roundtrips valid state and does not drop unknown Android app references', () => {
    const input: HomeWorkspaceState = { version: 1, activeId: 'page', theme: 'pixel', pages: [
      { id: 'page', title: 'Apps', items: [{ id: 'a', kind: 'app', ref: 'com.vendor.android.special' }] },
      { id: 'empty', title: '', items: [] },
    ] }
    expect(normalizeHomeState(JSON.parse(JSON.stringify(input)), [])).toEqual(input)
  })

  it('enforces page/item caps and keeps trailing empty page while under page cap', () => {
    const state = createHomeState([])
    state.pages = Array.from({ length: MAX_WORKSPACES - 1 }, (_, i) => ({ id: `p${i}`, title: '', items: [{ id: `i${i}`, kind: 'widget' as const, ref: 'x' }] }))
    ensureTrailingWorkspace(state)
    expect(state.pages).toHaveLength(MAX_WORKSPACES)
    expect(state.pages.at(-1)?.items).toEqual([])
    const crowded = state.pages[0]
    crowded.items = Array.from({ length: MAX_ITEMS_PER_WORKSPACE }, (_, i) => ({ id: `full${i}`, kind: 'app' as const, ref: `app${i}` }))
    expect(addHomeItem(state, crowded.id, { id: 'extra', kind: 'app', ref: 'extra' })).toBe(false)
  })

  it('adds, moves and removes items, maintaining the empty trailing page', () => {
    const state = createHomeState([])
    const [first, trailing] = state.pages
    expect(addHomeItem(state, first.id, { id: 'x', kind: 'widget', ref: 'clock' })).toBe(true)
    const next = state.pages.at(-1)!
    expect(next.items).toEqual([])
    expect(moveHomeItem(state, 'x', first.id, next.id)).toBe(true)
    expect(first.items).toEqual([])
    expect(next.items.map(i => i.id)).toEqual(['x'])
    removeHomeItem(state, next.id, 'x')
    expect(next.items).toEqual([])
    expect(state.pages.at(-1)?.items).toEqual([])
    expect(moveHomeItem(state, 'absent', first.id, trailing.id)).toBe(false)
  })

  it('preserves empty named pages and active IDs after the last item is removed', () => {
    const state = createHomeState(['files'])
    const first = state.pages[0]
    first.title = 'My quiet space'
    removeHomeItem(state, first.id, first.items[0].id)
    expect(state.pages[0].id).toBe(first.id)
    expect(state.activeId).toBe(first.id)
    expect(state.pages[0].title).toBe('My quiet space')
    expect(normalizeHomeState(JSON.parse(JSON.stringify(state)), []).pages).toEqual(state.pages)
  })
  it('refuses duplicate app or widget references when moving to a page', () => {
    const state = createHomeState(['files'])
    const [first, second] = state.pages
    addHomeItem(state, second.id, {id: 'second-files', kind: 'app', ref: 'files'})
    expect(moveHomeItem(state, first.items[0].id, first.id, second.id)).toBe(false)
    expect(first.items).toHaveLength(1)
  })
  it('bounds user text and rejects prototype-like data keys', () => {
    const state = normalizeHomeState({version: 1, pages: [{id: 'p', title: 'a'.repeat(100), items: [{id: 'i', kind: 'widget', ref: 'notes', data: JSON.parse('{"__proto__":"unsafe","text":"ok"}')}]}]}, [])
    expect(state.pages[0].title).toHaveLength(40)
    expect(Object.hasOwn(state.pages[0].items[0].data!, '__proto__')).toBe(false)
  })
  it('classifies directional one- and three-finger gestures at thresholds', () => {
    expect(classifyHomeGesture(0, -72, 3)).toBe('overview-open')
    expect(classifyHomeGesture(0, 72, 3)).toBe('overview-close')
    expect(classifyHomeGesture(64, 0, 1)).toBe('previous')
    expect(classifyHomeGesture(-64, 0, 1)).toBe('next')
    expect(classifyHomeGesture(64, 64, 1)).toBeNull()
    expect(classifyHomeGesture(200, 0, 2)).toBeNull()
    expect(classifyHomeGesture(0, 100, 1)).toBeNull()
  })
})
