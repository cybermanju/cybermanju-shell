<!-- CyberManju OS — DesktopShell (src/components/DesktopShell.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue, ./…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="desktop-shell" :class="{ 'desktop-shell--glow': theme.settings.glow, 'desktop-shell--plasma': isPlasma, 'desktop-shell--mobile': isMobileViewport }">
    <!-- Mobile always keeps the single unified header (logo + status +
      theme + one clock), on home and over sheets — no duplicate rows. -->
    <TopMenuBar v-if="isMobileViewport || chrome.topBar" />

    <div
      class="desktop-area"
      :style="{ '--dock-inset': `${dockInset}px` }"
      @click="handleWorkspaceClick"
    >
      <div class="desktop-wallpaper" :class="`wp-${theme.settings.wallpaper || 'slopes-dark'}`">
        <img
          v-if="customWallpaper.source.value"
          class="desktop-wallpaper__custom-image"
          :src="customWallpaper.source.value"
          alt=""
          aria-hidden="true"
          referrerpolicy="no-referrer"
          @error="customWallpaper.imageFailed"
        />
        <slot name="wallpaper" />
        <div class="desktop-aurora" aria-hidden="true" />
      </div>

      <!-- Mobile home: iOS-style launcher grid under the fullscreen sheets. -->
      <MobileLauncher class="mobile-only-launcher" />

      <!-- Niri-style infinite strip: columns append right (or down in
           vertical mode) and the viewport scrolls — existing windows keep
           their size. Native scroll + Alt+arrows both drive stripOffset. -->
      <div
        v-if="isStrip"
        ref="stripScrollRef"
        class="desktop-strip ui-scroll"
        :class="{ vertical: isVerticalStrip }"
        @wheel="onStripWheel"
      >
        <!-- Scroll extent: guarantees the viewport can reach every column
             even while windows animate in/out (absolute tiles alone are a
             fragile scroll driver across browsers). -->
        <div class="strip-sizer" :style="stripContentStyle" aria-hidden="true" />
        <div class="strip-position" aria-hidden="true">
          COL {{ wm.stripOffset.value + 1 }}/{{ wm.stripWindows.value.length || 1 }} · LINE {{ wm.stripLine.value }}
        </div>
        <TransitionGroup name="win">
          <AppWindow
            v-for="win in visibleWindows"
            :key="win.id"
            :win="win"
            :focused="win.id === focusedWindowId"
            @close="wm.close"
            @minimize="wm.minimize"
            @focus="wm.focus"
            @move="wm.updatePosition"
            @resize="wm.updateSize"
          />
        </TransitionGroup>
      </div>

      <!-- Overview: zoomed-out grid across every open window -->
      <div v-else-if="isOverview" class="desktop-overview" role="dialog" aria-label="Window overview">
        <button
          v-for="win in overviewWindows"
          :key="win.id"
          class="overview-card"
          :class="{ minimized: win.minimized }"
          type="button"
          @click="pickFromOverview(win.id)"
        >
          <span class="overview-card-icon"><AppIcon :name="win.icon" :size="18" /></span>
          <span class="overview-card-title">{{ win.title }}</span>
          <span class="overview-card-meta">COL {{ (win.column ?? 0) + 1 }} · LINE {{ win.line ?? 0 }}{{ win.minimized ? ' · MINIMIZED' : '' }}</span>
        </button>
        <div v-if="overviewWindows.length === 0" class="overview-empty">No windows open.</div>
      </div>

      <div v-else class="desktop-workspace">
        <div class="desktop-icons">
          <button
            v-for="shortcut in shortcuts"
            :key="shortcut.panel"
            class="desktop-shortcut"
            type="button"
            :title="shortcut.label"
            @click="openShortcut(shortcut.panel)"
          >
            <span class="shortcut-icon" :class="{ selected: selectShortcut === shortcut.panel }">
              <AppIcon :name="shortcut.icon" :size="20" />
            </span>
            <span class="shortcut-label">{{ shortcut.label }}</span>
          </button>
        </div>

        <TransitionGroup name="win">
          <AppWindow
            v-for="win in visibleWindows"
            :key="win.id"
            :win="win"
            :focused="win.id === focusedWindowId"
            @close="wm.close"
            @minimize="wm.minimize"
            @focus="wm.focus"
            @move="wm.updatePosition"
            @resize="wm.updateSize"
          />
        </TransitionGroup>
      </div>

      <!-- Floating dock: absolute overlay at the bottom of the desktop
           area, so the wallpaper (and windows in centered/autohide modes)
           run full-screen behind it. -->
      <Dock v-if="!isMobileViewport && chrome.bottom && !isPlasma" />
    </div>

    <!-- Right-bottom shell dock: shortcuts + tiling, above windows. -->
    <ShellShortcutDock v-if="!isMobileViewport && chrome.shortcutDock" />

    <!-- Plasma bottom panel (launcher, pager, tasks, tray, clock). -->
    <PlasmaPanel v-if="!isMobileViewport && isPlasma" />
    <!-- Kickoff launcher popup, anchored to the panel launcher button. -->
    <KickoffMenu v-if="!isMobileViewport && isPlasma && kickoff.kickoffOpen.value" />

    <div
      v-if="dockMenu.visible"
      class="dock-context-overlay"
      :style="{ left: dockMenu.x + 'px', top: dockMenu.y + 'px' }"
      @click="dockMenu.visible = false"
      @contextmenu.prevent="dockMenu.visible = false"
    >
      <div ref="dockMenuEl" class="dock-context-menu ui-shine" @click.stop>
        <template v-for="(item, i) in dockMenu.items" :key="i">
          <div v-if="item.separator" class="dock-context-sep" role="separator" />
          <button
            v-else
            class="dock-context-item"
            :class="{ checked: item.checked }"
            type="button"
            @click="item.action?.(); dockMenu.visible = false"
          >
            <span class="dock-context-check" aria-hidden="true">{{ item.checked ? '✓' : '' }}</span>
            <span>{{ item.label }}</span>
          </button>
        </template>
      </div>
    </div>

    <StatusBar v-if="!isMobileViewport && !isPlasma" />

  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import MobileLauncher from '@/components/MobileLauncher.vue'
import PlasmaPanel from '@/components/PlasmaPanel.vue'
import KickoffMenu from '@/components/KickoffMenu.vue'
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useWindowManager } from '@/composables/useWindowManager'
import { useTheme } from '@/composables/useTheme'
import { useCustomWallpaper } from '@/composables/useCustomWallpaper'
import { useKickoff } from '@/composables/useKickoff'
import { useDock, DOCK_HEIGHT } from '@/composables/useDock'
import { SHELL_CHROME } from '@/ui/shells'
import { computeStripRects, stripColumnWidth } from '@/utils/shellLayout'
import TopMenuBar from './TopMenuBar.vue'
import Dock from './Dock.vue'
import StatusBar from './StatusBar.vue'
import AppWindow from './AppWindow.vue'
import ShellShortcutDock from './ShellShortcutDock.vue'
import type { PanelType } from '@/types'

const wm = useWindowManager()
const theme = useTheme()
const customWallpaper = useCustomWallpaper()
const kickoff = useKickoff()
const dock = useDock()

/**
 * A full-width fixed dock is an opaque edge-to-edge bar — windows must not
 * slide under it, so the workspace strips inset by the dock height. The
 * centered pill and auto-hide modes deliberately overlap windows (the
 * dock floats above them, macOS-style).
 */
const dockInset = computed(() =>
  dock.layout.value === 'full' && dock.visibility.value === 'fixed' ? DOCK_HEIGHT : 0,
)

const selectShortcut = ref<PanelType | null>(null)
const mobileShellQuery = typeof window !== 'undefined' ? window.matchMedia?.('(max-width: 768px)') ?? null : null
const isMobileViewport = ref(mobileShellQuery?.matches ?? false)
function onMobileShellChange(event: MediaQueryListEvent) {
  isMobileViewport.value = event.matches
}
/** Chrome comes from the shell registry — TopMenuBar/Dock in macOS, PlasmaPanel in plasma. */
const chrome = computed(() => SHELL_CHROME[theme.shellStyle.value])
const isPlasma = computed(() => theme.shellStyle.value === 'plasma')

onMounted(() => { void customWallpaper.load() })

const stripScrollRef = ref<HTMLElement | null>(null)

const shortcuts: { panel: PanelType; label: string; icon: string }[] = [
  { panel: 'files', label: 'Files', icon: 'solar:folder-bold' },
  { panel: 'collections', label: 'Collections', icon: 'solar:library-bold' },
  { panel: 'map', label: 'Map', icon: 'solar:map-bold' },
  { panel: 'editor', label: 'Code Studio', icon: 'solar:code-square-bold' },
  { panel: 'devices', label: 'Devices', icon: 'solar:plug-circle-bold' },
  { panel: 'settings', label: 'Settings', icon: 'solar:settings-bold' },
  { panel: 'terminal', label: 'Terminal', icon: 'solar:file-terminal-bold' },
]

function openShortcut(panel: PanelType) {
  selectShortcut.value = panel
  wm.open(panel)
}

const visibleWindows = computed(() =>
  wm.windows.value.filter(w => !w.minimized)
)

/**
 * Overview ("three fingers up") zooms out to EVERY screen — open windows
 * and minimized ones alike — so nothing can hide from the grid.
 */
const overviewWindows = computed(() => wm.windows.value)

const isStrip = computed(() => wm.shellLayoutMode.value === 'strip')
const isOverview = computed(() => wm.shellLayoutMode.value === 'overview')
const isVerticalStrip = computed(() => wm.shellStripDirection.value === 'vertical')

/**
 * Explicit scroll extent for the infinite strip. Absolutely-positioned
 * tiles are a fragile scroll driver on their own (transition wrappers,
 * containing-block quirks), so a dedicated sizer guarantees the viewport
 * can always reach the last column / line.
 */
const stripContentStyle = computed(() => {
  const list = wm.stripWindows.value
  if (list.length === 0) return {}
  const maxRight = Math.max(...list.map(w => w.x + w.width))
  const maxBottom = Math.max(...list.map(w => w.y + w.height))
  if (isVerticalStrip.value) {
    return { width: '100%', height: `${Math.max(maxBottom + 24, 400)}px` }
  }
  return { width: `${Math.max(maxRight + 24, 800)}px`, height: '100%' }
})

function layoutStripColumns() {
  const list = [...wm.windows.value]
    .filter(w => !w.minimized && (w.line ?? 0) === wm.stripLine.value)
    .sort((a, b) => (a.column ?? 0) - (b.column ?? 0))
  if (list.length === 0) return
  const vertical = wm.shellStripDirection.value === 'vertical'
  const host = stripScrollRef.value
  const vw = host?.clientWidth || window.innerWidth
  const vh = host?.clientHeight || Math.max(400, window.innerHeight - 160)
  const rects = computeStripRects(list.length, vw, vh, 12, vertical, stripColumnWidth(vw))
  list.forEach((w, i) => {
    const r = rects[i]
    w.x = r.x
    w.y = r.y
    w.width = r.width
    w.height = r.height
  })
}

function scrollStripIntoView() {
  const host = stripScrollRef.value
  if (!host || !isStrip.value) return
  const vertical = isVerticalStrip.value
  const list = wm.stripWindows.value
  const target = list[wm.stripOffset.value]
  if (!target) return
  if (vertical) {
    host.scrollTo({ top: Math.max(0, target.y - 12), behavior: 'smooth' })
  } else {
    host.scrollTo({ left: Math.max(0, target.x - 12), behavior: 'smooth' })
  }
}

watch(
  () => [wm.shellLayoutMode.value, wm.shellStripDirection.value, wm.stripLine.value, wm.windows.value.length],
  async () => {
    if (wm.shellLayoutMode.value === 'strip') {
      await nextTick()
      layoutStripColumns()
      scrollStripIntoView()
    }
  },
  { immediate: true },
)

watch(
  () => wm.stripOffset.value,
  () => { scrollStripIntoView() },
)

/** Niri-style wheel: vertical wheel drives a horizontal strip (and vice versa
 *  only via Shift / horizontal deltas). Wheel events that start inside a
 *  nested scroller which can still move in that direction are left alone so
 *  terminal / file / code panes keep their own scroll. */
function onStripWheel(e: WheelEvent) {
  if (!isStrip.value) return
  const host = stripScrollRef.value
  if (!host) return
  const vertical = isVerticalStrip.value
  const target = e.target as HTMLElement | null
  const inner = target?.closest?.('.window-content, .term-scroll, .cs-term, .fm-grid, .fm-lbody, .window-content-panel') as HTMLElement | null
  if (inner && inner !== host) {
    const canY = inner.scrollHeight - inner.scrollTop - inner.clientHeight > 1 && e.deltaY !== 0
    const canX = inner.scrollWidth - inner.scrollLeft - inner.clientWidth > 1 && (e.deltaX !== 0 || e.shiftKey)
    if (!vertical && canY) return
    if (vertical && (canY || canX)) return
  }
  if (!vertical) {
    // Horizontal strip: plain vertical wheel would do nothing against
    // `overflow-y: hidden` — translate it (niri-style) instead.
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY
    if (delta === 0) return
    e.preventDefault()
    host.scrollLeft += delta
  } else {
    const delta = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX
    if (Math.abs(delta) < 1) return
    if (Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      e.preventDefault()
      host.scrollTop += e.deltaX
    }
    // Otherwise let the native vertical scroll run, then sync the index.
  }
  // Sync the column index after the viewport settles.
  requestAnimationFrame(() => {
    const list = wm.stripWindows.value
    if (list.length === 0) return
    const pos = vertical ? host.scrollTop : host.scrollLeft
    let best = 0
    let bestDist = Infinity
    list.forEach((w, i) => {
      const p = vertical ? w.y : w.x
      const d = Math.abs(p - pos - 12)
      if (d < bestDist) { bestDist = d; best = i }
    })
    wm.stripOffset.value = best
  })
}

function pickFromOverview(id: string) {
  const win = wm.windows.value.find(w => w.id === id)
  if (!win) return
  // Minimized windows restore straight out of the zoomed-out grid.
  if (win.minimized) wm.restore(id)
  else wm.focus(id)
  wm.setLayoutMode(wm.shellAutoTile.value ? 'tiled' : 'floating')
}

/** Viewport resizes re-run the active auto-layout so tiles stay dense. */
function handleViewportResize() {
  if (wm.shellLayoutMode.value === 'strip') layoutStripColumns()
  else if (wm.shellLayoutMode.value === 'tiled') wm.tileWindows()
}

/** Dock layout/visibility changes resize the workspace — re-flow tiles. */
watch(dockInset, () => { handleViewportResize() })

const focusedWindowId = computed(() => wm.activeWindow.value?.id ?? null)

/** One entry of the dock context menu (separator + checkmark aware). */
interface DockMenuItem {
  label: string
  action?: () => void
  checked?: boolean
  separator?: boolean
}

const dockMenu = ref({
  visible: false,
  x: 0,
  y: 0,
  items: [] as DockMenuItem[],
})

const dockMenuEl = ref<HTMLElement | null>(null)

function handleDockContext(e: CustomEvent) {
  const rawX = typeof e.detail?.x === 'number' ? e.detail.x : 0
  const rawY = typeof e.detail?.y === 'number' ? e.detail.y : 0
  dockMenu.value = {
    visible: true,
    x: rawX,
    y: rawY,
    items: e.detail.items,
  }
  // The dock sits at the bottom edge, so a menu opening downward would
  // fall off-screen. Measure after paint and clamp into the viewport
  // (opens upward / leftward when too close to an edge).
  void nextTick(() => {
    const el = dockMenuEl.value
    if (!el) return
    const r = el.getBoundingClientRect()
    const margin = 8
    let x = rawX
    let y = rawY
    if (x + r.width + margin > window.innerWidth) x = Math.max(margin, window.innerWidth - r.width - margin)
    if (y + r.height + margin > window.innerHeight) y = Math.max(margin, window.innerHeight - r.height - margin)
    if (x < margin) x = margin
    if (y < margin) y = margin
    dockMenu.value.x = x
    dockMenu.value.y = y
  })
}

function handleClickOutside() {
  dockMenu.value.visible = false
}

/** Dismiss the dock menu on any context menu elsewhere (named handler so
 *  the removal in onUnmounted actually matches the registration). */
function handleDocumentContextMenu() {
  dockMenu.value.visible = false
}

function handleWorkspaceClick(e: MouseEvent) {
  const target = e.target as HTMLElement | null
  if (!target) return
  if (target.closest('.app-window')) {
    selectShortcut.value = null
    return
  }
  if (target.closest('.desktop-shortcut')) return
  selectShortcut.value = null
  wm.blurAll()
}

onMounted(() => {
  mobileShellQuery?.addEventListener?.('change', onMobileShellChange)
  window.addEventListener('cybermanju:dock-context', handleDockContext as EventListener)
  document.addEventListener('click', handleClickOutside)
  document.addEventListener('contextmenu', handleDocumentContextMenu)
  window.addEventListener('resize', handleViewportResize)
})

onUnmounted(() => {
  mobileShellQuery?.removeEventListener?.('change', onMobileShellChange)
  window.removeEventListener('cybermanju:dock-context', handleDockContext as EventListener)
  document.removeEventListener('click', handleClickOutside)
  document.removeEventListener('contextmenu', handleDocumentContextMenu)
  window.removeEventListener('resize', handleViewportResize)
})
</script>

<style scoped>
.desktop-shell {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  background: var(--ui-bg);
  position: relative;
}

@media (max-width: 768px) {
  .desktop-shell { width: 100%; height: 100%; min-width: 0; min-height: 0; box-sizing: border-box; }
  .desktop-shell--mobile :deep(.top-menu-bar) { display: flex !important; }
}

.desktop-area {
  flex: 1;
  position: relative;
  overflow: hidden;
}

.desktop-wallpaper {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 0;
}

.desktop-wallpaper__custom-image {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  object-position: center;
  pointer-events: none;
}

/* Static wallpapers — original gradient artwork (no third-party assets).
   The canvas engine (MatrixRain/PrayerFlags) is opt-in via Settings →
   Appearance → Wallpaper effects. The aurora overlay is retired. */
.desktop-wallpaper {
  background:
    linear-gradient(180deg, #1a1a1e 0%, #000000 60%, #000000 100%);
}

/* Breeze-like dark slopes. */
.desktop-wallpaper.wp-slopes-dark {
  background:
    linear-gradient(115deg, transparent 0%, transparent 46%, rgba(61, 174, 233, 0.16) 47%, transparent 60%),
    linear-gradient(100deg, transparent 0%, transparent 60%, rgba(61, 174, 233, 0.10) 72%, transparent 85%),
    linear-gradient(180deg, #2b3136 0%, #1b1e20 55%, #101315 100%);
}

/* Breeze-like light slopes. */
.desktop-wallpaper.wp-slopes-light {
  background:
    linear-gradient(115deg, transparent 0%, transparent 46%, rgba(24, 141, 194, 0.14) 47%, transparent 60%),
    linear-gradient(100deg, transparent 0%, transparent 60%, rgba(24, 141, 194, 0.10) 72%, transparent 85%),
    linear-gradient(180deg, #f4f5f6 0%, #dcdfe3 60%, #c9ced4 100%);
}

/* Warm night dunes. */
.desktop-wallpaper.wp-dunes {
  background:
    radial-gradient(120% 90% at 80% 110%, rgba(246, 116, 0, 0.22), transparent 55%),
    radial-gradient(100% 70% at 10% 100%, rgba(61, 174, 233, 0.12), transparent 60%),
    linear-gradient(180deg, #232629 0%, #1b1e20 55%, #0e1113 100%);
}

.desktop-aurora {
  display: none;
}



.desktop-workspace {
  position: absolute;
  inset: 0;
  /* Full-width fixed dock: the opaque bar owns the strip below; every
     other dock mode keeps the workspace full-height and the dock floats. */
  bottom: var(--dock-inset, 0px);
  z-index: 1;
}

.desktop-icons {
  position: absolute;
  top: 12px;
  left: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  z-index: 2;
}

.desktop-shortcut {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 7px 8px;
  cursor: pointer;
  border-radius: var(--ui-radius-md);
  border: 1px solid transparent;
  background: transparent;
  width: 74px;
  text-align: center;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out);
  animation: ui-rise var(--ui-dur-slow) var(--ui-ease-out) both;
}

.desktop-shortcut:hover {
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  border-color: var(--ui-border);
}

.desktop-shortcut:focus-visible {
  outline: none;
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}

.shortcut-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  border-radius: var(--ui-radius-md);
  color: var(--ui-text-2);
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  backdrop-filter: blur(var(--ui-blur));
  -webkit-backdrop-filter: blur(var(--ui-blur));
  transition:
    color var(--ui-dur) ease-out,
    border-color var(--ui-dur) ease-out,
    box-shadow var(--ui-dur) ease-out;
}

.shortcut-icon.selected {
  color: var(--ui-text);
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}

.desktop-shortcut:hover .shortcut-icon {
  color: var(--ui-text);
  border-color: var(--ui-border-strong);
}

.shortcut-label {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 500;
  color: var(--ui-text-2);
  white-space: nowrap;
  transition: color var(--ui-dur) var(--ui-ease-out);
}

.desktop-shortcut:hover .shortcut-label,
.shortcut-icon.selected + .shortcut-label {
  color: var(--ui-text);
}

.dock-context-overlay {
  position: fixed;
  z-index: 10000;
}

.dock-context-menu {
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  padding: 4px;
  min-width: 170px;
  max-width: calc(100vw - 16px);
  max-height: calc(100vh - 16px);
  overflow-y: auto;
  box-shadow: var(--ui-shadow-menu);
  animation: ui-fade-in var(--ui-dur) ease-out both;
}

.dock-context-item {
  display: block;
  width: 100%;
  text-align: left;
  min-height: 22px;
  padding: 3px 8px;
  font-family: var(--ui-font);
  font-size: 13px;
  color: var(--ui-text);
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.dock-context-item:hover {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
}

.dock-context-item.checked {
  font-weight: 600;
}

.dock-context-check {
  display: inline-block;
  width: 14px;
  color: var(--ui-accent);
  font-weight: 700;
}

.dock-context-sep {
  height: 1px;
  margin: 3px 6px;
  background: var(--ui-separator);
}

/* window open / close transitions */
.win-enter-active,
.win-leave-active {
  transition:
    opacity var(--ui-dur) var(--ui-ease-out),
    transform var(--ui-dur-slow) var(--ui-ease-out),
    filter var(--ui-dur) var(--ui-ease-out);
}

.win-enter-from {
  opacity: 0;
  transform: translateY(16px) scale(0.96);
  filter: blur(6px);
}

.win-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.97);
  filter: blur(4px);
}

.win-leave-active {
  pointer-events: none;
}

/* ── niri-style infinite strip ─────────────────────────────────── */
.desktop-strip {
  position: absolute;
  inset: 0;
  bottom: var(--dock-inset, 0px);
  z-index: 1;
  overflow-x: auto;
  overflow-y: hidden;
  /* Wheel deltas drive `scrollLeft` directly per tick — `smooth` here would
     queue an animation per tick and feel laggy. Programmatic jumps
     (stripOffset) still pass `behavior: 'smooth'` explicitly. */
  scroll-behavior: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  -webkit-overflow-scrolling: touch;
}
.desktop-strip.vertical {
  overflow-x: hidden;
  overflow-y: auto;
}
.desktop-strip::-webkit-scrollbar {
  height: 10px;
  width: 10px;
}
.desktop-strip::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-text) 24%, transparent);
  border-radius: var(--ui-radius-full);
  border: 3px solid transparent;
  background-clip: content-box;
}
.desktop-strip::-webkit-scrollbar-thumb:hover {
  background: color-mix(in srgb, var(--ui-text) 38%, transparent);
  background-clip: content-box;
  border: 3px solid transparent;
}
/* Explicit scroll extent behind the absolute tiles (see template). */
.strip-sizer {
  position: absolute;
  left: 0;
  top: 0;
  pointer-events: none;
  visibility: hidden;
}
.desktop-strip :deep(.app-window) {
  /* columns keep their size; the viewport moves, never the tiles */
  transition: box-shadow var(--ui-dur-slow) var(--ui-ease-out), border-color var(--ui-dur-slow) var(--ui-ease-out);
}
.strip-position {
  position: sticky;
  left: 12px;
  top: 10px;
  display: inline-block;
  z-index: 5;
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 500;
  color: var(--ui-text-3);
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-full);
  padding: 3px 10px;
  margin: 10px 0 0 12px;
  backdrop-filter: blur(var(--ui-blur));
  -webkit-backdrop-filter: blur(var(--ui-blur));
  pointer-events: none;
}
.desktop-strip.vertical .strip-position {
  position: sticky;
  top: 10px;
}

/* ── overview grid ─────────────────────────────────────────────── */
.desktop-overview {
  position: absolute;
  inset: 0;
  bottom: var(--dock-inset, 0px);
  z-index: 1;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 12px;
  padding: 24px;
  overflow-y: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  background: color-mix(in srgb, var(--ui-bg-deep) 35%, transparent);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}
.desktop-overview::-webkit-scrollbar {
  width: 10px;
}
.desktop-overview::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-text) 24%, transparent);
  border-radius: var(--ui-radius-full);
  border: 3px solid transparent;
  background-clip: content-box;
}
.desktop-overview::-webkit-scrollbar-thumb:hover {
  background: color-mix(in srgb, var(--ui-text) 38%, transparent);
  background-clip: content-box;
  border: 3px solid transparent;
}
.overview-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  padding: 16px;
  border-radius: var(--ui-radius-xl);
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid color-mix(in srgb, var(--ui-text) 12%, transparent);
  box-shadow: var(--ui-shadow-2), inset 0 1px 0 var(--ui-glass-highlight);
  color: var(--ui-text);
  cursor: pointer;
  text-align: left;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), box-shadow var(--ui-dur-fast) var(--ui-ease-out), transform var(--ui-dur-fast) var(--ui-ease-out);
  animation: ui-pop var(--ui-dur) var(--ui-ease-out) both;
}
.overview-card:hover {
  border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  box-shadow: var(--ui-shadow-3);
  transform: translateY(-2px);
}
.overview-card.minimized {
  opacity: 0.72;
  border-style: dashed;
}
.overview-card-icon { color: var(--ui-text-2); }
.overview-card-title { font-size: 13px; font-weight: 600; }
.overview-card-meta { font-family: var(--ui-font); font-size: 11px; color: var(--ui-text-3); }
.overview-empty { grid-column: 1 / -1; text-align: center; color: color-mix(in srgb, var(--ui-text) 50%, transparent); font-size: 12px; padding: 40px; }

/* Launcher home lives under the sheets on phones only. `!important` wins
   the specificity tie with the launcher's own `display: flex` regardless of
   stylesheet order. */
.mobile-only-launcher { display: none !important; }
@media (max-width: 768px) {
  .mobile-only-launcher { display: flex !important; }
  /* The empty workspace stacks above the launcher. Let taps pass through it,
     while keeping the fullscreen app sheets themselves interactive. */
  .desktop-workspace { pointer-events: none; }
  .desktop-workspace :deep(.app-window) { pointer-events: auto; }
  .desktop-icons { display: none; }
}
</style>
