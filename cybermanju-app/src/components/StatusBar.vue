<!-- CyberManju OS — StatusBar (src/components/StatusBar.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: signals: AGENT-. -->
<template>
  <footer class="statusbar">
    <div class="sb-left">
      <span class="sb-path">{{ store.currentPath }}</span>
      <div v-if="store.isLoading" class="sb-progress-bar">
        <div class="sb-progress-fill" />
      </div>
    </div>

    <div class="sb-center">
      <span class="sb-item">{{ store.files.length }} FILES</span>
      <template v-if="store.selectedFile">
        <span class="sb-div">|</span>
        <span class="sb-item">SEL: {{ store.selectedFile.name }}</span>
      </template>

      <template v-if="store.selectedFile?.encrypted">
        <span class="sb-div">|</span>
        <span class="sb-badge">ENC {{ store.selectedFile.encryptionAlgorithm?.toUpperCase() || '' }}</span>
      </template>

      <template v-if="store.selectedFile?.compressionLayers && store.selectedFile.compressionLayers[0] && store.selectedFile.compressionLayers[0] !== 'none'">
        <span class="sb-div">|</span>
        <span class="sb-badge">{{ store.selectedFile.compressionLayers[0].toUpperCase() }}</span>
      </template>

      <template v-if="store.selectedFile?.hashBlake3">
        <span class="sb-div">|</span>
        <span class="sb-hash">B3:{{ store.selectedFile.hashBlake3.substring(0, 10) }}..</span>
      </template>
    </div>

    <div class="sb-right">
      <span
        class="sb-clickable job-icon"
        :class="{ 'sb-active': hasJob }"
        title="RUNNING TASKS (OPENS TASKS)"
        aria-label="RUNNING TASKS"
        @click="wm.open('processes')"
      >{{ jobLabel }}</span>
      <span class="sb-div">|</span>
      <span
        class="sb-clickable agent-icon"
        :class="{ 'sb-active': agentBusy, 'sb-wait': agentWaiting }"
        :title="agentTip"
        aria-label="AGENT STATUS (OPENS AGENT PANEL)"
        @click="wm.open('agent')"
      >{{ agentLabel }}</span>
      <span class="sb-div">|</span>
      <span
        class="sb-clickable sync-icon"
        :class="{ 'sb-active': isSyncActive }"
        title="SYNC STATUS"
        aria-label="SYNC STATUS"
      >{{ isSyncActive ? 'SYNC:' + (store.syncProgress?.status?.toUpperCase() ?? '…') : 'SYNC:IDLE' }}</span>
      <span class="sb-div">|</span>
      <span
        class="sb-clickable"
        :class="{ 'sb-active': store.matrixRainEnabled }"
        @click="store.matrixRainEnabled = !store.matrixRainEnabled"
        title="TOGGLE MATRIX RAIN"
        aria-label="TOGGLE MATRIX RAIN BACKGROUND"
      >{{ store.matrixRainEnabled ? 'GFX:ON' : 'GFX:OFF' }}</span>
      <span class="sb-div">|</span>
      <span class="sb-clickable" @click="store.commandPaletteOpen = true" title="Command palette (Ctrl+K)" aria-label="Open command palette">Ctrl+K</span>
      <span class="sb-div">|</span>
      <span class="sb-clickable" @click="store.showShortcutsHelp = true" title="Keyboard shortcuts (?) — all shortcuts" aria-label="Open keyboard shortcuts help">?</span>
      <span class="sb-div">|</span>
      <span
        class="sb-clickable"
        title="DEVICES (OPENS DEVICES)"
        aria-label="DEVICE STATUS"
        @click="wm.open('devices')"
      >{{ deviceLabel }}</span>
      <span class="sb-div">|</span>
      <span class="sb-tech" :title="presenceTitle">{{ presenceLabel }}</span>
      <span class="sb-div">|</span>
      <span class="sb-tech" :title="netTitle">{{ netLabel }}</span>
      <span class="sb-div">|</span>
      <span class="sb-tech">{{ transportShort }} MODE</span>
      <span class="sb-div">|</span>
      <a class="sb-legal" :href="legalPageUrl('privacy.html')" target="_blank" rel="noopener" title="Privacy policy">PRIVACY</a>
      <span class="sb-div">|</span>
      <a class="sb-legal" :href="legalPageUrl('terms.html')" target="_blank" rel="noopener" title="Terms of service">TERMS</a>
    </div>
  </footer>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { useTransport } from '@/composables/useTransport'
import { useWindowManager } from '@/composables/useWindowManager'
import { useSystemHardware } from '@/composables/useSystemHardware'
import { legalPageUrl } from '@/utils/legalPageUrl'

const store = useAppStore()
const wm = useWindowManager()
const hw = useSystemHardware()

// 3-state transport tag (tauri | wasm | rest) — single source of truth.
const transportShort = computed(() => useTransport().short)

// Live device / power / network readout (VueUse-backed, shared subscription).
const deviceLabel = computed(() => {
  const n = hw.pluggedDevices.value.length
  const batt = hw.battery.isSupported.value ? ` ${Math.round((hw.battery.level.value ?? 0) * 100)}%${hw.battery.charging.value ? '+' : ''}` : ''
  return `DEV:${n}${batt}`
})
const netLabel = computed(() => {
  if (!hw.online.value) return 'OFFLINE'
  return hw.network.effectiveType.value ? hw.network.effectiveType.value.toUpperCase() : 'ONLINE'
})
const netTitle = computed(() => `type ${hw.network.type.value}, rtt ${hw.network.rtt.value ?? '—'}ms, downlink ${hw.network.downlink.value ?? '—'}Mbps`)
// Presence: idle / hidden tab at a glance (VueUse useIdle + visibility).
const presenceLabel = computed(() => {
  if (hw.idle.idle.value) return 'AWAY'
  return hw.visibility.value === 'visible' ? (hw.focused.value ? 'HERE' : 'BG') : 'HIDDEN'
})
const presenceTitle = computed(() => `idle ${hw.idle.idle.value ? 'yes' : 'no'}, visibility ${hw.visibility.value}, focused ${hw.focused.value ? 'yes' : 'no'}`)

// AGENT-8 item 11: the bar shows background work — a `cybsh` line in flight
// or any task running in the table — and jumps to the task list on click.
// NOTE: `counts` itself is optional-chained: a malformed/shapeless `os_ps`
// payload must render 0, never throw `Cannot read properties of undefined`.
const runningTasks = computed(() => store.osPs?.counts?.running ?? 0)
const hasJob = computed(() => store.shellBusy || runningTasks.value > 0)
const jobLabel = computed(() => {
  if (store.shellBusy) return 'CYBSH:BUSY'
  return `JOB:${runningTasks.value}`
})

const pollMs = 4000
let poll = 0

onMounted(() => {
  void store.fetchOsPs()
  poll = window.setInterval(() => {
    // Hidden/minimized app: no one can read the bar — skip the Rust call and
    // the reactive re-render; the next visible tick refreshes it anyway.
    if (document.hidden) return
    void store.fetchOsPs()
  }, pollMs)
})

onBeforeUnmount(() => {
  if (poll) window.clearInterval(poll)
})

const isSyncActive = computed(() => {
  const status = store.syncProgress?.status
  if (!status) return false
  return (
    status === 'scanning' ||
    status === 'compressing' ||
    status === 'uploading' ||
    status === 'linking' ||
    status === 'cleaning' ||
    status === 'syncing'
  )
})

// The agent is the one background actor that can be *blocked on the user* —
// surface that distinctly, and jump straight to the panel on click.
const agentJob = computed(() => store.activeAgentJob)
const agentStatus = computed(() => agentJob.value?.status ?? '')
const agentWaiting = computed(() => agentStatus.value === 'waiting_approval')
const agentBusy = computed(() => agentStatus.value === 'running' || agentWaiting.value)
const agentLabel = computed(() => {
  if (agentWaiting.value) return 'AGENT:WAIT'
  if (agentStatus.value === 'running') {
    return `AGENT:${agentJob.value?.turnsUsed ?? 0}/${agentJob.value?.maxTurns ?? 0}`
  }
  if (agentStatus.value) return `AGENT:${agentStatus.value.slice(0, 7).toUpperCase()}`
  return 'AGENT:IDLE'
})
const agentTip = computed(() => {
  if (agentWaiting.value) return 'AGENT NEEDS APPROVAL (OPENS AGENT)'
  const act = agentJob.value?.activity
  return act ? `AGENT — ${act} (OPENS AGENT)` : 'AGENT (OPENS AGENT)'
})
</script>

<style scoped>
.statusbar {
  display: flex;
  align-items: center;
  height: 26px;
  padding: 0 10px;
  gap: 8px;
  background: var(--ui-glass);
  backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  border-top: 1px solid var(--ui-border);
  box-shadow: 0 -1px 0 var(--ui-glass-highlight);
  font-size: 10px;
  overflow: hidden;
  z-index: 10;
  font-family: var(--ui-font-mono);
  color: var(--ui-text-3);
}

.sb-left {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  min-width: 0;
}

.sb-path {
  font-size: var(--ui-fs-xs);
  font-weight: 600;
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--ui-text-2);
  padding: 1px 8px;
  border-radius: var(--ui-radius-full);
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  border: 1px solid var(--ui-hairline);
}

.sb-progress-bar {
  width: 64px;
  height: 5px;
  border: 1px solid color-mix(in srgb, var(--ui-accent) 40%, transparent);
  border-radius: var(--ui-radius-full);
  overflow: hidden;
  background: color-mix(in srgb, var(--ui-accent) 10%, transparent);
}

.sb-progress-fill {
  height: 100%;
  width: 40%;
  border-radius: inherit;
  background: var(--ui-accent);
  animation: sb-progress 1.2s ease-in-out infinite;
}

@keyframes sb-progress {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(260%); }
}

.sb-center {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  overflow: hidden;
}

.sb-item {
  white-space: nowrap;
  color: var(--ui-text-3);
  font-size: var(--ui-fs-xs);
}

.sb-div {
  color: var(--ui-text-faint);
  opacity: 0.6;
}

.sb-badge {
  font-weight: 600;
  font-size: 10px;
  color: var(--ui-text-2);
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  border: 1px solid var(--ui-border);
  padding: 1px 6px;
  border-radius: var(--ui-radius-full);
}

.sb-hash {
  font-size: 10px;
  color: var(--ui-text-3);
}

.sb-right {
  margin-left: auto;
  display: flex;
  align-items: center;
}

.sb-tech {
  font-size: 10px;
  color: var(--ui-text-faint);
  padding-left: 8px;
}

/* Legal links (privacy / terms) — same footprint as the tech readouts; the
 * URL helper keeps Pages previews scoped while root/native builds stay canonical. */
.sb-legal {
  font-size: 10px;
  color: var(--ui-text-faint);
  padding: 2px 2px 2px 8px;
  text-decoration: none;
  white-space: nowrap;
}

.sb-legal:hover {
  color: var(--ui-text-2);
  text-decoration: underline;
}

.sb-clickable {
  cursor: pointer;
  color: var(--ui-text-3);
  font-size: 9px;
  letter-spacing: 0.06em;
  padding: 2px 7px;
  margin: 0 3px;
  border-radius: var(--ui-radius-full);
  border: 1px solid transparent;
  background: transparent;
  transition:
    color var(--ui-dur-fast) var(--ui-ease-out),
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out);
}

.sb-clickable:hover {
  color: var(--ui-text);
  background: var(--ui-accent-softer);
  border-color: color-mix(in srgb, var(--ui-accent) 30%, transparent);
  text-decoration: none;
}

.sb-active {
  color: var(--ui-accent);
  font-weight: 700;
  background: var(--ui-accent-softer);
  border-color: color-mix(in srgb, var(--ui-accent) 34%, transparent);
  animation: bw-pulse 2.2s ease-in-out infinite;
}

.job-icon {
  font-size: 9px;
}

.agent-icon {
  font-size: 9px;
}

/* Blocked on the human, not just busy — louder than the running state. */
.sb-wait {
  color: var(--ui-warning);
  font-weight: 700;
  background: color-mix(in srgb, var(--ui-warning) 16%, transparent);
  border-color: color-mix(in srgb, var(--ui-warning) 45%, transparent);
  animation: bw-pulse 1.4s ease-in-out infinite;
}

@media (max-width: 768px) {
  .sb-center {
    display: none;
  }
  .sb-path {
    max-width: 120px;
  }
}
</style>
