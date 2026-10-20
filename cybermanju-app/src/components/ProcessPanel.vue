<script setup lang="ts">
// CyberManju OS — ProcessPanel (src/components/ProcessPanel.vue).
// Role: Vue desktop UI: windows, panels, overlays.
// Exports: module root (see items below).
// Integrates: @/…, vue.
// Contracts: signals: AGENT-.
import AppIcon from '@/components/AppIcon.vue'
import UiCheckbox from '@/components/ui/UiCheckbox.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiModal from '@/components/ui/UiModal.vue'
// CyberManju OS — process table (AGENT-8, item 10) + Schedules tab (Phase 1)
//
// Processes: fed by `GET /api/os/ps` and `GET /api/os/top`; every control
// goes through the same syscall boundary the terminal uses (`kill <id>`,
// `compute run …`).
//
// Schedules: the cron family — `GET/POST /api/cron` (REST), `cron_*` over
// IPC on desktop/mobile, and the browser tick on Pages (no daemon thread
// there; the store fires due rows itself). Next-fire countdowns come from
// `utils/schedule`, the TypeScript twin of `crates/os/src/schedule.rs`, so
// the preview agrees with the daemon to the minute.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { humanBytes } from '@/utils/format'
import { formatCpuPercent, formatLoadTriplet } from '@/utils/deviceStats'
import { formatIn, previewSchedule } from '@/utils/schedule'
import { createWatcher, evaluateWatchers, parseWatchersStore, serializeWatchersStore, type WatchRule } from '@/utils/watchers'
import { t } from '@/utils/i18n'
import { useNotifications } from '@/composables/useNotifications'
import type { OsTask, ScheduleRow, ScheduleRun } from '@/types'

const store = useAppStore()
const { notify } = useNotifications()
const refreshMs = 2000
let timer = 0

// Merged window: `wm.open('cron' | 'automation' | 'schedules')` lands here
// with `{ tab: 'schedules' }` (panels.ts ALIAS_TAB_PROPS).
const props = defineProps<{ tab?: string }>()
type ProcessTab = 'processes' | 'schedules' | 'watchers'
function coerceProcessTab(v: unknown): ProcessTab | null {
  return v === 'processes' || v === 'schedules' || v === 'watchers' ? v : null
}
const activeTab = ref<ProcessTab>(coerceProcessTab(props.tab) ?? 'processes')
watch(
  () => props.tab,
  t => {
    const c = coerceProcessTab(t)
    if (c) activeTab.value = c
  },
)

const jobPath = ref('/')
const starting = ref(false)
const note = ref('')

const tasks = computed<OsTask[]>(() => store.osPs?.tasks ?? store.osTop?.tasks ?? [])
const counts = computed(() => store.osTop?.counts ?? store.osPs?.counts ?? null)
const load = computed(() => store.osTop?.load ?? null)
const mem = computed(() => store.osTop?.mem ?? null)

function clock(ms: number): string {
  const total = Math.floor(ms / 1000)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  return h > 0 ? `${h}h${m}m` : m > 0 ? `${m}m${s}s` : `${s}s`
}

function pct(task: OsTask): number {
  return Math.max(0, Math.min(100, Math.round(task.progress * 100)))
}

function kill(task: OsTask) {
  void store.killOsTask(task.id)
}

async function startJob(job: string) {
  starting.value = true
  const result = await store.runComputeJob(job, jobPath.value || '/')
  starting.value = false
  note.value = result.output
}

async function refresh() {
  await Promise.all([store.fetchOsPs(), store.fetchOsTop(), store.fetchOsWorkers()])
}

// ── Watchers (proactive agent watchers, ANALYSIS.md §4.6) ──────────────
// Rules live in localStorage; EVALUATE NOW runs them against the vault file
// list (notify via the notification center, tag via setFileTags). The cron
// scheduler above is the unattended runner — point a schedule at
// `watchers run` semantics by opening this tab from automation.
const WATCHERS_KEY = 'cybermanju.watchers.v1'
const watchRules = ref<WatchRule[]>([])
const wName = ref('')
const wTag = ref('')
const wMime = ref('')
const wPattern = ref('')
const wNotify = ref(true)
const wAddTag = ref('')
const wAgent = ref('')
const watchNote = ref('')
const watchHits = ref(0)

function loadWatchers() {
  try {
    watchRules.value = parseWatchersStore(localStorage.getItem(WATCHERS_KEY))
  } catch {
    watchRules.value = []
  }
}

function persistWatchers() {
  try {
    localStorage.setItem(WATCHERS_KEY, serializeWatchersStore(watchRules.value))
  } catch {
    /* private mode — session-only */
  }
}

function addWatchRule() {
  const created = createWatcher(
    watchRules.value,
    wName.value,
    {
      ...(wTag.value.trim() ? { tag: wTag.value.trim() } : {}),
      ...(wMime.value.trim() ? { mimePrefix: wMime.value.trim() } : {}),
      ...(wPattern.value.trim() ? { namePattern: wPattern.value.trim() } : {}),
    },
    {
      notify: wNotify.value,
      ...(wAddTag.value.trim() ? { addTag: wAddTag.value.trim() } : {}),
      ...(wAgent.value ? { agentConfigId: wAgent.value } : {}),
    },
  )
  if (!created) {
    watchNote.value = 'name the rule first'
    return
  }
  watchRules.value = created.rules
  wName.value = ''
  wTag.value = ''
  wMime.value = ''
  wPattern.value = ''
  wAddTag.value = ''
  wAgent.value = ''
  watchNote.value = ''
  persistWatchers()
}

function toggleWatchRule(id: string) {
  watchRules.value = watchRules.value.map((r) => (r.id === id ? { ...r, enabled: !r.enabled } : r))
  persistWatchers()
}

function removeWatchRule(id: string) {
  watchRules.value = watchRules.value.filter((r) => r.id !== id)
  persistWatchers()
}

/** Run every enabled rule against the current vault listing. */
async function evaluateWatchersNow() {
  await store.fetchFiles()
  const files = store.files.map((f) => ({
    id: f.id,
    name: f.name,
    mimeType: f.mimeType,
    sizeBytes: f.sizeBytes,
    tags: f.tags,
  }))
  const hits = evaluateWatchers(watchRules.value, files)
  watchHits.value = hits.length
  let tagged = 0
  let classified = 0
  for (const { rule, file } of hits) {
    if (rule.actions.addTag) {
      const node = store.files.find((f) => f.id === file.id)
      const tags = node?.tags ?? []
      if (!tags.includes(rule.actions.addTag)) {
        await store.setFileTags(file.id, [...tags, rule.actions.addTag])
        tagged += 1
      }
    }
    if (rule.actions.notify) {
      notify('info', `watcher '${rule.name}': ${file.name}`, 5000, 'Watcher')
    }
    // §4.6 classify hook: fire-and-forget agent pass over the new file.
    if (rule.actions.agentConfigId) {
      try {
        await store.startAgentRun(
          rule.actions.agentConfigId,
          `Classify this new file and reply with tags + one summary line: ${file.name} (${file.mimeType ?? 'unknown type'})`,
        )
        classified += 1
      } catch {
        /* a dead assistant must not fail the watcher run */
      }
    }
  }
  watchNote.value =
    hits.length === 0
      ? 'no matches in the current vault listing'
      : `${hits.length} match${hits.length === 1 ? '' : 'es'}${tagged ? ` · ${tagged} tagged` : ''}${classified ? ` · ${classified} sent to agent` : ''}`
}

// ── Schedules (cron) ────────────────────────────────────────
const nowMs = ref(Date.now())
const schedNote = ref('')

const editVisible = ref(false)
const editId = ref('')
const editPath = ref('')
const editExpr = ref('')
const editDesc = ref('')
const editRunOnBoot = ref(false)
const editError = ref('')

const historyVisible = ref(false)
const historyRow = ref<ScheduleRow | null>(null)
const historyRuns = ref<ScheduleRun[]>([])
const historyLoading = ref(false)

const schedules = computed<ScheduleRow[]>(() => store.schedules)
const editPreview = computed(() => previewSchedule(editExpr.value, new Date(nowMs.value)))

const EXPR_PRESETS = [
  { label: 'every 10m', expr: 'every 10m' },
  { label: 'hourly', expr: '@hourly' },
  { label: 'daily 2am', expr: '30 2 * * *' },
  { label: 'weekly sun', expr: '0 0 * * 0' },
]

/** Countdown to the next fire (`paused` when disabled, `—` when settled). */
function nextLabel(row: ScheduleRow): string {
  if (!row.enabled) return 'paused'
  if (!row.nextFireAt) return 'pending'
  return formatIn(new Date(row.nextFireAt).getTime() - nowMs.value)
}

/** How long ago the last fire happened (`never` before the first one). */
function agoLabel(iso?: string | null): string {
  if (!iso) return 'never'
  const diff = nowMs.value - new Date(iso).getTime()
  if (diff < 60_000) return 'just now'
  return `${formatIn(diff).replace(/^in /, '')} ago`
}

function openCreate() {
  editId.value = ''
  editPath.value = ''
  editExpr.value = 'every 1h'
  editDesc.value = ''
  editRunOnBoot.value = false
  editError.value = ''
  editVisible.value = true
}

function openEdit(row: ScheduleRow) {
  editId.value = row.id
  editPath.value = row.path
  editExpr.value = row.expr
  editDesc.value = row.description ?? ''
  editRunOnBoot.value = row.runOnBoot ?? false
  editError.value = ''
  editVisible.value = true
}

async function saveSchedule() {
  editError.value = ''
  const path = editPath.value.trim()
  if (!path.toLowerCase().endsWith('.cybsh')) {
    editError.value = `invalid: schedule path must be a .cybsh script (got \`${path || '∅'}\`)`
    return
  }
  if (!editPreview.value.ok) {
    editError.value = editPreview.value.error ?? 'invalid: bad schedule expression'
    return
  }
  const saved = await store.cronSave({
    id: editId.value,
    path,
    expr: editExpr.value.trim(),
    description: editDesc.value.trim() || null,
    enabled: true,
    runOnBoot: editRunOnBoot.value,
  })
  if (saved) editVisible.value = false
  else editError.value = store.lastError ?? 'the schedule could not be saved'
}

function toggleSchedule(row: ScheduleRow) {
  void store.cronSetEnabled(row.id, !row.enabled)
}

async function runNow(row: ScheduleRow) {
  schedNote.value = `Running ${row.path}…`
  const run = await store.cronRun(row.id)
  schedNote.value = run
    ? `${run.scheduleId}: ${run.status} (${run.finishedAt})${run.outputTail ? `\n${run.outputTail}` : ''}`
    : 'the run failed — see the toast for the reason'
}

function removeSchedule(row: ScheduleRow) {
  if (!window.confirm(`Delete schedule ${row.id}? (${row.path} · ${row.expr})`)) return
  void store.cronDelete(row.id)
}

async function openHistory(row: ScheduleRow) {
  historyRow.value = row
  historyRuns.value = []
  historyLoading.value = true
  historyVisible.value = true
  historyRuns.value = await store.cronHistory(row.id)
  historyLoading.value = false
}

onMounted(() => {
  void refresh()
  if (store.osJobs.length === 0) void store.fetchOsJobs()
  void store.fetchSchedules()
  timer = window.setInterval(() => {
    if (document.hidden) return
    nowMs.value = Date.now()
    void refresh()
  }, refreshMs)
})

watch(activeTab, tab => {
  if (tab === 'schedules') void store.fetchSchedules()
  if (tab === 'watchers') loadWatchers()
})

onBeforeUnmount(() => {
  if (timer) window.clearInterval(timer)
})
</script>

<template>
  <div class="process-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-processes"><AppIcon name="solar:cpu-bold" /></span>
        <h2 class="panel-title">Tasks</h2>
        <span v-if="activeTab === 'processes'" class="text-muted">
          {{ counts ? `${counts.running} running / ${counts.total} total` : '…' }}
        </span>
        <span v-else-if="activeTab === 'schedules'" class="text-muted">{{ schedules.length }} scheduled</span>
        <span v-else class="text-muted">{{ watchRules.length }} watchers</span>
      </div>
      <div class="header-right">
        <button class="ghost-btn" type="button" @click="activeTab === 'schedules' ? store.fetchSchedules() : activeTab === 'watchers' ? evaluateWatchersNow() : refresh()">
          Refresh
        </button>
      </div>
    </div>

    <div class="proc-tabs" role="tablist" aria-label="Tasks views">
      <button
        role="tab"
        type="button"
        :aria-selected="activeTab === 'processes'"
        :class="{ on: activeTab === 'processes' }"
        @click="activeTab = 'processes'"
      >Processes</button>
      <button
        role="tab"
        type="button"
        :aria-selected="activeTab === 'schedules'"
        :class="{ on: activeTab === 'schedules' }"
        @click="activeTab = 'schedules'"
      >Schedules</button>
      <button
        role="tab"
        type="button"
        :aria-selected="activeTab === 'watchers'"
        :class="{ on: activeTab === 'watchers' }"
        @click="activeTab = 'watchers'; loadWatchers()"
      >{{ t('watchers.title') }}</button>
    </div>

    <template v-if="activeTab === 'processes'">
      <div class="stats-row">
        <div class="stat">
          <span class="stat-key">LOAD</span>
          <span class="stat-val">
            {{ load && load.source === 'proc' ? formatLoadTriplet(load) : 'n/a' }}
          </span>
        </div>
        <div class="stat">
          <span class="stat-key">CPU</span>
          <span class="stat-val">{{ formatCpuPercent(store.osTop) }}</span>
        </div>
        <div class="stat">
          <span class="stat-key">RSS</span>
          <span class="stat-val">{{ mem ? humanBytes(mem.rssBytes) : '—' }}</span>
        </div>
        <div class="stat">
          <span class="stat-key">UP</span>
          <span class="stat-val">{{ store.osTop ? clock(store.osTop.uptimeMs) : '—' }}</span>
        </div>
        <div class="stat">
          <span class="stat-key">WORKERS</span>
          <span class="stat-val">
            {{ store.osWorkers ? `${store.osWorkers.total} (${store.osWorkers.localThreads} local + ${store.osWorkers.providerSlots} provider)` : '—' }}
          </span>
        </div>
      </div>

      <div class="section">
        <h3 class="section-title"><AppIcon name="solar:cpu-bold" :size="13" /> Process table</h3>
        <table v-if="tasks.length" class="task-table">
          <thead>
            <tr>
              <th>PID</th>
              <th>Kind</th>
              <th>Name</th>
              <th>State</th>
              <th>Progress</th>
              <th>Provider</th>
              <th>Bytes</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="task in tasks" :key="task.id">
              <td data-label="PID">{{ task.id }}</td>
              <td data-label="Kind">{{ task.kind }}</td>
              <td data-label="Name" class="truncate name-cell" :title="task.name">{{ task.name }}</td>
              <td data-label="State">
                <span class="state" :class="`state-${task.state}`">{{ task.state }}</span>
              </td>
              <td data-label="Progress" class="progress-cell">
                <span class="bar"><span class="bar-fill" :style="{ width: `${pct(task)}%` }"></span></span>
                <span class="pct">{{ pct(task) }}%</span>
              </td>
              <td data-label="Provider">{{ task.provider }}</td>
              <td data-label="Bytes">{{ humanBytes(task.bytes) }}</td>
              <td class="task-action">
                <button
                  v-if="task.state === 'running' || task.state === 'pending'"
                  class="ghost-btn danger"
                  type="button"
                  :aria-label="`Kill task ${task.id}`"
                  @click="kill(task)"
                >Kill</button>
                <span v-else class="text-muted">—</span>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-else class="text-muted empty">No tasks yet — start one below, or run `compute run …` in cybsh.</p>
      </div>

      <div class="section">
        <h3 class="section-title"><AppIcon name="solar:server-square-bold" :size="13" /> Compute jobs</h3>
        <div class="jobs">
          <div v-for="job in store.osJobs" :key="job.name" class="job">
            <div class="job-info">
              <span class="job-name">{{ job.name }}</span>
              <span class="text-muted">{{ job.description }}</span>
            </div>
            <button
              class="ghost-btn"
              type="button"
              :disabled="starting"
              :aria-label="`Run ${job.name}`"
              @click="startJob(job.name)"
            >Run</button>
          </div>
          <p v-if="!store.osJobs.length" class="text-muted empty">Catalogue not loaded.</p>
        </div>
        <label class="path-row">
          <span class="text-muted">Path</span>
          <input v-model="jobPath" class="path-input" type="text" spellcheck="false" aria-label="Job path" />
        </label>
        <p v-if="note" class="note">{{ note }}</p>
      </div>
    </template>

    <!-- ══ SCHEDULES (cron) ══ -->
    <template v-else-if="activeTab === 'schedules'">
      <div class="section">
        <div class="sched-toolbar">
          <h3 class="section-title"><AppIcon name="solar:alarm-bold" :size="13" /> Scheduled scripts</h3>
          <button class="ghost-btn" type="button" @click="openCreate">New schedule</button>
        </div>
        <p class="text-muted sched-hint">
          Runs <code>.cybsh</code> scripts on cron or interval expressions (<code>* * * * *</code>,
          <code>every 10m</code>, <code>@daily</code>). Desktop and Docker fire them from the
          background daemon; this tab's browser build ticks them while the tab is visible.
        </p>

        <table v-if="schedules.length" class="task-table sched-table">
          <thead>
            <tr>
              <th>Script</th>
              <th>Schedule</th>
              <th>Next</th>
              <th>State</th>
              <th>Last fired</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in schedules" :key="row.id">
              <td data-label="Script" class="truncate name-cell" :title="row.path">
                {{ row.path }}
                <span v-if="row.description" class="text-muted sched-desc">{{ row.description }}</span>
              </td>
              <td data-label="Schedule"><code>{{ row.expr }}</code></td>
              <td data-label="Next">{{ nextLabel(row) }}</td>
              <td data-label="State">
                <button
                  class="ghost-btn state-toggle"
                  type="button"
                  :class="{ on: row.enabled }"
                  :aria-label="`${row.enabled ? 'Disable' : 'Enable'} schedule ${row.id}`"
                  @click="toggleSchedule(row)"
                >{{ row.enabled ? 'on' : 'off' }}</button>
              </td>
              <td data-label="Last fired">{{ agoLabel(row.lastFiredAt) }}</td>
              <td class="task-action sched-actions">
                <button class="ghost-btn" type="button" :aria-label="`Run ${row.id} now`" @click="runNow(row)">Run</button>
                <button class="ghost-btn" type="button" :aria-label="`Edit ${row.id}`" @click="openEdit(row)">Edit</button>
                <button class="ghost-btn" type="button" :aria-label="`History of ${row.id}`" @click="openHistory(row)">History</button>
                <button class="ghost-btn danger" type="button" :aria-label="`Delete ${row.id}`" @click="removeSchedule(row)">Delete</button>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-else class="text-muted empty">
          No schedules yet — add one below, or run <code>cron add /scripts/job.cybsh "30 2 * * *"</code> in cybsh.
        </p>
        <p v-if="schedNote" class="note">{{ schedNote }}</p>
      </div>

      <UiModal
        v-model:visible="editVisible"
        :title="editId ? 'Edit schedule' : 'New schedule'"
        icon="solar:alarm-bold"
        size="sm"
      >
        <div class="sched-form">
          <UiInput
            v-model="editPath"
            label="Script"
            placeholder="/scripts/backup.cybsh"
            hint="Path to a .cybsh script inside the vault"
            spellcheck="false"
          />
          <UiInput
            v-model="editExpr"
            label="Expression"
            placeholder="30 2 * * *"
            hint="5-field cron (m h dom mon dow), `every 10m`, or an alias like @daily"
            spellcheck="false"
          />
          <div class="sched-presets" aria-label="Expression presets">
            <button
              v-for="p in EXPR_PRESETS"
              :key="p.expr"
              class="ghost-btn"
              type="button"
              @click="editExpr = p.expr"
            >{{ p.label }}</button>
          </div>
          <p class="sched-preview" :class="{ bad: !editPreview.ok }">
            <template v-if="editPreview.ok">
              {{ editPreview.label }} → next {{ editPreview.in }}
              <span v-if="editPreview.next" class="text-muted">({{ editPreview.next.toISOString().replace('T', ' ').slice(0, 16) }} UTC)</span>
            </template>
            <template v-else>{{ editPreview.error }}</template>
          </p>
          <UiInput
            v-model="editDesc"
            label="Description (optional)"
            placeholder="Nightly vault backup"
          />
          <UiCheckbox v-model="editRunOnBoot" label="Run once when the daemon starts" />
          <p v-if="editError" class="note sched-error">{{ editError }}</p>
        </div>
        <template #footer>
          <button class="ghost-btn" type="button" @click="editVisible = false">Cancel</button>
          <button class="ghost-btn primary" type="button" @click="saveSchedule">Save</button>
        </template>
      </UiModal>

      <UiModal
        v-model:visible="historyVisible"
        :title="`Run history${historyRow ? ` — ${historyRow.path}` : ''}`"
        icon="solar:history-bold"
        size="md"
      >
        <p v-if="historyLoading" class="text-muted">Loading runs…</p>
        <table v-else-if="historyRuns.length" class="task-table hist-table">
          <thead>
            <tr>
              <th>Run</th>
              <th>Status</th>
              <th>Finished</th>
              <th>Output</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="run in historyRuns" :key="run.runId">
              <td data-label="Run" class="truncate">{{ run.runId }}</td>
              <td data-label="Status">
                <span class="state" :class="`state-${run.status === 'ok' ? 'done' : 'failed'}`">{{ run.status }}</span>
              </td>
              <td data-label="Finished">{{ run.finishedAt.replace('T', ' ').slice(0, 19) }}</td>
              <td data-label="Output" class="truncate hist-tail" :title="run.outputTail ?? ''">
                {{ (run.outputTail ?? '—').split('\n')[0] }}
              </td>
            </tr>
          </tbody>
        </table>
        <p v-else class="text-muted empty">No runs recorded yet.</p>
        <template #footer>
          <button class="ghost-btn" type="button" @click="historyVisible = false">Close</button>
        </template>
      </UiModal>
    </template>

    <!-- ══ WATCHERS (proactive agent watchers, ANALYSIS.md §4.6) ══ -->
    <template v-else>
      <div class="section">
        <div class="sched-toolbar">
          <h3 class="section-title"><AppIcon name="solar:bell-bold" :size="13" /> {{ t('watchers.title') }}</h3>
          <button class="ghost-btn primary" type="button" @click="evaluateWatchersNow">{{ t('watchers.evaluate') }}</button>
        </div>
        <p class="text-muted sched-hint">{{ t('watchers.hint') }}</p>
        <div class="sched-form">
          <UiInput v-model="wName" :label="t('watchers.name')" placeholder="New photos" />
          <div class="watch-grid">
            <UiInput v-model="wTag" label="Tag equals" placeholder="trip" spellcheck="false" />
            <UiInput v-model="wMime" label="MIME starts with" placeholder="image/" spellcheck="false" />
            <UiInput v-model="wPattern" label="Name regex" placeholder="\.(jpg|png)$" spellcheck="false" />
            <UiInput v-model="wAddTag" label="Then add tag" placeholder="review" spellcheck="false" />
          </div>
          <UiCheckbox v-model="wNotify" label="Notify in the notification center on match" />
          <label class="watch-agent">
            <span class="text-muted">Classify with assistant</span>
            <select v-model="wAgent" class="watch-select" aria-label="Classify with assistant">
              <option value="">off</option>
              <option v-for="c in store.agentConfigs" :key="c.id" :value="c.id">{{ c.name }}</option>
            </select>
          </label>
          <button class="ghost-btn" type="button" @click="addWatchRule">{{ t('watchers.add') }}</button>
        </div>
        <p v-if="watchNote" class="note">{{ watchNote }}</p>
        <table v-if="watchRules.length" class="task-table sched-table">
          <thead>
            <tr>
              <th>Rule</th>
              <th>Match</th>
              <th>Then</th>
              <th>State</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="rule in watchRules" :key="rule.id">
              <td data-label="Rule" class="truncate name-cell" :title="rule.name">{{ rule.name }}</td>
              <td data-label="Match" class="truncate">
                {{ [rule.match.tag && `tag=${rule.match.tag}`, rule.match.mimePrefix && `mime=${rule.match.mimePrefix}`, rule.match.namePattern && `name~${rule.match.namePattern}`].filter(Boolean).join(' · ') || 'everything' }}
              </td>
              <td data-label="Then">{{ [rule.actions.notify && 'notify', rule.actions.addTag && `tag:${rule.actions.addTag}`, rule.actions.agentConfigId && 'agent'].filter(Boolean).join(' · ') }}</td>
              <td data-label="State">
                <button
                  class="ghost-btn state-toggle"
                  type="button"
                  :class="{ on: rule.enabled }"
                  :aria-label="`${rule.enabled ? 'Disable' : 'Enable'} watcher ${rule.id}`"
                  @click="toggleWatchRule(rule.id)"
                >{{ rule.enabled ? 'on' : 'off' }}</button>
              </td>
              <td class="task-action">
                <button class="ghost-btn danger" type="button" :aria-label="`Delete watcher ${rule.id}`" @click="removeWatchRule(rule.id)">Delete</button>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-else class="text-muted empty">{{ t('watchers.noRules') }}</p>
        <p v-if="watchHits > 0" class="note">last run: {{ watchHits }} match{{ watchHits === 1 ? '' : 'es' }}</p>
      </div>
    </template>
  </div>
</template>

<style scoped>
.process-panel {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border);
}

.header-left {
  display: flex;
  align-items: center;
  gap: 10px;
}

.icon-processes {
  color: var(--ui-info);
}

.panel-title {
  margin: 0;
  font-size: 13px;
  letter-spacing: 2px;
}

.ghost-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  font-family: inherit;
  font-size: 11px;
  padding: 3px 8px;
  cursor: pointer;
  min-height: 32px;
}

.ghost-btn:hover:not(:disabled) {
  color: var(--ui-text);
  border-color: var(--ui-border-strong);
}

.ghost-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.ghost-btn.danger {
  color: var(--ui-danger);
  border-color: color-mix(in srgb, var(--ui-danger) 60%, transparent);
}

.ghost-btn.danger:hover {
  color: var(--ui-text);
  background: var(--ui-danger);
  border-color: var(--ui-danger);
}

.ghost-btn.primary {
  color: var(--ui-text);
  border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent);
}

.ghost-btn.primary:hover {
  background: var(--ui-accent-softer);
}

.proc-tabs {
  display: flex;
  gap: 4px;
  padding: 8px 12px 0;
}

.proc-tabs button {
  flex: 1;
  min-width: 0;
  min-height: 34px;
  padding: 4px;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.08em;
  background: transparent;
  border: 1px solid var(--ui-hairline);
  border-radius: 8px;
  color: var(--ui-text-3);
  cursor: pointer;
}

.proc-tabs button.on {
  color: var(--ui-accent);
  border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent);
  background: var(--ui-accent-softer);
}

.stats-row {
  display: flex;
  flex-wrap: wrap;
  gap: 18px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border);
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.stat-key {
  font-size: 11px;
  font-weight: 600;
  color: var(--ui-text-2);
  letter-spacing: 0;
  text-transform: lowercase;
}

.stat-key::first-letter {
  text-transform: uppercase;
}

.stat-val {
  font-size: 13px;
}

.section {
  padding: 12px;
}

.section-title {
  margin: 0 0 8px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
}

.task-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

/* Activity-Monitor style: 11px/600 secondary headers, zebra rows. */
.task-table th {
  text-align: left;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text-2);
  border-bottom: 1px solid var(--ui-separator);
  padding: 4px 6px;
}

.task-table td {
  padding: 5px 6px;
  border-bottom: 1px solid var(--ui-hairline);
  vertical-align: middle;
}

.task-table tbody tr:nth-child(even):not(.sel) {
  background: color-mix(in srgb, var(--ui-text) 3%, transparent);
}

.task-action {
  text-align: right;
  white-space: nowrap;
}

.name-cell {
  max-width: 220px;
}

.state {
  font-size: 11px;
}

.state-running {
  color: var(--ui-accent);
}

.state-pending {
  color: var(--ui-warning);
}

.state-done {
  color: color-mix(in srgb, var(--ui-text) 50%, transparent);
}

.state-failed {
  color: var(--ui-danger);
}

.state-killed {
  color: var(--ui-info);
}

.progress-cell {
  display: flex;
  align-items: center;
  gap: 6px;
}

.bar {
  display: inline-block;
  width: 80px;
  height: 8px;
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
}

.bar-fill {
  display: block;
  height: 100%;
  background: var(--ui-accent);
}

.pct {
  font-size: 11px;
  color: color-mix(in srgb, var(--ui-text) 60%, transparent);
}

.jobs {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.job {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  border: 1px solid var(--ui-border);
  padding: 6px 8px;
}

.job-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.job-name {
  font-size: 12px;
}

.path-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
}

.path-input {
  flex: 1;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 4px 6px;
  outline: none;
}

.path-input:focus {
  border-color: var(--ui-accent);
}

.note {
  margin: 8px 0 0;
  font-size: 11px;
  color: var(--ui-info);
  white-space: pre-wrap;
}

.empty {
  margin: 4px 0 0;
  font-size: 12px;
}

.text-muted {
  color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important;
}

/* ── Schedules tab ── */
.sched-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.sched-hint {
  margin: 0 0 10px;
  font-size: 11px;
  line-height: 1.5;
}

.sched-hint code,
.empty code {
  padding: 0 3px;
  border: 1px solid var(--ui-hairline);
  border-radius: 4px;
  background: color-mix(in srgb, var(--ui-text) 5%, transparent);
  font-size: 10.5px;
}

.sched-desc {
  display: block;
  font-size: 11px;
}

.state-toggle.on {
  color: var(--ui-accent);
  border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent);
}

.sched-actions {
  display: flex;
  gap: 4px;
  justify-content: flex-end;
}

.sched-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.watch-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 8px;
}

.watch-agent {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.watch-select {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 4px 8px;
}

.sched-presets {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.sched-preview {
  margin: 0;
  font-size: 12px;
  color: var(--ui-accent);
}

.sched-preview.bad {
  color: var(--ui-danger);
}

.sched-error {
  color: var(--ui-danger);
}

.hist-tail {
  max-width: 260px;
}

@media (max-width: 700px) {
  .process-panel {
    padding-bottom: env(safe-area-inset-bottom, 0px);
  }

  .panel-header {
    gap: 12px;
    padding: 12px 16px;
  }

  .header-left {
    min-width: 0;
    gap: 8px;
  }

  .header-left .text-muted {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .header-right,
  .header-right .ghost-btn {
    flex: 0 0 auto;
  }

  .ghost-btn {
    min-width: 44px;
    min-height: 44px;
    padding: 8px 12px;
    border-radius: 12px;
  }

  .proc-tabs {
    padding: 10px 16px 0;
  }

  .stats-row {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    padding: 12px 16px;
  }

  .stat {
    min-width: 0;
    padding: 10px 12px;
    border: 1px solid var(--ui-border);
    border-radius: 14px;
    background: color-mix(in srgb, var(--ui-text) 3%, transparent);
  }

  .stat-val {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .section {
    padding: 16px;
  }

  .section-title {
    margin-bottom: 10px;
  }

  .task-table,
  .task-table tbody,
  .task-table tr,
  .task-table td {
    display: block;
    width: auto;
  }

  .task-table thead {
    display: none;
  }

  .task-table tr {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 16px;
    margin-bottom: 10px;
    padding: 14px;
    border: 1px solid var(--ui-border);
    border-radius: 16px;
    background: color-mix(in srgb, var(--ui-text) 3%, transparent);
  }

  .task-table tbody tr:nth-child(even):not(.sel) {
    background: color-mix(in srgb, var(--ui-text) 3%, transparent);
  }

  .task-table td {
    min-width: 0;
    padding: 0;
    border: 0;
    overflow: hidden;
  }

  .task-table td::before {
    display: block;
    margin-bottom: 3px;
    color: var(--ui-text-2);
    content: attr(data-label);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .task-table .name-cell,
  .task-table .progress-cell,
  .task-table .task-action {
    grid-column: 1 / -1;
  }

  .task-table .name-cell {
    max-width: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .task-table .progress-cell {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .task-table .progress-cell::before {
    flex: 0 0 auto;
  }

  .task-table .bar {
    flex: 1;
    min-width: 0;
    width: auto;
    height: 8px;
    border-radius: 999px;
    overflow: hidden;
  }

  .task-table .bar-fill {
    border-radius: inherit;
  }

  .task-table .task-action {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    min-height: 44px;
  }

  .task-table .task-action::before {
    display: none;
  }

  .task-table .task-action .ghost-btn {
    min-width: 72px;
  }

  .job {
    align-items: flex-start;
    gap: 12px;
    padding: 12px;
    border-radius: 14px;
  }

  .job .ghost-btn {
    flex: 0 0 auto;
  }

  .path-row {
    align-items: stretch;
    flex-direction: column;
    gap: 6px;
  }

  .path-input {
    box-sizing: border-box;
    width: 100%;
    min-height: 44px;
    padding: 10px 12px;
    border-radius: 12px;
    font-size: 16px;
  }

  .sched-toolbar {
    align-items: flex-start;
  }

  .sched-actions {
    justify-content: flex-start;
  }
}

@media (prefers-reduced-motion: reduce) {
  .process-panel * {
    scroll-behavior: auto !important;
    transition-duration: 0.01ms !important;
  }
}
</style>
