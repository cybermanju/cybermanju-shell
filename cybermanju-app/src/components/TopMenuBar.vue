<!-- CyberManju OS — TopMenuBar (src/components/TopMenuBar.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <header class="top-menu-bar">
    <div class="tmb-left">
      <button class="app-logo" type="button" aria-label="Home" title="Home" @click="handleHome">
        <span class="logo-mark"><img src="/bhumisparsha.png" alt="Bhumisparsha" width="22" height="22" /></span>
        <span class="logo-brand">CyberManju</span>
      </button>

      <nav class="menu-items" ref="menuRef">
        <div
          v-for="item in menuStructure"
          :key="item.id"
          class="menu-item"
          :class="{ open: openMenu === item.id }"
          role="button"
          tabindex="0"
          :aria-expanded="openMenu === item.id"
          aria-haspopup="menu"
          @click="toggleMenu(item.id)"
          @keydown.enter.prevent="toggleMenu(item.id)"
          @keydown.space.prevent="toggleMenu(item.id)"
          @mouseenter="hoverMenu(item.id)"
        >
          <span class="menu-label">{{ item.label }}</span>
          <Transition name="menu">
            <div v-if="openMenu === item.id" class="menu-dropdown">
              <template v-for="sub in item.children" :key="sub.id">
                <div v-if="sub.divider" class="menu-divider" />
                <button
                  v-else
                  class="menu-dropdown-item"
                  type="button"
                  @click.stop="executeMenuItem(sub)"
                >
                  <span class="mdi-icon"><AppIcon :name="sub.icon" :size="14" /></span>
                  <span class="mdi-label">{{ sub.label }}</span>
                  <span v-if="sub.shortcut" class="mdi-shortcut">{{ sub.shortcut }}</span>
                  <span v-if="sub.checked" class="mdi-check"><AppIcon name="solar:check-circle-bold" :size="13" /></span>
                </button>
              </template>
            </div>
          </Transition>
        </div>
      </nav>
    </div>

    <div class="tmb-center">
      <div class="search-wrap" :class="{ searching: store.isSearching, focused: searchFocused }">
        <AppIcon name="solar:magnifer-bold" :size="13" class="search-icon" />
        <input
          v-model="store.searchQuery"
          class="search-input"
          type="text"
          placeholder="Search files…"
          aria-label="Search files"
          title="Search files (Ctrl+F) — Enter to search, Esc to clear"
          @focus="searchFocused = true"
          @blur="searchFocused = false"
          @keyup.enter="handleSearch"
          @keyup.esc="clearSearch"
        />
        <span v-if="store.isSearching" class="search-cursor" aria-hidden="true" />
        <span v-else-if="searchQueryShort" class="search-hint">ENTER</span>
      </div>
    </div>

    <div class="tmb-right">
      <!-- Mobile-only status chips: single header owns online + vault count
        so the launcher never renders a second clock/status row. -->
      <span class="tmb-mobile-meta" aria-label="Connection and vault status">
        <span class="tmb-net" :class="{ off: !online }"><span class="tmb-dot" aria-hidden="true" />{{ online ? 'Online' : 'Offline' }}</span>
        <span class="tmb-vaults">{{ vaultCount }}</span>
      </span>
      <div class="sys-tray">
        <button
          class="tray-icon tray-icon--encryption"
          :class="{ active: store.encryptionStatus.isEncrypted }"
          type="button"
          @click="wm.open('encryption', { tab: 'shield' })"
          :title="`Encryption: ${store.encryptionStatus.isEncrypted ? 'ON' : 'OFF'}`"
        >
          <AppIcon :name="store.encryptionStatus.isEncrypted ? 'solar:shield-check-bold' : 'solar:shield-bold'" :size="14" />
        </button>

        <button
          class="tray-icon tray-icon--compression"
          :class="{ active: store.compressedFiles.length > 0 }"
          type="button"
          @click="wm.open('encryption', { tab: 'compress' })"
          :title="`Compression: ${store.compressedFiles.length} files`"
        >
          <AppIcon name="solar:archive-bold" :size="14" />
        </button>

        <button
          class="tray-icon tray-icon--account"
          :class="{ active: store.activeAccount }"
          type="button"
          @click="wm.open('accounts')"
          :title="store.activeAccount?.name || 'No account'"
        >
          <AppIcon name="solar:user-circle-bold" :size="14" />
        </button>

        <button
          class="tray-icon tray-icon--theme"
          type="button"
          title="Cycle theme"
          @click="theme.cycleTheme()"
        >
          <AppIcon :name="theme.mode.value === 'light' ? 'solar:sun-bold' : 'solar:moon-bold'" :size="14" />
        </button>

        <button
          class="tray-icon tray-icon--notifications"
          type="button"
          title="Notifications"
          aria-label="Notifications"
          @click="launcher.setMessagesOpen(true, 'system')"
        >
          <AppIcon name="solar:bell-bold" :size="14" />
          <span v-if="sysUnread > 0" class="tray-badge">{{ sysUnread > 99 ? '99+' : sysUnread }}</span>
        </button>

        <button
          v-if="msgHub.supported"
          class="tray-icon tray-icon--messages"
          type="button"
          title="Messages"
          aria-label="Messages"
          @click="launcher.setMessagesOpen(true, 'messages')"
        >
          <AppIcon name="solar:chat-round-dots-bold" :size="14" />
          <span v-if="msgHub.unread.value > 0" class="tray-badge">{{ msgHub.unread.value > 99 ? '99+' : msgHub.unread.value }}</span>
        </button>

        <button
          class="tray-icon tray-icon--effects"
          :class="{ active: store.matrixRainEnabled }"
          type="button"
          @click="store.matrixRainEnabled = !store.matrixRainEnabled"
          title="Toggle background effects"
        >
          <AppIcon name="solar:widget-bold" :size="14" />
        </button>

        <button
          class="tray-icon tray-icon--commands"
          type="button"
          @click="store.commandPaletteOpen = true"
          title="Command Palette"
        >
          <AppIcon name="solar:command-bold" :size="14" />
        </button>

        <button
          class="tray-icon tray-icon--login"
          type="button"
          @click="wm.open('accounts')"
          :title="store.currentUser ? `Signed in — ${store.currentUser.username}` : 'Accounts — OAuth sign-in, .cybermanju disk'"
        >
          <AppIcon name="solar:login-2-bold" :size="14" />
        </button>
      </div>

      <div class="tmb-separator" />

      <button class="clock" type="button" @click="openDateInfo">
        <span class="clock-time">{{ timeStr }}</span>
        <span class="clock-date">{{ dateStr }}</span>
      </button>
    </div>
  </header>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { useLauncherStore } from '@/stores/launcher'
import { useMessages } from '@/composables/useMessages'
import { useNotifications } from '@/composables/useNotifications'
import { useWindowManager } from '@/composables/useWindowManager'
import { useTheme } from '@/composables/useTheme'
import { openExternalUrl } from '@/utils/openExternal'

const store = useAppStore()
const launcher = useLauncherStore()
const msgHub = useMessages()
const sysNotifs = useNotifications()
const sysUnread = computed(() => sysNotifs.unreadCount.value)
const wm = useWindowManager()
const theme = useTheme()
const searchQueryShort = computed(() => store.searchQuery.trim().length > 0)

const searchFocused = ref(false)

// Mobile single-header status: connectivity + vault count live here so the
// launcher must not render its own clock/status row (no duplicated clocks).
const online = ref(typeof navigator === 'undefined' ? true : navigator.onLine)
const vaultCount = computed(() => {
  const n = store.disks.length
  return `${n} vault${n === 1 ? '' : 's'}`
})
function onOnline() { online.value = true }
function onOffline() { online.value = false }

const timeStr = ref('')
const dateStr = ref('')
let clockTimer: ReturnType<typeof setInterval> | null = null

function updateClock() {
  const now = new Date()
  timeStr.value = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  dateStr.value = now.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

onMounted(() => {
  updateClock()
  // Tick only while visible: a hidden bar can't be read, and the first
  // visible tick repaints the correct time anyway.
  clockTimer = setInterval(() => { if (!document.hidden) updateClock() }, 1000)
  window.addEventListener('online', onOnline)
  window.addEventListener('offline', onOffline)
  // Best-effort so the mobile vault chip is correct on first paint.
  void store.fetchDisks().catch(() => {})
})

onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer)
  window.removeEventListener('online', onOnline)
  window.removeEventListener('offline', onOffline)
})

const openMenu = ref<string | null>(null)
const menuRef = ref<HTMLElement | null>(null)

interface MenuItem {
  id: string
  label?: string
  icon?: string
  shortcut?: string
  checked?: boolean
  divider?: true
  action?: () => void
}

interface MenuGroup {
  id: string
  label: string
  children: MenuItem[]
}

const menuStructure = computed<MenuGroup[]>(() => [
  {
    id: 'file',
    label: 'File',
    children: [
      { id: 'new-folder', label: 'New Folder', icon: 'solar:add-folder-bold', shortcut: 'Ctrl+N', action: () => { store.createFolderPromptOpen = true } },
      { id: 'upload', label: 'Upload Files', icon: 'solar:upload-bold', action: () => { window.dispatchEvent(new CustomEvent('cybermanju:upload')) } },
      { id: 'div1', divider: true },
      { id: 'open-terminal', label: 'Open Terminal (cybsh)', icon: 'solar:file-terminal-bold', shortcut: 'Ctrl+`', action: () => { wm.open('terminal') } },
      { id: 'open-tasks', label: 'Tasks (ps/top)', icon: 'solar:cpu-bold', action: () => { wm.open('processes') } },
      { id: 'open-disks', label: 'Disks & Volume', icon: 'solar:flash-drive-bold', action: () => { wm.open('disks') } },
      { id: 'div2', divider: true },
      { id: 'settings', label: 'Settings', icon: 'solar:settings-bold', shortcut: 'Ctrl+,', action: () => { wm.open('settings') } },
      { id: 'quit', label: 'Quit', icon: 'solar:power-bold', action: () => {} },
    ],
  },
  {
    id: 'edit',
    label: 'Edit',
    children: [
      { id: 'cut', label: 'Cut', icon: 'solar:scissors-bold', shortcut: 'Ctrl+X', action: () => {} },
      { id: 'copy', label: 'Copy', icon: 'solar:copy-bold', shortcut: 'Ctrl+C', action: () => {} },
      { id: 'paste', label: 'Paste', icon: 'solar:clipboard-bold', shortcut: 'Ctrl+V', action: () => {} },
      { id: 'div1', divider: true },
      { id: 'select-all', label: 'Select All', icon: 'solar:check-circle-bold', shortcut: 'Ctrl+A', action: () => { store.selectedFileIds = store.files.map(f => f.id) } },
      { id: 'deselect', label: 'Deselect', icon: 'solar:close-square-bold', action: () => { store.selectedFileIds = [] } },
    ],
  },
  {
    id: 'view',
    label: 'View',
    children: [
      { id: 'file-browser', label: 'File Browser', icon: 'solar:folder-bold', shortcut: 'Ctrl+1', action: () => { wm.open('files') } },
      { id: 'code', label: 'Code Studio', icon: 'solar:code-square-bold', action: () => { wm.open('editor') } },
      { id: 'div1', divider: true },
      { id: 'search', label: 'Search (apps, collections, people, files)', icon: 'solar:magnifer-bold', shortcut: 'Ctrl+F', action: () => { store.searchQuery = ''; wm.open('search') } },
      { id: 'map', label: 'Map View', icon: 'solar:map-bold', action: () => { wm.open('map') } },
      { id: 'storage', label: 'Storage & Disks', icon: 'solar:database-bold', action: () => { wm.open('disks') } },
      { id: 'sync-panel', label: 'Sync Panel', icon: 'solar:refresh-bold', action: () => { wm.open('sync') } },
      { id: 'devices', label: 'Devices & Sensors', icon: 'solar:plug-circle-bold', action: () => { wm.open('devices') } },
      { id: 'overlay', label: 'Remote Dashboard', icon: 'solar:kanban-square-bold', action: () => { wm.open('dashboard') } },
    ],
  },
  {
    id: 'window',
    label: 'Window',
    children: [
      { id: 'minimize-all', label: 'Minimize All', icon: 'solar:minimize-square-bold', action: () => wm.minimizeAll() },
      { id: 'close-all', label: 'Close All Windows', icon: 'solar:close-circle-bold', action: () => wm.closeAll() },
      { id: 'div1', divider: true },
      { id: 'trash', label: 'Trash', icon: 'solar:trash-bin-trash-bold', action: () => { wm.open('trash'); store.fetchTrashItems() } },
      { id: 'activity', label: 'Activity Log', icon: 'solar:chart-bold', action: () => { wm.open('activity'); store.fetchAuditLog() } },
      { id: 'favorites', label: 'Search', icon: 'solar:star-bold', action: () => { wm.open('search') } },
      { id: 'recent', label: 'Recent Files', icon: 'solar:history-2-bold', action: () => { wm.open('recent') } },
      { id: 'div1', divider: true },
      { id: 'accounts', label: 'Account Manager', icon: 'solar:user-id-bold', action: () => { wm.open('accounts') } },
      { id: 'sync-panel-link', label: 'Provider Connections (OAuth)', icon: 'solar:link-bold', action: () => { wm.open('accounts', { tab: 'connections' }) } },
      { id: 'transfer', label: 'Transfer Board (mv/cp)', icon: 'solar:share-bold', action: () => { wm.open('transfer') } },
      { id: 'terminal', label: 'Terminal (cybsh)', icon: 'solar:file-terminal-bold', shortcut: 'Ctrl+`', action: () => { wm.open('terminal') } },
      { id: 'tasks', label: 'Tasks (ps/top)', icon: 'solar:cpu-bold', action: () => { wm.open('processes') } },
      { id: 'disks', label: 'Disks & Volume', icon: 'solar:flash-drive-bold', action: () => { wm.open('disks') } },
      { id: 'storage', label: 'Storage & Disks', icon: 'solar:database-bold', action: () => { wm.open('disks') } },
      { id: 'users', label: 'Accounts & Users', icon: 'solar:users-group-rounded-bold', action: () => { wm.open('accounts', { tab: 'users' }) } },
      { id: 'div2', divider: true },
      { id: 'command-palette', label: 'Command Palette', icon: 'solar:command-bold', shortcut: 'Ctrl+K', action: () => { store.commandPaletteOpen = true } },
      { id: 'keyboard-shortcuts', label: 'Keyboard Shortcuts', icon: 'solar:keyboard-bold', shortcut: '?', action: () => { store.showShortcutsHelp = true } },
    ],
  },
  {
    id: 'help',
    label: 'Help',
    children: [
      { id: 'about', label: 'About CyberManju OS', icon: 'solar:info-circle-bold', action: () => {} },
      { id: 'docs', label: 'Documentation', icon: 'solar:book-bookmark-bold', action: () => { void openExternalUrl('https://github.com/cybermanju/cybermanju.github.io').catch((e) => store.notifyError('Cannot open the documentation', e)) } },
      { id: 'setup', label: 'Setup wizard', icon: 'solar:magic-wand-bold', action: () => { window.dispatchEvent(new CustomEvent('cybermanju:open-setup')) } },
      { id: 'div1', divider: true },
      { id: 'matrix', label: 'Toggle Matrix Rain', icon: 'solar:widget-bold', checked: store.matrixRainEnabled, action: () => { store.matrixRainEnabled = !store.matrixRainEnabled } },
    ],
  },
])

function toggleMenu(id: string) {
  openMenu.value = openMenu.value === id ? null : id
}

function hoverMenu(id: string) {
  if (openMenu.value !== null) {
    openMenu.value = id
  }
}

function executeMenuItem(item: MenuItem) {
  openMenu.value = null
  item.action?.()
}

function handleSearch() {
  if (store.searchQuery.trim()) {
    store.searchFiles(store.searchQuery)
    wm.open('search')
  }
}

function clearSearch() {
  store.searchQuery = ''
}

function handleHome() {
  if (window.matchMedia?.('(max-width: 768px)').matches) {
    wm.closeAll()
    store.currentPanel = 'files'
  } else {
    store.currentPanel = 'landing'
  }
}

function openDateInfo() {
  wm.open('settings')
}

function handleClickOutside(e: MouseEvent) {
  if (menuRef.value && !menuRef.value.contains(e.target as Node)) {
    openMenu.value = null
  }
}

function handleEscape(e: KeyboardEvent) {
  if (e.key === 'Escape' && openMenu.value !== null) {
    openMenu.value = null
  }
}

onMounted(() => {
  document.addEventListener('click', handleClickOutside)
  document.addEventListener('keydown', handleEscape)
})

onUnmounted(() => {
  document.removeEventListener('click', handleClickOutside)
  document.removeEventListener('keydown', handleEscape)
})
</script>

<style scoped>
.top-menu-bar {
  display: flex;
  align-items: center;
  height: 24px;
  padding: 0 10px;
  background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border-bottom: 1px solid var(--ui-separator);
  z-index: 100;
  position: relative;
  gap: 8px;
  -webkit-app-region: drag;
  user-select: none;
}

.tmb-left {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  /* Must stay visible: the menu dropdowns overflow below the 36px bar.
     `hidden` here clips them to zero height so clicks appear to do nothing. */
  overflow: visible;
  -webkit-app-region: no-drag;
}

.app-logo {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 0 6px;
  height: 22px;
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.app-logo:hover {
  background: color-mix(in srgb, var(--ui-text) 8%, transparent);
}

.logo-mark {
  display: inline-flex;
  border-radius: 6px;
  overflow: hidden;
}
.logo-mark img {
  display: block;
  width: 15px;
  height: 15px;
  object-fit: cover;
  border-radius: 4px;
}

.logo-brand {
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0;
  color: var(--ui-text);
  white-space: nowrap;
}

.menu-items {
  display: flex;
  align-items: center;
  /* Must stay visible: any non-visible overflow-x forces overflow-y to
     compute to auto/hidden and clips the absolutely-positioned dropdowns. */
  overflow: visible;
  min-width: 0;
  flex-shrink: 1;
}

.menu-items::-webkit-scrollbar {
  display: none;
}

.menu-item {
  position: relative;
  padding: 2px 8px;
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  transition: background-color var(--ui-dur-fast) ease-out;
}

.menu-item:hover,
.menu-item.open {
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
}

.menu-item:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}

.menu-label {
  font-family: var(--ui-font);
  font-size: 13px;
  color: var(--ui-text);
  font-weight: 400;
  letter-spacing: 0;
  white-space: nowrap;
}

.menu-item:hover .menu-label,
.menu-item.open .menu-label {
  color: var(--ui-text);
}

.menu-dropdown {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  min-width: 248px;
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  padding: 4px;
  box-shadow: var(--ui-shadow-menu);
  z-index: 200;
  animation: ui-fade-in var(--ui-dur) ease-out both;
  transform-origin: top left;
}

.menu-dropdown-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-height: 22px;
  padding: 3px 8px;
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 400;
  color: var(--ui-text);
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  text-align: left;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.menu-dropdown-item:hover {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
}

.mdi-icon {
  width: 18px;
  display: inline-flex;
  justify-content: center;
  color: var(--ui-text-3);
  transition: color var(--ui-dur-fast) var(--ui-ease-out);
}

.menu-dropdown-item:hover .mdi-icon {
  color: inherit;
}

.mdi-label {
  flex: 1;
}

.mdi-shortcut {
  font-family: var(--ui-font);
  font-size: 12px;
  color: var(--ui-text-3);
  margin-left: auto;
  flex-shrink: 0;
}

.menu-dropdown-item:hover .mdi-shortcut {
  color: var(--ui-on-accent);
}

.mdi-check {
  color: inherit;
  display: inline-flex;
}
.menu-dropdown-item:hover .mdi-check {
  color: inherit;
}

.menu-divider {
  height: 1px;
  background: var(--ui-border);
  margin: 4px 6px;
}

/* dropdown transition */
.menu-enter-active,
.menu-leave-active {
  transition:
    opacity var(--ui-dur) var(--ui-ease-out),
    transform var(--ui-dur) var(--ui-ease-out);
}

.menu-enter-from,
.menu-leave-to {
  opacity: 0;
  transform: translateY(-6px) scale(0.98);
}

/* ── search ───────────────────────────────────────────────────────────── */

.tmb-center {
  flex: 1;
  display: flex;
  justify-content: center;
  max-width: 420px;
  margin: 0 auto;
}

.search-wrap {
  position: relative;
  display: flex;
  align-items: center;
  width: 100%;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  padding: 0 8px;
  height: 20px;
  gap: 6px;
  transition:
    border-color var(--ui-dur) ease-out,
    box-shadow var(--ui-dur) ease-out;
}

.search-wrap:hover {
  border-color: var(--ui-border-hover);
}

.search-wrap.focused,
.search-wrap.searching {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
  background: var(--ui-surface-2);
}

.search-icon {
  color: var(--ui-text-3);
  flex-shrink: 0;
  transition: color var(--ui-dur);
}

.search-wrap.focused .search-icon,
.search-wrap.searching .search-icon {
  color: var(--ui-accent);
}

.search-input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  height: 100%;
  outline: none;
}

.search-input::placeholder {
  color: var(--ui-text-faint);
}

.search-hint {
  font-family: var(--ui-font);
  font-size: 9px;
  font-weight: 600;
  color: var(--ui-text-3);
  flex-shrink: 0;
}

.search-cursor {
  color: var(--ui-accent);
  animation: blink 0.8s step-end infinite;
  font-size: 11px;
}

@keyframes blink {
  50% { opacity: 0; }
}

/* ── tray + clock ─────────────────────────────────────────────────────── */

.tmb-right {
  display: flex;
  align-items: center;
  gap: 6px;
  -webkit-app-region: no-drag;
}

.sys-tray {
  display: flex;
  align-items: center;
  gap: 2px;
  overflow-x: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-width: thin;
  scrollbar-color: transparent transparent;
  max-width: 100%;
}

.sys-tray::-webkit-scrollbar {
  height: 6px;
}
.sys-tray::-webkit-scrollbar-track {
  background: transparent;
}
.sys-tray::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: var(--ui-radius-full);
}
.sys-tray:hover::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-text) 30%, transparent);
  border: 1px solid transparent;
  background-clip: content-box;
}
.sys-tray:hover {
  scrollbar-color: color-mix(in srgb, var(--ui-text) 30%, transparent) transparent;
}

.tray-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 20px;
  color: var(--ui-text-2);
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: 1px solid transparent;
  transition:
    color var(--ui-dur-fast) ease-out,
    background-color var(--ui-dur-fast) ease-out;
}

.tray-icon:hover {
  color: var(--ui-text);
  background: color-mix(in srgb, var(--ui-text) 8%, transparent);
}

.tray-icon.active {
  color: var(--ui-accent);
}

.tray-icon--messages {
  position: relative;
}

.tray-badge {
  position: absolute;
  top: 0;
  right: 0;
  min-width: 15px;
  height: 15px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 3px;
  border-radius: 8px;
  background: #f2475e;
  color: #fff;
  font-size: 9px;
  font-weight: 800;
}

.tmb-separator {
  width: 1px;
  height: 18px;
  background: var(--ui-border);
  flex-shrink: 0;
}

.clock {
  display: flex;
  align-items: baseline;
  gap: 6px;
  padding: 1px 6px;
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: 1px solid transparent;
  white-space: nowrap;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.clock:hover {
  background: color-mix(in srgb, var(--ui-text) 8%, transparent);
}

.clock-time {
  font-family: var(--ui-font);
  font-size: 12px;
  font-weight: 600;
  color: var(--ui-text);
}

.clock-date {
  font-family: var(--ui-font);
  font-size: 12px;
  font-weight: 400;
  color: var(--ui-text-2);
}

@media (max-width: 960px) {
  .tmb-center {
    max-width: 220px;
  }
  .clock-date {
    display: none;
  }
}

@media (max-width: 720px) {
  .tmb-center {
    display: none;
  }
  .logo-brand {
    display: none;
  }
  .menu-item {
    padding: 5px 7px;
  }
}

/* Mobile status chips live in this header only — hidden on desktop. */
.tmb-mobile-meta {
  display: none;
}
.tmb-net {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  white-space: nowrap;
}
.tmb-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--ui-success);
  box-shadow: 0 0 10px color-mix(in srgb, var(--ui-success) 55%, transparent);
  flex-shrink: 0;
}
.tmb-net.off .tmb-dot {
  background: var(--ui-danger);
  box-shadow: none;
}
.tmb-vaults {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

@media (max-width: 768px) {
  /* Single-line status-bar header: logo + status + theme + one clock in a
     slim `--mobile-header-h` strip (the shell owns the notch inset, so this
     bar never grows around it). Nothing wraps — extra tray icons stay
     hidden, meta ellipsizes. */
  .top-menu-bar {
    box-sizing: border-box;
    height: var(--mobile-header-h);
    min-height: var(--mobile-header-h);
    padding: 0 8px;
    gap: 6px;
    flex-wrap: nowrap;
    overflow: hidden;
  }
  .menu-items,
  .tmb-center {
    display: none !important;
  }
  .tmb-left { flex: 0 0 auto; min-width: 0; }
  .app-logo { width: 34px; height: var(--mobile-header-h); justify-content: center; padding: 0; flex-shrink: 0; }
  .logo-mark img { width: 16px; height: 16px; }
  .tmb-right { flex: 1; min-width: 0; justify-content: flex-end; align-items: center; gap: 4px; flex-wrap: nowrap; overflow: hidden; }
  .tmb-mobile-meta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
    font-size: 11px;
    font-weight: 600;
    color: var(--ui-text-2);
  }
  .tmb-vaults { color: var(--ui-text-3); font-weight: 500; }
  .tmb-dot { width: 6px; height: 6px; }
  .sys-tray { flex: 0 0 auto; gap: 2px; overflow: visible; }
  .tray-icon { flex: 0 0 26px; width: 26px; height: 26px; border-radius: 8px; }
  .tray-icon--encryption,
  .tray-icon--compression,
  .tray-icon--effects,
  .tray-icon--login,
  .tray-icon--account,
  .tray-icon--commands { display: none; }
  .tmb-separator { display: none; }
  .clock { min-height: var(--mobile-header-h); padding: 0 6px; flex-shrink: 0; }
  .clock-time { font-size: 12px; }
}
</style>
