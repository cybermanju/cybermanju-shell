<!-- CyberManju OS — AppWindow (src/components/AppWindow.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div
    ref="rootRef"
    class="app-window"
    :class="{
      minimized: win.minimized,
      focused: isFocused,
      blurred: !isFocused,
      maximized: isMaximized,
      'app-window--narrow': isNarrow,
      'app-window--bare-title': isBareTitle,
    }"
    :style="windowStyle"
    @pointerdown="onFocus"
  >
    <div
      class="window-titlebar"
      :class="{ 'window-titlebar--narrow': isNarrow, 'window-titlebar--plasma': isPlasma }"
      @pointerdown.prevent="startDrag"
      @dblclick="toggleMaximize"
      @contextmenu.prevent="openWindowMenu($event)"
    >
      <template v-if="isPlasma">
        <div class="titlebar-app" aria-hidden="true">
          <AppIcon :name="win.icon" :size="15" />
        </div>
        <div class="titlebar-label titlebar-label--left" :title="win.title">{{ win.title }}</div>
        <div class="titlebar-spacer" />
        <UiTitlebarButtons
          :focused="isFocused"
          :maximized="isMaximized"
          @close="onClose"
          @minimize="onMinimize"
          @zoom="toggleMaximize"
        />
      </template>
      <template v-else>
        <UiTrafficLights
          :focused="isFocused"
          @close="onClose"
          @minimize="onMinimize"
          @zoom="toggleMaximize"
        />
        <!-- Phone sheets hide the 12px traffic lights (CSS) — without this
             the only exit would be the Home tab's closeAll(). One 44px close
             per sheet, iOS-style. -->
        <button
          v-if="isMobileApp"
          class="titlebar-mobile-close"
          type="button"
          aria-label="Close window"
          @pointerdown.stop
          @click.stop="onClose"
        >
          <AppIcon name="solar:close-bold" :size="18" />
        </button>

        <div class="titlebar-spacer" />

        <div class="titlebar-label" :title="win.title">{{ win.title }}</div>

        <div class="titlebar-spacer" />
      </template>
    </div>

    <div class="window-content" ref="contentRef">
      <div v-if="panelError" class="panel-async-state is-error">
        <p class="panel-async-title">This window crashed</p>
        <p class="panel-async-msg">{{ panelError }}</p>
        <p class="panel-async-hint">Close and reopen the window — if it persists, hard-refresh (Ctrl+Shift+R) to load the current build.</p>
        <button class="panel-async-retry" type="button" @click="onClose">Close window</button>
      </div>
      <component v-else :is="win.component" v-bind="win.props" @close="onClose" />
    </div>

    <!-- Pointer Events (not mouse): one code path for mouse, touch and pen,
         so tablets and phones in landscape (>768px, desktop metaphor) can
         drag/resize too. pointercancel = the browser claimed the gesture. -->
    <div class="resize-handle n" @pointerdown.prevent.stop="startResize('n', $event)"></div>
    <div class="resize-handle s" @pointerdown.prevent.stop="startResize('s', $event)"></div>
    <div class="resize-handle e" @pointerdown.prevent.stop="startResize('e', $event)"></div>
    <div class="resize-handle w" @pointerdown.prevent.stop="startResize('w', $event)"></div>
    <div class="resize-handle ne" @pointerdown.prevent.stop="startResize('ne', $event)"></div>
    <div class="resize-handle nw" @pointerdown.prevent.stop="startResize('nw', $event)"></div>
    <div class="resize-handle se" @pointerdown.prevent.stop="startResize('se', $event)"></div>
    <div class="resize-handle sw" @pointerdown.prevent.stop="startResize('sw', $event)"></div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, computed, onMounted, onUnmounted, onErrorCaptured } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useWindowManager } from '@/composables/useWindowManager'
import { useContextMenu } from '@/composables/useContextMenu'
import type { WindowState } from '@/composables/useWindowManager'
import { createWindowUi, provideWindowUi } from '@/composables/useWindowUi'

const props = defineProps<{
  win: WindowState
  focused: boolean
}>()

const emit = defineEmits<{
  close: [id: string]
  minimize: [id: string]
  focus: [id: string]
  move: [id: string, x: number, y: number]
  resize: [id: string, w: number, h: number]
}>()

const contentRef = ref<HTMLElement | null>(null)
const rootRef = ref<HTMLElement | null>(null)
const isFocused = computed(() => props.focused)
const theme = useTheme()
const isMobileApp = ref(false)
let mobileWindowQuery: MediaQueryList | null = null
function onMobileWindowChange(event: MediaQueryListEvent) { isMobileApp.value = event.matches }
const isPlasma = computed(() => !isMobileApp.value && theme.shellStyle.value === 'plasma')
const wmShell = useWindowManager()
const ctx = useContextMenu()
const isMaximized = ref(false)
/** Plasma "always on top" pin — renders above tiled siblings. */
const pinned = ref(false)
const savedRect = ref({ x: 0, y: 0, width: 0, height: 0 })

ctx.registerContext('window_titlebar', [
  { id: 'w-max', label: 'Maximize', icon: 'solar:maximize-bold', action: (d) => d?.toggleMax?.() },
  { id: 'w-min', label: 'Minimize', icon: 'solar:minus-bold', action: (d) => d?.minimize?.() },
  { id: 'w-pin', label: 'Always on top', icon: 'solar:layers-bold', action: (d) => d?.togglePin?.() },
  { id: 'div1', label: '', divider: true },
  { id: 'w-close', label: 'Close', icon: 'solar:close-bold', action: (d) => d?.close?.() },
])

function openWindowMenu(e: MouseEvent) {
  ctx.open(e, 'window_titlebar', {
    toggleMax: () => toggleMaximize(),
    minimize: () => onMinimize(),
    close: () => onClose(),
    togglePin: () => { pinned.value = !pinned.value },
    pinned: pinned.value,
  })
}

/**
 * Window-aware context: everything rendered inside this window (via
 * `useWindowUi()`) now knows its host window's live size, focus state and
 * effective density, so panels can adapt as the window is resized.
 */
const winUi = provideWindowUi(
  createWindowUi({
    id: props.win.id,
    panelType: props.win.panelType,
    title: props.win.title,
    icon: props.win.icon,
    width: computed(() => props.win.width),
    height: computed(() => props.win.height),
    focused: isFocused,
  })
)

const isNarrow = computed(() => winUi.isNarrow.value)

/**
 * Settings on a phone owns its whole header (large-title root + back bar for
 * the pushed section), so the generic sheet title would be a second, purely
 * decorative row — 44px the standard stack already spends. Every other panel
 * keeps the shared mobile titlebar.
 */
const isBareTitle = computed(() => isMobileApp.value && props.win.panelType === 'settings')

/** Setup/render crashes in the hosted panel (e.g. a TDZ in a panel's
 * setup()) otherwise leave a blank window with only a console error. */
const panelError = ref('')
onErrorCaptured((err) => {
  panelError.value = err instanceof Error ? err.message : String(err ?? 'Unknown error')
  return false
})

const windowStyle = computed(() => {
  // Plasma pin: stay above tiled siblings without touching manager order.
  const z = pinned.value ? 9999 : props.win.zIndex
  if (props.win.minimized) {
    return { display: 'none' }
  }
  if (isMaximized.value) {
    // Maximized = fill the whole desktop workspace: from the bottom of the
    // desktop area up to the start of the top header. The window lives inside
    // `.desktop-workspace` (inset:0 of `.desktop-area`, directly below
    // TopMenuBar), so 0/0 + 100% is exactly that region — no magic offsets.
    return {
      left: '0px',
      top: '0px',
      width: '100%',
      height: '100%',
      zIndex: z,
    }
  }
  const style: Record<string, string | number> = {
    left: `${props.win.x}px`,
    top: `${props.win.y}px`,
    width: `${props.win.width}px`,
    height: `${props.win.height}px`,
    zIndex: z,
  }
  return style
})

let dragging = false
let dragPointerId: number | null = null
let dragStartX = 0
let dragStartY = 0
let dragOrigX = 0
let dragOrigY = 0

/**
 * Drag/resize coalescing: pointer devices fire pointermove far faster than
 * the display refreshes (a 1000 Hz mouse is 1000 reactive writes + style/
 * layout passes per second). Geometry is staged here and flushed at most
 * once per animation frame, so a drag costs one Vue update per frame no
 * matter how fast the pointer reports — the final position is always
 * flushed synchronously on pointerup so nothing can be dropped.
 */
let geomRaf = 0
let pendingMove: { x: number; y: number } | null = null
let pendingSize: { w: number; h: number } | null = null

function flushGeometry() {
  if (geomRaf) {
    cancelAnimationFrame(geomRaf)
    geomRaf = 0
  }
  const move = pendingMove
  const size = pendingSize
  pendingMove = null
  pendingSize = null
  if (move) emit('move', props.win.id, move.x, move.y)
  if (size) emit('resize', props.win.id, size.w, size.h)
}

function scheduleGeometry() {
  if (geomRaf) return
  geomRaf = requestAnimationFrame(() => {
    geomRaf = 0
    flushGeometry()
  })
}

function stageMove(x: number, y: number) {
  pendingMove = { x, y }
  scheduleGeometry()
}

function stageResize(w: number, h: number) {
  pendingSize = { w, h }
  scheduleGeometry()
}

function startDrag(e: PointerEvent) {
  if (isMaximized.value || isMobileApp.value) return
  dragging = true
  dragPointerId = e.pointerId
  dragStartX = e.clientX
  dragStartY = e.clientY
  dragOrigX = props.win.x
  dragOrigY = props.win.y
  document.addEventListener('pointermove', onDrag)
  document.addEventListener('pointerup', stopDrag)
  // A system gesture (back swipe, notification shade) claims the pointer:
  // land the last staged position, skip snap logic — the drop wasn't ours.
  document.addEventListener('pointercancel', onDragCancel)
}

function onDrag(e: PointerEvent) {
  if (!dragging || e.pointerId !== dragPointerId) return
  const dx = e.clientX - dragStartX
  const dy = e.clientY - dragStartY
  const ws = getWorkspaceSize()
  const newX = Math.max(0, dragOrigX + dx)
  // Keep the titlebar reachable: never let the window sink so far that its
  // only drag handle leaves the workspace (rotating a tablet used to strand
  // floating windows off-screen with no recovery).
  const newY = Math.min(Math.max(0, dragOrigY + dy), Math.max(0, ws.h - 40))
  stageMove(newX, newY)
}

function onDragCancel() {
  stopDrag(undefined, true)
}

function stopDrag(e?: PointerEvent, cancelled = false) {
  dragging = false
  dragPointerId = null
  document.removeEventListener('pointermove', onDrag)
  document.removeEventListener('pointerup', stopDrag)
  document.removeEventListener('pointercancel', onDragCancel)
  // Land the last staged position before any snap logic reads geometry.
  flushGeometry()
  // Plasma snap: drag to the top edge maximizes, to a side edge tiles.
  if (isPlasma.value && e && !cancelled && !isMaximized.value) {
    try {
      if (e.clientY <= 2) {
        toggleMaximize()
      } else if (e.clientX <= 2 || e.clientX >= window.innerWidth - 2) {
        wmShell.setLayoutMode('tiled')
      }
    } catch { /* pointer left the viewport — keep the drop position */ }
  }
}

let resizing = false
let resizePointerId: number | null = null
let resizeDir = ''
let resizeStartX = 0
let resizeStartY = 0
let resizeOrigX = 0
let resizeOrigY = 0
let resizeOrigW = 0
let resizeOrigH = 0

function startResize(dir: string, e?: PointerEvent) {
  if (isMaximized.value || isMobileApp.value) return
  resizing = true
  resizePointerId = e ? e.pointerId : null
  resizeDir = dir
  resizeStartX = e ? e.clientX : 0
  resizeStartY = e ? e.clientY : 0
  resizeOrigX = props.win.x
  resizeOrigY = props.win.y
  resizeOrigW = props.win.width
  resizeOrigH = props.win.height
  document.addEventListener('pointermove', onResize)
  document.addEventListener('pointerup', stopResize)
  document.addEventListener('pointercancel', stopResize)
}

function onResize(e: PointerEvent) {
  if (!resizing || (resizePointerId != null && e.pointerId !== resizePointerId)) return
  const dx = e.clientX - resizeStartX
  const dy = e.clientY - resizeStartY
  let newX = resizeOrigX
  let newY = resizeOrigY
  let newW = resizeOrigW
  let newH = resizeOrigH

  if (resizeDir.includes('e')) newW = Math.max(320, resizeOrigW + dx)
  if (resizeDir.includes('w')) {
    newW = Math.max(320, resizeOrigW - dx)
    newX = resizeOrigX + (resizeOrigW - newW)
    // West edge hit the workspace border (below TopMenuBar): pin it and
    // shrink the growth instead of pushing the window off-screen.
    if (newX < 0) {
      newX = 0
      newW = resizeOrigX + resizeOrigW
    }
  }
  if (resizeDir.includes('s')) newH = Math.max(240, resizeOrigH + dy)
  if (resizeDir.includes('n')) {
    newH = Math.max(240, resizeOrigH - dy)
    newY = resizeOrigY + (resizeOrigH - newH)
    // North edge hit the workspace top (directly below TopMenuBar): pin it
    // instead of sliding the titlebar up under (behind) the header.
    if (newY < 0) {
      newY = 0
      newH = resizeOrigY + resizeOrigH
    }
  }

  if (newX !== resizeOrigX || newY !== resizeOrigY) {
    stageMove(newX, newY)
  }
  stageResize(newW, newH)
}

function stopResize() {
  resizing = false
  resizeDir = ''
  resizePointerId = null
  document.removeEventListener('pointermove', onResize)
  document.removeEventListener('pointerup', stopResize)
  document.removeEventListener('pointercancel', stopResize)
  flushGeometry()
}

function onClose() {
  emit('close', props.win.id)
}

function onMinimize() {
  emit('minimize', props.win.id)
}

function onFocus() {
  emit('focus', props.win.id)
}

function getWorkspaceSize(): { w: number; h: number } {
  // The window is a child of `.desktop-workspace`, whose inset:0 box is
  // exactly the usable desktop area (below TopMenuBar, above Dock/StatusBar).
  const host: HTMLElement | null | undefined =
    rootRef.value?.closest('.desktop-workspace') ??
    rootRef.value?.parentElement
  if (host && host.clientWidth > 0 && host.clientHeight > 0) {
    return { w: host.clientWidth, h: host.clientHeight }
  }
  const area = document.querySelector('.desktop-workspace') ?? document.querySelector('.desktop-area')
  if (area && (area as HTMLElement).clientWidth > 0) {
    return {
      w: (area as HTMLElement).clientWidth,
      h: (area as HTMLElement).clientHeight,
    }
  }
  return { w: window.innerWidth, h: window.innerHeight }
}

function applyMaximizedRect() {
  // Keep the stored rect in sync so window-aware children (useWindowUi)
  // see the real maximized geometry.
  const { w, h } = getWorkspaceSize()
  emit('move', props.win.id, 0, 0)
  emit('resize', props.win.id, w, h)
}

function toggleMaximize() {
  if (isMaximized.value) {
    isMaximized.value = false
    emit('move', props.win.id, savedRect.value.x, savedRect.value.y)
    emit('resize', props.win.id, savedRect.value.width, savedRect.value.height)
  } else {
    savedRect.value = {
      x: props.win.x,
      y: props.win.y,
      width: props.win.width,
      height: props.win.height,
    }
    isMaximized.value = true
    applyMaximizedRect()
  }
}

function handleViewportResize() {
  if (isMaximized.value) applyMaximizedRect()
}

function handleGlobalKeydown(e: KeyboardEvent) {
  if (e.defaultPrevented) return
  // A teleported modal (ConfirmDialog, UiModal, …) owns Escape while it is
  // open — never let it fall through to "unmaximize the window behind it".
  if (document.querySelector('.confirm-overlay, .ui-modal-layer')) return
  if (!isFocused.value) return
  if (e.key === 'Escape' && isMaximized.value) {
    toggleMaximize()
  }
}

/** `maximize_toggle` shortcut: App dispatches here with the active id. */
function handleMaximizeRequest(e: Event) {
  const id = (e as CustomEvent<{ id?: string } | undefined>)?.detail?.id
  if (id && id !== props.win.id) return
  if (!isFocused.value) return
  toggleMaximize()
}

onMounted(() => {
  mobileWindowQuery = window.matchMedia?.('(max-width: 768px)') ?? null
  if (mobileWindowQuery) {
    isMobileApp.value = mobileWindowQuery.matches
    mobileWindowQuery.addEventListener?.('change', onMobileWindowChange)
  }
  document.addEventListener('keydown', handleGlobalKeydown)
  window.addEventListener('resize', handleViewportResize)
  window.addEventListener('cybermanju:maximize-focused', handleMaximizeRequest)
  // Hand the live element to the window context so descendants get real geometry.
  winUi.observe(rootRef.value)
})

onUnmounted(() => {
  mobileWindowQuery?.removeEventListener?.('change', onMobileWindowChange)
  document.removeEventListener('keydown', handleGlobalKeydown)
  window.removeEventListener('resize', handleViewportResize)
  window.removeEventListener('cybermanju:maximize-focused', handleMaximizeRequest)
  stopDrag()
  stopResize()
})
</script>

<style scoped>
.app-window {
  position: absolute;
  display: flex;
  flex-direction: column;
  background: var(--ui-window);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xl);
  overflow: hidden;
  /* Blur lives on the TITLEBAR, not the frame: the body is covered by the
     opaque .window-content surface, so a window-sized backdrop-filter (up to
     1240×720 per window, re-blitted whenever the desktop or a sibling moves)
     bought nothing visually while dominating WebKitGTK compositing cost.
     The titlebar strip is the only translucent part, so it does the blur. */
  box-shadow: var(--ui-shadow-window);
  transition:
    box-shadow var(--ui-dur) ease-out,
    border-color var(--ui-dur) ease-out,
    background-color var(--ui-dur) ease-out;
  min-width: 320px;
  min-height: 240px;
  will-change: left, top, width, height;
}

.app-window.focused {
  border-color: var(--ui-border);
  background: var(--ui-window);
  box-shadow: var(--ui-shadow-window);
}

.app-window.blurred {
  background: var(--ui-window-idle);
  border-color: var(--ui-border);
  box-shadow: var(--ui-shadow-window-idle);
}

.app-window.minimized {
  pointer-events: none;
}

.app-window.maximized {
  border-radius: 0;
  border-left: none;
  border-right: none;
}

/* ── titlebar ─────────────────────────────────────────────────────────── */

.window-titlebar {
  display: flex;
  align-items: center;
  height: var(--ui-titlebar-h);
  min-height: 28px;
  padding: 0 12px;
  gap: 8px;
  /* Transparent on purpose: the frame's own `--ui-window` tint sits directly
     under it, so colour is unchanged — only the blur pass is added here. */
  background: transparent;
  backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  border-bottom: 1px solid var(--ui-separator);
  cursor: default;
  user-select: none;
  flex-shrink: 0;
  position: relative;
  /* Drag by touch: without this the browser starts a scroll and the pointer
     is cancelled before the window ever moves. */
  touch-action: none;
}

/* Phone sheets: the per-sheet close (traffic lights are CSS-hidden ≤768). */
.titlebar-mobile-close {
  display: none;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 44px;
  height: 44px;
  margin-left: -8px;
  padding: 0;
  border: 0;
  border-radius: 12px;
  background: transparent;
  color: var(--ui-text-2);
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}
.titlebar-mobile-close:active {
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
}

/* Centered 13px/600 title, no icon. Dimmed when the window is inactive. */
.titlebar-label {
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: center;
  max-width: 46ch;
  transition: color var(--ui-dur) ease-out;
}

.app-window.focused .titlebar-label {
  color: var(--ui-text);
}

.titlebar-spacer {
  flex: 1;
}

/* Plasma titlebar: 30px, app icon + left title, glyph buttons right,
   rounded top corners, 1px outline, dimmed title when inactive. */
.window-titlebar--plasma {
  padding: 0 4px 0 10px;
  gap: 8px;
}

.titlebar-app {
  display: inline-flex;
  align-items: center;
  color: var(--ui-text-2);
  flex-shrink: 0;
}

.titlebar-label--left {
  text-align: left;
  font-weight: 400;
  max-width: 60ch;
}

.window-titlebar--narrow {
  gap: 6px;
  padding: 0 8px;
}

.window-titlebar--narrow .titlebar-label {
  font-size: var(--ui-fs-xs);
  max-width: 18ch;
}

/* ── content ──────────────────────────────────────────────────────────── */

.window-content {
  flex: 1;
  min-height: 0;
  overflow: auto;
  position: relative;
  background: var(--ui-surface);
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
}

/* Mobile sheets: use a clean centered title and no draggable desktop chrome. */
@media (max-width: 768px) {
  .window-titlebar {
    height: 44px;
    min-height: 44px;
    padding: 0 12px;
    cursor: default;
  }
  .window-titlebar .ui-lights {
    display: none;
  }
  .titlebar-mobile-close {
    display: flex;
  }
  .window-titlebar .titlebar-spacer:first-of-type {
    display: none;
  }
  /* Settings renders its own nav bar (root large title / back stack). */
  .app-window--bare-title .window-titlebar {
    display: none;
  }
}

.window-content > :deep(*) {
  /* Fill short windows but grow tall content so the outer scroller can
     actually scroll: `height: 100%` pinned every panel to the viewport
     height, clipping anything taller with no way to reach it. */
  min-height: 100%;
  /* Global overflow safety net: no window child may push content past the
     window frame horizontally — shrink, wrap, and scroll internally. */
  min-width: 0;
  max-width: 100%;
  box-sizing: border-box;
  overflow-wrap: break-word;
}

/* Wide media / code / tables must scroll inside the window, never widen it. */
.window-content :deep(img, video, canvas, table) {
  max-width: 100%;
}
.window-content :deep(pre) {
  max-width: 100%;
  overflow: auto;
}

/* ── resize handles ───────────────────────────────────────────────────── */

.resize-handle {
  position: absolute;
  z-index: 10;
  /* Touch resize: keep the browser from stealing the gesture as a scroll. */
  touch-action: none;
}

.resize-handle.n { top: -3px; left: 4px; right: 4px; height: 6px; cursor: n-resize; }
.resize-handle.s { bottom: -3px; left: 4px; right: 4px; height: 6px; cursor: s-resize; }
.resize-handle.e { right: -3px; top: 4px; bottom: 4px; width: 6px; cursor: e-resize; }
.resize-handle.w { left: -3px; top: 4px; bottom: 4px; width: 6px; cursor: w-resize; }
.resize-handle.ne { top: -4px; right: -4px; width: 10px; height: 10px; cursor: ne-resize; }
.resize-handle.nw { top: -4px; left: -4px; width: 10px; height: 10px; cursor: nw-resize; }
.resize-handle.se { bottom: -4px; right: -4px; width: 10px; height: 10px; cursor: se-resize; }
.resize-handle.sw { bottom: -4px; left: -4px; width: 10px; height: 10px; cursor: sw-resize; }
</style>

<style>
/* Shared async-panel states: used by AppWindow's crash fallback AND the
 * PanelLoading/PanelLoadError components in useWindowManager (unscoped so
 * both trees match). */
.panel-async-state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
  padding: 20px;
  font-family: var(--ui-font);
  font-size: 13px;
  color: var(--ui-text-2);
}
.panel-async-state.is-error { color: var(--ui-text); }
.panel-async-title { margin: 0; font-weight: 700; }
.panel-async-msg {
  margin: 0;
  font-family: var(--ui-font-mono);
  font-size: 12px;
  color: var(--ui-text-2);
  overflow-wrap: anywhere;
}
.panel-async-hint { margin: 0; font-size: 12px; color: var(--ui-text-3); }
.panel-async-retry {
  margin-top: 4px;
  padding: 6px 14px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border-strong);
  background: var(--ui-surface-2);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 12px;
  font-weight: 700;
  cursor: pointer;
}
.panel-async-retry:hover { border-color: var(--ui-accent); }
</style>
