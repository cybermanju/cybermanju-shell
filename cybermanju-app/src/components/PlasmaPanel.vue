<!-- CyberManju OS — PlasmaPanel (src/components/PlasmaPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <footer class="plasma-panel" :class="{ docked: docked === '1' }">
    <!-- Launcher -->
    <button
      class="pp-launcher"
      type="button"
      :class="{ open: kickoff.kickoffOpen.value }"
      aria-label="Application launcher (Alt+F1)"
      title="Application launcher (Alt+F1)"
      @click="kickoff.toggle()"
    >
      <AppIcon name="solar:widget-bold" :size="20" />
    </button>

    <div class="pp-sep" />

    <!-- Pager: layouts act as virtual desktops -->
    <div class="pp-pager" role="group" aria-label="Virtual desktops (layouts)">
      <button
        v-for="m in layouts"
        :key="m.id"
        type="button"
        class="pp-page"
        :class="{ active: wm.shellLayoutMode.value === m.id }"
        :title="`${m.label} desktop`"
        :aria-pressed="wm.shellLayoutMode.value === m.id"
        @click="wm.setLayoutMode(m.id)"
      >
        <span class="pp-page-box" />
        <span class="pp-page-label">{{ m.label }}</span>
      </button>
    </div>

    <div class="pp-sep" />

    <!-- Task manager: one entry per live window (single-window invariant
         means one entry per panel type), grouped press-and-hold free. -->
    <div class="pp-tasks" role="group" aria-label="Task manager">
      <button
        v-for="win in wm.windows.value"
        :key="win.id"
        type="button"
        class="pp-task"
        :class="{
          active: wm.activeWindow.value?.id === win.id && !win.minimized,
          minimized: win.minimized,
        }"
        :title="win.title"
        @click="activateWindow(win.id)"
        @auxclick="onTaskAux($event, win.id)"
        @contextmenu.prevent="openTaskMenu($event, win.id)"
      >
        <AppIcon :name="win.icon" :size="16" />
        <span class="pp-task-label">{{ win.title }}</span>
      </button>
      <span v-if="wm.windows.value.length === 0" class="pp-tasks-empty">No open windows</span>
    </div>

    <div class="pp-spacer" />

    <!-- System tray -->
    <div class="pp-tray" role="group" aria-label="System tray">
      <button
        class="pp-tray-btn"
        type="button"
        :title="`Transport: ${transportLabel}`"
        :aria-label="`Transport: ${transportLabel}`"
        @click="togglePopover('net')"
      >
        <AppIcon name="solar:server-bold" :size="16" />
      </button>
      <button
        class="pp-tray-btn"
        type="button"
        :class="{ active: syncBusy }"
        :title="syncBusy ? 'Sync running' : 'Sync idle'"
        :aria-label="syncBusy ? 'Sync running' : 'Sync idle'"
        @click="togglePopover('sync')"
      >
        <AppIcon name="solar:refresh-bold" :size="16" />
      </button>
      <button
        class="pp-tray-btn"
        type="button"
        :title="`${unreadCount} notifications — open notification center`"
        :aria-label="`${unreadCount} notifications — open notification center`"
        @click="openCenter()"
      >
        <AppIcon name="solar:bell-bold" :size="16" />
        <span v-if="unreadCount > 0" class="pp-badge">{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
      </button>
      <button
        class="pp-tray-btn"
        type="button"
        title="Volume (placeholder)"
        aria-label="Volume (placeholder)"
        @click="togglePopover('vol')"
      >
        <AppIcon name="solar:music-note-bold" :size="16" />
      </button>
      <button
        class="pp-tray-btn"
        type="button"
        :title="theme.mode.value === 'light' ? 'Switch to dark' : 'Switch to light'"
        :aria-label="theme.mode.value === 'light' ? 'Switch to dark' : 'Switch to light'"
        @click="toggleAppearance()"
      >
        <AppIcon :name="theme.mode.value === 'light' ? 'solar:moon-bold' : 'solar:sun-bold'" :size="16" />
      </button>
    </div>

    <!-- Clock + calendar popover -->
    <button
      class="pp-clock"
      type="button"
      :title="dateTitle"
      @click="togglePopover('clock')"
    >
      <span class="pp-clock-time">{{ timeStr }}</span>
      <span class="pp-clock-date">{{ dateStr }}</span>
    </button>

    <!-- Popovers -->
    <div v-if="popover" class="pp-pop" role="dialog" :aria-label="popover">
      <div class="pp-pop-head">
        <span class="pp-pop-title">{{ popoverTitle }}</span>
        <button class="pp-pop-x" type="button" aria-label="Close" @click="popover = null">
          <AppIcon name="solar:close-bold" :size="12" />
        </button>
      </div>
      <div v-if="popover === 'clock'" class="pp-cal">
        <div class="pp-cal-head">{{ calTitle }}</div>
        <div class="pp-cal-grid">
          <span v-for="d in ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su']" :key="d" class="pp-cal-dow">{{ d }}</span>
          <span v-for="b in calBlanks" :key="'b' + b" class="pp-cal-day blank" />
          <span
            v-for="day in calDays"
            :key="day"
            class="pp-cal-day"
            :class="{ today: day === todayNum }"
          >{{ day }}</span>
        </div>
      </div>
      <div v-else-if="popover === 'net'" class="pp-pop-body">
        <div class="pp-kv"><span>Transport</span><b>{{ transportLabel }}</b></div>
        <div class="pp-kv"><span>Endpoint</span><b class="mono">{{ endpointLabel }}</b></div>
      </div>
      <div v-else-if="popover === 'sync'" class="pp-pop-body">
        <div class="pp-kv"><span>Status</span><b>{{ syncBusy ? 'Running' : 'Idle' }}</b></div>
        <button class="pp-act" type="button" @click="wm.open('sync')">Open sync panel</button>
      </div>
      <div v-else-if="popover === 'vol'" class="pp-pop-body">
        <span class="pp-empty">Volume control is a placeholder in this build.</span>
      </div>
    </div>
  </footer>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useWindowManager, type ShellLayoutMode } from '@/composables/useWindowManager'
import { useTheme } from '@/composables/useTheme'
import { useKickoff } from '@/composables/useKickoff'
import { useNotifications } from '@/composables/useNotifications'
import { useLauncherStore } from '@/stores/launcher'
import { useContextMenu } from '@/composables/useContextMenu'
import { useTransport } from '@/composables/useTransport'
import { useAppStore } from '@/stores/app'
import { getServerUrl, isTauri } from '@/composables/useTauri'

const wm = useWindowManager()
const theme = useTheme()
const kickoff = useKickoff()
const store = useAppStore()
const ctx = useContextMenu()
const launcher = useLauncherStore()
const notifs = useNotifications()
const unreadCount = computed(() => notifs.unreadCount.value)
function openCenter() {
  popover.value = null
  launcher.setMessagesOpen(true, 'system')
}

const layouts: { id: ShellLayoutMode; label: string }[] = [
  { id: 'floating', label: 'Float' },
  { id: 'tiled', label: 'Tile' },
  { id: 'strip', label: 'Strip' },
  { id: 'overview', label: 'Grid' },
]

/** Floating (margin + radius) vs docked (edge-to-edge) panel. */
const docked = ref('0')
try {
  docked.value = localStorage.getItem('cybermanju_panel_docked') || '0'
} catch {
  docked.value = '0'
}

const transportLabel = computed(() => useTransport().label)
const endpointLabel = computed(() => {
  const url = getServerUrl()
  if (url) return url
  return isTauri() ? 'Tauri IPC' : 'localhost:3456'
})
const syncBusy = computed(() => store.shellBusy)

const popover = ref<null | 'clock' | 'net' | 'sync' | 'vol'>(null)
function togglePopover(name: NonNullable<typeof popover.value>) {
  popover.value = popover.value === name ? null : name
}
const popoverTitle = computed(() => {
  switch (popover.value) {
    case 'clock': return 'Calendar'
    case 'net': return 'Network / transport'
    case 'sync': return 'Sync'
    case 'vol': return 'Volume'
    default: return ''
  }
})

function toggleAppearance() {
  const light = theme.mode.value === 'light'
  const plasma = theme.themeId.value.startsWith('plasma')
  theme.setTheme(plasma ? (light ? 'plasma-dark' : 'plasma-light') : light ? 'os-dark' : 'os-light')
}

function activateWindow(id: string) {
  const win = wm.windows.value.find((w) => w.id === id)
  if (!win) return
  if (win.minimized) wm.restore(id)
  else wm.focus(id)
}

function onTaskAux(e: MouseEvent, id: string) {
  // Middle-click closes, like Plasma task manager.
  if (e.button === 1) {
    e.preventDefault()
    wm.close(id)
  }
}

function openTaskMenu(e: MouseEvent, id: string) {
  ctx.open(e, 'window_titlebar', {
    toggleMax: () => {},
    minimize: () => wm.minimize(id),
    close: () => wm.close(id),
    togglePin: () => {},
    pinned: false,
  })
}

// ── clock ──
const timeStr = ref('')
const dateStr = ref('')
const dateTitle = ref('')
let clockTimer: ReturnType<typeof setInterval> | null = null
const now = ref(new Date())
function updateClock() {
  const d = new Date()
  now.value = d
  timeStr.value = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  dateStr.value = d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })
  dateTitle.value = d.toLocaleDateString([], { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' })
}
const calTitle = computed(() =>
  now.value.toLocaleDateString([], { month: 'long', year: 'numeric' }),
)
const todayNum = computed(() => now.value.getDate())
const calDays = computed(() => {
  const y = now.value.getFullYear()
  const m = now.value.getMonth()
  return Array.from({ length: new Date(y, m + 1, 0).getDate() }, (_, i) => i + 1)
})
const calBlanks = computed(() => {
  const y = now.value.getFullYear()
  const m = now.value.getMonth()
  return (new Date(y, m, 1).getDay() + 6) % 7
})

onMounted(() => {
  updateClock()
  clockTimer = setInterval(() => { if (!document.hidden) updateClock() }, 1000)
})
onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer)
})
</script>

<style scoped>
.plasma-panel {
  position: relative;
  display: flex;
  align-items: center;
  gap: 4px;
  height: var(--ui-panel-h, 44px);
  min-height: 44px;
  margin: 0 8px 8px;
  padding: 0 8px;
  background: var(--ui-panel);
  backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xl);
  box-shadow: var(--ui-shadow-popup);
  z-index: 60;
  font-family: var(--ui-font);
  user-select: none;
}

.plasma-panel.docked {
  margin: 0;
  border-left: none;
  border-right: none;
  border-bottom: none;
  border-radius: 0;
}

.pp-launcher {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 34px;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-2);
  background: transparent;
  border: none;
  cursor: pointer;
  flex-shrink: 0;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.pp-launcher:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.pp-launcher.open {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
}

.pp-sep {
  width: 1px;
  align-self: stretch;
  margin: 8px 2px;
  background: var(--ui-separator);
  flex-shrink: 0;
}

.pp-pager {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.pp-page {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 3px 7px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  color: var(--ui-text-3);
  cursor: pointer;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.pp-page:hover {
  background: var(--ui-hover);
  color: var(--ui-text-2);
}

.pp-page.active {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.pp-page-box {
  width: 22px;
  height: 13px;
  border-radius: 2px;
  border: 1px solid currentColor;
}

.pp-page.active .pp-page-box {
  border-color: var(--ui-accent);
  background: color-mix(in srgb, var(--ui-accent) 25%, transparent);
}

.pp-page-label {
  font-size: 9px;
  font-weight: 400;
  line-height: 1;
}

.pp-tasks {
  display: flex;
  align-items: center;
  gap: 3px;
  flex: 1;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
  padding: 2px;
}

.pp-tasks::-webkit-scrollbar {
  display: none;
}

.pp-task {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  max-width: 210px;
  min-width: 0;
  padding: 0 10px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--ui-text-2);
  font-size: 13px;
  cursor: pointer;
  flex-shrink: 0;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.pp-task:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.pp-task.active {
  background: var(--ui-hover);
  color: var(--ui-text);
  border-bottom-color: var(--ui-accent);
}

.pp-task.minimized {
  opacity: 0.55;
}

.pp-task-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pp-tasks-empty {
  font-size: 12px;
  color: var(--ui-text-3);
  padding: 0 8px;
  white-space: nowrap;
}

.pp-spacer {
  flex: 1;
}

.pp-tray {
  display: flex;
  align-items: center;
  gap: 1px;
  flex-shrink: 0;
}

.pp-tray-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  color: var(--ui-text-2);
  cursor: pointer;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.pp-tray-btn:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.pp-tray-btn.active {
  color: var(--ui-accent);
}

.pp-badge {
  position: absolute;
  top: 2px;
  right: 2px;
  min-width: 14px;
  height: 14px;
  padding: 0 3px;
  border-radius: var(--ui-radius-full);
  background: var(--ui-accent);
  color: var(--ui-on-accent);
  font-size: 9px;
  font-weight: 600;
  line-height: 14px;
  text-align: center;
}

.pp-clock {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  justify-content: center;
  padding: 2px 8px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  cursor: pointer;
  flex-shrink: 0;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.pp-clock:hover {
  background: var(--ui-hover);
}

.pp-clock-time {
  font-size: 12px;
  font-weight: 600;
  color: var(--ui-text);
  line-height: 1.25;
}

.pp-clock-date {
  font-size: 11px;
  color: var(--ui-text-2);
  line-height: 1.25;
  white-space: nowrap;
}

.pp-pop {
  position: absolute;
  right: 8px;
  bottom: calc(100% + 8px);
  width: 300px;
  max-width: calc(100vw - 32px);
  background: var(--ui-panel);
  backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-popup);
  animation: ui-fade-in var(--ui-dur) ease-out both;
}

.pp-pop-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border-bottom: 1px solid var(--ui-separator);
}

.pp-pop-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--ui-text);
}

.pp-pop-x {
  display: inline-flex;
  padding: 4px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  color: var(--ui-text-3);
  cursor: pointer;
}

.pp-pop-x:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.pp-pop-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  font-size: 13px;
  color: var(--ui-text-2);
}

.pp-kv {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}

.pp-kv b {
  color: var(--ui-text);
  font-weight: 600;
  overflow-wrap: anywhere;
  text-align: right;
}

.pp-kv .mono {
  font-family: var(--ui-font-mono);
  font-size: 12px;
}

.pp-act {
  align-self: flex-start;
  padding: 4px 12px;
  min-height: 26px;
  border-radius: var(--ui-radius-sm);
  border: 1px solid var(--ui-border-strong);
  background: var(--ui-surface-2);
  color: var(--ui-text);
  font-size: 13px;
  cursor: pointer;
}

.pp-act:hover {
  border-color: var(--ui-accent);
}

.pp-empty {
  font-size: 12px;
  color: var(--ui-text-3);
}

.pp-cal {
  padding: 10px;
}

.pp-cal-head {
  font-size: 13px;
  font-weight: 600;
  color: var(--ui-text);
  margin-bottom: 8px;
}

.pp-cal-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 2px;
  text-align: center;
  font-size: 12px;
}

.pp-cal-dow {
  color: var(--ui-text-3);
  font-weight: 600;
  padding: 3px 0;
}

.pp-cal-day {
  padding: 4px 0;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-2);
}

.pp-cal-day.blank {
  visibility: hidden;
}

.pp-cal-day.today {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
  font-weight: 600;
}

@media (max-width: 768px) {
  .plasma-panel {
    display: none;
  }
}
</style>
