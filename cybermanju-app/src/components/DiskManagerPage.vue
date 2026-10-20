<script setup lang="ts">
// CyberManju OS — DiskManagerPage: disk/volume manager (AGENT-8 item 9).
// Role: one merged `df` bar over every `.cybermanju` disk + per-provider cards (attach/detach/resize/check).
// Exports: module root (see items below).
// Integrates: `/api/os/df` merged bar, AGENT-6 catalog rows, @/…, vue.
// Contracts: signals: AGENT-.
import AppIcon from '@/components/AppIcon.vue'
import BackupPanel from '@/components/BackupPanel.vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { humanBytes, diskPct } from '@/utils/format'
import { dedupSummary, fromFileNodes, groupDuplicates } from '@/utils/dedup'

const store = useAppStore()

// Merged storage window: `storage` opens here on the overview tab.
// `wm.open('disks', { tab: 'disks', diskId })` focuses one card — FileManager
// "Manage disk" steers here so the right disk is highlighted + scrolled to.
// `wm.open('disks', { tab: 'backup' })` lands on snapshot exports + restore.
// `wm.open('disks', { tab: 'diet' })` lands on the dedup report (ANALYSIS §4.5).
const props = defineProps<{ tab?: string; diskId?: string; highlightId?: string }>()
type DiskView = 'overview' | 'disks' | 'backup' | 'diet'
const view = ref<DiskView>(props.tab === 'backup' ? 'backup' : props.tab === 'overview' ? 'overview' : props.tab === 'diet' ? 'diet' : 'disks')
watch(() => props.tab, (t) => { if (t === 'overview' || t === 'disks' || t === 'backup' || t === 'diet') view.value = t })
const focusDiskId = computed(() => props.highlightId || props.diskId || '')
watch(focusDiskId, (id) => {
  if (!id) return
  view.value = 'disks'
  requestAnimationFrame(() => {
    document.getElementById(`disk-card-${id}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  })
})

const configId = ref('')
const createSizeMb = ref(512)
const passphrase = ref('')
const creating = ref(false)
const resizeTarget = ref<string | null>(null)
const resizeMb = ref(512)
const busyId = ref<string | null>(null)
const note = ref('')

// A provider deleted in Accounts must not stay selected here — CREATE would
// fire against a dead config id. Fall back to empty the moment it vanishes.
watch(
  () => store.syncConfigs.map(c => c.id).join(','),
  () => {
    if (configId.value && !store.syncConfigs.some(c => c.id === configId.value)) {
      configId.value = ''
      note.value = 'Selected provider was deleted in Accounts — pick another configuration.'
    }
  },
)

/** Provider ids still alive — disks whose config was deleted stay in the
 * catalog by design, so mark them instead of showing a raw dead id. */
const liveConfigIds = computed(() => new Set(store.syncConfigs.map(c => c.id)))
function isOrphanDisk(diskConfigId: string): boolean {
  return !!diskConfigId && !liveConfigIds.value.has(diskConfigId)
}

const MIN_MB = 64
const MAX_MB = 8192

const df = computed(() => store.osDf)
const usedPct = computed(() => {
  const d = df.value
  if (!d || d.totalBytes === 0) return 0
  return Math.min(100, (d.usedBytes / d.totalBytes) * 100)
})

async function refresh() {
  await Promise.all([store.fetchDisks(), store.fetchOsDf(), store.fetchFiles(), store.fetchTrashItems()])
  if (store.syncConfigs.length === 0) await store.fetchSyncConfigs()
}

async function create() {
  if (!configId.value) {
    note.value = 'create: pick a provider configuration first'
    return
  }
  creating.value = true
  const created = await store.createDisk(configId.value, createSizeMb.value * 1024 * 1024, passphrase.value)
  creating.value = false
  note.value = created ? `created ${created.id}` : ''
  if (created) passphrase.value = ''
}

async function attach(diskId: string) {
  busyId.value = diskId
  await store.attachDisk(diskId, passphrase.value)
  busyId.value = null
}

async function detach(diskId: string) {
  busyId.value = diskId
  await store.detachDisk(diskId)
  busyId.value = null
}

function beginResize(diskId: string, capacityBytes: number) {
  resizeTarget.value = diskId
  resizeMb.value = Math.max(MIN_MB, Math.min(MAX_MB, Math.round(capacityBytes / (1024 * 1024))))
}

async function applyResize(diskId: string) {
  await store.resizeDisk(diskId, resizeMb.value * 1024 * 1024)
  resizeTarget.value = null
}

async function check(diskId: string) {
  busyId.value = diskId
  await store.checkDisk(diskId)
  busyId.value = null
}

async function makeKeyHolder(diskId: string) {
  busyId.value = diskId
  await store.setDiskKeyHolder(diskId)
  busyId.value = null
}

/* ── overview stats (merged storage dashboard) ── */
const trashCount = computed(() => store.trashItems.length)
const totalSize = computed(() => store.files.reduce((s, f) => s + f.sizeBytes, 0))
const totalSizeFormatted = computed(() => humanBytes(totalSize.value))
const largestFile = computed(() => {
  if (store.files.length === 0) return '--'
  const biggest = [...store.files].sort((a, b) => b.sizeBytes - a.sizeBytes)[0]
  return `${biggest.name} (${humanBytes(biggest.sizeBytes)})`
})
const avgSizeFormatted = computed(() => {
  if (store.files.length === 0) return '--'
  return humanBytes(Math.round(totalSize.value / store.files.length))
})
const gpsCount = computed(() => store.files.filter(f => f.gpsLat).length)
const faceCount = computed(() => store.files.filter(f => f.faceGroupIds && f.faceGroupIds.length > 0).length)
const byType = computed(() => {
  const groups: Record<string, { totalBytes: number; count: number }> = {}
  for (const f of store.files) {
    const type = f.mimeType?.split('/')[0] || f.fileType || 'unknown'
    if (!groups[type]) groups[type] = { totalBytes: 0, count: 0 }
    groups[type].totalBytes += f.sizeBytes
    groups[type].count++
  }
  const total = totalSize.value
  const entries = Object.entries(groups).map(([label, data]) => ({
    label: label.toUpperCase(),
    totalBytes: data.totalBytes,
    count: data.count,
    percent: total > 0 ? (data.totalBytes / total) * 100 : 0,
  }))
  entries.sort((a, b) => b.totalBytes - a.totalBytes)
  return entries
})

/* ── storage diet / dedup report (ANALYSIS.md §4.5) ── */
// BLAKE3 hashes already ride on every FileNode; grouping is pure frontend.
// Reclaim deletes all but the newest copy via the standard delete path.
const dupGroups = computed(() => groupDuplicates(fromFileNodes(store.files)))
const dupSummary = computed(() => dedupSummary(fromFileNodes(store.files), dupGroups.value))
const reclaimArmed = ref<string | null>(null)
const reclaiming = ref(false)
const dietNote = ref('')

async function reclaimGroup(hash: string, keepId: string, victimIds: string[]) {
  if (reclaimArmed.value !== hash) {
    reclaimArmed.value = hash
    dietNote.value = ''
    return
  }
  reclaimArmed.value = null
  reclaiming.value = true
  let removed = 0
  for (const id of victimIds) {
    if (id === keepId) continue
    await store.deleteFile(id)
    removed += 1
  }
  reclaiming.value = false
  dietNote.value = removed > 0 ? `reclaimed ${removed} duplicate${removed === 1 ? '' : 's'} (kept newest)` : ''
  await store.fetchFiles()
}

onMounted(async () => {
  await refresh()
  if (focusDiskId.value) {
    view.value = 'disks'
    requestAnimationFrame(() => {
      document.getElementById(`disk-card-${focusDiskId.value}`)?.scrollIntoView({ block: 'nearest' })
    })
  }
})
</script>

<template>
  <div class="disk-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-disks"><AppIcon name="solar:ssd-square-bold" /></span>
        <h2 class="panel-title">DISKS &amp; VOLUME</h2>
        <span class="text-muted">{{ df ? `${df.diskCount} DISKS` : '…' }}</span>
      </div>
      <div class="header-right">
        <button class="ghost-btn" type="button" @click="refresh">REFRESH</button>
      </div>
    </div>

    <div class="dm-tabs" role="tablist" aria-label="Disks views">
      <button role="tab" :aria-selected="view === 'overview'" :class="{ on: view === 'overview' }" type="button" @click="view = 'overview'">OVERVIEW</button>
      <button role="tab" :aria-selected="view === 'disks'" :class="{ on: view === 'disks' }" type="button" @click="view = 'disks'">DISKS</button>
      <button role="tab" :aria-selected="view === 'backup'" :class="{ on: view === 'backup' }" type="button" @click="view = 'backup'">BACKUP</button>
      <button role="tab" :aria-selected="view === 'diet'" :class="{ on: view === 'diet' }" type="button" @click="view = 'diet'">DIET</button>
    </div>

    <!-- ══ OVERVIEW (merged storage dashboard) ══ -->
    <template v-if="view === 'overview'">
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:checklist-bold" :size="13" /> FILE COUNTS</h3>
      <div class="stats-grid">
        <div class="stat-card">
          <span class="stat-value">{{ store.files.length }}</span>
          <span class="stat-label text-muted">TOTAL FILES</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ store.folders.length }}</span>
          <span class="stat-label text-muted">FOLDERS</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ store.encryptedFiles.length }}</span>
          <span class="stat-label text-muted">ENCRYPTED</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ store.compressedFiles.length }}</span>
          <span class="stat-label text-muted">COMPRESSED</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ store.starredFiles.length }}</span>
          <span class="stat-label text-muted">STARRED</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ trashCount }}</span>
          <span class="stat-label text-muted">IN TRASH</span>
        </div>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:ruler-bold" :size="13" /> TOTAL BY TYPE</h3>
      <div class="type-breakdown">
        <div v-for="entry in byType" :key="entry.label" class="type-row">
          <span class="type-label">{{ entry.label }}</span>
          <span class="type-bar"><span class="type-bar-fill" :style="{ width: entry.percent + '%' }" /></span>
          <span class="type-size">{{ humanBytes(entry.totalBytes) }}</span>
        </div>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:database-bold" :size="13" /> STORAGE FOOTPRINT</h3>
      <div class="info-card">
        <div class="info-row"><span class="info-key text-muted">TOTAL SIZE</span><span class="info-value">{{ totalSizeFormatted }}</span></div>
        <div class="info-row"><span class="info-key text-muted">LARGEST FILE</span><span class="info-value">{{ largestFile }}</span></div>
        <div class="info-row"><span class="info-key text-muted">AVG FILE SIZE</span><span class="info-value">{{ avgSizeFormatted }}</span></div>
        <div class="info-row"><span class="info-key text-muted">FILES WITH GPS</span><span class="info-value">{{ gpsCount }}</span></div>
        <div class="info-row"><span class="info-key text-muted">FILES WITH FACES</span><span class="info-value">{{ faceCount }}</span></div>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:ssd-square-bold" :size="13" /> MERGED DISKS ({{ df ? df.diskCount : 0 }})</h3>
      <div class="df-bar" role="img" :aria-label="`Volume ${usedPct.toFixed(1)} percent used`">
        <div class="df-used" :style="{ width: usedPct + '%' }"></div>
      </div>
      <div class="df-figures">
        <span>USED {{ df ? humanBytes(df.usedBytes) : '—' }}</span>
        <span>FREE {{ df ? humanBytes(df.freeBytes) : '—' }}</span>
        <span>TOTAL {{ df ? humanBytes(df.totalBytes) : '—' }}</span>
        <span class="text-muted">ROOT {{ df ? df.root : '—' }}</span>
      </div>
      <button class="open-disks" type="button" @click="view = 'disks'">MANAGE DISKS</button>
    </div>
    </template>

    <template v-if="view === 'disks'">
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:layers-bold" :size="13" /> MERGED VOLUME</h3>
      <div class="df-bar" role="img" :aria-label="`Volume ${usedPct.toFixed(1)} percent used`">
        <div class="df-used" :style="{ width: `${usedPct}%` }"></div>
      </div>
      <div class="df-legend">
        <span><i class="dot used"></i>USED {{ humanBytes(df?.usedBytes ?? 0) }}</span>
        <span><i class="dot free"></i>FREE {{ humanBytes(df?.freeBytes ?? 0) }}</span>
        <span>TOTAL {{ humanBytes(df?.totalBytes ?? 0) }}</span>
        <span class="text-muted">ATTACHED {{ humanBytes(df?.attachedBytes ?? 0) }} · SCRATCH {{ humanBytes(df?.scratchBytes ?? 0) }}</span>
      </div>
      <p class="df-root text-muted">root {{ df?.root ?? '—' }}</p>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:add-bold" :size="13" /> CREATE DISK</h3>
      <div class="create-row">
        <label class="field">
          <span class="text-muted">PROVIDER</span>
          <select v-model="configId" class="input" aria-label="Provider configuration">
            <option value="" disabled>select a configuration</option>
            <option v-for="cfg in store.syncConfigs" :key="cfg.id" :value="cfg.id">
              {{ cfg.name || cfg.backendType }} · {{ cfg.id }}
            </option>
          </select>
        </label>
        <label class="field grow">
          <span class="text-muted">SIZE — {{ createSizeMb }} MB</span>
          <input
            v-model.number="createSizeMb"
            class="slider"
            type="range"
            :min="MIN_MB"
            :max="MAX_MB"
            :step="64"
            aria-label="Disk size in megabytes"
          />
        </label>
        <label class="field">
          <span class="text-muted">PASSPHRASE</span>
          <input v-model="passphrase" class="input" type="password" autocomplete="off" aria-label="Passphrase" />
        </label>
        <button class="ghost-btn primary" type="button" :disabled="creating" @click="create">
          {{ creating ? 'CREATING…' : 'CREATE' }}
        </button>
      </div>
      <p v-if="note" class="note">{{ note }}</p>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:ssd-square-bold" :size="13" /> PER-PROVIDER DISKS ({{ store.disks.length }})</h3>

      <div v-if="store.disks.length" class="cards">
        <article v-for="disk in store.disks" :key="disk.id" :id="`disk-card-${disk.id}`" class="card" :class="{ flash: focusDiskId === disk.id }">
          <header class="card-head">
            <div>
              <span class="card-name">{{ disk.name || disk.id }}</span>
              <span class="card-provider text-muted">{{ disk.provider }}</span>
            </div>
            <span class="health" :class="`health-${disk.health}`">{{ disk.health.toUpperCase() }}</span>
          </header>

          <div class="card-bar">
            <div class="card-used" :style="{ width: `${diskPct(disk.usedBytes, disk.capacityBytes)}%` }"></div>
          </div>
          <div class="card-figures">
            <span>{{ humanBytes(disk.usedBytes) }} / {{ humanBytes(disk.capacityBytes) }}</span>
            <span class="text-muted">{{ diskPct(disk.usedBytes, disk.capacityBytes).toFixed(0) }}% used</span>
          </div>

          <dl class="card-meta">
            <div><dt class="text-muted">STATE</dt><dd>{{ disk.state }}</dd></div>
            <div><dt class="text-muted">CONFIG</dt><dd class="truncate" :title="disk.configId">{{ isOrphanDisk(disk.configId) ? `${disk.configId} (provider deleted)` : disk.configId }}</dd></div>
            <div><dt class="text-muted">PATH</dt><dd class="truncate" :title="disk.containerPath">{{ disk.containerPath }}</dd></div>
            <div v-if="disk.holdsKeys"><dt class="text-muted">KEYS</dt><dd>🔑 holder — unwraps the others</dd></div>
          </dl>

          <div class="card-actions">
            <button
              v-if="disk.state !== 'attached'"
              class="ghost-btn primary"
              type="button"
              :disabled="busyId === disk.id"
              @click="attach(disk.id)"
            >ATTACH</button>
            <button
              v-else
              class="ghost-btn"
              type="button"
              :disabled="busyId === disk.id"
              @click="detach(disk.id)"
            >DETACH</button>
            <button class="ghost-btn" type="button" @click="beginResize(disk.id, disk.capacityBytes)">RESIZE</button>
            <button class="ghost-btn" type="button" :disabled="busyId === disk.id" @click="check(disk.id)">CHECK</button>
            <button
              v-if="!disk.holdsKeys"
              class="ghost-btn primary"
              type="button"
              :disabled="busyId === disk.id"
              title="This disk's passphrase unwraps the other disks"
              @click="makeKeyHolder(disk.id)"
            >KEY HOLDER</button>
          </div>

          <div v-if="resizeTarget === disk.id" class="resize-row">
            <input
              v-model.number="resizeMb"
              class="slider"
              type="range"
              :min="MIN_MB"
              :max="MAX_MB"
              :step="64"
              :aria-label="`Resize ${disk.name}`"
            />
            <span class="resize-size">{{ resizeMb }} MB</span>
            <button class="ghost-btn primary" type="button" @click="applyResize(disk.id)">APPLY</button>
            <button class="ghost-btn" type="button" @click="resizeTarget = null">CANCEL</button>
          </div>
        </article>
      </div>

      <p v-else class="text-muted empty">
        No disks yet — create one above. Every attached disk grows the merged volume and adds
        provider compute slots to the fan-out pool.
      </p>
    </div>
    </template>

    <template v-if="view === 'backup'">
      <BackupPanel />
    </template>

    <!-- ══ DIET (storage-diet / dedup report, ANALYSIS.md §4.5) ══ -->
    <template v-if="view === 'diet'">
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:layers-bold" :size="13" /> DUPLICATES</h3>
      <div class="stats-grid">
        <div class="stat-card">
          <span class="stat-value">{{ dupGroups.length }}</span>
          <span class="stat-label text-muted">DUP SETS</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ dupSummary.fileCount }}</span>
          <span class="stat-label text-muted">DUP FILES</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ humanBytes(dupSummary.reclaimableBytes) }}</span>
          <span class="stat-label text-muted">RECLAIMABLE</span>
        </div>
        <div class="stat-card">
          <span class="stat-value">{{ dupSummary.unhashableCount }}</span>
          <span class="stat-label text-muted">NO HASH</span>
        </div>
      </div>
      <p v-if="dietNote" class="note">{{ dietNote }}</p>
      <p class="note text-muted">Grouped by BLAKE3 content hash — files without a recorded hash are invisible to this report, never guessed.</p>
    </div>

    <div v-if="dupGroups.length === 0" class="section">
      <p class="note">No duplicates — every hashed file holds unique content.</p>
    </div>

    <div v-for="group in dupGroups" :key="group.hash" class="section">
      <h3 class="section-title">
        <span class="truncate" :title="group.hash">{{ group.files.length }}× {{ group.files[0]?.name }} · {{ humanBytes(group.reclaimableBytes) }} RECLAIMABLE</span>
      </h3>
      <div class="info-card">
        <div
          v-for="f in group.files"
          :key="f.id"
          class="info-row"
        >
          <span class="info-key truncate" :title="f.path || f.name">{{ f.name }}</span>
          <span class="info-value">{{ humanBytes(f.sizeBytes) }}{{ f.id === group.keepId ? ' · KEEP' : '' }}</span>
        </div>
      </div>
      <button
        class="ghost-btn"
        :class="reclaimArmed === group.hash ? 'danger' : 'primary'"
        type="button"
        :disabled="reclaiming"
        @click="reclaimGroup(group.hash, group.keepId, group.files.map(f => f.id))"
      >{{ reclaimArmed === group.hash ? 'CONFIRM RECLAIM' : 'RECLAIM' }}</button>
    </div>
    </template>
  </div>
</template>

<style scoped>
.disk-panel {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
  box-sizing: border-box;
  padding-bottom: env(safe-area-inset-bottom);
}

.disk-panel *,
.disk-panel *::before,
.disk-panel *::after {
  box-sizing: border-box;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border);
  gap: 8px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.icon-disks {
  color: var(--ui-accent);
}

.panel-title {
  margin: 0;
  font-size: 13px;
  letter-spacing: 2px;
  white-space: nowrap;
}

.ghost-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  font-family: inherit;
  font-size: 11px;
  padding: 4px 9px;
  cursor: pointer;
}

.ghost-btn:hover:not(:disabled) {
  color: var(--ui-text);
  border-color: var(--ui-border-strong);
}

.ghost-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.ghost-btn.primary {
  color: var(--ui-accent);
  border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent);
}

.ghost-btn.primary:hover:not(:disabled) {
  color: var(--ui-text);
  background: var(--ui-accent);
  border-color: var(--ui-accent);
}

.ghost-btn.danger {
  color: var(--ui-danger);
  border-color: color-mix(in srgb, var(--ui-danger) 55%, transparent);
}

.ghost-btn.danger:hover:not(:disabled) {
  color: var(--ui-text);
  background: var(--ui-danger);
  border-color: var(--ui-danger);
}

.section {
  padding: 12px;
  border-bottom: 1px solid var(--ui-border);
}

.dm-tabs {
  display: flex;
  gap: 4px;
  padding: 8px 12px 0;
}
.dm-tabs button {
  flex: 1;
  padding: 6px 0;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.1em;
  background: transparent;
  border: 1px solid var(--ui-hairline);
  border-radius: 8px;
  color: var(--ui-text-3);
  cursor: pointer;
}
.dm-tabs button.on {
  color: var(--ui-accent);
  border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent);
  background: var(--ui-accent-softer);
}

.stats-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
}
.stat-card {
  border: 1px solid var(--ui-border);
  padding: 10px;
  text-align: center;
}
.stat-value {
  display: block;
  font-size: 18px;
  font-weight: 800;
  margin-bottom: 4px;
}
.stat-label {
  font-size: 10px;
  color: var(--ui-text-3);
}
.type-breakdown {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.type-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 9px;
}
.type-label {
  width: 80px;
  flex-shrink: 0;
  font-weight: 700;
}
.type-bar {
  flex: 1;
  height: 10px;
  border: 1px solid var(--ui-border-strong);
  position: relative;
  background: transparent;
}
.type-bar-fill {
  display: block;
  height: 100%;
  background: var(--ui-glass-2);
}
.type-size {
  width: 70px;
  text-align: right;
  flex-shrink: 0;
  color: color-mix(in srgb, var(--ui-text) 60%, transparent);
}
.info-card {
  border: 1px solid var(--ui-border);
  padding: 10px;
}
.info-row {
  display: flex;
  justify-content: space-between;
  font-size: 10px;
  padding: 3px 0;
  border-bottom: 1px solid var(--ui-border);
}
.info-row:last-child {
  border-bottom: none;
}
.info-key { color: color-mix(in srgb, var(--ui-text) 50%, transparent); }
.info-value { font-weight: 700; }
.df-figures {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  font-size: 10px;
  margin-top: 6px;
}
.open-disks {
  margin-top: 8px;
  background: transparent;
  border: 1px solid var(--ui-border-strong);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 10px;
  padding: 4px 8px;
  cursor: pointer;
}
.open-disks:hover {
  background: var(--ui-glass-2);
}

.section-title {
  margin: 0 0 10px;
  font-size: 12px;
  font-weight: 600;
  color: var(--ui-text);
}

.df-bar {
  height: 8px;
  border-radius: 5px;
  overflow: hidden;
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
  border: none;
}

.df-used {
  height: 100%;
  background: var(--ui-accent);
}

.df-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 14px;
  margin-top: 8px;
  font-size: 11px;
}

.dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 5px;
}

.dot.used {
  background: var(--ui-accent);
}

.dot.free {
  background: color-mix(in srgb, var(--ui-text) 30%, transparent);
}

.df-root {
  margin: 6px 0 0;
  font-size: 11px;
}

.create-row {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  min-width: 160px;
}

.field.grow {
  flex: 1;
  min-width: 200px;
}

.input {
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 5px 6px;
  outline: none;
}

.input:focus {
  border-color: var(--ui-accent);
}

.slider {
  width: 100%;
  accent-color: var(--ui-accent);
}

.note {
  margin: 8px 0 0;
  font-size: 11px;
  color: var(--ui-info);
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 12px;
}

.card {
  border: 1px solid var(--ui-border);
  padding: 10px;
  border-radius: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.card.flash {
  border-color: color-mix(in srgb, var(--ui-accent) 65%, transparent);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--ui-accent) 45%, transparent);
}

.card-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}

.card-name {
  display: block;
  font-size: 13px;
}

.card-provider {
  font-size: 11px;
}

.health {
  font-size: 10px;
  padding: 1px 6px;
  border: 1px solid currentColor;
}

.health-ok {
  color: var(--ui-accent);
}

.health-degraded,
.health-repairing {
  color: var(--ui-warning);
}

.health-failed {
  color: var(--ui-danger);
}

.card-bar {
  height: 10px;
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
}

.card-used {
  height: 100%;
  background: var(--ui-accent);
}

.card-figures {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
}

.card-meta {
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 11px;
}

.card-meta div {
  display: flex;
  gap: 6px;
  min-width: 0;
}

.card-meta dt {
  width: 52px;
  flex: 0 0 auto;
}

.card-meta dd {
  margin: 0;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.resize-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}

.resize-size {
  font-size: 11px;
  white-space: nowrap;
}

.empty {
  margin: 0;
  font-size: 12px;
}

.text-muted {
  color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important;
}

@media (max-width: 600px) {
  .disk-panel {
    font-size: 14px;
  }

  .panel-header {
    align-items: stretch;
    padding: max(10px, env(safe-area-inset-top)) 12px 10px;
  }

  .header-left,
  .header-right {
    width: 100%;
  }

  .header-right .ghost-btn {
    width: 100%;
  }

  .ghost-btn,
  .open-disks,
  .dm-tabs button,
  .input {
    min-height: 44px;
  }

  .ghost-btn,
  .open-disks {
    padding: 8px 12px;
    border-radius: 10px;
  }

  .input {
    padding: 8px 10px;
    border-radius: 10px;
  }

  .panel-title {
    font-size: 12px;
    letter-spacing: 1.5px;
  }

  .section {
    padding: 16px 12px;
  }

  .stats-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .stat-card {
    min-height: 64px;
    padding: 12px 8px;
    border-radius: 12px;
  }

  .type-row {
    gap: 6px;
    font-size: 11px;
  }

  .type-label {
    width: 58px;
  }

  .type-size {
    width: 66px;
    font-size: 10px;
  }

  .info-card {
    padding: 10px 12px;
    border-radius: 12px;
  }

  .info-row {
    align-items: flex-start;
    gap: 12px;
    min-height: 32px;
    font-size: 11px;
  }

  .info-value {
    min-width: 0;
    max-width: 65%;
    text-align: right;
    overflow-wrap: anywhere;
  }

  .df-legend,
  .df-figures {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    font-size: 11px;
  }

  .df-legend > span,
  .df-figures > span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .create-row {
    align-items: stretch;
    flex-direction: column;
    gap: 10px;
  }

  .field,
  .field.grow {
    width: 100%;
    min-width: 0;
  }

  .cards {
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
  }

  .card {
    padding: 14px;
    gap: 10px;
  }

  .card-head {
    align-items: center;
  }

  .card-name,
  .card-provider {
    overflow-wrap: anywhere;
  }

  .health {
    flex: 0 0 auto;
    min-height: 28px;
    padding: 5px 8px;
    border-radius: 999px;
  }

  .card-bar {
    height: 12px;
    border-radius: 999px;
    overflow: hidden;
  }

  .card-figures {
    gap: 8px;
    flex-wrap: wrap;
    font-size: 12px;
  }

  .card-actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .card-actions .ghost-btn {
    width: 100%;
  }

  .resize-row {
    align-items: stretch;
  }

  .resize-row .slider {
    flex: 1 1 100%;
  }

  .resize-row .ghost-btn {
    flex: 1 1 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .disk-panel *,
  .disk-panel *::before,
  .disk-panel *::after {
    scroll-behavior: auto !important;
    transition-duration: 0.01ms !important;
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
  }
}
</style>
