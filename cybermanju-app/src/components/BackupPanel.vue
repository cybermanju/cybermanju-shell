<!-- CyberManju OS — BackupPanel (src/components/BackupPanel.vue). -->
<!-- Role: Vault snapshot exports, schedules and restore (ANALYSIS §3 backup-as-feature). -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/stores/app (backup actions), @/utils/backup, @/utils/schedule. -->
<!-- Contracts: passphrase never leaves the device (sealing is client-side); -->
<!-- restore on Pages replaces OPFS + reloads, elsewhere shows guided steps. -->
<template>
  <div class="bk">
    <!-- ══ EXPORT NOW ══ -->
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:download-bold" :size="13" /> EXPORT NOW</h3>
      <p class="bk-hint">A point-in-time vault snapshot, sealed with your passphrase into a
        <code>.cybermanju</code> container. Downloads land in this browser; provider sends ride the sync transports.</p>
      <div class="bk-grid">
        <label class="field">
          <span class="text-muted">PASSPHRASE (SEALS THE FILE)</span>
          <input v-model="passphrase" type="password" autocomplete="new-password" placeholder="vault passphrase" />
        </label>
        <label class="field">
          <span class="text-muted">DESTINATION</span>
          <select v-model="dest">
            <option value="">Download to this device</option>
            <option v-for="m in mounts" :key="m.id" :value="`/providers/${m.id}/backups`">
              {{ m.name }} ({{ m.backendType }})
            </option>
          </select>
        </label>
        <label class="field">
          <span class="text-muted">KEEP (PROVIDER PRUNE)</span>
          <input v-model.number="keep" type="number" min="0" max="90" />
        </label>
      </div>
      <div class="bk-row">
        <UiButton size="sm" icon="solar:download-bold" :disabled="busy" @click="doDownload">Download snapshot</UiButton>
        <UiButton size="sm" icon="solar:upload-bold" :disabled="busy || !dest" @click="doSend">Send to provider</UiButton>
      </div>
      <p v-if="exportNote" class="bk-note">{{ exportNote }}</p>
    </div>

    <!-- ══ SNAPSHOTS ══ -->
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:archive-bold" :size="13" /> SNAPSHOTS</h3>
      <div class="bk-row">
        <label class="field grow">
          <span class="text-muted">LOCATION</span>
          <select v-model="listDest" @change="refreshList">
            <option v-for="m in mounts" :key="m.id" :value="`/providers/${m.id}/backups`">
              {{ m.name }} ({{ m.backendType }})
            </option>
          </select>
        </label>
        <UiButton size="sm" icon="solar:refresh-bold" :disabled="listBusy || !listDest" @click="refreshList">Refresh</UiButton>
      </div>
      <UiSpinner v-if="listBusy" />
      <UiEmpty v-else-if="listDest && !entries.length" title="No snapshots here yet" />
      <p v-else-if="!listDest" class="bk-hint">Connect a provider to browse its snapshots.</p>
      <div v-else class="bk-list">
        <div v-for="e in entries" :key="e.name" class="bk-rowitem">
          <span class="bk-name">{{ e.name }}</span>
          <span class="bk-size text-muted">{{ humanBytes(e.bytes) }}</span>
          <UiButton size="sm" variant="ghost" :disabled="verifyBusy" @click="verifyEntry(e.name, false)">Verify</UiButton>
          <UiButton size="sm" variant="ghost" :disabled="verifyBusy" @click="verifyEntry(e.name, true)">Deep</UiButton>
        </div>
      </div>
      <p v-if="verifyNote" class="bk-note">{{ verifyNote }}</p>
    </div>

    <!-- ══ SCHEDULE ══ -->
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:clock-circle-bold" :size="13" /> SCHEDULED BACKUP</h3>
      <p class="bk-hint">Writes <code>/.cybermanju/scheduled-backup.cybsh</code> (frontmatter owns the
        expression) and keeps one cron row on it. Unattended runs seal with the vault
        passphrase — a locked vault records an <code>auth:</code> run, never plaintext.</p>
      <div class="bk-grid">
        <label class="field">
          <span class="text-muted">EXPRESSION</span>
          <input v-model="schedExpr" placeholder="30 2 * * *" @input="previewExpr" />
        </label>
        <label class="field">
          <span class="text-muted">DESTINATION</span>
          <select v-model="schedDest">
            <option v-for="m in mounts" :key="m.id" :value="`/providers/${m.id}/backups`">
              {{ m.name }} ({{ m.backendType }})
            </option>
          </select>
        </label>
        <label class="field">
          <span class="text-muted">KEEP</span>
          <input v-model.number="schedKeep" type="number" min="1" max="90" />
        </label>
      </div>
      <p class="bk-hint">{{ schedPreview }}</p>
      <div class="bk-row">
        <UiButton size="sm" icon="solar:check-circle-bold" :disabled="schedBusy || !schedDest" @click="saveSchedule">Save schedule</UiButton>
      </div>
      <div v-if="schedRow" class="bk-rowitem">
        <span class="bk-name">{{ schedRow.expr }}</span>
        <span class="bk-size text-muted">next {{ schedRow.nextFireAt ?? '—' }}</span>
        <UiButton size="sm" variant="ghost" @click="toggleSched">{{ schedRow.enabled ? 'Disable' : 'Enable' }}</UiButton>
        <UiButton size="sm" variant="ghost" @click="runSchedNow">Run now</UiButton>
        <UiButton size="sm" variant="danger" @click="dropSched">Delete</UiButton>
      </div>
    </div>

    <!-- ══ RESTORE ══ -->
    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:history-bold" :size="13" /> RESTORE WIZARD</h3>
      <div class="bk-steps">
        <p><b>1 · Source.</b>
          <label class="bk-radio"><input v-model="wizSource" type="radio" value="upload" /> Upload a file</label>
          <label class="bk-radio"><input v-model="wizSource" type="radio" value="provider" /> Provider file</label>
        </p>
        <div v-if="wizSource === 'upload'" class="bk-row">
          <input type="file" aria-label="Choose backup snapshot file" accept=".cybermanju" @change="pickFile" />
        </div>
        <div v-else class="bk-grid">
          <label class="field">
            <span class="text-muted">LOCATION</span>
            <select v-model="wizDest">
              <option v-for="m in mounts" :key="m.id" :value="`/providers/${m.id}/backups`">
                {{ m.name }} ({{ m.backendType }})
              </option>
            </select>
          </label>
          <label class="field">
            <span class="text-muted">FILE NAME</span>
            <input v-model="wizName" placeholder="cybermanju-….cybermanju" />
          </label>
          <div class="bk-row"><UiButton size="sm" :disabled="wizBusy" @click="pullProviderFile">Load file</UiButton></div>
        </div>
        <p><b>2 · Verify.</b>
          <label class="field inline">
            <span class="text-muted">PASSPHRASE</span>
            <input v-model="wizPass" type="password" autocomplete="new-password" placeholder="sealing passphrase" />
          </label>
          <UiButton size="sm" :disabled="wizBusy || !wizBytes" @click="verifyPicked">Verify backup</UiButton>
        </p>
        <p v-if="wizResult" class="bk-note">{{ wizResult }}</p>
        <p><b>3 · Apply.</b>
          <UiButton size="sm" variant="danger" :disabled="wizBusy || !wizImage" @click="applyRestore">Restore this backup</UiButton>
        </p>
        <div v-if="manualSteps" class="bk-manual">
          <p><b>Desktop / Docker restore (the database file is live while the app runs):</b></p>
          <ol>
            <li>Copy the verified <code>{{ wizName || 'backup file' }}</code> somewhere safe (second copy).</li>
            <li>Stop the app (desktop) or the container (<code>docker compose down</code>).</li>
            <li>Replace <code>cybermanju.db</code> with the backup's decoded image (rename to <code>cybermanju.db</code>).</li>
            <li>Start the app again, open this panel and run <b>Rebuild catalog</b> below.</li>
          </ol>
          <UiButton size="sm" icon="solar:refresh-bold" :disabled="rebuildBusy" @click="rebuild">Rebuild catalog</UiButton>
        </div>
      </div>
    </div>
    <UiError v-if="error" size="sm" title="Backup" :message="error" retryable @retry="clearError" />
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, ref } from 'vue'
import { useAppStore } from '@/stores/app'
import { humanBytes } from '@/utils/format'
import { previewSchedule } from '@/utils/schedule'
import type { BackupEntry, ScheduleRow } from '@/types'
import { isStaticHost } from '@/composables/useTauri'

const store = useAppStore()

const mounts = ref<{ id: string; name: string; backendType: string; configId: string }[]>([])
const busy = ref(false)
const passphrase = ref('')
const dest = ref('')
const keep = ref(7)
const exportNote = ref('')
const error = ref('')

const listDest = ref('')
const entries = ref<BackupEntry[]>([])
const listBusy = ref(false)
const verifyBusy = ref(false)
const verifyNote = ref('')

const schedExpr = ref('30 2 * * *')
const schedDest = ref('')
const schedKeep = ref(7)
const schedBusy = ref(false)
const schedPreview = ref('')
const schedRow = ref<ScheduleRow | null>(null)

const wizSource = ref<'upload' | 'provider'>('upload')
const wizBytes = ref<Uint8Array | null>(null)
const wizName = ref('')
const wizDest = ref('')
const wizPass = ref('')
const wizResult = ref('')
const wizImage = ref<Uint8Array | null>(null)
const wizBusy = ref(false)
const manualSteps = ref(false)
const rebuildBusy = ref(false)

const onPages = computed(() => isStaticHost())

function clearError() {
  error.value = ''
}

function previewExpr() {
  const preview = previewSchedule(schedExpr.value.trim())
  schedPreview.value = preview.ok
    ? `Next: ${preview.next ? preview.next.toLocaleString() : 'never'} (${preview.in ?? ''}) — ${preview.label ?? ''}`
    : (preview.error ?? 'invalid: bad schedule expression')
}

async function boot() {
  mounts.value = await store.listBackupMounts()
  const first = mounts.value[0]
  if (first && !dest.value) dest.value = `/providers/${first.id}/backups`
  if (first && !listDest.value) listDest.value = `/providers/${first.id}/backups`
  if (first && !schedDest.value) schedDest.value = `/providers/${first.id}/backups`
  if (first && !wizDest.value) wizDest.value = `/providers/${first.id}/backups`
  await store.fetchSchedules()
  schedRow.value = store.findBackupSchedule() ?? null
  previewExpr()
  if (listDest.value) await refreshList()
}

onMounted(() => {
  void boot()
})

async function doDownload() {
  busy.value = true
  exportNote.value = ''
  try {
    if (!passphrase.value) {
      exportNote.value = 'Enter a passphrase — downloads are always sealed (empty would be plaintext at rest).'
      return
    }
    const name = await store.downloadBackup(passphrase.value)
    exportNote.value = name ? `Saved ${name} — verify it before you need it.` : ''
  } finally {
    busy.value = false
  }
}

async function doSend() {
  busy.value = true
  exportNote.value = ''
  try {
    const report = await store.exportBackupToProvider(dest.value, keep.value)
    if (report) exportNote.value = `${report.path} · blake3 ${report.blake3}`
  } finally {
    busy.value = false
  }
}

async function refreshList() {
  if (!listDest.value) return
  listBusy.value = true
  try {
    entries.value = await store.fetchBackupList(listDest.value)
  } finally {
    listBusy.value = false
  }
}

async function verifyEntry(name: string, deep: boolean) {
  verifyBusy.value = true
  verifyNote.value = ''
  try {
    const report = await store.verifyBackupPath(`${listDest.value}/${name}`, deep)
    if (report) {
      verifyNote.value = report.legacy
        ? `${name}: legacy raw image, ${report.totalBytes} bytes`
        : `${name}: CYBMJ01 v${report.version} ${report.encrypted ? 'sealed' : 'plaintext'}${report.compressed ? '+lz4' : ''}` +
          (report.imageBytes != null ? `, image ${report.imageBytes} bytes verified` : ', header only')
    }
  } finally {
    verifyBusy.value = false
  }
}

async function saveSchedule() {
  schedBusy.value = true
  try {
    const saved = await store.scheduleBackup(schedExpr.value.trim(), schedDest.value, schedKeep.value)
    schedRow.value = saved ?? store.findBackupSchedule() ?? null
  } finally {
    schedBusy.value = false
  }
}

async function toggleSched() {
  if (!schedRow.value) return
  await store.cronSetEnabled(schedRow.value.id, !schedRow.value.enabled)
  await store.fetchSchedules()
  schedRow.value = store.findBackupSchedule() ?? null
}

async function runSchedNow() {
  if (!schedRow.value) return
  schedBusy.value = true
  try {
    await store.cronRun(schedRow.value.id)
  } finally {
    schedBusy.value = false
  }
}

async function dropSched() {
  if (!schedRow.value) return
  await store.cronDelete(schedRow.value.id)
  schedRow.value = null
}

async function pickFile(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  wizName.value = file.name
  wizBytes.value = new Uint8Array(await file.arrayBuffer())
  wizImage.value = null
  wizResult.value = `${file.name}: ${wizBytes.value.length} bytes loaded — verify next.`
}

async function pullProviderFile() {
  if (!wizDest.value || !wizName.value.trim()) return
  wizBusy.value = true
  wizResult.value = ''
  try {
    const bytes = await store.fetchBackupBytes(wizDest.value, wizName.value.trim())
    if (bytes) {
      wizBytes.value = bytes
      wizImage.value = null
      wizResult.value = `${wizName.value.trim()}: ${bytes.length} bytes loaded — verify next.`
    }
  } finally {
    wizBusy.value = false
  }
}

async function verifyPicked() {
  if (!wizBytes.value) return
  wizBusy.value = true
  wizResult.value = ''
  try {
    const checked = await store.verifyBackupBytes(wizBytes.value, wizPass.value)
    if (checked) {
      if (checked.legacy) {
        const restoreNote = onPages.value
          ? 'The browser restore validates the redb structure before replacing the vault.'
          : 'This wizard cannot validate its redb structure on this transport; restore only from a trusted copy.'
        wizResult.value = `Legacy raw image selected: ${checked.imageBytes} bytes; it has no container authentication or checksum. ${restoreNote}`
      } else {
        wizResult.value = checked.encrypted
          ? `Verified: sealed container, image ${checked.imageBytes} bytes — passphrase opens it.`
          : `Verified: plaintext image, ${checked.imageBytes} bytes.`
      }
      if (onPages.value) {
        // Pages restores the decoded image directly.
        const { decodeContainer } = await import('@/utils/container')
        wizImage.value = (await decodeContainer(wizBytes.value, wizPass.value)).image
      } else {
        // Desktop/Docker: the wizard only verifies; apply shows the steps.
        wizImage.value = new Uint8Array([1])
      }
    } else {
      wizImage.value = null
    }
  } finally {
    wizBusy.value = false
  }
}

async function applyRestore() {
  if (!wizImage.value) return
  if (
    onPages.value &&
    !window.confirm(
      'This will replace the current browser vault with the selected backup and reload the app. Continue only if you have another copy of any data you need.',
    )
  ) {
    return
  }
  wizBusy.value = true
  try {
    if (onPages.value) {
      await store.restoreBackupImage(wizImage.value)
    } else {
      manualSteps.value = true
    }
  } catch (e) {
    const detail = e instanceof Error ? e.message : String(e)
    wizResult.value = `Restore failed: ${detail}`
  } finally {
    wizBusy.value = false
  }
}

async function rebuild() {
  rebuildBusy.value = true
  try {
    await store.runRebuild()
  } finally {
    rebuildBusy.value = false
  }
}
</script>

<style scoped>
.bk { display: flex; flex-direction: column; gap: 14px; }
.section { display: flex; flex-direction: column; gap: 10px; }
.section-title { display: flex; align-items: center; gap: 6px; margin: 0; font-size: 12px; letter-spacing: 0.08em; }
.bk-hint { margin: 0; font-size: 12.5px; color: var(--ui-text-2); line-height: 1.5; }
.bk-hint code { font-family: var(--ui-font-mono); font-size: 11.5px; }
.bk-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 10px; }
.field { display: flex; flex-direction: column; gap: 4px; font-size: 11px; }
.field.inline { display: inline-flex; margin: 0 8px 0 4px; vertical-align: middle; }
.field input, .field select { min-height: 34px; padding: 0 10px; border-radius: var(--ui-radius-sm); border: 1px solid var(--ui-border-strong); background: var(--ui-surface); color: var(--ui-text); font: inherit; font-size: 13px; }
.bk-row { display: flex; gap: 8px; align-items: end; flex-wrap: wrap; }
.field.grow { flex: 1; min-width: 200px; }
.bk-note { margin: 0; font-size: 12.5px; color: var(--ui-text-2); overflow-wrap: anywhere; }
.bk-list { display: flex; flex-direction: column; gap: 6px; }
.bk-rowitem { display: flex; gap: 8px; align-items: center; padding: 8px 10px; border: 1px solid var(--ui-border); border-radius: var(--ui-radius-sm); background: var(--ui-surface); }
.bk-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
.bk-size { font-size: 12px; white-space: nowrap; }
.bk-steps { display: flex; flex-direction: column; gap: 8px; font-size: 13px; }
.bk-steps p { margin: 0; }
.bk-radio { margin-left: 10px; font-weight: 400; }
.bk-manual { border: 1px solid var(--ui-border-strong); border-radius: var(--ui-radius-sm); padding: 10px 12px; font-size: 13px; }
.bk-manual ol { margin: 6px 0 10px; padding-left: 20px; }
.bk-manual code { font-family: var(--ui-font-mono); font-size: 12px; }
input[type='file'] { font-size: 13px; color: var(--ui-text-2); }
</style>
