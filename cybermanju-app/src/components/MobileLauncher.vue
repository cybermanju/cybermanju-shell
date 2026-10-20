<!-- CyberManju OS — MobileLauncher (src/components/MobileLauncher.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div
    ref="rootRef"
    class="mla"
    :class="{ 'with-app-nav': hasOpenWindows }"
    role="main"
    aria-label="CyberManju home"
    @click.stop
    @touchstart.passive="onTouchStart"
    @touchmove.passive="onTouchMove"
    @touchend.passive="onTouchEnd"
  >
    <!-- The single mobile header (TopMenuBar) owns connectivity, vault
      count and the clock — no duplicate status row here. -->
    <div class="mla-topbtns">
      <button
        class="mla-msgbtn"
        type="button"
        aria-label="Notifications"
        title="Notifications"
        @click="launcher.setMessagesOpen(true, 'system')"
      >
        <AppIcon name="solar:bell-bold" :size="19" />
        <span v-if="sysUnread > 0" class="mla-badge static">{{ sysUnread > 99 ? '99+' : sysUnread }}</span>
      </button>
      <button
        v-if="msgHub.supported"
        class="mla-msgbtn"
        type="button"
        aria-label="Messages"
        title="Messages"
        @click="launcher.setMessagesOpen(true, 'messages')"
      >
        <AppIcon name="solar:chat-round-dots-bold" :size="19" />
        <span v-if="msgHub.unread.value > 0" class="mla-badge static">{{ msgHub.unread.value > 99 ? '99+' : msgHub.unread.value }}</span>
      </button>
    </div>
    <header class="mla-hello">
      <p class="mla-eyebrow">{{ dateStr }}</p>
      <h1 class="mla-title">{{ greeting }}</h1>
      <p class="mla-subtitle">Your private space, ready when you are.</p>
    </header>

    <section class="mla-widgets" aria-label="At a glance">
      <button class="mla-widget vault-widget" type="button" @click="open('storage')">
        <span class="mla-widget-icon vault-widget-icon"><AppIcon name="solar:shield-check-bold" :size="19" /></span>
        <span class="mla-widget-copy">
          <span class="mla-widget-label">Private vaults</span>
          <strong>{{ store.disks.length ? `${store.disks.length} ready` : 'Set up your first' }}</strong>
        </span>
        <AppIcon class="mla-widget-arrow" name="solar:arrow-right-bold" :size="14" />
      </button>
      <button class="mla-widget sync-widget" type="button" @click="open('sync')">
        <span class="mla-widget-icon sync-widget-icon"><AppIcon name="solar:refresh-bold" :size="19" /></span>
        <span class="mla-widget-copy">
          <span class="mla-widget-label">Connected sources</span>
          <strong>{{ store.syncConfigs.length }} provider{{ store.syncConfigs.length === 1 ? '' : 's' }}</strong>
        </span>
        <AppIcon class="mla-widget-arrow" name="solar:arrow-right-bold" :size="14" />
      </button>
    </section>

    <label class="mla-search">
      <AppIcon name="solar:magnifier-bold" :size="17" />
      <input
        ref="searchRef"
        v-model="query"
        class="mla-search-input"
        type="search"
        placeholder="Search"
        aria-label="Search apps"
        autocomplete="off"
        @keyup.esc="query = ''"
      />
      <kbd v-if="!query" class="mla-search-hint">⌄</kbd>
      <button v-else class="mla-search-clear" type="button" aria-label="Clear search" @click="query = ''">
        <AppIcon name="solar:close-circle-bold" :size="17" />
      </button>
    </label>

    <!-- iOS springboard pager (cf. iOS4Android PageView + IconGrid):
      8 icons per page (2 rows × 4), horizontal snap scroll with dots. -->
    <div
      v-if="filtered.length"
      ref="pagerRef"
      class="mla-pages"
      role="group"
      aria-label="Apps"
      @scroll.passive="onPagerScroll"
    >
      <div
        v-for="(page, pageIndex) in pages"
        :key="pageIndex"
        class="mla-grid"
        :aria-label="pages.length > 1 ? `Apps, page ${pageIndex + 1} of ${pages.length}` : 'Apps'"
      >
        <button
          v-for="app in page"
          :key="app.panel"
          class="mla-app"
          type="button"
          :aria-label="`Open ${homeName(app)}${app.hint ? ` — ${app.hint}` : ''}`"
          :title="app.hint ?? homeName(app)"
          @click="open(app.panel)"
          @pointerdown="onHomePressStart(app, $event)"
          @pointerup="onHomePressEnd"
          @pointerleave="onHomePressEnd"
          @pointercancel="onHomePressEnd"
          @contextmenu.prevent="onHomeContextMenu(app)"
        >
          <span class="mla-tile" :class="{ 'has-photo': homeTile(app).kind === 'image' }" :style="{ '--app-tint': app.tint }">
            <img
              v-if="homeTile(app).kind === 'image'"
              class="mla-photo"
              :src="homeTile(app).src"
              :alt="`${homeName(app)} icon`"
              draggable="false"
            />
            <AppIcon v-else :name="homeTile(app).src" :size="25" />
            <span v-if="app.badge" class="mla-badge">{{ app.badge }}</span>
            <span v-if="app.dot" class="mla-live" aria-hidden="true" />
          </span>
          <span class="mla-name">{{ homeName(app) }}</span>
        </button>
      </div>
    </div>
    <p v-if="filtered.length === 0" class="mla-empty">
      No apps match “{{ query }}”.
      <button class="mla-link" type="button" @click="query = ''">Clear</button>
    </p>

    <!-- iOS page dots (cf. iOS4Android DotsIndicator): 7.5px dots, white vs dim. -->
    <div v-if="pages.length > 1" class="mla-dots" aria-hidden="true">
      <button
        v-for="(_, dotIndex) in pages"
        :key="dotIndex"
        class="mla-dot-btn"
        :class="{ on: activePage === dotIndex }"
        type="button"
        tabindex="-1"
        :aria-label="`Go to page ${dotIndex + 1}`"
        @click="goToPage(dotIndex)"
      >
        <span class="mla-page-dot" />
      </button>
    </div>

    <button class="mla-allapps" type="button" @click="openDrawer" aria-label="Open all apps">
      <span class="mla-allapps-pill" aria-hidden="true" />
      All apps
    </button>
    <button v-if="hasHomeOverrides" class="mla-reset" type="button" @click="resetAllHome">
      Reset home
    </button>

    <div class="mla-dock" role="navigation" aria-label="Favorite apps">
      <button
        v-for="app in dock"
        :key="app.panel"
        class="mla-dock-btn"
        type="button"
        :aria-label="`Open ${homeName(app)}`"
        :title="homeName(app)"
        @click="open(app.panel)"
      >
        <span class="mla-dock-icon" :class="{ 'has-photo': homeTile(app).kind === 'image' }" :style="{ '--app-tint': app.tint }">
          <img
            v-if="homeTile(app).kind === 'image'"
            class="mla-photo"
            :src="homeTile(app).src"
            :alt="`${homeName(app)} icon`"
            draggable="false"
          />
          <AppIcon v-else :name="homeTile(app).src" :size="23" />
        </span>
        <span v-if="isPanelOpen(app.panel)" class="mla-dock-indicator" aria-hidden="true" />
      </button>
    </div>
    <!-- iPhone-X home indicator (cf. iOS4Android bottomArea 34px rounding + pill). -->
    <div class="mla-home" aria-hidden="true" />

    <!-- Gaveta de Apps: swipe up from the bottom (or tap All apps). -->
    <AppDrawer />

    <!-- Messages hub: top-right icon (header tray on open apps, floating
      button above on home). -->
    <MessagesHub />

    <!-- Home tile customization (click-and-hold a tile). -->
    <div v-if="customHome" class="mla-custom-veil" @click.self="customHome = null">
      <div class="mla-custom" role="dialog" aria-label="Customize home tile">
        <strong class="mla-custom-title">{{ homeName(customHome) }}</strong>
        <span class="mla-custom-sub">{{ customHome.hint ?? customHome.label }}</span>
        <label class="mla-custom-row">
          <span class="mla-custom-label">Name</span>
          <input
            v-model="homeAliasDraft"
            class="mla-custom-input"
            type="text"
            maxlength="48"
            placeholder="Custom name"
            aria-label="Custom tile name"
          />
        </label>
        <div class="mla-custom-row">
          <span class="mla-custom-label">Icon</span>
          <div class="mla-custom-icon-row">
            <span class="mla-tile sm has-photo" v-if="customHomeTile.kind === 'image'">
              <img class="mla-photo" :src="customHomeTile.src" alt="" draggable="false" />
            </span>
            <button class="mla-mini-btn" type="button" @click="pickHomeGallery">
              <AppIcon name="solar:folder-bold" :size="15" /> Gallery
            </button>
            <input
              ref="homeFileRef"
              type="file"
              accept="image/*"
              class="mla-file"
              aria-label="Pick a custom icon from the gallery"
              @change="onHomeGalleryFile"
            />
          </div>
        </div>
        <div class="mla-custom-actions">
          <button class="mla-mini-btn" type="button" @click="moveHomeCustom(-1)">← Move</button>
          <button class="mla-mini-btn" type="button" @click="moveHomeCustom(1)">Move →</button>
          <button class="mla-mini-btn" type="button" @click="hideHomeCustom">Hide</button>
        </div>
        <div class="mla-custom-actions">
          <button class="mla-mini-btn primary" type="button" @click="saveHomeCustom">Save</button>
          <button class="mla-mini-btn" type="button" @click="resetHomeCustom">Reset</button>
          <button class="mla-mini-btn" type="button" @click="customHome = null">Close</button>
        </div>
        <p v-if="homeMsg" class="mla-custom-msg" role="status">{{ homeMsg }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import AppDrawer from '@/components/AppDrawer.vue'
import MessagesHub from '@/components/MessagesHub.vue'
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useLauncherStore } from '@/stores/launcher'
import { useAndroidApps } from '@/composables/useAndroidApps'
import { useMessages } from '@/composables/useMessages'
import { useNotifications } from '@/composables/useNotifications'
import { useWindowManager } from '@/composables/useWindowManager'
import {
  fileToLauncherIcon,
  launcherKeyForPanel,
} from '@/utils/launcher'
import type { PanelType } from '@/types'

/** Icons per springboard page — mirrors iOS4Android IconGrid (2 rows × 4). */
const MLA_PAGE_SIZE = 8

const store = useAppStore()
const wm = useWindowManager()
const launcher = useLauncherStore()
const androidApps = useAndroidApps()
const msgHub = useMessages()
// One composable instance for setup scope — instantiating it inside a
// computed would re-run it on every recompute.
const sys = useNotifications()
const sysUnread = computed(() => sys.unreadCount.value)
const query = ref('')
const dateStr = ref('')
const greeting = ref('')
const rootRef = ref<HTMLElement | null>(null)
const searchRef = ref<HTMLInputElement | null>(null)
const pagerRef = ref<HTMLElement | null>(null)
const activePage = ref(0)
let pagerRaf = 0
const hasOpenWindows = computed(() => wm.windows.value.some(window => !window.minimized))
let timer: ReturnType<typeof setInterval> | null = null

// Greeting + date for the hello header. The clock/connectivity/vault
// status live in the single mobile TopMenuBar header (no second clock here).
function tickClock() {
  try {
    const now = new Date()
    dateStr.value = now.toLocaleDateString([], { weekday: 'long', month: 'long', day: 'numeric' })
    const hour = now.getHours()
    greeting.value = hour < 5 ? 'Good night' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening'
  } catch {
    dateStr.value = ''
  }
}

/** A downward gesture from the top edge focuses the app search, like system search. */
let touchY: number | null = null
let touchX: number | null = null
/** Swipe-up (→ Gaveta drawer) anchors: kept for the whole gesture, because
 *  `touchY` is deliberately cleared mid-move by the pull-to-search path —
 *  reusing it for the end-check made the drawer gesture dead code. */
let drawerStartY: number | null = null
let drawerStartX: number | null = null
let swipeUpArmed = false
function onTouchStart(event: TouchEvent) {
  touchY = event.touches[0]?.clientY ?? null
  touchX = event.touches[0]?.clientX ?? null
  drawerStartY = touchY
  drawerStartX = touchX
  // Swipe-up-to-drawer only arms near the bottom of the screen so normal
  // vertical scrolling of the home never triggers the Gaveta by accident.
  try {
    swipeUpArmed =
      !launcher.drawerOpen &&
      touchY != null &&
      touchY > window.innerHeight * 0.55 &&
      (event.touches.length ?? 0) === 1
  } catch {
    swipeUpArmed = false
  }
}
function onTouchEnd(event: TouchEvent) {
  if (swipeUpArmed && drawerStartY != null && drawerStartX != null && !launcher.drawerOpen) {
    const endY = event.changedTouches[0]?.clientY ?? drawerStartY
    const endX = event.changedTouches[0]?.clientX ?? drawerStartX
    const vertical = drawerStartY - endY
    const horizontal = Math.abs(endX - drawerStartX)
    const tag = (event.target as HTMLElement | null)?.tagName ?? ''
    if (vertical >= 96 && horizontal < 60 && tag !== 'INPUT' && tag !== 'TEXTAREA') {
      launcher.setDrawer(true)
      try {
        ;(navigator as Navigator & { vibrate?: (pattern: number) => boolean }).vibrate?.(10)
      } catch { /* optional haptic feedback */ }
    }
  }
  swipeUpArmed = false
  touchY = null
  touchX = null
  drawerStartY = null
  drawerStartX = null
}
function onTouchMove(event: TouchEvent) {
  const root = rootRef.value
  // Once the home list itself scrolls, the gesture belongs to content —
  // never the drawer.
  if ((root?.scrollTop ?? 0) > 0) swipeUpArmed = false
  if (touchY == null) return
  const y = event.touches[0]?.clientY ?? touchY
  if (y - touchY > 72 && (root?.scrollTop ?? 0) <= 0 && document.activeElement !== searchRef.value) {
    touchY = null
    searchRef.value?.focus()
    return
  }
  if (Math.abs(y - touchY) > 10) touchY = null
}

function handleMsgVisible() {
  if (document.visibilityState !== 'visible') return
  // `refresh()` early-returns on a cached 'ready' — force so returning to the
  // app actually updates the badge.
  if (msgHub.supported) void msgHub.refresh(true).catch(() => {})
  // Resume hook (existing visibility listener): the system uninstall sheet
  // backgrounds the app, so re-list while the drawer is on screen instead of
  // leaving uninstalled rows stale.
  if (launcher.drawerOpen && androidApps.supported) {
    void androidApps.refresh(true).catch(() => {})
  }
}

onMounted(() => {
  tickClock()
  timer = setInterval(tickClock, 20_000)
  void store.fetchDisks().catch(() => {})
  void store.fetchTrashItems().catch(() => {})
  void store.fetchSyncConfigs().catch(() => {})
  // Silent badge feed for the top-right messages icon (stored mirror only).
  if (msgHub.supported) {
    void msgHub.refresh().catch(() => {})
    document.addEventListener('visibilitychange', handleMsgVisible)
  }
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
  if (pagerRaf) cancelAnimationFrame(pagerRaf)
  document.removeEventListener('visibilitychange', handleMsgVisible)
})

interface LauncherApp {
  panel: PanelType
  label: string
  icon: string
  tint: string
  badge?: string
  dot?: boolean
  hint?: string
}

function count(value: number): string {
  if (!value) return ''
  return value > 99 ? '99+' : String(value)
}

const apps = computed<LauncherApp[]>(() => [
  { panel: 'files', label: 'Files', icon: 'solar:folder-bold', tint: '#3984e8', badge: count(store.files.length), hint: 'Browse and manage files' },
  { panel: 'search', label: 'Search', icon: 'solar:magnifier-bold', tint: '#727be8', hint: 'Full-text search' },
  { panel: 'agent', label: 'Agent', icon: 'solar:bot-bold', tint: '#8b5ad8', dot: store.activeAgentJob?.status === 'running', hint: 'AI assistant' },
  { panel: 'storage', label: 'Vaults', icon: 'solar:diskette-bold', tint: '#2198c3', badge: count(store.disks.length), hint: 'Private vaults and disks' },
  { panel: 'accounts', label: 'Accounts', icon: 'solar:user-circle-bold', tint: '#38a87b', hint: 'Providers and users' },
  { panel: 'terminal', label: 'Terminal', icon: 'solar:file-terminal-bold', tint: '#4c596b', hint: 'cybsh shell' },
  { panel: 'editor', label: 'Code', icon: 'solar:file-code-bold', tint: '#df7b38', hint: 'Code studio' },
  { panel: 'sync', label: 'Sync', icon: 'solar:refresh-bold', tint: '#159ba0', hint: 'Provider sync jobs' },
  { panel: 'map', label: 'Map', icon: 'solar:map-bold', tint: '#4b9a5a', hint: 'GPS-tagged files' },
  { panel: 'collections', label: 'Library', icon: 'solar:library-bold', tint: '#d45d96', hint: 'Collections and favorites' },
  { panel: 'settings', label: 'Settings', icon: 'solar:settings-bold', tint: '#65758c', hint: 'Theme and transport' },
  { panel: 'trash', label: 'Trash', icon: 'solar:trash-bin-trash-bold', tint: '#d74a61', badge: count(store.trashItems.length), hint: 'Deleted files' },
])

const dockApps: PanelType[] = ['files', 'agent', 'terminal', 'settings']

/** Home springboard merged with launcher overrides (`os:<panel>` keys):
 * hidden tiles drop out, ordered slots come first, the rest label-sort. */
const orderedApps = computed<LauncherApp[]>(() => {
  const rows = apps.value.map((app) => {
    const key = launcherKeyForPanel(app.panel)
    const ov = launcher.overrides[key]
    return { app, key, ov, order: typeof ov?.order === 'number' ? ov.order : -1 }
  }).filter((r) => !r.ov?.hidden)
  rows.sort((a, b) => {
    const ao = a.order >= 0 ? a.order : Number.MAX_SAFE_INTEGER
    const bo = b.order >= 0 ? b.order : Number.MAX_SAFE_INTEGER
    if (ao !== bo) return ao - bo
    return a.app.label.toLowerCase().localeCompare(b.app.label.toLowerCase())
  })
  return rows.map((r) => r.app)
})

/** Display name honoring the rename override. */
function homeName(app: LauncherApp): string {
  const alias = (launcher.overrides[launcherKeyForPanel(app.panel)]?.alias ?? '').trim()
  return alias || app.label
}

/** Display icon honoring the gallery-icon override. */
function homeTile(app: LauncherApp): { kind: 'icon' | 'image'; src: string } {
  const custom = (launcher.overrides[launcherKeyForPanel(app.panel)]?.customIcon ?? '').trim()
  if (custom) return { kind: 'image', src: custom }
  return { kind: 'icon', src: app.icon }
}

const dock = computed(() => orderedApps.value.filter(app => dockApps.includes(app.panel)))
const filtered = computed(() => {
  const term = query.value.trim().toLowerCase()
  const list = orderedApps.value
  return term
    ? list.filter(app => homeName(app).toLowerCase().includes(term) || app.label.toLowerCase().includes(term))
    : list
})
/** Chunk the springboard into swipeable pages of MLA_PAGE_SIZE. */
const pages = computed<LauncherApp[][]>(() => {
  const out: LauncherApp[][] = []
  for (let i = 0; i < filtered.value.length; i += MLA_PAGE_SIZE) {
    out.push(filtered.value.slice(i, i + MLA_PAGE_SIZE))
  }
  return out.length ? out : []
})

/** Track the visible page (rAF-throttled; mirrors DotsIndicator controller.page). */
function onPagerScroll() {
  if (pagerRaf) return
  pagerRaf = requestAnimationFrame(() => {
    pagerRaf = 0
    const el = pagerRef.value
    if (!el || el.clientWidth === 0) return
    activePage.value = Math.min(
      pages.value.length - 1,
      Math.max(0, Math.round(el.scrollLeft / el.clientWidth)),
    )
  })
}

function goToPage(index: number) {
  const el = pagerRef.value
  activePage.value = index
  if (el) el.scrollTo({ left: index * el.clientWidth, behavior: 'smooth' })
  try {
    ;(navigator as Navigator & { vibrate?: (pattern: number) => boolean }).vibrate?.(5)
  } catch { /* optional haptic feedback */ }
}

/* A new search always restarts on the first springboard page. */
watch(filtered, () => {
  activePage.value = 0
  pagerRef.value?.scrollTo({ left: 0 })
})

function isPanelOpen(panel: PanelType): boolean {
  return wm.windows.value.some(window => window.panelType === panel && !window.minimized)
}

function open(panel: PanelType) {
  if (contextMenuFired) {
    // The contextmenu already opened the customize sheet for this gesture —
    // swallow the click that follows instead of launching the app under it.
    contextMenuFired = false
    return
  }
  if (longPressedHome) {
    longPressedHome = false
    return
  }
  if (store.currentPanel === 'landing') store.currentPanel = 'files'
  wm.open(panel)
  try {
    ;(navigator as Navigator & { vibrate?: (pattern: number) => boolean }).vibrate?.(8)
  } catch { /* optional haptic feedback */ }
}

function openDrawer() {
  launcher.setDrawer(true)
  if (androidApps.supported) void androidApps.refresh().catch(() => {})
}

// ── Click-and-hold customization for home tiles ────────────────
// Rename / gallery icon / move / hide / reset, stored per `os:<panel>`.
let homePressTimer: ReturnType<typeof setTimeout> | null = null
let longPressedHome = false
/** Android WebView fires `contextmenu` at ~500ms — BEFORE the 550ms
 * long-press timer. Without this flag a release in the 500-550ms window lets
 * the follow-up `click` run `open()` and launch the app under the sheet. */
let contextMenuFired = false
const customHome = ref<LauncherApp | null>(null)
const homeAliasDraft = ref('')
const homeMsg = ref('')
/** Staged gallery icon for the open sheet — component-local only; the store
 * deep-watch would persist `launcher.setIcon` instantly. `saveHomeCustom()`
 * is the single commit point (closing without Save discards it). */
const homeIconDraft = ref('')
const homeFileRef = ref<HTMLInputElement | null>(null)

/** Sheet preview: the staged gallery icon wins over the stored override. */
const customHomeTile = computed(() => {
  const staged = homeIconDraft.value.trim()
  if (staged) return { kind: 'image' as const, src: staged }
  return customHome.value
    ? homeTile(customHome.value)
    : { kind: 'icon' as const, src: 'solar:settings-bold' }
})

function openHomeCustom(app: LauncherApp) {
  customHome.value = app
  homeAliasDraft.value = launcher.overrides[launcherKeyForPanel(app.panel)]?.alias ?? ''
  homeMsg.value = ''
  homeIconDraft.value = ''
}

function onHomePressStart(app: LauncherApp, e: PointerEvent) {
  if (e.pointerType === 'mouse' && e.button !== 0) return
  longPressedHome = false
  contextMenuFired = false
  if (homePressTimer) clearTimeout(homePressTimer)
  homePressTimer = setTimeout(() => {
    longPressedHome = true
    try {
      ;(navigator as Navigator & { vibrate?: (pattern: number) => boolean }).vibrate?.(15)
    } catch { /* optional */ }
    openHomeCustom(app)
  }, 550)
}

function onHomePressEnd() {
  if (homePressTimer) {
    clearTimeout(homePressTimer)
    homePressTimer = null
  }
}

/** Right-click / Android touch long-press: `contextmenu` arrives at ~500ms,
 * before the 550ms timer — cancel the timer (so it cannot arm
 * `longPressedHome` for the next click) and flag the gesture as consumed. */
function onHomeContextMenu(app: LauncherApp) {
  if (homePressTimer) {
    clearTimeout(homePressTimer)
    homePressTimer = null
  }
  longPressedHome = false
  contextMenuFired = true
  openHomeCustom(app)
}

function homeKey(app: LauncherApp): string {
  return launcherKeyForPanel(app.panel)
}

function saveHomeCustom() {
  const app = customHome.value
  if (!app) return
  const key = homeKey(app)
  // Staged icon commits here too — the sheet closes below, so the confirmation
  // leaves with it as a transient toast (the in-sheet `homeMsg` line could
  // never render once `customHome` is null).
  if (homeIconDraft.value) launcher.setIcon(key, homeIconDraft.value)
  launcher.rename(key, homeAliasDraft.value)
  launcher.pinToEnd(key)
  store.notifySuccess(`Saved “${homeName(app)}”.`)
  customHome.value = null
}

function pickHomeGallery() {
  homeFileRef.value?.click()
}

async function onHomeGalleryFile(e: Event) {
  const input = e.target as HTMLInputElement | null
  const file = input?.files?.[0]
  const app = customHome.value
  if (!file || !app) return
  try {
    const url = await fileToLauncherIcon(file)
    // Staged only — `saveHomeCustom()` commits it (closing discards).
    homeIconDraft.value = url
    homeMsg.value = 'Gallery icon staged — Save to keep it.'
  } catch (err) {
    store.notifyError('Could not read that image', err)
  } finally {
    if (input) input.value = ''
  }
}

function moveHomeCustom(dir: -1 | 1) {
  const app = customHome.value
  if (!app) return
  launcher.pinToEnd(homeKey(app))
  launcher.move(homeKey(app), dir)
  homeMsg.value = dir < 0 ? 'Moved earlier.' : 'Moved later.'
}

function hideHomeCustom() {
  const app = customHome.value
  if (!app) return
  launcher.hide(homeKey(app), true)
  store.notifySuccess(`${homeName(app)} hidden — Reset home brings it back`)
  customHome.value = null
}

function resetHomeCustom() {
  const app = customHome.value
  if (!app) return
  launcher.reset(homeKey(app))
  customHome.value = null
}

const hasHomeOverrides = computed(() => Object.keys(launcher.overrides).some(k => k.startsWith('os:')))

function resetAllHome() {
  const next = { ...launcher.overrides }
  for (const k of Object.keys(next)) {
    if (k.startsWith('os:')) delete next[k]
  }
  launcher.overrides = next
}
</script>

<style scoped>
.mla {
  --mla-nav-height: 66px;
  position: absolute;
  inset: 0;
  z-index: 0;
  isolation: isolate;
  display: flex;
  flex-direction: column;
  /* iOS springboard density: tight rhythm so page 1 + dock fit a phone
     viewport without vertical scrolling (the reference never scrolls). */
  gap: 16px;
  /* The TopMenuBar header owns the notch/safe-area — the launcher sits
     below it, so no safe-area-top padding here (avoids a duplicate row). */
  padding: 12px 20px 150px;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  overscroll-behavior: contain;
  overscroll-behavior-y: contain;
  background:
    radial-gradient(ellipse at 8% 0%, color-mix(in srgb, var(--ui-accent) 25%, transparent), transparent 42%),
    radial-gradient(ellipse at 100% 38%, rgba(183, 151, 255, 0.16), transparent 42%),
    linear-gradient(160deg, var(--ui-bg), color-mix(in srgb, var(--ui-bg) 88%, #c9d9ff));
  color: var(--ui-text);
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", var(--ui-font), sans-serif;
}
/* iOS-11-style wallpaper wash (cf. iOS4Android $Asset.background):
   full-bleed warm/cool blobs behind the springboard, theme-aware. */
.mla::before {
  position: fixed;
  inset: 0;
  z-index: -1;
  content: '';
  pointer-events: none;
  background:
    radial-gradient(ellipse 90% 42% at 50% 108%, rgba(255, 149, 0, 0.20), transparent 62%),
    radial-gradient(ellipse 70% 46% at 88% 30%, rgba(88, 86, 214, 0.22), transparent 60%),
    radial-gradient(ellipse 75% 48% at 8% 66%, rgba(0, 122, 255, 0.18), transparent 62%);
  opacity: 0.85;
}

/* Connectivity/clock/vault status live in the single TopMenuBar header on
   mobile — the launcher renders no status row (no duplicated clocks). */
.mla-hello { margin: 0; }
.mla-eyebrow { margin: 0 0 2px; color: var(--ui-text-3); font-size: 13px; font-weight: 550; text-transform: capitalize; }
.mla-title { margin: 0; font-size: clamp(24px, 7vw, 30px); line-height: 1.1; font-weight: 750; letter-spacing: -0.04em; }
.mla-subtitle { margin: 5px 0 0; color: var(--ui-text-3); font-size: 13px; font-weight: 450; }

.mla-widgets { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
.mla-widget {
  min-width: 0;
  min-height: 70px;
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 10px;
  border: 1px solid color-mix(in srgb, var(--ui-border-strong) 64%, transparent);
  /* iOS widget corner (~22px on small widgets). */
  border-radius: 22px;
  background: color-mix(in srgb, var(--ui-surface) 66%, transparent);
  color: var(--ui-text);
  text-align: left;
  box-shadow: 0 8px 24px rgba(16, 24, 40, 0.07), inset 0 1px rgba(255, 255, 255, 0.16);
  backdrop-filter: blur(18px) saturate(1.3);
  -webkit-backdrop-filter: blur(18px) saturate(1.3);
  cursor: pointer;
  transition: transform 140ms ease, background 140ms ease;
}
.mla-widget:active { transform: scale(0.97); background: var(--ui-surface-2); }
.mla-widget-icon { flex: 0 0 34px; width: 34px; height: 34px; display: grid; place-items: center; border-radius: 12px; color: #fff; }
.vault-widget-icon { background: linear-gradient(145deg, #63b5ff, #3974dc); }
.sync-widget-icon { background: linear-gradient(145deg, #58d5bd, #188f95); }
.mla-widget-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
.mla-widget-label { color: var(--ui-text-3); font-size: 10px; line-height: 1.2; font-weight: 600; white-space: nowrap; }
.mla-widget-copy strong { overflow: hidden; color: var(--ui-text); font-size: 12px; line-height: 1.15; font-weight: 700; text-overflow: ellipsis; white-space: nowrap; }
.mla-widget-arrow { flex: 0 0 auto; color: var(--ui-text-3); opacity: 0.72; }

.mla-search {
  min-height: 47px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 13px;
  border: 1px solid color-mix(in srgb, var(--ui-border-strong) 58%, transparent);
  border-radius: 16px;
  background: color-mix(in srgb, var(--ui-surface) 72%, transparent);
  color: var(--ui-text-3);
  box-shadow: 0 5px 18px rgba(16, 24, 40, 0.045);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
}
.mla-search:focus-within { border-color: color-mix(in srgb, var(--ui-accent) 62%, var(--ui-border)); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ui-accent) 13%, transparent); }
.mla-search-input { flex: 1; min-width: 0; border: 0; outline: 0; background: none; color: var(--ui-text); font: inherit; font-size: 16px; }
.mla-search-input::placeholder { color: var(--ui-text-3); opacity: 0.9; }
.mla-search-hint { color: var(--ui-text-3); font-family: inherit; font-size: 16px; font-weight: 600; opacity: 0.62; }
.mla-search-clear { width: 40px; height: 40px; display: grid; place-items: center; border: 0; border-radius: 50%; background: var(--ui-surface-2); color: var(--ui-text-3); cursor: pointer; }

/* Springboard pager: horizontal PageView with iOS bounce
   (cf. iOS4Android PageView.builder, scrollDirection horizontal). */
.mla-pages {
  display: flex;
  margin: 0 -20px;
  padding: 0 20px 4px;
  overflow-x: auto;
  overflow-y: hidden;
  scroll-snap-type: x mandatory;
  scroll-behavior: smooth;
  -webkit-overflow-scrolling: touch;
  overscroll-behavior-x: contain;
  scrollbar-width: none;
  gap: 0;
}
.mla-pages::-webkit-scrollbar { display: none; }
.mla-grid {
  flex: 0 0 100%;
  scroll-snap-align: center;
  scroll-snap-stop: always;
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px 7px;
  align-content: start;
  min-height: 188px;
  content-visibility: auto;
  contain-intrinsic-size: auto 400px;
}
.mla-app {
  min-width: 0;
  min-height: 88px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  padding: 2px 1px;
  border: 0;
  border-radius: 20px;
  background: transparent;
  cursor: pointer;
  transition: transform 160ms cubic-bezier(.34,1.4,.4,1), filter 160ms ease;
  -webkit-tap-highlight-color: transparent;
}
/* iOS launch press: spring down + dim, like a SpringBoard icon tap. */
.mla-app:active { transform: scale(0.87); filter: brightness(0.92); }
.mla-app:focus-visible { outline: 2px solid var(--ui-accent); outline-offset: 2px; }
.mla-tile {
  position: relative;
  width: clamp(55px, 16vw, 60px);
  height: clamp(55px, 16vw, 60px);
  display: grid;
  place-items: center;
  overflow: visible;
  border: 1px solid rgba(255, 255, 255, 0.30);
  /* iOS squircle ≈ 25% continuous corner (cf. RoundedIconClipper r=15/60). */
  border-radius: 16px;
  background: linear-gradient(145deg, color-mix(in srgb, var(--app-tint) 72%, #fff), var(--app-tint) 76%);
  color: #fff;
  box-shadow: 0 8px 18px color-mix(in srgb, var(--app-tint) 23%, transparent), inset 0 1px rgba(255,255,255,.36);
}
/* iOS icon gloss: top-light sweep (LauncherIcon clipped highlight). */
.mla-tile::before { position: absolute; inset: 0; border-radius: inherit; content: ''; background: linear-gradient(180deg, rgba(255,255,255,.28), transparent 46%); pointer-events: none; }
.mla-tile::after { position: absolute; inset: 0; border-radius: inherit; content: ''; background: linear-gradient(140deg, rgba(255,255,255,.20), transparent 60%); pointer-events: none; }
.mla-tile :deep(svg) { position: relative; z-index: 1; filter: drop-shadow(0 1px 1px rgba(0,0,0,.10)); }
/* iOS icon label: 12pt white-style caption (LauncherIcon belowIcon). */
.mla-name { max-width: 100%; overflow: hidden; color: var(--ui-text-2); font-size: 12px; line-height: 1.2; font-weight: 500; text-overflow: ellipsis; white-space: nowrap; text-shadow: 0 1px 10px color-mix(in srgb, var(--ui-bg) 70%, transparent); }
.mla-badge { position: absolute; z-index: 2; top: -6px; right: -7px; min-width: 20px; height: 20px; display: flex; align-items: center; justify-content: center; padding: 0 5px; border: 1.5px solid var(--ui-bg); border-radius: 11px; background: #f2475e; color: #fff; font-size: 10px; font-weight: 800; box-shadow: 0 2px 5px rgba(0,0,0,.14); }
.mla-live { position: absolute; z-index: 2; bottom: -4px; left: 50%; width: 9px; height: 9px; transform: translateX(-50%); border: 2px solid var(--ui-bg); border-radius: 50%; background: #35c980; }
/* Page dots (cf. DotsIndicator: 7.5px dots, 16px spacing, white vs 47% white). */
.mla-dots { display: flex; align-items: center; justify-content: center; gap: 8.5px; min-height: 12px; margin-top: -10px; }
.mla-dot-btn { width: 16px; height: 16px; display: grid; place-items: center; border: 0; background: none; padding: 0; cursor: pointer; -webkit-tap-highlight-color: transparent; }
.mla-page-dot { width: 7.5px; height: 7.5px; border-radius: 50%; background: color-mix(in srgb, var(--ui-text-2) 38%, transparent); transition: transform 180ms cubic-bezier(.34,1.4,.4,1), background 180ms ease; }
.mla-dot-btn.on .mla-page-dot { transform: scale(1.15); background: var(--ui-text-2); }
.mla-empty { margin: 4px 0; color: var(--ui-text-3); text-align: center; font-size: 13px; }
.mla-link { padding: 7px; border: 0; background: none; color: var(--ui-accent); font: inherit; font-weight: 650; cursor: pointer; }

.mla-dock {
  position: sticky;
  bottom: calc(var(--mla-nav-height) + env(safe-area-inset-bottom, 0px));
  z-index: 2;
  display: flex;
  justify-content: space-around;
  gap: 7px;
  margin: auto -3px 0;
  padding: 11px 11px 9px;
  border: 1px solid color-mix(in srgb, var(--ui-border-strong) 62%, transparent);
  /* iPhone-X dock rounding (cf. bottomAreaBorder 34px; 28px fits our height). */
  border-radius: 28px;
  background: color-mix(in srgb, var(--ui-surface) 52%, transparent);
  box-shadow: 0 12px 35px rgba(16, 24, 40, .12), inset 0 1px rgba(255,255,255,.2);
  backdrop-filter: blur(28px) saturate(1.6);
  -webkit-backdrop-filter: blur(28px) saturate(1.6);
}
.mla:not(.with-app-nav) { padding-bottom: 96px; }
.mla:not(.with-app-nav) .mla-dock { bottom: 0; }
.mla-dock-btn { position: relative; flex: 1; min-width: 0; min-height: 62px; display: grid; place-items: center; padding: 0; border: 0; border-radius: 18px; background: transparent; cursor: pointer; transition: transform 150ms cubic-bezier(.34,1.4,.4,1), filter 150ms ease; -webkit-tap-highlight-color: transparent; }
.mla-dock-btn:active { transform: scale(.88); filter: brightness(0.94); }
.mla-dock-icon { position: relative; width: 52px; height: 52px; display: grid; place-items: center; overflow: hidden; border: 1px solid rgba(255,255,255,.25); border-radius: 15px; background: linear-gradient(145deg, color-mix(in srgb, var(--app-tint) 72%, #fff), var(--app-tint) 76%); color: #fff; box-shadow: 0 5px 12px color-mix(in srgb, var(--app-tint) 24%, transparent), inset 0 1px rgba(255,255,255,.28); }
.mla-dock-icon::before { position: absolute; inset: 0; border-radius: inherit; content: ''; background: linear-gradient(180deg, rgba(255,255,255,.26), transparent 48%); pointer-events: none; }
.mla-dock-indicator { position: absolute; bottom: 0; left: 50%; width: 4px; height: 4px; transform: translateX(-50%); border-radius: 50%; background: var(--ui-text-2); opacity: .7; }
/* Home indicator pill (iPhone-X gesture bar homage). */
.mla-home { width: 134px; height: 5px; margin: 10px auto 0; border-radius: 3px; background: color-mix(in srgb, var(--ui-text-2) 42%, transparent); }
.mla:not(.with-app-nav) .mla-home { position: sticky; bottom: calc(10px + env(safe-area-inset-bottom, 0px)); }
.with-app-nav .mla-home { display: none; }

.mla-photo {
  position: relative;
  z-index: 1;
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: inherit;
  display: block;
}
/* Floating top-right messages button (home has no header bar when no
   window is open; the tray button in TopMenuBar covers open apps). The
   launcher sits below TopMenuBar inside the shell, which already carries
   the notch inset — no safe-area here (it double-counted and sank the
   buttons below the status strip). */
.mla-topbtns {
  position: absolute;
  top: 10px;
  right: 12px;
  z-index: 5;
  display: flex;
  gap: 8px;
}
.mla-msgbtn {
  position: relative;
  width: 44px;
  height: 44px;
  display: grid;
  place-items: center;
  border: 1px solid color-mix(in srgb, var(--ui-border-strong) 64%, transparent);
  border-radius: 50%;
  background: color-mix(in srgb, var(--ui-surface) 66%, transparent);
  color: var(--ui-text);
  box-shadow: 0 8px 24px rgba(16, 24, 40, 0.07);
  backdrop-filter: blur(18px) saturate(1.3);
  -webkit-backdrop-filter: blur(18px) saturate(1.3);
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}
.mla-msgbtn:active {
  transform: scale(0.92);
}
.mla-badge.static {
  top: -4px;
  right: -4px;
}
.mla-tile.has-photo,
.mla-dock-icon.has-photo {
  background: var(--ui-surface-2);
}
.mla-tile.sm {
  width: 46px;
  height: 46px;
}
/* Gaveta affordance: pill handle + label above the dock. */
.mla-allapps {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  margin: 0 auto -6px;
  padding: 8px 22px;
  border: 0;
  background: transparent;
  color: var(--ui-text-3);
  font: inherit;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
}
.mla-allapps-pill {
  width: 44px;
  height: 5px;
  border-radius: 3px;
  background: color-mix(in srgb, var(--ui-text-2) 42%, transparent);
}
.mla-reset {
  align-self: center;
  margin-top: -4px;
  min-height: 44px;
  display: inline-flex;
  align-items: center;
  padding: 4px 10px;
  border: 0;
  background: none;
  color: var(--ui-text-3);
  font: inherit;
  font-size: 11px;
  font-weight: 600;
  text-decoration: underline;
  cursor: pointer;
}
/* Home tile customization sheet. */
.mla-custom-veil {
  position: fixed;
  inset: 0;
  z-index: 9500;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  background: color-mix(in srgb, var(--ui-bg-deep) 55%, transparent);
}
/* Home tile customization sheet — capped so landscape phones (short
   viewport) scroll the sheet instead of clipping Save/Close off-screen. */
.mla-custom {
  width: min(480px, 100vw);
  max-height: min(68dvh, 560px);
  overflow-y: auto;
  overscroll-behavior: contain;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 16px calc(16px + env(safe-area-inset-bottom, 0px));
  border-radius: 20px 20px 0 0;
  border: 1px solid var(--ui-border-strong);
  border-bottom: 0;
  background: color-mix(in srgb, var(--ui-surface) 92%, var(--ui-bg));
  color: var(--ui-text);
}
.mla-custom-title {
  font-size: 16px;
}
.mla-custom-sub {
  margin-top: -8px;
  color: var(--ui-text-3);
  font-size: 12px;
}
.mla-custom-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.mla-custom-label {
  flex: 0 0 52px;
  color: var(--ui-text-3);
  font-size: 12px;
  font-weight: 600;
}
.mla-custom-input {
  flex: 1;
  min-width: 0;
  min-height: 38px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: 10px;
  background: var(--ui-surface);
  color: var(--ui-text);
  font: inherit;
  font-size: 14px;
}
.mla-custom-icon-row {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
}
.mla-file {
  display: none;
}
.mla-custom-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.mla-mini-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 44px;
  padding: 0 14px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 12px;
  background: var(--ui-surface);
  color: var(--ui-text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.mla-mini-btn.primary {
  background: var(--ui-accent);
  border-color: var(--ui-accent);
  color: var(--ui-on-accent);
}
.mla-custom-msg {
  margin: 0;
  color: var(--ui-text-3);
  font-size: 12px;
}

@media (min-width: 600px) and (max-width: 768px) {
  .mla { padding-left: 32px; padding-right: 32px; gap: 22px; }
  .mla-pages { margin: 0 -32px; padding-left: 32px; padding-right: 32px; }
  .mla-grid { grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 23px 12px; }
  .mla-widgets { max-width: 540px; }
  .mla-dock { max-width: 520px; align-self: center; width: 100%; }
}
@media (prefers-reduced-motion: reduce) {
  .mla-app, .mla-dock-btn, .mla-widget, .mla-pages { transition: none; scroll-behavior: auto; }
  .mla-page-dot { transition: none; }
}
</style>
