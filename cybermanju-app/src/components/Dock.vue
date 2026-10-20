<!-- CyberManju OS — Dock (src/components/Dock.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div
    class="dock-container"
    :class="[`layout-${dock.layout.value}`, `visibility-${dock.visibility.value}`, { revealed: autohideShown }]"
  >
    <div
      ref="dockEl"
      class="dock"
      @click.stop
      @contextmenu.prevent.stop="handleDockSurfaceContext"
      @mouseenter="onDockEnter"
      @mouseleave="onDockLeave"
    >
      <div
        v-for="app in pinnedApps"
        :key="app.panelType"
        class="dock-item"
        :class="{ active: isAppActive(app.panelType), open: wm.isOpen(app.panelType) }"
        @click="handleDockClick(app.panelType)"
        @contextmenu.prevent.stop="handleDockContext($event, app.panelType)"
      >
        <span class="dock-tip" aria-hidden="true">{{ app.label }}</span>
        <div class="dock-icon">
          <AppIcon class="dock-icon-text" :name="app.icon" :size="20" />
        </div>
        <div class="dock-indicator">
          <div v-if="wm.isOpen(app.panelType)" class="indicator-dot" :class="{ active: isAppActive(app.panelType) }" />
        </div>
      </div>

      <div v-if="openUnpinned.length > 0" class="dock-divider" />

      <div
        v-for="win in openUnpinned"
        :key="win.id"
        class="dock-item"
        :class="{ active: isWindowActive(win.id), 'minimized-item': win.minimized }"
        @click="handleOpenWindowClick(win.id)"
        @contextmenu.prevent.stop="handleOpenWindowContext($event, win.id)"
      >
        <span class="dock-tip" aria-hidden="true">{{ win.title }}</span>
        <div class="dock-icon" :class="{ minimized: win.minimized }">
          <AppIcon class="dock-icon-text" :name="win.icon" :size="20" />
        </div>
        <div class="dock-indicator">
          <div class="indicator-dot" :class="{ active: isWindowActive(win.id), muted: win.minimized }" />
        </div>
      </div>

      <div class="dock-divider" />

      <div
        class="dock-item"
        :class="{ active: isAppActive('trash') }"
        @click="handleDockClick('trash')"
        @contextmenu.prevent.stop="handleDockContext($event, 'trash')"
      >
        <span class="dock-tip" aria-hidden="true">Trash</span>
        <div class="dock-icon">
          <AppIcon class="dock-icon-text" name="solar:trash-bin-trash-bold" :size="20" />
        </div>
        <div class="dock-indicator">
          <div v-if="wm.isOpen('trash')" class="indicator-dot" :class="{ active: isAppActive('trash') }" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { useDock } from '@/composables/useDock'
import type { PanelType } from '@/types'

const store = useAppStore()
const wm = useWindowManager()
const dock = useDock()

const dockEl = ref<HTMLElement | null>(null)

/** Auto-hide + menu-pin state (declared before emitDockMenu uses it). */
const autohideShown = ref(false)
const menuOpen = ref(false)
let hideTimer: number | undefined

const HIDE_DELAY_MS = 450

/** One entry of the shell-rendered dock context menu (DesktopShell.vue). */
interface DockMenuEntry {
  label: string
  action?: () => void
  checked?: boolean
  separator?: boolean
}

function emitDockMenu(x: number, y: number, items: DockMenuEntry[]) {
  menuOpen.value = true
  window.dispatchEvent(new CustomEvent('cybermanju:dock-context', {
    detail: { x, y, items },
  }))
}

/** Shared dock-preference section: appended to every item menu. */
function dockSettingsItems(): DockMenuEntry[] {
  return [
    { label: '', separator: true },
    { label: 'Full width', checked: dock.layout.value === 'full', action: () => dock.setLayout('full') },
    { label: 'Centered', checked: dock.layout.value === 'centered', action: () => dock.setLayout('centered') },
    { label: '', separator: true },
    { label: 'Fixed', checked: dock.visibility.value === 'fixed', action: () => dock.setVisibility('fixed') },
    { label: 'Auto-hide', checked: dock.visibility.value === 'autohide', action: () => dock.setVisibility('autohide') },
  ]
}

interface DockApp {
  panelType: PanelType
  label: string
  icon: string
  category: string
}

const dockApps = computed<DockApp[]>(() => [
  { panelType: 'files', label: 'File Browser', icon: 'solar:folder-bold', category: 'core' },
  { panelType: 'search', label: 'Search', icon: 'solar:magnifier-bold', category: 'core' },
  { panelType: 'collections', label: 'Organize', icon: 'solar:library-bold', category: 'organize' },
  { panelType: 'faces', label: 'People', icon: 'solar:face-scan-circle-bold', category: 'organize' },
  { panelType: 'archive', label: 'Archive', icon: 'solar:notebook-bold', category: 'organize' },
  { panelType: 'map', label: 'Map', icon: 'solar:map-bold', category: 'tools' },
  { panelType: 'editor', label: 'Code Studio', icon: 'solar:file-code-bold', category: 'tools' },
  { panelType: 'agent', label: 'Agent', icon: 'solar:bot-bold', category: 'tools' },
  { panelType: 'sync', label: 'Sync', icon: 'solar:refresh-bold', category: 'tools' },
  { panelType: 'disks', label: 'Storage & Disks', icon: 'solar:ssd-square-bold', category: 'tools' },
  { panelType: 'devices', label: 'Devices', icon: 'solar:plug-circle-bold', category: 'tools' },
  { panelType: 'terminal', label: 'Terminal', icon: 'solar:file-terminal-bold', category: 'system' },
  { panelType: 'processes', label: 'Tasks', icon: 'solar:cpu-bold', category: 'system' },
  { panelType: 'settings', label: 'Settings', icon: 'solar:settings-bold', category: 'system' },
  { panelType: 'trash', label: 'Trash', icon: 'solar:trash-bin-trash-bold', category: 'system' },
  { panelType: 'accounts', label: 'Accounts & Users', icon: 'solar:user-circle-bold', category: 'system' },
])

const pinnedApps = computed(() => dockApps.value.filter((a) => a.panelType !== 'trash'))

/** Open windows whose app is not pinned (or minimized copies): live section. */
const openUnpinned = computed(() => {
  const pinned = new Set(pinnedApps.value.map((a) => a.panelType))
  return wm.windows.value.filter((w) => !pinned.has(w.panelType) || w.minimized)
})

function isAppActive(panelType: PanelType): boolean {
  return wm.windows.value.some(
    w => w.panelType === panelType && !w.minimized
  )
}

function isWindowActive(id: string): boolean {
  return wm.activeWindow.value?.id === id
}

function handleOpenWindowClick(id: string) {
  const win = wm.windows.value.find((w) => w.id === id)
  if (!win) return
  if (win.minimized) wm.restore(id)
  else wm.focus(id)
}

function handleDockClick(panelType: PanelType) {
  const existing = wm.windows.value.find(w => w.panelType === panelType)
  if (existing) {
    if (existing.minimized) {
      wm.restore(existing.id)
    } else {
      wm.focus(existing.id)
    }
  } else {
    wm.open(panelType)
  }
}

function handleDockContext(e: MouseEvent, panelType: PanelType) {
  const existing = wm.windows.value.find(w => w.panelType === panelType)
  const specific: DockMenuEntry[] = existing
    ? [
        { label: 'Focus', action: () => wm.focus(existing.id) },
        { label: 'Minimize', action: () => wm.minimize(existing.id) },
        { label: 'Close', action: () => wm.close(existing.id) },
      ]
    : [{ label: 'Open', action: () => wm.open(panelType) }]
  emitDockMenu(e.clientX, e.clientY, [...specific, ...dockSettingsItems()])
}

/** Right-click an open (unpinned/minimized) window icon: window actions + dock options. */
function handleOpenWindowContext(e: MouseEvent, id: string) {
  const win = wm.windows.value.find((w) => w.id === id)
  if (!win) return
  const specific: DockMenuEntry[] = win.minimized
    ? [
        { label: 'Restore', action: () => wm.restore(id) },
        { label: 'Close', action: () => wm.close(id) },
      ]
    : [
        { label: 'Focus', action: () => wm.focus(id) },
        { label: 'Minimize', action: () => wm.minimize(id) },
        { label: 'Close', action: () => wm.close(id) },
      ]
  emitDockMenu(e.clientX, e.clientY, [...specific, ...dockSettingsItems()])
}

/** Right-click the dock surface itself: layout + visibility sub-options. */
function handleDockSurfaceContext(e: MouseEvent) {
  emitDockMenu(e.clientX, e.clientY, dockSettingsItems().slice(1))
}

/** Auto-hide: the dock slides away until the pointer kisses the bottom edge.
 *  A short hide delay + an expanded grace zone keep it up while hovering:
 *  moving from the screen edge onto the icons must not retreat mid-travel,
 *  and an open context menu pins the dock until dismissed. */
function cancelHide() {
  if (hideTimer !== undefined) {
    window.clearTimeout(hideTimer)
    hideTimer = undefined
  }
}

function scheduleHide() {
  if (hideTimer !== undefined) return
  hideTimer = window.setTimeout(() => {
    hideTimer = undefined
    if (!menuOpen.value) autohideShown.value = false
  }, HIDE_DELAY_MS)
}

/** Pointer arrived on the dock surface itself — pin it open. */
function onDockEnter() {
  if (dock.visibility.value !== 'autohide') return
  cancelHide()
  autohideShown.value = true
}

/** Pointer left the dock — retreat after a grace delay (menu keeps it up). */
function onDockLeave() {
  if (dock.visibility.value !== 'autohide') return
  if (menuOpen.value) return
  scheduleHide()
}

function onMenuClosed() {
  menuOpen.value = false
  // If the pointer already left while the menu was open, retreat now.
  if (dock.visibility.value !== 'autohide') return
  const el = dockEl.value
  if (el && el.matches(':hover')) return
  scheduleHide()
}

function onAutohideMove(e: MouseEvent) {
  if (dock.visibility.value !== 'autohide') {
    autohideShown.value = false
    return
  }
  if (!autohideShown.value) {
    if (e.clientY >= window.innerHeight - 6) {
      cancelHide()
      autohideShown.value = true
    }
    return
  }
  // Open menu pins the dock; a reveal just happened at the edge.
  if (menuOpen.value) return
  const inBottomStrip = e.clientY >= window.innerHeight - 72
  const r = dockEl.value?.getBoundingClientRect()
  const inside =
    !!r &&
    e.clientX >= r.left - 24 &&
    e.clientX <= r.right + 24 &&
    e.clientY >= r.top - 56 &&
    e.clientY <= window.innerHeight + 24
  if (inside || inBottomStrip) cancelHide()
  else scheduleHide()
}

watch(() => dock.visibility.value, (v) => {
  cancelHide()
  if (v !== 'autohide') autohideShown.value = false
})

onMounted(() => {
  window.addEventListener('mousemove', onAutohideMove, { passive: true })
  document.addEventListener('click', onMenuClosed)
  document.addEventListener('contextmenu', onMenuClosed)
})

onUnmounted(() => {
  window.removeEventListener('mousemove', onAutohideMove)
  document.removeEventListener('click', onMenuClosed)
  document.removeEventListener('contextmenu', onMenuClosed)
  cancelHide()
})
</script>

<style scoped>
/* The container is a transparent overlay pinned to the bottom of the
   desktop area — the wallpaper (and windows, in centered/autohide modes)
   run full-screen behind it. Never clip: the shadow must fade smoothly
   around the dock's rounded surface, not hard-cut into a dark band. */
.dock-container {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  display: flex;
  justify-content: center;
  align-items: flex-end;
  height: 72px; /* matches DOCK_HEIGHT — the workspace inset in full+fixed */
  padding: 0 16px 10px;
  background: transparent;
  z-index: 50;
  pointer-events: none;
  min-width: 0;
  max-width: 100vw;
  overflow: visible;
}

.dock {
  display: flex;
  align-items: flex-end;
  gap: 4px;
  padding: 6px 10px 8px;
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: 14px;
  /* Directional shadow: biased downward, hugging the rounded dock shape
     (the hairline ring keeps the silhouette crisp on bright wallpapers). */
  box-shadow:
    0 12px 28px rgba(0, 0, 0, 0.36),
    0 4px 12px rgba(0, 0, 0, 0.20),
    0 0 0 0.5px var(--ui-border);
  pointer-events: auto;
  position: relative;
  max-width: 100%;
  overflow-x: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-width: thin;
  scrollbar-color: transparent transparent;
  transition: transform var(--ui-dur-slow) var(--ui-ease-out);
}

/* Full-width bar: edge-to-edge, square corners, shadow falls only
   downward (a floating look would leak darkness above the bar). */
.dock-container.layout-full {
  padding: 0 0 10px;
}
.dock-container.layout-full .dock {
  flex: 1;
  max-width: none;
  border-radius: 0;
  border-left: none;
  border-right: none;
  border-bottom: none;
  box-shadow:
    0 8px 24px rgba(0, 0, 0, 0.34),
    0 0 0 0.5px var(--ui-border);
}

/* Auto-hide: slides below the viewport until the pointer reaches the
   bottom edge (revealed class flips it back up). */
.dock-container.visibility-autohide .dock {
  transform: translateY(calc(100% + 48px));
}
.dock-container.visibility-autohide.revealed .dock {
  transform: translateY(0);
}

/* Auto-hide stylized bar: invisible until hovered, then accent-tinted. */
.dock::-webkit-scrollbar {
  height: 6px;
}
.dock::-webkit-scrollbar-track {
  background: transparent;
}
.dock::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: var(--ui-radius-full);
}
.dock:hover::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-text) 30%, transparent);
  border: 1px solid transparent;
  background-clip: content-box;
}
.dock:hover {
  scrollbar-color: color-mix(in srgb, var(--ui-text) 30%, transparent) transparent;
}

.dock::before {
  content: none;
}

.dock-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 2px 3px;
  cursor: pointer;
  border-radius: 10px;
  transition: background-color var(--ui-dur-fast) ease-out;
  position: relative;
  min-width: 44px;
  background: transparent;
  border: none;
}

.dock-item:hover {
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
}

/* Magnification: the hovered icon grows while neighbours stay put. */
.dock-icon {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--ui-text) 7%, var(--ui-surface-2));
  border: 1px solid var(--ui-border);
  border-radius: 10px;
  transition: transform var(--ui-dur-fast) ease-out;
}

.dock-item:hover .dock-icon {
  transform: scale(1.18) translateY(-3px);
}

.dock-item:active .dock-icon {
  transform: scale(1.05);
}

.dock-icon-text {
  color: var(--ui-text-2);
  transition: color var(--ui-dur-fast) ease-out;
}

.dock-item:hover .dock-icon-text,
.dock-item.active .dock-icon-text {
  color: var(--ui-text);
}

/* Hover label above the icon. */
.dock-tip {
  position: absolute;
  bottom: calc(100% + 10px);
  left: 50%;
  transform: translateX(-50%) translateY(2px);
  padding: 2px 8px;
  font-family: var(--ui-font);
  font-size: 12px;
  white-space: nowrap;
  color: var(--ui-text);
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  box-shadow: var(--ui-shadow-menu);
  opacity: 0;
  pointer-events: none;
  transition:
    opacity var(--ui-dur-fast) ease-out,
    transform var(--ui-dur-fast) ease-out;
}

.dock-item:hover .dock-tip {
  opacity: 1;
  transform: translateX(-50%) translateY(0);
}

.dock-icon.minimized {
  opacity: 0.55;
}

.dock-indicator {
  height: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.indicator-dot {
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--ui-text-faint);
  transition: background-color var(--ui-dur-fast) ease-out;
}

.indicator-dot.active {
  background: var(--ui-text-2);
}

.indicator-dot.muted {
  background: var(--ui-border-strong);
}

.minimized-item .dock-icon {
  opacity: 0.5;
}

.dock-divider {
  width: 1px;
  align-self: stretch;
  margin: 6px 4px 12px;
  background: var(--ui-separator);
}
</style>
