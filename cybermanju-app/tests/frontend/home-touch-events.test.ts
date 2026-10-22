// CyberManju OS — Home touch event regression tests.
// Role: compile the real Home SFC and exercise pointer/touch sessions in a Node host.
// Integrates: Vue custom renderer, SFC compiler, Vitest, persisted workspaces.
// Contracts: real open paths, no accidental invocation or page creation after cancellation.
import { readFileSync } from 'node:fs'
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as Vue from 'vue'
import { compileScript, parse } from '@vue/compiler-sfc'
import { ModuleKind, ScriptTarget, transpileModule } from 'typescript'
import * as HomeUtils from '../../src/utils/homeWorkspaces'

const source = readFileSync('src/components/HomeWorkspaceStudio.vue', 'utf8')
const { descriptor } = parse(source)
const compiled = compileScript(descriptor, { id: 'home-touch-events', inlineTemplate: true })
const { outputText } = transpileModule(compiled.content, {
  compilerOptions: { module: ModuleKind.CommonJS, target: ScriptTarget.ES2022 },
})
const openApp = vi.fn(async () => {})
const openId = vi.fn()
const noop = { render: () => null }
const VueRuntime = { ...Vue, vModelText: {}, vModelSelect: {}, Transition: { inheritAttrs: false, props: ['name', 'mode'], setup: (_props: unknown, context: { slots: { default?: () => any } }) => () => context.slots.default?.() } }
const loaded = { exports: {} as Record<string, any> }
new Function('require', 'module', 'exports', outputText)((id: string) => {
  if (id === 'vue') return VueRuntime
  if (id === '@/components/AppIcon.vue' || id === '@/components/HomeLocalWidget.vue') return { default: noop }
  if (id === '@/composables/useAndroidApps') return { useAndroidApps: () => ({ apps: Vue.ref([{ packageName: 'com.example.android', label: 'Android' }]), supported: false, openApp }) }
  if (id === '@/data/homeWidgetCatalog') return { homeWidgetCatalog: [] }
  if (id === '@/utils/homeWorkspaces') return HomeUtils
  throw new Error(`Unexpected HomeWorkspaceStudio dependency: ${id}`)
}, loaded, loaded.exports)
const Studio = loaded.exports.default as Vue.Component

interface HostNode { type: string; props: Record<string, any>; children: Array<HostNode | string>; parent?: HostNode }
const renderer = Vue.createRenderer<HostNode, HostNode>({
  createElement: type => ({ type, props: {}, children: [], classList: { add() {}, remove() {} } } as any),
  createText: text => ({ type: '#text', props: {}, children: [text] }),
  createComment: text => ({ type: '#comment', props: {}, children: [text] }),
  setText: (node, text) => { node.children = [text] },
  setElementText: (node, text) => { node.children = text ? [{ type: '#text', props: {}, children: [text], parent: node }] : [] },
  patchProp: (node, key, _prev, next) => { node.props[key] = next },
  insert: (node, parent, anchor) => { node.parent = parent; const at = anchor ? parent.children.indexOf(anchor) : -1; if (at < 0) parent.children.push(node); else parent.children.splice(at, 0, node) },
  remove: node => { if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1) },
  parentNode: node => node.parent ?? null,
  nextSibling: node => { if (!node.parent) return null; return node.parent.children[node.parent.children.indexOf(node) + 1] as HostNode ?? null },
})
const app = { id: 'notes', label: 'Notes', icon: 'notes' }
const saved = { version: 1, activeId: 'page-a', theme: 'aurora', pages: [
  { id: 'page-a', title: 'Home', items: [{ id: 'item-notes', kind: 'app', ref: 'notes' }] },
  { id: 'page-b', title: 'Empty saved page', items: [] },
] }
let storageValue = JSON.stringify(saved)
let originalLocalStorage: any
let originalWindow: any
let originalDocument: any
let originalWidth: PropertyDescriptor | undefined
let originalHeight: PropertyDescriptor | undefined

function all(node: HostNode, predicate: (node: HostNode) => boolean): HostNode[] {
  return [...(predicate(node) ? [node] : []), ...node.children.filter((x): x is HostNode => typeof x !== 'string').flatMap(child => all(child, predicate))]
}
function mount(fixture = saved) {
  storageValue = JSON.stringify(fixture)
  openApp.mockClear()
  openId.mockClear()
  originalWindow = (globalThis as any).window
  originalDocument = (globalThis as any).document
  const doc = new EventTarget() as any
  doc.visibilityState = 'visible'
  const win = new EventTarget() as any
  win.document = doc
  win.innerWidth = 390
  win.innerHeight = 844
  ;(globalThis as any).window = win
  ;(globalThis as any).document = doc
  originalLocalStorage = globalThis.localStorage
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: { getItem: () => storageValue, setItem: (_key: string, value: string) => { storageValue = value } } })
  originalWidth = Object.getOwnPropertyDescriptor(globalThis, 'innerWidth')
  originalHeight = Object.getOwnPropertyDescriptor(globalThis, 'innerHeight')
  Object.defineProperty(globalThis, 'innerWidth', { configurable: true, value: 390 })
  Object.defineProperty(globalThis, 'innerHeight', { configurable: true, value: 844 })
  const root: HostNode = { type: 'root', props: {}, children: [] }
  const vm = renderer.createApp(Studio, { apps: [app], initialIds: ['notes'], onOpen: openId })
  vm.mount(root)
  const find = (selector: string) => all(root, n => (` ${n.props.class ?? ''} `).includes(` ${selector} `))
  return { root, vm, find, close: () => vm.unmount() }
}
function fire(node: HostNode, type: string, event: any = {}) {
  const capitalized = `on${type[0]!.toUpperCase()}${type.slice(1)}`
  const handlerKey = Object.keys(node.props).find(key => key === capitalized || key.slice(2).toLowerCase().startsWith(type.toLowerCase()))
  const handler = handlerKey ? node.props[handlerKey] : undefined
  event.stopPropagation ??= () => {}
  event.preventDefault ??= () => {}
  if (Array.isArray(handler)) handler.forEach((fn: Function) => fn(event))
  else handler?.(event)
}
function target(closestResult: string | null = null) { return { closest: (selector: string) => selector.split(',').some(s => s.trim() === closestResult) ? {} : null } }
function touch(x: number, y: number, identifier: number, targetClass: string | null = null) { return { pageX: x, pageY: y, clientX: x, clientY: y, identifier, target: target(targetClass) } }
function touchEvent(type: 'touchStart' | 'touchMove' | 'touchEnd' | 'touchCancel', touches: any[], changedTouches: any[] = touches, targetClass: string | null = null) {
  fire(rootNode!, type, { touches, changedTouches, target: target(targetClass), preventDefault() {}, stopPropagation() {} })
}
let rootNode: HostNode | undefined
function setup() { const mounted = mount(); rootNode = mounted.find('home-studio')[0]; return mounted }
function pages() { return JSON.parse(storageValue).pages }
function launchButton(m: ReturnType<typeof setup>) { return m.find('workspace-launch')[0]! }
function homeRoot(m: ReturnType<typeof setup>) { return m.find('home-studio')[0]! }
function tap(m: ReturnType<typeof setup>, detail = 1) { fire(launchButton(m), 'click', { detail }) }
function pointer(m: ReturnType<typeof setup>, phase: string, values: any = {}) { fire(homeRoot(m), `pointer${phase}`, { pointerId: 1, pointerType: 'touch', clientX: 100, clientY: 100, button: 0, ...values, target: target() }) }
function twoFinger(m: ReturnType<typeof setup>, moves: number[] = [], opts: { staggered?: boolean; target?: string | null } = {}) {
  let dx = 0
  const first = touch(150, 300, 1, opts.target), second = touch(240, 300, 2, opts.target)
  touchEvent('touchStart', [first], [first], opts.target)
  touchEvent('touchStart', [first, second], [second], opts.target)
  for (const delta of moves) { dx = delta; touchEvent('touchMove', [touch(150 + delta, 300, 1, opts.target), touch(240 + delta, 300, 2, opts.target)]) }
  if (opts.staggered) {
    touchEvent('touchEnd', [touch(150 + dx, 300, 1, opts.target)], [touch(240 + dx, 300, 2, opts.target)])
    touchEvent('touchEnd', [], [touch(150 + dx, 300, 1, opts.target)])
  } else touchEvent('touchEnd', [], [touch(150 + dx, 300, 1, opts.target), touch(240 + dx, 300, 2, opts.target)])
}

afterEach(() => {
  if (originalWindow === undefined) delete (globalThis as any).window
  else (globalThis as any).window = originalWindow
  if (originalDocument === undefined) delete (globalThis as any).document
  else (globalThis as any).document = originalDocument
  if (originalLocalStorage === undefined) delete (globalThis as any).localStorage
  else Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: originalLocalStorage })
  if (originalWidth) Object.defineProperty(globalThis, 'innerWidth', originalWidth)
  else delete (globalThis as any).innerWidth
  if (originalHeight) Object.defineProperty(globalThis, 'innerHeight', originalHeight)
  else delete (globalThis as any).innerHeight
  rootNode = undefined
})

describe('HomeWorkspaceStudio touch and pointer regressions', () => {
  it('launches an ordinary pointer/touch tap and compatibility click exactly once', async () => { const m = setup(); pointer(m, 'down'); touchEvent('touchStart', [touch(100, 100, 1, 'button')]); pointer(m, 'up'); touchEvent('touchEnd', [], [touch(100, 100, 1, 'button')]); tap(m); expect(openId).toHaveBeenCalledTimes(1); expect(openId).toHaveBeenCalledWith('notes'); m.close() })
  it('launches by keyboard click (detail=0)', async () => { const m = setup(); tap(m, 0); expect(openId).toHaveBeenCalledTimes(1); m.close() })
  it('calls mocked Android openApp exactly once for an Android shortcut', async () => { const fixture = structuredClone(saved); fixture.pages[0]!.items[0]!.ref = 'android:com.example.android'; const n = mount(fixture); rootNode = n.find('home-studio')[0]; tap(n); await Vue.nextTick(); expect(openApp).toHaveBeenCalledTimes(1); expect(openApp).toHaveBeenCalledWith('com.example.android'); n.close() })
  it('retains explicit workspace icon click wiring', async () => { const m = setup(); const button = launchButton(m); expect(button.props.onClick).toBeTypeOf('function'); m.close() })
  it('switches to the existing empty page on a two-finger left swipe without adding pages', async () => { const m = setup(); twoFinger(m, [-12, -28, -48, -70]); await Vue.nextTick(); expect(pages()).toHaveLength(2); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-b'); expect(pages()[1].id).toBe('page-b'); m.close() })
  it('navigates to previous page on a two-finger right swipe', async () => { const fixture = structuredClone(saved); fixture.activeId = 'page-b'; const n = mount(fixture); rootNode = n.find('home-studio')[0]; twoFinger(n, [12, 28, 48, 70]); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); expect(pages()).toHaveLength(2); n.close() })
  it.each([['no movement', [0]], ['sub-threshold travel', [2]], ['short parallel travel', [8, 15]], ['below-threshold swipe', [0, 25]]])('does not navigate for %s', async (_name, moves) => { const m = setup(); twoFinger(m, moves); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); expect(pages()).toHaveLength(2); m.close() })
  it('accepts staggered lift only when the other contact remains and the gesture has completed', async () => { const m = setup(); twoFinger(m, [-14, -32, -52, -70], { staggered: true }); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-b'); m.close() })
  it('rejects staggered lift if the remaining contact moves afterward', async () => { const m = setup(); touchEvent('touchStart', [touch(150, 300, 1)]); touchEvent('touchStart', [touch(150, 300, 1), touch(240, 300, 2)], [touch(240, 300, 2)]); touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2)]); touchEvent('touchEnd', [touch(80, 300, 1)], [touch(170, 300, 2)]); touchEvent('touchMove', [touch(100, 300, 1)]); touchEvent('touchEnd', [], [touch(100, 300, 1)]); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); m.close() })
  it('does not form a two-finger gesture after the 160ms formation window', async () => { vi.useFakeTimers(); const m = setup(); touchEvent('touchStart', [touch(150, 300, 1)]); vi.advanceTimersByTime(161); touchEvent('touchStart', [touch(150, 300, 1), touch(240, 300, 2)], [touch(240, 300, 2)]); touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2)]); touchEvent('touchEnd', [], [touch(80, 300, 1), touch(170, 300, 2)]); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); m.close(); vi.useRealTimers() })
  it('rejects a gesture that starts at the screen edge', async () => { const m = setup(); touchEvent('touchStart', [touch(10, 300, 1)]); touchEvent('touchStart', [touch(10, 300, 1), touch(100, 300, 2)], [touch(100, 300, 2)]); touchEvent('touchMove', [touch(10, 300, 1), touch(170, 300, 2)]); touchEvent('touchEnd', [], [touch(170, 300, 2)]); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); m.close() })
  it.each(['button', 'input', 'textarea', 'select', 'a', '.workspace-widget', '[data-native-widget]'])('does not navigate when started on %s', async control => { const m = setup(); twoFinger(m, [-14, -32, -52, -70], { target: control }); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); m.close() })
  it('does not navigate in overview and keeps two-finger interactions available there', async () => { const m = setup(); const overviewButton = all(m.root, n => n.props['aria-label'] === 'Eagle view of home workspaces')[0]!; fire(overviewButton, 'click'); await Vue.nextTick(); twoFinger(m, [18, 34, 50, 64], { target: '.eagle-viewport' }); await Vue.nextTick(); expect(m.find('eagle-canvas')[0]!.props.style.transform).toContain('translate(64px, 0px)'); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); expect(pages()).toHaveLength(2); m.close() })
  it('does not switch pages on a two-finger pinch in overview', async () => { const m = setup(); const overviewButton = all(m.root, n => n.props['aria-label'] === 'Eagle view of home workspaces')[0]!; fire(overviewButton, 'click'); await Vue.nextTick(); touchEvent('touchStart', [touch(150, 300, 1, '.eagle-viewport'), touch(240, 300, 2, '.eagle-viewport')]); touchEvent('touchMove', [touch(130, 300, 1), touch(260, 300, 2)]); touchEvent('touchEnd', [], [touch(130, 300, 1), touch(260, 300, 2)]); await Vue.nextTick(); expect(m.find('eagle-canvas')[0]!.props.style.transform).toContain('scale(1.4)'); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); expect(pages()).toHaveLength(2); m.close() })
  it('ignores touch cancellation', async () => { const m = setup(); touchEvent('touchStart', [touch(150, 300, 1)]); touchEvent('touchStart', [touch(150, 300, 1), touch(240, 300, 2)], [touch(240, 300, 2)]); touchEvent('touchCancel', [], []); touchEvent('touchEnd', [], [touch(240, 300, 2)]); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); m.close() })
  it('ignores pointer cancellation', async () => { const m = setup(); pointer(m, 'down'); pointer(m, 'cancel'); pointer(m, 'up'); tap(m); expect(openId).not.toHaveBeenCalled(); m.close() })
  it('does not launch after a pointer drag even if a compatibility click follows', async () => { const m = setup(); pointer(m, 'down'); pointer(m, 'move', { clientX: 140 }); pointer(m, 'up', { clientX: 140 }); tap(m); expect(openId).not.toHaveBeenCalled(); m.close() })
  it('does not launch after a canceled pointer interaction followed by compatibility click', async () => { const m = setup(); pointer(m, 'down'); pointer(m, 'cancel'); tap(m); expect(openId).not.toHaveBeenCalled(); m.close() })
  it('does not launch after a touch drag and its compatibility click', async () => { const m = setup(); touchEvent('touchStart', [touch(100, 100, 1)], [touch(100, 100, 1)]); touchEvent('touchMove', [touch(145, 100, 1)]); touchEvent('touchEnd', [], [touch(145, 100, 1)]); tap(m); expect(openId).not.toHaveBeenCalled(); m.close() })
  it('does not launch after multitouch and its compatibility click', async () => { const m = setup(); touchEvent('touchStart', [touch(100, 100, 1)]); touchEvent('touchStart', [touch(100, 100, 1), touch(200, 100, 2)], [touch(200, 100, 2)]); touchEvent('touchEnd', [], [touch(200, 100, 2)]); tap(m); expect(openId).not.toHaveBeenCalled(); m.close() })
  it('does not launch while editing', async () => { const m = setup(); const edit = all(m.root, n => n.type === 'button' && n.props['aria-pressed'] !== undefined)[1]!; fire(edit, 'click', {}); tap(m); expect(openId).not.toHaveBeenCalled(); m.close() })
  it('wires root-level pointer start/move/end and cancel handlers', async () => { const m = setup(); const root = homeRoot(m); for (const name of ['onPointerdown', 'onPointermove', 'onPointerup', 'onPointercancel']) expect(root.props[name]).toBeDefined(); m.close() })
  it('wires root-level touch start/move/end/cancel handlers', async () => { const m = setup(); const root = homeRoot(m); for (const name of ['touchstart', 'touchmove', 'touchend', 'touchcancel']) expect(Object.keys(root.props).some(key => key.slice(2).toLowerCase().startsWith(name))).toBe(true); m.close() })
  it('cancels an in-progress gesture on window blur', async () => { const m = setup(); touchEvent('touchStart', [touch(150, 300, 1)]); touchEvent('touchStart', [touch(150, 300, 1), touch(240, 300, 2)], [touch(240, 300, 2)]); touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2)]); (globalThis as any).window.dispatchEvent(new Event('blur')); touchEvent('touchEnd', [], [touch(80, 300, 1), touch(170, 300, 2)]); await Vue.nextTick(); expect(JSON.parse(storageValue).activeId).toBe('page-a'); m.close() })
})

describe('Home gesture cancellation sessions', () => {
  const startChord = () => {
    touchEvent('touchStart', [touch(150, 300, 1)])
    touchEvent('touchStart', [touch(150, 300, 1), touch(240, 300, 2)], [touch(240, 300, 2)])
  }
  async function unchanged(m: ReturnType<typeof setup>) {
    tap(m)
    await Vue.nextTick()
    expect(JSON.parse(storageValue).activeId).toBe('page-a')
    expect(pages()).toEqual(saved.pages)
    expect(openId).not.toHaveBeenCalled()
    expect(openApp).not.toHaveBeenCalled()
    m.close()
  }
  it('cancels when a third finger joins after translation begins', async () => {
    const m = setup(); startChord()
    touchEvent('touchMove', [touch(125, 300, 1), touch(215, 300, 2)])
    touchEvent('touchStart', [touch(125, 300, 1), touch(215, 300, 2), touch(300, 300, 3)])
    touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2), touch(255, 300, 3)])
    touchEvent('touchEnd', [], [touch(80, 300, 1), touch(170, 300, 2), touch(255, 300, 3)])
    await unchanged(m)
  })
  it('does not promote a moving first contact into a new two-finger session', async () => {
    const m = setup(); touchEvent('touchStart', [touch(150, 300, 1)])
    touchEvent('touchMove', [touch(120, 300, 1)])
    touchEvent('touchStart', [touch(120, 300, 1), touch(240, 300, 2)])
    touchEvent('touchMove', [touch(50, 300, 1), touch(170, 300, 2)])
    touchEvent('touchEnd', [], [touch(50, 300, 1), touch(170, 300, 2)])
    await unchanged(m)
  })
  it('rejects contact disappearance without a normal lift event', async () => {
    const m = setup(); startChord()
    touchEvent('touchMove', [touch(80, 300, 1)])
    touchEvent('touchEnd', [], [touch(80, 300, 1)])
    await unchanged(m)
  })
  it('rejects an actual above-threshold diagonal even if it later straightens', async () => {
    const m = setup(); startChord()
    touchEvent('touchMove', [touch(110, 340, 1), touch(200, 340, 2)])
    touchEvent('touchMove', [touch(50, 300, 1), touch(140, 300, 2)])
    touchEvent('touchEnd', [], [touch(50, 300, 1), touch(140, 300, 2)])
    await unchanged(m)
  })
  it('rejects an actual pinch that later becomes parallel translation', async () => {
    const m = setup(); startChord()
    touchEvent('touchMove', [touch(110, 300, 1), touch(280, 300, 2)])
    touchEvent('touchMove', [touch(50, 300, 1), touch(140, 300, 2)])
    touchEvent('touchEnd', [], [touch(50, 300, 1), touch(140, 300, 2)])
    await unchanged(m)
  })
  it('keeps cancellation latched if another contact arrives before all contacts lift', async () => {
    const m = setup(); startChord(); touchEvent('touchCancel', [], [])
    touchEvent('touchStart', [touch(150, 300, 1), touch(240, 300, 2)])
    touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2)])
    touchEvent('touchEnd', [], [touch(80, 300, 1), touch(170, 300, 2)])
    await unchanged(m)
  })
  it('cancels on visibility loss without launching a compatibility click', async () => {
    const m = setup(); startChord()
    touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2)])
    ;(globalThis as any).document.visibilityState = 'hidden'
    ;(globalThis as any).document.dispatchEvent(new Event('visibilitychange'))
    touchEvent('touchEnd', [], [touch(80, 300, 1), touch(170, 300, 2)])
    await unchanged(m)
  })
  it('does not navigate while editing and does not launch the touched tile', async () => {
    const m = setup()
    const edit = all(m.root, n => n.type === 'button' && n.props['aria-pressed'] !== undefined)[1]!
    fire(edit, 'click'); await Vue.nextTick()
    startChord(); touchEvent('touchMove', [touch(80, 300, 1), touch(170, 300, 2)])
    touchEvent('touchEnd', [], [touch(80, 300, 1), touch(170, 300, 2)])
    await unchanged(m)
  })
})

describe('mobile launcher pass-through CSS contract', () => {
  it('allows workspace strip taps through but restores pointer events on app windows', () => {
    const css = readFileSync('src/components/DesktopShell.vue', 'utf8')
    expect(css).toMatch(/\.desktop-workspace\s*\{\s*pointer-events:\s*none\s*;\s*\}/)
    expect(css).toMatch(/\.desktop-strip\s*\{\s*pointer-events:\s*none\s*;\s*\}/)
    expect(css).toMatch(/\.desktop-workspace\s*:deep\(\.app-window\)\s*\{\s*pointer-events:\s*auto\s*;\s*\}/)
    expect(css).toMatch(/\.desktop-strip\s*:deep\(\.app-window\)\s*\{\s*pointer-events:\s*auto\s*;\s*\}/)
    expect(css).toMatch(/\.mobile-only-launcher\s*\{\s*display:\s*flex\s*!important\s*;\s*\}/)
  })
})
