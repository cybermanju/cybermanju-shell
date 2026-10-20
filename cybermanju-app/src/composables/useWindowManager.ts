// CyberManju OS — UseWindowManager (src/composables/useWindowManager.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: WindowState, ShellLayoutMode, StripDirection, shellLayoutMode, shellStripDirection, shellAutoTile, stripOffset, stripLine.
// Integrates: vue, @vueuse, @/….
// Contracts: signals: AGENT-.
import { ref, computed, markRaw, defineAsyncComponent, defineComponent, h, type Component } from 'vue'
import { useLocalStorage } from '@vueuse/core'
import type { PanelType } from '@/types'
import { MODULE_METADATA } from '@/types'
import {
  computeTileRects,
  clampStripOffset,
  resolveStripLine,
} from '@/utils/shellLayout'
import { PANEL_ALIASES, ALIAS_TAB_PROPS, resolvePanel } from '@/utils/panels'
import WindowContent from '@/components/WindowContent.vue'

// Async panels render through AppWindow's error boundary, but a failed chunk
// import (stale hashed asset after a Pages redeploy, offline, flaky net)
// otherwise leaves an empty window with only a console error. The shared
// loader/error pair below shows a retryable state instead — and retries a
// chunk fetch once, since a single retry covers transient fetch failures
// (a stale index.html still needs a hard refresh, which the error text says).
const PanelLoading = defineComponent({
  name: 'PanelLoading',
  setup() {
    return () => h('div', { class: 'panel-async-state' }, 'Loading…')
  },
})

const PanelLoadError = defineComponent({
  name: 'PanelLoadError',
  props: { error: { type: null, required: false, default: null } },
  setup(props) {
    const message = (() => {
      const e = props.error as unknown
      const text = e instanceof Error ? e.message : String(e ?? '')
      if (/dynamically imported module|Failed to fetch|Loading chunk/i.test(text)) {
        return 'Panel chunk failed to load — the site likely redeployed since this tab opened. Hard-refresh (Ctrl+Shift+R) to fetch the current bundle, then reopen the window.'
      }
      return text || 'Panel failed to load.'
    })()
    return () =>
      h('div', { class: 'panel-async-state is-error' }, [
        h('p', { class: 'panel-async-title' }, 'Could not open this window'),
        h('p', { class: 'panel-async-msg' }, message),
        h(
          'button',
          {
            class: 'panel-async-retry',
            type: 'button',
            onClick: () => window.location.reload(),
          },
          'Reload app',
        ),
      ])
  },
})

function asyncPanel(loader: () => Promise<Component>) {
  return defineAsyncComponent({
    loader,
    loadingComponent: PanelLoading,
    errorComponent: PanelLoadError,
    delay: 200,
    timeout: 30000,
    suspensible: false,
    onError(error, retry, fail, attempts) {
      // One automatic retry for chunk fetches; anything else (or a second
      // failure, i.e. a stale hashed file that no longer exists) surfaces
      // PanelLoadError instead of looping.
      if (attempts <= 1) retry()
      else fail()
    },
  })
}

// AGENT-8: the terminal is the heaviest new panel (a few thousand scrollback
// lines), so it is the first panel in the shell to be code-split — it is
// fetched the first time someone opens it, not on boot.
const TerminalPanel = asyncPanel(() => import('@/components/TerminalPanel.vue'))

// The agent panel is as heavy as the terminal (thread + approvals), so it
// is code-split the same way — fetched on first open, not on boot.
const AgentPanel = asyncPanel(() => import('@/components/AgentPanel.vue'))

// The code studio (editor + AI sidecar) is as heavy as the terminal, so it
// is code-split like the terminal — fetched on first open, not on boot.
const CodeStudio = asyncPanel(() => import('@/components/CodeStudio.vue'))

// Every remaining windowed surface is code-split the same way. These panels
// run to 1000–3300 lines each (AccountManagerPanel, FileManager, SettingsPage,
// DiskManagerPage…) and none of them render until someone opens the window —
// shipping them in the boot bundle only cost parse time, heap and a slower
// first paint on every launch. Chunks are local assets, so the first open of
// a panel is a disk read, not a network round trip.
const FileManager = asyncPanel(() => import('@/components/FileManager.vue'))
// Merged omni-search: FACES + COLLECTIONS + SEARCH render one window
// (OmniSearchPanel). The retired OrganizePanel/FaceGroupingPanel files stay
// in the tree for reference but no window opens them — `search`,
// `collections` and `faces` all map to the unified panel below.
const OmniSearchPanel = asyncPanel(() => import('@/components/OmniSearchPanel.vue'))
const MapView = asyncPanel(() => import('@/components/MapView.vue'))
const WebDashboardPanel = asyncPanel(() => import('@/components/WebDashboardPanel.vue'))
const SyncPanel = asyncPanel(() => import('@/components/SyncPanel.vue'))
const SettingsPage = asyncPanel(() => import('@/components/SettingsPage.vue'))
const DiskManagerPage = asyncPanel(() => import('@/components/DiskManagerPage.vue'))
const ShieldPanel = asyncPanel(() => import('@/components/ShieldPanel.vue'))
const FilePermissionsPanel = asyncPanel(() => import('@/components/FilePermissionsPanel.vue'))
const ProcessPanel = asyncPanel(() => import('@/components/ProcessPanel.vue'))
const SecretsPanel = asyncPanel(() => import('@/components/SecretsPanel.vue'))
const DevicesPanel = asyncPanel(() => import('@/components/DevicesPanel.vue'))
const AccountManagerPanel = asyncPanel(() => import('@/components/AccountManagerPanel.vue'))
const TransferGraph = asyncPanel(() => import('@/components/TransferGraph.vue'))
const InstallPanel = asyncPanel(() => import('@/components/InstallPanel.vue'))
const ArchivePanel = asyncPanel(() => import('@/components/ArchivePanel.vue'))

export interface WindowState {
  id: string
  panelType: PanelType
  title: string
  icon: string
  x: number
  y: number
  width: number
  height: number
  minimized: boolean
  zIndex: number
  component: Component | null
  props?: Record<string, unknown>
  /** Niri-style strip column index (order on the infinite strip). */
  column?: number
  /** Niri-style line/row index (vertical workspace line on the strip). */
  line?: number
}

export type ShellLayoutMode = 'floating' | 'tiled' | 'strip' | 'overview'
/**
 * Niri/Noctalia-inspired strip axis:
 * - `horizontal`: one infinite screen scrolling left ↔ right, lines stack
 *   top ↔ bottom (separate vertical workspace lines).
 * - `vertical`: one infinite screen scrolling top ↔ bottom, lines run
 *   left ↔ right.
 */
export type StripDirection = 'horizontal' | 'vertical'

type SizeMap = { [K in PanelType]?: { width: number; height: number } } & {
  permissions?: { width: number; height: number }
}
const defaultSizes: SizeMap = {
  files: { width: 1240, height: 720 },
  search: { width: 640, height: 600 },
  collections: { width: 640, height: 600 },
  faces: { width: 640, height: 600 },
  map: { width: 720, height: 520 },
  code: { width: 1280, height: 740 },
  editor: { width: 1280, height: 740 },
  agent: { width: 1020, height: 680 },
  users: { width: 520, height: 460 },
  sync: { width: 580, height: 440 },
  transfer: { width: 980, height: 660 },
  settings: { width: 560, height: 520 },
  trash: { width: 500, height: 400 },
  activity: { width: 540, height: 400 },
  favorites: { width: 420, height: 360 },
  recent: { width: 420, height: 360 },
  accounts: { width: 780, height: 640 },
  'loose-groups': { width: 440, height: 380 },
  style: { width: 440, height: 360 },
  storage: { width: 580, height: 480 },
  terminal: { width: 760, height: 520 },
  processes: { width: 660, height: 460 },
  disks: { width: 700, height: 540 },
  devices: { width: 680, height: 560 },
  dashboard: { width: 600, height: 460 },
  webdash: { width: 640, height: 500 },
  encryption: { width: 480, height: 420 },
  compression: { width: 480, height: 420 },
  permissions: { width: 440, height: 380 },
  preview: { width: 480, height: 540 },
  secrets: { width: 860, height: 600 },
  install: { width: 880, height: 620 },
  archive: { width: 720, height: 580 },
}

const inlinePanels: PanelType[] = [
  'search', 'trash', 'activity', 'recent',
]

const panelComponentMap: Record<string, Component> = {
  files: FileManager,
  // Omni-search: every merged id renders the unified panel. Aliases resolve
  // in open() first, but the map stays explicit so direct lookups agree.
  search: OmniSearchPanel,
  collections: OmniSearchPanel,
  favorites: OmniSearchPanel,
  'loose-groups': OmniSearchPanel,
  style: OmniSearchPanel,
  faces: OmniSearchPanel,
  map: MapView,
  // Merged studio: `code` (tree-sitter intel) and `editor` (VSCode-like)
  // are one window now — `code` stays as an alias so old shortcuts,
  // palette entries and menu items keep working.
  code: CodeStudio,
  editor: CodeStudio,
  agent: AgentPanel,
  users: AccountManagerPanel,
  dashboard: WebDashboardPanel,
  sync: SyncPanel,
  transfer: TransferGraph,
  settings: SettingsPage,
  storage: DiskManagerPage,
  terminal: TerminalPanel,
  processes: ProcessPanel,
  disks: DiskManagerPage,
  devices: DevicesPanel,
  accounts: AccountManagerPanel,
  encryption: ShieldPanel,
  compression: ShieldPanel,
  permissions: FilePermissionsPanel,
  preview: FileManager,
  webdash: WebDashboardPanel,
  secrets: SecretsPanel,
  install: InstallPanel,
  archive: ArchivePanel,
}

function getComponent(panelType: PanelType): Component | null {
  if (panelComponentMap[panelType]) return markRaw(panelComponentMap[panelType])
  if (inlinePanels.includes(panelType)) return markRaw(WindowContent)
  return null
}

let windowCounter = 0

const windows = ref<WindowState[]>([])
const nextZIndex = ref(10)
const windowFocusHistory = ref<string[]>([])

// ── Shell layout (VueUse-persisted) ────────────────────────────
// `floating`: classic free windows. `tiled`: auto-grid. `strip`: niri-like
// infinite strip (new windows append right, never resize existing ones —
// the viewport scrolls). `overview`: zoomed-out grid to pick a window.
export const shellLayoutMode = useLocalStorage<ShellLayoutMode>(
  'cybermanju_shell_layout',
  'floating',
)
export const shellStripDirection = useLocalStorage<StripDirection>(
  'cybermanju_shell_strip_dir',
  'horizontal',
)
export const shellAutoTile = useLocalStorage<boolean>(
  'cybermanju_shell_autotile',
  false,
)
/** Niri-style viewport: first visible column (horizontal) or line (vertical). */
export const stripOffset = ref(0)
/** Niri-style vertical line selector (which "row" of the strip is live). */
export const stripLine = ref(0)

function cascadePosition(index: number): { x: number; y: number } {
  const offset = 30 + (index % 10) * 28
  return { x: offset, y: offset }
}

function workspaceSize(): { w: number; h: number } {
  if (typeof window === 'undefined') return { w: 1280, h: 800 }
  const el = document.querySelector('.desktop-workspace') as HTMLElement | null
  if (el && el.clientWidth > 0) return { w: el.clientWidth, h: el.clientHeight }
  // Fallback: viewport minus the shell insets (menu bar on top in macOS,
  // 44px panel at the bottom in plasma) so tiled/strip/overview math never
  // slides windows under the chrome when the workspace box is unmeasurable.
  try {
    const cs = getComputedStyle(document.documentElement)
    const top = parseFloat(cs.getPropertyValue('--ui-menubar-h')) || 0
    const bottom = parseFloat(cs.getPropertyValue('--ui-panel-h')) || 0
    // The 400px floor exists so bad inset reads never produce slivers, but
    // it must not exceed the viewport either — a phone landscape window is
    // ~390px tall, and a "workspace" taller than the screen (P2) pushed
    // every tile/snap below the fold. Cap at the real innerHeight.
    return {
      w: window.innerWidth,
      h: Math.min(window.innerHeight, Math.max(400, window.innerHeight - top - bottom)),
    }
  } catch {
    return {
      w: window.innerWidth,
      h: Math.min(window.innerHeight, Math.max(400, window.innerHeight - 120)),
    }
  }
}

export function useWindowManager() {
  const activeWindow = computed(() => {
    if (windowFocusHistory.value.length === 0) return null
    const id = windowFocusHistory.value[windowFocusHistory.value.length - 1]
    return windows.value.find(w => w.id === id) || null
  })

  /** Windows on the live strip line, in column order (niri strip). */
  const stripWindows = computed(() =>
    windows.value
      .filter(w => !w.minimized && (w.line ?? 0) === stripLine.value)
      .sort((a, b) => (a.column ?? 0) - (b.column ?? 0)),
  )

  function nextColumn(): number {
    const cols = windows.value.map(w => w.column ?? 0)
    return cols.length === 0 ? 0 : Math.max(...cols) + 1
  }

  function open(panelType: PanelType, props?: Record<string, unknown>) {
    // Merged-window aliases: `preview` opens Files with the inspector up,
    // `storage` opens Disks on its overview tab, etc.
    const target = resolvePanel(panelType)
    const mergedProps = { ...(ALIAS_TAB_PROPS[panelType] || {}), ...(props || {}) }
    const existing = windows.value.find(
      w => w.panelType === target && !w.minimized
    )
    if (existing) {
      // Re-steer the live window (e.g. switch the organize tab) instead of
      // stacking a second copy of the same surface.
      existing.props = { ...(existing.props || {}), ...mergedProps }
      focus(existing.id)
      return existing.id
    }

    const meta = MODULE_METADATA[target] || { label: target.toUpperCase(), icon: 'solar:grid-2x2-bold' }
    const size = defaultSizes[target] || { width: 520, height: 440 }
    const pos = cascadePosition(windows.value.length)
    // Viewport clamp: desktop defaults (1240×720, 1280×740…) would open
    // mostly off-screen on a phone landscape / small laptop viewport where
    // the desktop metaphor still runs (769–1024px wide). Fit the size to
    // the workspace box and keep the cascade offset + titlebar inside it;
    // min sizes match `updateSize` so later resizes don't fight the open.
    const ws = workspaceSize()
    const width = Math.min(size.width, Math.max(320, ws.w - 16))
    const height = Math.min(size.height, Math.max(240, ws.h - 16))
    const maxX = Math.max(0, ws.w - width)
    const maxY = Math.max(0, ws.h - 40)
    const id = `win-${++windowCounter}`
    const comp = getComponent(target)

    const resolvedProps = { ...mergedProps }
    if (inlinePanels.includes(target)) {
      resolvedProps.panelType = target
    }

    const win: WindowState = {
      id,
      panelType: target,
      title: meta.label,
      icon: meta.icon,
      x: Math.max(0, Math.min(pos.x, maxX)),
      y: Math.max(0, Math.min(pos.y, maxY)),
      width,
      height,
      minimized: false,
      zIndex: nextZIndex.value++,
      component: comp,
      props: resolvedProps,
      column: nextColumn(),
      line: stripLine.value,
    }
    windows.value.push(win)
    windowFocusHistory.value = windowFocusHistory.value.filter(w => w !== id)
    windowFocusHistory.value.push(id)
    // Tiled mode (or floating + autotile) re-tiles on open so the grid
    // stays dense. Strip mode never resizes — the viewport follows instead.
    if (shouldAutoTile()) {
      tileWindows()
    }
    // Niri rule: a new window never resizes existing ones — the strip
    // viewport follows it instead.
    if (shellLayoutMode.value === 'strip') {
      stripOffset.value = Math.max(0, stripWindows.value.length - 1)
    }
    return id
  }

  function close(id: string) {
    windows.value = windows.value.filter(w => w.id !== id)
    windowFocusHistory.value = windowFocusHistory.value.filter(w => w !== id)
    resequenceColumns()
    // Niri dynamic workspaces: a line with no windows is gone — pull the
    // viewport back onto the nearest live line instead of an empty index.
    const live = new Set(windows.value.map(w => w.line ?? 0))
    if (!live.has(stripLine.value)) {
      const sorted = [...live].sort((a, b) => a - b)
      stripLine.value = sorted.length === 0 ? 0 : sorted[sorted.length - 1]
    }
    stripOffset.value = clampStripOffset(stripOffset.value, stripWindows.value.length)
    // Keep the grid dense: a closed tile lets the survivors expand.
    if (shouldAutoTile()) tileWindows()
  }

  function minimize(id: string) {
    const win = windows.value.find(w => w.id === id)
    if (win) {
      win.minimized = true
      windowFocusHistory.value = windowFocusHistory.value.filter(w => w !== id)
      // A minimized tile leaves the grid — survivors expand to fill it.
      if (shouldAutoTile()) tileWindows()
    }
  }

  function restore(id: string) {
    const win = windows.value.find(w => w.id === id)
    if (win) {
      win.minimized = false
      focus(id)
      // A restored tile rejoins the grid instead of overlapping it.
      if (shouldAutoTile()) tileWindows()
    }
  }

  function focus(id: string) {
    const win = windows.value.find(w => w.id === id)
    if (win) {
      win.zIndex = nextZIndex.value++
      windowFocusHistory.value = windowFocusHistory.value.filter(w => w !== id)
      windowFocusHistory.value.push(id)
      if (shellLayoutMode.value === 'strip') {
        const idx = stripWindows.value.findIndex(w => w.id === id)
        if (idx >= 0) stripOffset.value = idx
      }
    }
  }

  function blurAll() {
    windowFocusHistory.value = []
  }

  function toggle(panelType: PanelType, props?: Record<string, unknown>) {
    // Resolve merged aliases first so toggling an old id (faces,
    // collections, …) toggles the unified window instead of a ghost.
    const target = resolvePanel(panelType)
    const existing = windows.value.find(
      w => w.panelType === target
    )
    if (existing) {
      if (existing.minimized) {
        restore(existing.id)
      } else {
        close(existing.id)
      }
    } else {
      open(panelType, props)
    }
  }

  function closeAll() {
    windows.value = []
    windowFocusHistory.value = []
    stripOffset.value = 0
  }

  function minimizeAll() {
    windows.value.forEach(w => { w.minimized = true })
    windowFocusHistory.value = []
  }

  function updatePosition(id: string, x: number, y: number) {
    const win = windows.value.find(w => w.id === id)
    if (win) {
      // The workspace sits BELOW TopMenuBar, so y < 0 slides the titlebar
      // under (behind) the header where it can't be grabbed. Clamp the top
      // (and left) edge here so every mover — mouse drag, resize, nudge,
      // shortcuts — respects it. Only lower bounds: strip mode needs
      // unbounded +x for its infinite columns.
      win.x = Math.max(0, x)
      win.y = Math.max(0, y)
    }
  }

  function updateSize(id: string, width: number, height: number) {
    const win = windows.value.find(w => w.id === id)
    if (win) {
      win.width = Math.max(320, width)
      win.height = Math.max(240, height)
    }
  }

  // ── Focus movement ──────────────────────────────────────────
  function focusByOffset(delta: number) {
    const list = shellLayoutMode.value === 'strip'
      ? stripWindows.value
      : windows.value.filter(w => !w.minimized)
    if (list.length === 0) return
    const activeId = activeWindow.value?.id
    const idx = list.findIndex(w => w.id === activeId)
    const next = list[(idx < 0 ? (delta > 0 ? -1 : 0) : idx + delta + list.length) % list.length]
    if (next) focus(next.id)
  }
  function focusNext() { focusByOffset(1) }
  function focusPrev() { focusByOffset(-1) }

  function closeFocused() {
    const a = activeWindow.value
    if (a) close(a.id)
  }
  function minimizeFocused() {
    const a = activeWindow.value
    if (a) minimize(a.id)
  }

  // ── Auto-organization ───────────────────────────────────────
  /** True when the active layout should keep windows grid-tiled. */
  function shouldAutoTile(): boolean {
    return (
      shellLayoutMode.value === 'tiled' ||
      (shellAutoTile.value && shellLayoutMode.value === 'floating')
    )
  }

  /** Grid-tile every visible window into the workspace (classic autotile).
   *
   * 1 window fills the screen, 2 split halves, 3 use master + stacked
   * pair, 4 fill equal 2×2 quadrants, 5+ fall back to a dense grid.
   * The focused window is re-raised on top so tiling never steals focus.
   */
  function tileWindows() {
    const list = windows.value.filter(w => !w.minimized)
    if (list.length === 0) return
    const activeId = activeWindow.value?.id
    const { w, h } = workspaceSize()
    const rects = computeTileRects(list.length, w, h)
    list.forEach((win, i) => {
      const r = rects[i]
      win.x = r.x
      win.y = r.y
      win.width = r.width
      win.height = r.height
      win.zIndex = 10 + i
    })
    nextZIndex.value = 10 + list.length
    // Keep the focused window visible on top of its fresh tile.
    if (activeId) {
      const active = list.find(win => win.id === activeId)
      if (active) active.zIndex = nextZIndex.value++
    }
  }

  /** Cascade from top-left (classic floating cleanup). */
  function cascadeWindows() {
    const list = windows.value.filter(w => !w.minimized)
    list.forEach((win, i) => {
      const p = cascadePosition(i)
      win.x = p.x
      win.y = p.y
      win.zIndex = 10 + i
    })
    nextZIndex.value = 10 + list.length
  }

  function resequenceColumns() {
    const lines = new Map<number, WindowState[]>()
    for (const w of windows.value) {
      const line = w.line ?? 0
      if (!lines.has(line)) lines.set(line, [])
      lines.get(line)!.push(w)
    }
    for (const group of lines.values()) {
      group.sort((a, b) => (a.column ?? 0) - (b.column ?? 0))
      group.forEach((w, i) => { w.column = i })
    }
  }

  // ── Niri-style strip viewport ───────────────────────────────
  /** Scroll the infinite strip by `delta` columns. Focus follows. */
  function scrollStrip(delta: number) {
    const n = stripWindows.value.length
    if (n === 0) return
    stripOffset.value = clampStripOffset(stripOffset.value + delta, n)
    const target = stripWindows.value[stripOffset.value]
    if (target) focus(target.id)
  }
  function stripLeft() { scrollStrip(-1) }
  function stripRight() { scrollStrip(1) }
  /**
   * Move between vertical lines (workspaces) on the strip. Niri keeps one
   * empty line below the lowest occupied one — moving down onto it lets a
   * new window claim a fresh line; empty lines otherwise vanish (see close).
   */
  function stripLineMove(delta: number) {
    const lines = [...new Set(windows.value.map(w => w.line ?? 0))]
    stripLine.value = resolveStripLine(lines, stripLine.value, delta)
    stripOffset.value = 0
  }
  function moveFocusedOnStrip(delta: number) {
    const a = activeWindow.value
    if (!a) return
    a.column = (a.column ?? 0) + delta
    resequenceColumns()
  }
  function moveFocusedToLine(delta: number) {
    const a = activeWindow.value
    if (!a) return
    a.line = Math.max(0, (a.line ?? 0) + delta)
    resequenceColumns()
  }

  /**
   * Nudge the focused window by a few pixels (floating/tiled arrow-key
   * moves). In strip mode callers prefer moveFocusedOnStrip/ToLine instead
   * so columns keep their size.
   */
  function nudgeFocused(dx: number, dy: number) {
    const a = activeWindow.value
    if (!a) return
    a.x = Math.max(0, a.x + dx)
    a.y = Math.max(0, a.y + dy)
  }

  // ── Layout mode ─────────────────────────────────────────────
  function setLayoutMode(mode: ShellLayoutMode) {
    shellLayoutMode.value = mode
    if (mode === 'tiled') tileWindows()
    if (mode === 'floating') cascadeWindows()
    if (mode === 'strip') {
      resequenceColumns()
      stripOffset.value = Math.max(0, stripWindows.value.length - 1)
    }
  }
  function cycleLayout() {
    const order: ShellLayoutMode[] = ['floating', 'tiled', 'strip', 'overview']
    const next = order[(order.indexOf(shellLayoutMode.value) + 1) % order.length]
    setLayoutMode(next)
  }
  function toggleAutoTile() {
    shellAutoTile.value = !shellAutoTile.value
    if (shellAutoTile.value) tileWindows()
  }
  function toggleStripDirection() {
    shellStripDirection.value = shellStripDirection.value === 'horizontal' ? 'vertical' : 'horizontal'
  }

  const openWindowCount = computed(() =>
    windows.value.filter(w => !w.minimized).length
  )

  const isOpen = (panelType: PanelType) =>
    windows.value.some(w => w.panelType === resolvePanel(panelType))

  return {
    windows,
    activeWindow,
    nextZIndex,
    stripWindows,
    shellLayoutMode,
    shellStripDirection,
    shellAutoTile,
    stripOffset,
    stripLine,
    open,
    close,
    minimize,
    restore,
    focus,
    blurAll,
    toggle,
    closeAll,
    minimizeAll,
    updatePosition,
    updateSize,
    openWindowCount,
    isOpen,
    inlinePanels,
    // organization
    tileWindows,
    cascadeWindows,
    focusNext,
    focusPrev,
    focusByOffset,
    closeFocused,
    minimizeFocused,
    // niri strip
    scrollStrip,
    stripLeft,
    stripRight,
    stripLineMove,
    moveFocusedOnStrip,
    moveFocusedToLine,
    nudgeFocused,
    setLayoutMode,
    cycleLayout,
    toggleAutoTile,
    toggleStripDirection,
  }
}

export type WindowManager = ReturnType<typeof useWindowManager>
