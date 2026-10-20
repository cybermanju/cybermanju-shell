<!-- CyberManju OS — FaceGroupingPanel (src/components/FaceGroupingPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: signals: engine. -->
<template>
  <div class="spot" @keydown="handleKeydown">
    <div class="spot-col">
      <!-- Hero search — Alfred/Spotlight style -->
      <div class="spot-bar" :class="{ focused: searchFocused }">
        <AppIcon name="solar:magnifer-bold" :size="18" class="spot-bar__icon" />
        <input
          ref="searchRef"
          v-model="query"
          class="spot-bar__input"
          type="text"
          placeholder="Search people by name…"
          aria-label="Search people by name"
          title="Type a name to filter · ↑↓ to navigate · Enter to open · Esc to clear"
          autocomplete="off"
          spellcheck="false"
          @focus="searchFocused = true"
          @blur="searchFocused = false"
        />
        <button
          v-if="query"
          class="spot-bar__clear"
          type="button"
          aria-label="Clear people search"
          title="Clear search (Esc)"
          @click="clearQuery()"
        >
          <AppIcon name="solar:close-bold" :size="13" />
        </button>
        <span v-else class="spot-bar__hint">TYPE TO FILTER</span>
      </div>
      <p class="spot-hint text-muted">Find a person, open their photos, rename or clean up groups. Nothing is deleted without asking first.</p>

      <!-- People strip: icons + names -->
      <div class="spot-people" role="listbox" aria-label="People">
        <button
          v-for="(g, i) in visibleGroups"
          :key="g.id"
          type="button"
          role="option"
          :aria-selected="isActivePerson(g.id)"
          class="person-chip"
          :class="{ active: isActivePerson(g.id) }"
          :title="`${g.name} — ${g.fileIds.length} files (Enter to open)`"
          @click="choosePerson(g.id)"
          @mouseenter="setActivePerson(g.id)"
        >
          <span class="person-avatar" :style="avatarStyle(g)" aria-hidden="true">
            {{ initials(g.name) }}
          </span>
          <span class="person-name truncate">{{ g.name }}</span>
          <span class="person-count">{{ g.fileIds.length }}</span>
        </button>
      </div>

      <!-- Dynamic listing -->
      <div ref="listRef" class="spot-list">
        <UiError
          v-if="store.lastError"
          size="sm"
          title="People unavailable"
          :message="store.lastError"
          retryable
          @retry="refresh()"
        />

        <div v-else-if="store.isLoading && faceGroups.length === 0" class="spot-loading">
          <UiSpinner show-label label="Loading people" />
        </div>

        <template v-else>
          <div v-if="!query && faceGroups.length === 0" class="spot-nomatch">
            <UiEmpty
              size="sm"
              icon="solar:face-scan-circle-bold"
              title="No people yet"
              description="Run batch detect to group faces from your photos. Groups appear here with their photo counts."
            >
              <template #actions>
                <UiButton size="sm" icon="solar:face-scan-circle-bold" :loading="detecting" @click="runDetect()">Run batch detect</UiButton>
                <UiButton size="sm" variant="ghost" icon="solar:refresh-bold" :loading="store.isLoading" @click="refresh()">Rescan</UiButton>
              </template>
            </UiEmpty>
          </div>

          <div v-if="query && matchedGroups.length === 0 && matchedFiles.length === 0" class="spot-nomatch">
            <UiEmpty
              size="sm"
              icon="solar:face-scan-circle-bold"
              :title="`No match for “${query}”`"
              description="Try a different name, or run batch detect to find more faces."
            />
          </div>

          <div v-if="matchedGroups.length > 0" class="spot-section">
            <div class="spot-section__label">PEOPLE · {{ matchedGroups.length }}</div>
            <div
              v-for="g in matchedGroups"
              :key="'p-' + g.id"
              :data-nav="`p:${g.id}`"
              class="spot-row"
              :class="{ active: activeKey === `p:${g.id}` }"
              @click="choosePerson(g.id)"
              @mouseenter="activeKey = `p:${g.id}`"
            >
              <span class="person-avatar person-avatar--sm" :style="avatarStyle(g)" aria-hidden="true">
                {{ initials(g.name) }}
              </span>
              <span class="spot-row__body">
                <span class="spot-row__name truncate" v-html="highlight(g.name)" />
                <span class="spot-row__sub text-muted">{{ g.fileIds.length }} FILES{{ g.algorithm ? ` · ${g.algorithm.toUpperCase()}` : '' }}{{ engineTag(g) }}</span>
              </span>
              <span class="spot-row__enter" aria-hidden="true">⏎</span>
            </div>
          </div>

          <div v-if="matchedFiles.length > 0" class="spot-section">
            <div class="spot-section__label">PHOTOS · {{ matchedFiles.length }}{{ filesTruncated ? '+' : '' }}</div>
            <div
              v-for="f in matchedFiles"
              :key="'f-' + f.entry.id"
              :data-nav="`f:${f.entry.id}`"
              class="spot-row"
              :class="{ active: activeKey === `f:${f.entry.id}` }"
              @click="openFile(f.entry.id)"
              @mouseenter="activeKey = `f:${f.entry.id}`"
            >
              <span class="spot-row__fileicon" aria-hidden="true">
                <AppIcon name="solar:gallery-bold" :size="15" />
              </span>
              <span class="spot-row__body">
                <span class="spot-row__name truncate" v-html="highlight(f.entry.name)" />
                <span class="spot-row__sub text-muted">{{ f.owner }}</span>
              </span>
              <span class="spot-row__enter" aria-hidden="true">⏎</span>
            </div>
          </div>

          <div v-if="!query && faceGroups.length > 0" class="spot-section">
            <div class="spot-section__label">ALL PEOPLE · {{ faceGroups.length }}</div>
            <div
              v-for="g in faceGroups"
              :key="'a-' + g.id"
              :data-nav="`p:${g.id}`"
              class="spot-row"
              :class="{ active: activeKey === `p:${g.id}` }"
              @click="choosePerson(g.id)"
              @mouseenter="activeKey = `p:${g.id}`"
            >
              <span class="person-avatar person-avatar--sm" :style="avatarStyle(g)" aria-hidden="true">
                {{ initials(g.name) }}
              </span>
              <span class="spot-row__body">
                <span class="spot-row__name truncate">{{ g.name }}</span>
                <span class="spot-row__sub text-muted">{{ g.fileIds.length }} FILES{{ g.algorithm ? ` · ${g.algorithm.toUpperCase()}` : '' }}{{ engineTag(g) }}</span>
              </span>
              <span class="spot-row__enter" aria-hidden="true">⏎</span>
            </div>
          </div>
        </template>
      </div>

      <!-- Detail / actions for the active person -->
      <div v-if="activeGroup && !store.lastError" class="spot-detail">
        <div class="spot-detail__head">
          <span class="person-avatar" :style="avatarStyle(activeGroup)" aria-hidden="true">
            {{ initials(activeGroup.name) }}
          </span>
          <div v-if="editingId !== activeGroup.id" class="spot-detail__id">
            <span class="spot-detail__name truncate">{{ activeGroup.name }}</span>
            <span class="spot-detail__meta text-muted">{{ activeGroup.fileIds.length }} FILES</span>
          </div>
          <form v-else class="spot-detail__edit" @submit.prevent="saveRename()">
            <input
              v-model="draftName"
              class="spot-detail__input"
              aria-label="Person name"
              title="Type a name, then press Enter to save"
              maxlength="60"
            />
            <UiButton size="xs" type="submit" :disabled="!draftName.trim()">Save</UiButton>
            <UiButton size="xs" variant="ghost" @click="editingId = null">Cancel</UiButton>
          </form>
        </div>

        <div v-if="memberFiles.length > 0" class="spot-detail__members">
          <button
            v-for="f in memberFiles.slice(0, 6)"
            :key="f.id"
            type="button"
            class="lg-member"
            :title="f.name"
            @click="openFile(f.id)"
          >
            {{ f.name }}
          </button>
          <span v-if="memberFiles.length > 6" class="text-muted spot-detail__more">
            +{{ memberFiles.length - 6 }} MORE
          </span>
        </div>

        <div class="spot-detail__actions">
          <UiButton size="xs" icon="solar:folder-open-bold" title="Open this person's photos in Files" @click="openPerson()">Open photos</UiButton>
          <UiButton size="xs" variant="ghost" icon="solar:pen-bold" :title="`Rename ${activeGroup.name}`" @click="startRename(activeGroup)">Rename</UiButton>
          <UiButton
            size="xs"
            variant="danger"
            icon="solar:trash-bin-trash-bold"
            :title="`Delete ${activeGroup.name} (asks first)`"
            :aria-label="`Delete ${activeGroup.name}`"
            @click="askDelete(activeGroup)"
          >
            Delete
          </UiButton>
        </div>
      </div>

      <UiModal
        v-model:visible="confirmDeleteVisible"
        title="Delete person?"
        :subtitle="pendingDelete?.name ?? ''"
        icon="solar:trash-bin-trash-bold"
        size="sm"
        danger
      >
        <p class="spot-modal-text">
          “{{ pendingDelete?.name }}” with {{ pendingDelete?.fileIds.length ?? 0 }} photos will be removed from People.
          The photos themselves stay in Files.
        </p>
        <template #footer>
          <UiButton variant="ghost" @click="confirmDeleteVisible = false">Keep</UiButton>
          <UiButton variant="danger" icon="solar:trash-bin-trash-bold" :loading="deleting" @click="doDelete()">Delete person</UiButton>
        </template>
      </UiModal>

      <!-- Footer toolbar -->
      <div class="spot-foot">
        <UiButton
          size="sm"
          icon="solar:face-scan-circle-bold"
          title="Group faces from your photos into people"
          :loading="detecting"
          @click="runDetect()"
        >
          Batch detect
        </UiButton>
        <UiButton
          size="sm"
          variant="ghost"
          icon="solar:refresh-bold"
          title="Reload people and photos"
          :loading="store.isLoading"
          @click="refresh()"
        >
          Rescan
        </UiButton>
        <UiButton
          size="sm"
          variant="ghost"
          icon="solar:keyboard-bold"
          icon-only
          title="Keyboard shortcuts (?)"
          aria-label="Open keyboard shortcuts help"
          @click="store.showShortcutsHelp = true"
        />
        <span v-if="lastResult" class="spot-foot__stats text-muted">
          {{ lastResult.clustersCreated }} clusters · {{ lastResult.totalFaces }} faces · {{ lastResult.avgCohesion.toFixed(2) }} cohesion<span v-if="lastResult.detectionEngines.length"> · via {{ lastResult.detectionEngines.join('+').toUpperCase() }}</span>
        </span>
        <span class="spot-foot__keys text-muted" title="Up/Down moves, Enter opens, Esc clears search">↑↓ navigate · ⏎ open · Esc clear</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiModal from '@/components/ui/UiModal.vue'
import UiSpinner from '@/components/ui/UiSpinner.vue'
import { ref, computed, onMounted, nextTick } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import type { FaceGroup, FileNode } from '@/types'

const store = useAppStore()
const wm = useWindowManager()

const faceGroups = computed(() => store.faceGroups)

const query = ref('')
const searchFocused = ref(false)
const searchRef = ref<HTMLInputElement | null>(null)
const listRef = ref<HTMLElement | null>(null)
const detecting = ref(false)
const deleting = ref(false)
const activeKey = ref<string | null>(null)
const editingId = ref<string | null>(null)
const draftName = ref('')
const confirmDeleteVisible = ref(false)
const pendingDelete = ref<FaceGroup | null>(null)
const lastResult = ref<{
  clustersCreated: number
  totalFaces: number
  avgCohesion: number
  strategyUsed: string
  detectionEngines: string[]
} | null>(null)

const q = computed(() => query.value.trim().toLowerCase())

const matchedGroups = computed<FaceGroup[]>(() => {
  if (!q.value) return []
  return faceGroups.value.filter(g => g.name.toLowerCase().includes(q.value))
})

const groupedFileIds = computed(() => new Set(faceGroups.value.flatMap(g => g.fileIds)))

const matchedFiles = computed(() => {
  if (!q.value) return [] as { entry: FileNode; owner: string }[]
  const ownerOf = new Map<string, string>()
  for (const g of faceGroups.value) {
    for (const fid of g.fileIds) {
      if (!ownerOf.has(fid)) ownerOf.set(fid, g.name)
    }
  }
  return store.files
    .filter(f => groupedFileIds.value.has(f.id) && f.name.toLowerCase().includes(q.value))
    .slice(0, 12)
    .map(entry => ({ entry, owner: ownerOf.get(entry.id) ?? 'GROUPED PHOTO' }))
})

const filesTruncated = computed(() => {
  if (!q.value) return false
  const total = store.files.filter(
    f => groupedFileIds.value.has(f.id) && f.name.toLowerCase().includes(q.value)
  ).length
  return total > matchedFiles.value.length
})

/** People strip: full roster when idle, live matches while typing. */
const visibleGroups = computed<FaceGroup[]>(() =>
  q.value ? matchedGroups.value.slice(0, 12) : faceGroups.value.slice(0, 12)
)

const activeGroup = computed<FaceGroup | null>(() => {
  if (!activeKey.value || !activeKey.value.startsWith('p:')) return null
  const id = activeKey.value.slice(2)
  return faceGroups.value.find(g => g.id === id) ?? null
})

const memberFiles = computed<FileNode[]>(() => {
  const g = activeGroup.value
  if (!g) return []
  const byId = new Map(store.files.map(f => [f.id, f]))
  return g.fileIds
    .map(fid => byId.get(fid))
    .filter((f): f is FileNode => Boolean(f))
})

function isActivePerson(id: string): boolean {
  return activeKey.value === `p:${id}`
}

function setActivePerson(id: string) {
  activeKey.value = `p:${id}`
}

function choosePerson(id: string) {
  setActivePerson(id)
  scrollActiveIntoView()
}

function openPerson() {
  const g = activeGroup.value
  if (!g) return
  const first = g.fileIds[0]
  if (first) store.selectFile(first)
  wm.open('files')
}

function openFile(fileId: string) {
  store.selectFile(fileId)
  wm.open('files')
}

function moveActive(delta: 1 | -1) {
  const order: string[] = q.value
    ? [...matchedGroups.value.map(g => `p:${g.id}`), ...matchedFiles.value.map(f => `f:${f.entry.id}`)]
    : faceGroups.value.map(g => `p:${g.id}`)
  if (order.length === 0) return
  const idx = activeKey.value ? order.indexOf(activeKey.value) : -1
  const next = idx === -1 ? (delta === 1 ? 0 : order.length - 1) : (idx + delta + order.length) % order.length
  activeKey.value = order[next]
  scrollActiveIntoView()
}

function activateCurrent() {
  if (!activeKey.value) return
  const [kind, id] = activeKey.value.split(':')
  if (kind === 'p') openPerson()
  else if (kind === 'f' && id) openFile(id)
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    moveActive(1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    moveActive(-1)
  } else if (e.key === 'Enter' && activeKey.value) {
    e.preventDefault()
    activateCurrent()
  } else if (e.key === 'Escape' && query.value) {
    e.preventDefault()
    clearQuery()
  }
}

function scrollActiveIntoView() {
  void nextTick(() => {
    if (!activeKey.value || !listRef.value) return
    const el = listRef.value.querySelector(`[data-nav="${CSS.escape(activeKey.value)}"]`)
    el?.scrollIntoView({ block: 'nearest' })
  })
}

function clearQuery() {
  query.value = ''
  activeKey.value = null
  searchRef.value?.focus()
}

function startRename(g: FaceGroup) {
  editingId.value = g.id
  draftName.value = g.name
}

async function saveRename() {
  if (!editingId.value || !draftName.value.trim()) return
  await store.renameFaceGroup(editingId.value, draftName.value.trim())
  editingId.value = null
}

function askDelete(g: FaceGroup) {
  pendingDelete.value = g
  confirmDeleteVisible.value = true
}

async function doDelete() {
  if (!pendingDelete.value) return
  deleting.value = true
  try {
    const id = pendingDelete.value.id
    await store.deleteFaceGroup(id)
    if (activeKey.value === `p:${id}`) activeKey.value = null
    confirmDeleteVisible.value = false
    pendingDelete.value = null
  } finally {
    deleting.value = false
  }
}

async function runDetect() {
  detecting.value = true
  try {
    const result = await store.detectFacesBatch()
    if (result) {
      lastResult.value = {
        clustersCreated: result.clustersCreated,
        totalFaces: result.totalFaces,
        avgCohesion: result.avgCohesion,
        strategyUsed: result.strategyUsed,
        detectionEngines: Array.isArray(result.detectionEngines) ? result.detectionEngines : [],
      }
    }
  } finally {
    detecting.value = false
  }
}

async function refresh() {
  store.clearError()
  await Promise.allSettled([store.fetchFaceGroups(), store.fetchFiles()])
}

function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return '?'
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}

function avatarStyle(g: FaceGroup): Record<string, string> {
  if (g.color) {
    return {
      borderColor: `color-mix(in srgb, ${g.color} 60%, transparent)`,
      color: g.color,
    }
  }
  return {}
}

/** Short detector tag (`heuristic-*` → `HEURISTIC`, `onnx` → `ONNX`) so a
 *  group row says which engine filled it — approximate groups stay labeled. */
function engineTag(g: FaceGroup): string {
  const e = (g.detectionEngine ?? '').trim().toLowerCase()
  if (!e || e === 'unknown') return ''
  if (e.startsWith('heuristic')) return ' · HEURISTIC'
  if (e === 'mixed') return ' · MIXED ENGINES'
  return ` · ${e.toUpperCase()}`
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function highlight(name: string): string {
  const needle = query.value.trim()
  if (!needle) return escapeHtml(name)
  const safe = escapeHtml(needle).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return escapeHtml(name).replace(new RegExp(`(${safe})`, 'gi'), '<mark>$1</mark>')
}

onMounted(() => {
  void refresh()
  void nextTick(() => searchRef.value?.focus())
})
</script>

<style scoped>
.spot {
  width: 100%;
  min-height: 100%;
  min-height: 100dvh;
  display: flex;
  justify-content: center;
  padding: 22px 18px calc(16px + env(safe-area-inset-bottom, 0px));
  background:
    radial-gradient(60% 34% at 50% 0%, color-mix(in srgb, var(--ui-accent) 9%, transparent), transparent 70%),
    var(--ui-surface);
  overflow-y: auto;
  font-family: var(--ui-font);
  color: var(--ui-text);
}

.spot-col {
  width: 100%;
  max-width: 540px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  margin: 0 auto;
}

/* ── hero search bar ─────────────────────────────────────────────── */
.spot-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 16px;
  height: 52px;
  border-radius: var(--ui-radius-lg);
  background: var(--ui-glass-2);
  border: 1px solid var(--ui-border-strong);
  backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-2), inset 0 1px 0 var(--ui-glass-highlight);
  transition:
    border-color var(--ui-dur) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out);
}

.spot-bar.focused {
  border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent);
  box-shadow:
    var(--ui-shadow-2),
    0 0 0 3px color-mix(in srgb, var(--ui-accent) 16%, transparent),
    inset 0 1px 0 var(--ui-glass-highlight);
}

.spot-bar__icon {
  color: var(--ui-text-3);
  flex-shrink: 0;
}

.spot-bar.focused .spot-bar__icon {
  color: var(--ui-accent);
}

.spot-bar__input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  outline: none;
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 16px;
  font-weight: 550;
  height: 100%;
}

.spot-bar__input::placeholder {
  color: var(--ui-text-faint);
}

.spot-bar__clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: 50%;
  border: 1px solid var(--ui-border);
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  color: var(--ui-text-2);
  cursor: pointer;
  flex-shrink: 0;
}

.spot-bar__clear:hover {
  color: var(--ui-text);
  border-color: var(--ui-border-hover);
}

.spot-bar__hint {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.12em;
  color: var(--ui-text-faint);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs);
  padding: 2px 6px;
  flex-shrink: 0;
}

.spot-hint {
  margin: 0;
  font-size: 11px;
  line-height: 1.5;
  text-align: center;
}

.spot-modal-text {
  margin: 0;
  font-size: 13px;
  line-height: 1.5;
}

/* ── people strip: icons + names ─────────────────────────────────── */
.spot-people {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: center;
}

.person-chip {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  width: 86px;
  padding: 10px 6px 8px;
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  cursor: pointer;
  min-height: 44px;
  -webkit-tap-highlight-color: transparent;
  transition:
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    transform var(--ui-dur-fast) var(--ui-ease-spring);
}

.person-chip:hover {
  transform: translateY(-2px);
  border-color: var(--ui-border-hover);
}

.person-chip.active {
  border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent);
  background: var(--ui-accent-softer);
  box-shadow: var(--ui-glow-soft);
}

.person-chip:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 80%, transparent);
  outline-offset: 2px;
}

.person-avatar {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 0.04em;
  color: var(--ui-accent);
  background:
    radial-gradient(circle at 35% 30%, color-mix(in srgb, var(--ui-accent) 22%, transparent), transparent 70%),
    color-mix(in srgb, var(--ui-text) 5%, transparent);
  border: 1.5px solid color-mix(in srgb, var(--ui-accent) 45%, transparent);
  flex-shrink: 0;
}

.person-avatar--sm {
  width: 32px;
  height: 32px;
  font-size: 11px;
}

.person-name {
  font-size: 10.5px;
  font-weight: 650;
  max-width: 100%;
  text-align: center;
}

.person-count {
  font-family: var(--ui-font-mono);
  font-size: 8.5px;
  font-weight: 700;
  color: var(--ui-text-3);
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  border-radius: var(--ui-radius-full);
  padding: 1px 7px;
}

/* ── dynamic listing ─────────────────────────────────────────────── */
.spot-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 120px;
}

.spot-loading {
  display: flex;
  justify-content: center;
  padding: 26px 0;
}

.spot-nomatch {
  display: flex;
  justify-content: center;
}

.spot-section {
  display: flex;
  flex-direction: column;
  gap: 4px;
  animation: ui-rise var(--ui-dur) var(--ui-ease-out) both;
}

.spot-section__label {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.14em;
  color: var(--ui-text-3);
  padding: 2px 4px;
}

.spot-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 44px;
  padding: 8px 10px;
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
  transition:
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    background-color var(--ui-dur-fast) var(--ui-ease-out);
}

.spot-row:hover,
.spot-row.active {
  border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  background: var(--ui-accent-softer);
}

.spot-row__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.spot-row__name {
  font-size: 12.5px;
  font-weight: 650;
}

.spot-row__name :deep(mark) {
  background: color-mix(in srgb, var(--ui-accent) 35%, transparent);
  color: var(--ui-text);
  border-radius: 2px;
  padding: 0 2px;
}

.spot-row__sub {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  letter-spacing: 0.06em;
}

.spot-row__fileicon {
  display: inline-flex;
  color: var(--ui-text-3);
  flex-shrink: 0;
}

.spot-row.active .spot-row__fileicon {
  color: var(--ui-accent);
}

.spot-row__enter {
  font-family: var(--ui-font-mono);
  font-size: 11px;
  color: var(--ui-text-faint);
  flex-shrink: 0;
}

.spot-row.active .spot-row__enter {
  color: var(--ui-accent);
}

/* ── detail card ─────────────────────────────────────────────────── */
.spot-detail {
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-md);
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-1);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  animation: ui-rise var(--ui-dur) var(--ui-ease-out) both;
}

.spot-detail__head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.spot-detail__id {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.spot-detail__name {
  font-size: 14px;
  font-weight: 750;
}

.spot-detail__meta {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  letter-spacing: 0.08em;
}

.spot-detail__edit {
  flex: 1;
  display: flex;
  gap: 6px;
  min-width: 0;
}

.spot-detail__input {
  flex: 1;
  min-width: 0;
  background: color-mix(in srgb, var(--ui-surface) 80%, transparent);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 50%, transparent);
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 650;
  padding: 5px 9px;
  outline: none;
}

.spot-detail__members {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  align-items: center;
}

.spot-detail__more {
  font-family: var(--ui-font-mono);
  font-size: 9px;
}

.spot-detail__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.spot-detail__actions :deep(button),
.spot-foot :deep(button) {
  min-height: 44px;
}

.lg-member {
  font-family: var(--ui-font-mono);
  font-size: 9.5px;
  min-height: 44px;
  padding: 6px 10px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass);
  color: var(--ui-text-2);
  cursor: pointer;
  max-width: 170px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.lg-member:hover {
  border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent);
  color: var(--ui-accent);
}

/* ── footer ──────────────────────────────────────────────────────── */
.spot-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  border-top: 1px solid var(--ui-hairline);
  padding-top: 10px;
}

.spot-foot__stats {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  letter-spacing: 0.04em;
}

.spot-foot__keys {
  margin-left: auto;
  font-family: var(--ui-font-mono);
  font-size: 8.5px;
  letter-spacing: 0.08em;
}

.text-muted {
  color: var(--ui-text-3) !important;
}

.truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@media (max-width: 560px) {
  .spot {
    padding: max(14px, env(safe-area-inset-top, 0px)) 12px calc(16px + env(safe-area-inset-bottom, 0px));
  }
  .spot-bar {
    height: 52px;
  }
  .spot-bar__input {
    /* Keep iOS from zooming the search field when the keyboard opens. */
    font-size: 16px;
  }
  .spot-people {
    justify-content: flex-start;
    gap: 8px;
  }
  .person-chip {
    flex: 1 1 calc(25% - 8px);
    width: auto;
    min-width: 64px;
    max-width: 86px;
  }
  .spot-row {
    padding: 8px 12px;
  }
  .spot-detail {
    padding: 14px;
  }
  .spot-detail__edit {
    flex-wrap: wrap;
  }
  .spot-detail__input {
    flex-basis: 100%;
    min-height: 44px;
    font-size: 16px;
  }
  .spot-foot__stats {
    flex-basis: 100%;
    line-height: 1.45;
  }
  .spot-foot__keys {
    display: none;
  }
  :deep(.ui-modal-layer) {
    padding-bottom: env(safe-area-inset-bottom, 0px);
  }
  :deep(.ui-modal) {
    max-height: calc(100dvh - env(safe-area-inset-top, 0px));
  }
  :deep(.ui-modal__body) {
    overscroll-behavior: contain;
    -webkit-overflow-scrolling: touch;
  }
}

@media (max-width: 360px) {
  .spot {
    padding-left: 10px;
    padding-right: 10px;
  }
  .person-chip {
    flex-basis: calc(33.333% - 8px);
  }
  .spot-row__sub {
    font-size: 8px;
    letter-spacing: 0.03em;
  }
  .spot-detail__actions :deep(button) {
    flex: 1 1 calc(50% - 6px);
  }
}

@media (prefers-reduced-motion: reduce) {
  .spot-section,
  .spot-detail {
    animation: none;
  }
  .person-chip:hover {
    transform: none;
  }
}
</style>
