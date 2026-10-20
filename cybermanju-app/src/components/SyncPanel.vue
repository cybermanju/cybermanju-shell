<!-- CyberManju OS — SyncPanel (src/components/SyncPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="sync-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-sync"><AppIcon name="solar:refresh-bold" /></span>
        <h2 class="panel-title">Sync</h2>
      </div>
      <UiButton size="sm" icon="solar:cloud-bold" @click="openProviders">
        Providers
      </UiButton>
    </div>

    <div class="section providers-note">
      <p class="text-muted hint">Provider connections live in Accounts → Connections. This panel starts runs, watches progress and restores files.</p>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:settings-minimalistic-bold" :size="13" /> Configurations ({{ syncConfigs.length }})</h3>
      <div class="config-list">
        <div v-for="cfg in syncConfigs" :key="cfg.id" class="config-card">
          <div class="cfg-header">
            <span class="cfg-name">{{ cfg.name || cfg.backendType }}</span>
            <span class="cfg-type text-muted">{{ cfg.backendType }}</span>
            <UiBadge :tone="cfg.enabled ? 'accent' : 'neutral'" size="sm">{{ cfg.enabled ? 'On' : 'Off' }}</UiBadge>
          </div>
          <div class="cfg-meta text-muted">
            <span v-if="cfg.basePath">{{ cfg.basePath }}</span>
            <span v-if="cfg.repoName">{{ cfg.repoName }}</span>
            <span v-if="cfg.placement"> · {{ cfg.placement }}</span>
          </div>
          <div class="cfg-actions">
            <UiButton size="xs" @click="testCfg(cfg)">Test</UiButton>
            <UiButton class="sync-primary-action" size="xs" variant="primary" @click="startCfg(cfg)">Start</UiButton>
            <UiButton v-if="isOauthCapable(cfg.backendType)" size="xs" @click="oauthConnectCfg(cfg)">Connect</UiButton>
            <UiButton size="xs" @click="usageCfg(cfg)">Quota</UiButton>
          </div>
          <div v-if="quotaMsg[cfg.id]" class="cfg-msg">{{ quotaMsg[cfg.id] }}</div>
        </div>
      </div>
      <UiEmpty
        v-if="!syncConfigs.length"
        size="sm"
        icon="solar:cloud-storage-bold"
        title="No providers connected"
        description="Add one in Accounts → Connections, then come back to run it."
      >
        <template #actions>
          <UiButton size="sm" icon="solar:cloud-bold" @click="openProviders">Open Accounts</UiButton>
        </template>
      </UiEmpty>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:play-bold" :size="13" /> Start &amp; monitor</h3>
      <div class="w-row">
        <div class="w-field grow">
          <UiSelect
            aria-label="Sync configuration to run"
            :model-value="runConfigId"
            :options="runConfigOptions"
            @update:model-value="runConfigId = $event"
          />
        </div>
        <UiButton class="sync-primary-action" size="sm" :disabled="!runConfigId || busy" @click="startRun">Start</UiButton>
        <UiButton size="sm" @click="cancelRun">Cancel</UiButton>
        <UiButton size="sm" @click="refreshRuns">Refresh</UiButton>
      </div>
      <div v-if="jobMsg" class="w-msg">{{ jobMsg }}</div>
    </div>

    <div class="section" v-if="syncProgress">
      <h3 class="section-title"><AppIcon name="solar:speedometer-max-bold" :size="13" /> Progress</h3>
      <div class="progress-card">
        <div class="p-row"><span class="p-key text-muted">Status</span><span class="p-value">{{ syncProgress.status }}</span></div>
        <div class="p-row"><span class="p-key text-muted">Files</span><span class="p-value">{{ syncProgress.processedFiles }}/{{ syncProgress.totalFiles }}</span></div>
        <div class="p-row"><span class="p-key text-muted">Sent</span><span class="p-value">{{ humanBytes(syncProgress.bytesUploaded) }}</span></div>
        <div v-if="syncProgress.errors.length" class="p-errors">
          <div v-for="(e, i) in syncProgress.errors.slice(0, 5)" :key="i" class="p-err" :title="hintFor(e)">{{ e }}</div>
        </div>
      </div>
    </div>

    <div v-if="syncRuns.length" class="section">
      <h3 class="section-title"><AppIcon name="solar:history-bold" :size="13" /> Recent runs ({{ syncRuns.length }})</h3>
      <div v-for="r in syncRuns.slice(0, 5)" :key="r.runId" class="run-card">
        <span class="text-muted">{{ r.runId.slice(0, 18) }}</span>
        <span>{{ r.status }}</span>
        <span class="text-muted">{{ r.filesSynced }} files / {{ humanBytes(r.bytesUploaded) }}</span>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:branching-paths-up-bold" :size="13" /> Conflicts ({{ store.opConflicts.length }})</h3>
      <p class="text-muted hint">Fork pairs from the op-log exchange — resolving appends a <span class="mono">resolve</span> op, history is never deleted. Queue-and-replay: pull <span class="mono">since</span>, push buffered rows, render here.</p>
      <div class="w-actions">
        <UiButton size="sm" @click="store.fetchOpConflicts()">Refresh conflicts</UiButton>
      </div>
      <UiEmpty
        v-if="!store.opConflicts.length"
        size="sm"
        icon="solar:check-circle-bold"
        title="No conflicts"
        description="Devices agree — every fork dominates or touches a different file."
      />
      <div v-for="(pair, i) in store.opConflicts.slice(0, 10)" :key="`${pair.a.opId}:${pair.b.opId}`" class="conflict-card">
        <div class="cfg-header">
          <span class="cfg-name">{{ conflictTitle(pair) }}</span>
        </div>
        <div class="cfg-meta text-muted">{{ conflictSummary(pair) }}</div>
        <div class="cfg-actions">
          <UiButton size="xs" @click="store.resolveOpConflict(pair.a.opId, pair.b.opId, 'keep-a')">Keep first</UiButton>
          <UiButton size="xs" @click="store.resolveOpConflict(pair.a.opId, pair.b.opId, 'keep-b')">Keep second</UiButton>
          <UiButton size="xs" variant="primary" @click="store.resolveOpConflict(pair.a.opId, pair.b.opId, 'merge')">Merge</UiButton>
        </div>
        <div v-if="i === 9 && store.opConflicts.length > 10" class="cfg-meta text-muted">+ {{ store.opConflicts.length - 10 }} more — resolve these first.</div>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:transfer-horizontal-bold" :size="13" /> Move (single-copy home A→B)</h3>
      <div class="w-field">
        <UiInput v-model="moveFileId" placeholder="File ID" aria-label="Move file id" />
      </div>
      <div class="w-row">
        <div class="w-field grow">
          <UiSelect
            aria-label="Move source configuration"
            :model-value="moveFromId"
            :options="runConfigOptions"
            @update:model-value="moveFromId = $event"
          />
        </div>
        <div class="w-field grow">
          <UiSelect
            aria-label="Move destination configuration"
            :model-value="moveToId"
            :options="runConfigOptions"
            @update:model-value="moveToId = $event"
          />
        </div>
        <UiButton size="sm" variant="primary" :disabled="!moveFileId || !moveFromId || !moveToId || busy" @click="doMove">Move</UiButton>
      </div>
      <div v-if="moveMsg" class="w-msg">{{ moveMsg }}</div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:undo-left-round-bold" :size="13" /> Restore</h3>
      <div class="w-field">
        <UiInput v-model="restoreFileId" placeholder="File ID (optional if path given)" aria-label="File id" />
      </div>
      <div class="w-field">
        <UiInput v-model="restoreRemotePath" placeholder="Remote path" aria-label="Remote path" />
      </div>
      <div class="w-actions">
        <UiButton size="sm" :disabled="!runConfigId" @click="doRestore">Restore</UiButton>
        <UiButton size="sm" variant="danger" :disabled="!runConfigId || !restoreRemotePath" @click="doRemoteDelete">Delete remote</UiButton>
        <UiButton size="sm" :disabled="!runConfigId" @click="browseRemote">Browse</UiButton>
      </div>
      <div v-if="remoteFiles.length" class="remote-list">
        <div v-for="f in remoteFiles.slice(0, 20)" :key="f.path" class="remote-row">
          <span>{{ f.name }}</span><span class="text-muted">{{ humanBytes(f.sizeBytes) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiBadge from '@/components/ui/UiBadge.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { describeSyncError, isOauthCapable } from '@/types'
import type { SyncConfig } from '@/types'
import { humanBytes } from '@/utils/format'
import { conflictSummary, conflictTitle } from '@/utils/oplog'

const store = useAppStore()
const wm = useWindowManager()
const syncConfigs = computed(() => store.syncConfigs)
const syncProgress = computed(() => store.syncProgress)
const syncRuns = computed(() => store.syncRuns)

const busy = ref(false)
const jobMsg = ref('')
const quotaMsg = ref<Record<string, string>>({})
const runConfigId = ref('')
const restoreFileId = ref('')
const restoreRemotePath = ref('')
const remoteFiles = ref<{ name: string; path: string; sizeBytes: number }[]>([])
const moveFileId = ref('')
const moveFromId = ref('')
const moveToId = ref('')
const moveMsg = ref('')

onMounted(() => {
  store.fetchOpConflicts().catch(() => {})
})

/** Provider connections live in Accounts now — this panel runs them. */
function openProviders() {
  wm.open('accounts', { tab: 'connections' })
}

const runConfigOptions = computed(() => [
  { label: 'SELECT CONFIG', value: '' },
  ...syncConfigs.value.map(c => ({ label: c.name || c.backendType, value: c.id })),
])

// A provider deleted in Accounts → Connections must not linger here as a
// selected run target (or as quota/remote state): START would fire against a
// dead id. Reset + prune the moment the config list loses the id.
watch(
  () => syncConfigs.value.map(c => c.id).join(','),
  () => {
    const live = new Set(syncConfigs.value.map(c => c.id))
    if (runConfigId.value && !live.has(runConfigId.value)) {
      runConfigId.value = ''
      remoteFiles.value = []
      jobMsg.value = 'Selected provider was deleted in Accounts — pick another config.'
    }
    for (const id of Object.keys(quotaMsg.value)) {
      if (!live.has(id)) delete quotaMsg.value[id]
    }
  },
)

function hintFor(e: string) {
  const d = describeSyncError(e)
  return `${d.prefix}: ${d.hint}`
}

async function oauthConnectCfg(cfg: SyncConfig) {
  // Single verified OAuth path: Accounts → Connections owns the popup, the
  // provider-token match check, the save and the CONNECTED probe. Firing
  // oauth_start from here used to open a bare tab with no verification.
  runConfigId.value = cfg.id
  store.notifySuccess('Opening Accounts → Connections — finish OAuth on that card')
  openProviders()
}

async function testCfg(cfg: SyncConfig) {
  await store.testSyncConnection(cfg)
}

async function startCfg(cfg: SyncConfig) {
  runConfigId.value = cfg.id
  await startRun()
}

async function startRun() {
  if (!runConfigId.value) return
  jobMsg.value = 'Starting… (202 job, polling progress)'
  await store.startSync(runConfigId.value, [])
  await store.fetchSyncRuns()
  jobMsg.value = ''
}

async function cancelRun() {
  await store.cancelSync()
}

async function refreshRuns() {
  await store.fetchSyncRuns()
  await store.fetchSyncStatus()
}

async function usageCfg(cfg: SyncConfig) {
  const u = await store.fetchSyncUsage(cfg.id)
  quotaMsg.value[cfg.id] = u ? `quota: ${JSON.stringify(u).slice(0, 160)}` : 'quota unavailable'
}

async function doRestore() {
  if (!runConfigId.value) return
  await store.restoreSyncFile(runConfigId.value, restoreFileId.value || undefined, restoreRemotePath.value || undefined)
}

async function doRemoteDelete() {
  if (!runConfigId.value || !restoreRemotePath.value) return
  await store.deleteRemoteFile(runConfigId.value, restoreRemotePath.value)
}

async function doMove() {
  if (!moveFileId.value || !moveFromId.value || !moveToId.value) return
  busy.value = true
  moveMsg.value = 'Moving… (download A → upload B → verify → delete A)'
  const out = await store.moveSyncFile(moveFileId.value.trim(), moveFromId.value, moveToId.value)
  moveMsg.value = out ? (out.noop ? 'Already home — nothing moved.' : `Moved ${out.bytes} bytes, verified. Home is now ${moveToId.value}.`) : ''
  busy.value = false
}

async function browseRemote() {
  const cfg = syncConfigs.value.find(c => c.id === runConfigId.value)
  if (!cfg) return
  remoteFiles.value = await store.listRemoteFiles(cfg, restoreRemotePath.value || '')
}
</script>

<style scoped>
.sync-panel {
  width: 100%;
  height: 100%;
  background: var(--ui-surface);
  overflow-y: auto;
  padding: 16px max(16px, env(safe-area-inset-right)) 16px max(16px, env(safe-area-inset-left));
  font-family: var(--ui-font);
  color: var(--ui-text);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ui-border);
  margin-bottom: 16px;
}

.header-left { display: flex; align-items: center; gap: 8px; }
.icon-sync { font-size: 16px; }
.panel-title { font-size: 13px; font-weight: 600; margin: 0; }

.section { margin-bottom: 16px; }

.section-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--ui-text);
  margin: 0 0 8px;
}

.providers-note { margin-bottom: 12px; }
.hint { font-size: 10px; margin: 0; line-height: 1.5; }

.w-field { display: flex; flex-direction: column; gap: 4px; margin-bottom: 8px; min-width: 0; }
.w-field.grow { flex: 1; }
.w-label { font-size: 10px; font-weight: 700; letter-spacing: 0.06em; }
.w-row { display: flex; gap: 8px; margin-bottom: 8px; flex-wrap: wrap; }
.w-row.checks { align-items: center; gap: 14px; padding: 2px 0; }
.w-row .w-field { flex: 1; min-width: 140px; }
.w-actions { display: flex; gap: 8px; flex-wrap: wrap; }
.w-msg, .cfg-msg { font-size: 10px; margin-top: 6px; color: color-mix(in srgb, var(--ui-text) 70%, transparent); }

.config-list { display: flex; flex-direction: column; gap: 6px; }
.config-card {
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 8px 10px;
  background: color-mix(in srgb, var(--ui-glass) 55%, transparent);
  backdrop-filter: blur(calc(var(--ui-blur) * 0.4));
  -webkit-backdrop-filter: blur(calc(var(--ui-blur) * 0.4));
}
.cfg-header { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; }
.cfg-name { font-size: 12px; font-weight: 700; flex: 1; min-width: 0; overflow-wrap: anywhere; }
.cfg-type { font-size: 9px; min-width: 0; overflow-wrap: anywhere; }
.cfg-meta { font-size: 9px; overflow-wrap: anywhere; word-break: break-word; }
.cfg-actions { display: flex; gap: 6px; margin-top: 6px; flex-wrap: wrap; }

.progress-card {
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  background: color-mix(in srgb, var(--ui-glass) 55%, transparent);
}
.p-row { display: flex; justify-content: space-between; gap: 12px; min-width: 0; }
.p-key { font-size: 10px; }
.p-value { font-size: 10px; font-weight: 700; min-width: 0; text-align: right; overflow-wrap: anywhere; }
.p-errors { display: flex; flex-direction: column; gap: 2px; }
.p-err { font-size: 9px; color: var(--ui-danger); word-break: break-all; }
.run-card {
  display: flex;
  gap: 8px;
  font-size: 10px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  padding: 4px 6px;
  margin-bottom: 4px;
  background: var(--ui-glass);
}
.conflict-card {
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 8px 10px;
  margin-top: 6px;
  background: color-mix(in srgb, var(--ui-glass) 55%, transparent);
}
.mono { font-family: var(--ui-mono, monospace); font-size: 0.95em; }
.remote-list { margin-top: 6px; }
.remote-row { display: flex; justify-content: space-between; gap: 8px; min-width: 0; font-size: 10px; border-bottom: 1px solid var(--ui-hairline); padding: 2px 0; }
.remote-row > :first-child { min-width: 0; overflow-wrap: anywhere; }
.text-muted { color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important; }

/* Phones: keep each interaction group calm, stacked and comfortably tappable.
   The desktop arrangement above remains the default. */
@media (max-width: 600px) {
  .sync-panel {
    padding-top: max(12px, env(safe-area-inset-top));
    padding-bottom: max(16px, env(safe-area-inset-bottom));
  }

  .panel-header { align-items: center; gap: 12px; }
  .panel-header > :last-child { flex: 0 0 auto; }
  .sync-panel :deep(.ui-btn) { min-height: 44px; }
  .section { margin-bottom: 20px; }
  .config-card, .progress-card { border-radius: var(--ui-radius-lg, var(--ui-radius-md)); padding: 12px; }
  .cfg-header { align-items: flex-start; flex-wrap: wrap; }
  .cfg-name { flex: 1 1 100%; font-size: 13px; }
  .cfg-type { flex: 1 1 auto; }
  .cfg-actions, .w-actions { gap: 8px; }
  .cfg-actions :deep(.ui-btn), .w-actions :deep(.ui-btn), .w-row > :deep(.ui-btn) {
    min-height: 44px;
  }
  .cfg-actions :deep(.ui-btn) { flex: 1 1 calc(50% - 4px); }
  .cfg-actions .sync-primary-action { order: -1; }
  .w-row { flex-direction: column; gap: 8px; }
  .w-row .w-field, .w-row .w-field.grow { flex: 1 1 auto; width: 100%; min-width: 0; }
  .w-row > :deep(.ui-btn), .w-actions :deep(.ui-btn) { width: 100%; }
  .w-row .sync-primary-action { order: -1; }
  .p-row { align-items: baseline; }
  .run-card { align-items: flex-start; flex-wrap: wrap; padding: 8px; line-height: 1.35; }
  .run-card > :first-child { flex: 1 1 100%; overflow-wrap: anywhere; }
  .remote-row { padding: 6px 0; }
  .section-title { line-height: 1.4; }
}

@media (prefers-reduced-motion: reduce) {
  .sync-panel *, .sync-panel *::before, .sync-panel *::after { scroll-behavior: auto !important; transition-duration: 0.001ms !important; animation-duration: 0.001ms !important; }
}
</style>
