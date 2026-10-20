<!-- CyberManju OS — OmniSearchPanel (src/components/OmniSearchPanel.vue). -->
<!-- Role: merged FACES + COLLECTIONS + SEARCH window — one centered input, grouped glass dropdown. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/stores/app, @/composables/*, @/types, @/utils/omniSearch, vue. -->
<!-- Contracts: section order apps > collections > people | providers > folders > files > content; ctx menu only fires store-backed actions. -->
<template>
  <div class="omni" @keydown="handleKeydown">
    <div class="omni-col">
      <!-- Single centered line — the whole window is this input. -->
      <div class="omni-bar" :class="{ focused: focused || query }">
        <AppIcon name="solar:magnifier-bold" :size="18" class="omni-bar__icon" />
        <input
          ref="inputRef"
          v-model="query"
          class="omni-bar__input"
          type="text"
          placeholder="Search anything..."
          aria-label="Search anything — apps, collections, people, providers, files"
          title="Apps · collections · people · providers · folders · files · contents — ↑↓ navigate · Enter opens · right-click manages"
          autocomplete="off"
          spellcheck="false"
          @focus="focused = true"
          @blur="focused = false"
        />
        <button
          v-if="query"
          class="omni-bar__clear"
          type="button"
          aria-label="Clear search"
          title="Clear search (Esc)"
          @click="clearQuery()"
        >
          <AppIcon name="solar:close-bold" :size="13" />
        </button>
        <span v-else class="omni-bar__hint">TYPE TO SEARCH</span>
      </div>

      <!-- Animated glass dropdown — red-accent freeze style. -->
      <Transition name="omni-drop">
        <div
          v-if="dropdownOpen"
          ref="dropRef"
          class="omni-drop"
          role="listbox"
          aria-label="Search results"
          :aria-activedescendant="activeDescendantId"
        >
          <div v-if="store.isSearching" class="omni-loading" role="status" aria-live="polite">
            <UiSpinner size="sm" show-label label="Searching" />
          </div>

          <!-- Idle: recent searches jump straight back in. -->
          <div v-if="!q && recentSearches.length" class="omni-sec">
            <div class="omni-sec__head"><span class="omni-tag">RECENT</span><span class="omni-div" aria-hidden="true" /></div>
            <button
              v-for="sq in recentSearches"
              :key="sq"
              type="button"
              class="omni-row"
              :title="`Search again for ${sq}`"
              @click="rerun(sq)"
            >
              <span class="omni-row__icon"><AppIcon name="solar:history-bold" :size="14" /></span>
              <span class="omni-row__body"><span class="omni-row__name">{{ sq }}</span></span>
              <span class="omni-row__enter" aria-hidden="true">⏎</span>
            </button>
          </div>

          <div v-for="(sec, si) in sections" :key="sec.id" class="omni-sec">
            <div class="omni-sec__head">
              <span class="omni-tag">{{ OMNI_SECTION_TAG[sec.id] }} · {{ sec.rows.length }}</span>
              <span class="omni-div" aria-hidden="true" />
            </div>
            <div
              v-for="(row, ri) in sec.rows"
              :key="sec.id + ':' + row.key"
              :id="`omni-${si}-${ri}`"
              :data-nav="sec.id + ':' + row.key"
              class="omni-row"
              :class="{ active: activeKey === sec.id + ':' + row.key }"
              :style="{ '--i': ri }"
              role="option"
              :aria-selected="activeKey === sec.id + ':' + row.key"
              tabindex="0"
              :title="rowHint(sec.id, row)"
              @click="activate(sec.id, row)"
              @mouseenter="activeKey = sec.id + ':' + row.key"
              @contextmenu.prevent="openCtx($event, sec.id, row)"
            >
              <span class="omni-row__icon"><AppIcon :name="row.icon" :size="15" /></span>
              <span class="omni-row__body">
                <span class="omni-row__name" v-html="highlight(row.name)" />
                <span v-if="row.sub" class="omni-row__sub">{{ row.sub }}</span>
              </span>
              <!-- Inline rename editor (right-click → Rename). -->
              <form
                v-if="renameTarget?.key === sec.id + ':' + row.key"
                class="omni-inline"
                @submit.prevent="saveRename()"
                @click.stop
              >
                <input
                  v-model="renameDraft"
                  class="omni-inline__input"
                  aria-label="New name"
                  maxlength="120"
                />
                <button type="submit" class="omni-inline__btn" :disabled="!renameDraft.trim()">Save</button>
                <button type="button" class="omni-inline__btn omni-inline__btn--ghost" @click="renameTarget = null">Cancel</button>
              </form>
              <!-- Inline tag editor (right-click → Tags). -->
              <form
                v-else-if="tagTarget?.key === sec.id + ':' + row.key"
                class="omni-inline"
                @submit.prevent="saveTags()"
                @click.stop
              >
                <input
                  v-model="tagDraft"
                  class="omni-inline__input"
                  aria-label="Comma-separated tags"
                  placeholder="tag1, tag2"
                  maxlength="240"
                />
                <button type="submit" class="omni-inline__btn">Save</button>
                <button type="button" class="omni-inline__btn omni-inline__btn--ghost" @click="tagTarget = null">Cancel</button>
              </form>
              <span v-else class="omni-row__enter" aria-hidden="true">⏎</span>
            </div>
          </div>

          <div v-if="canLoadMoreContent" class="omni-more">
            <UiButton
              size="sm"
              icon="solar:double-alt-arrow-down-bold"
              :disabled="store.isSearching"
              @click="store.loadMoreSearchResults()"
            >
              LOAD MORE ({{ store.searchTotalResults - contentRows.length }} MORE)
            </UiButton>
          </div>

          <div v-if="q && empty" class="omni-empty">
            <UiEmpty
              size="sm"
              icon="solar:magnifier-bold"
              :title="`No results for “${query.trim()}”`"
              description="Try fewer words — or create a collection from this name with a right-click."
            >
              <template #actions>
                <UiButton size="sm" variant="ghost" @click="clearQuery()">Clear search</UiButton>
                <UiButton size="sm" icon="solar:add-bold" @click="createCollectionFromQuery()">New collection</UiButton>
              </template>
            </UiEmpty>
          </div>

          <div class="omni-foot">
            <span class="omni-foot__counts">{{ totalRows }} RESULT{{ totalRows === 1 ? '' : 'S' }}</span>
            <span class="omni-foot__keys">↑↓ navigate · ⏎ open · right-click manages · Esc clear</span>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiSpinner from '@/components/ui/UiSpinner.vue'
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { useContextMenu, type ContextMenuEntry } from '@/composables/useContextMenu'
import { MODULE_METADATA, type PanelType } from '@/types'
import { resolvePanel } from '@/utils/panels'
import { isEditableTarget } from '@/utils/dom'
import { OMNI_SECTION_TAG, buildOmniSections, type OmniSectionId } from '@/utils/omniSearch'

const store = useAppStore()
const wm = useWindowManager()
const ctx = useContextMenu()

interface OmniRow {
  key: string
  kind: OmniSectionId
  name: string
  sub?: string
  icon: string
  refId: string
}

const query = ref(store.searchQuery ?? '')
const focused = ref(false)
const inputRef = ref<HTMLInputElement | null>(null)
const dropRef = ref<HTMLElement | null>(null)
const activeKey = ref<string | null>(null)
const renameTarget = ref<{ key: string; kind: OmniSectionId; refId: string } | null>(null)
const renameDraft = ref('')
const tagTarget = ref<{ key: string; fileId: string } | null>(null)
const tagDraft = ref('')
let debounce: ReturnType<typeof setTimeout> | null = null

const q = computed(() => query.value.trim())
const dropdownOpen = computed(() => focused.value || q.value.length > 0 || totalRows.value > 0)

const recentSearches = ref<string[]>((() => {
  try { return JSON.parse(localStorage.getItem('cybermanju_recent_searches') || '[]') as string[] } catch { return [] }
})())

function rememberSearch(sq: string) {
  if (!sq.trim()) return
  const arr = recentSearches.value.filter((s) => s !== sq)
  arr.unshift(sq)
  if (arr.length > 10) arr.pop()
  recentSearches.value = arr
  try { localStorage.setItem('cybermanju_recent_searches', JSON.stringify(arr)) } catch {}
}

/* ── section inputs (ordered downstream by buildOmniSections) ── */

const appRows = computed<OmniRow[]>(() => {
  const seen = new Set<string>()
  const rows: OmniRow[] = []
  for (const [id, meta] of Object.entries(MODULE_METADATA)) {
    const canonical = resolvePanel(id as PanelType)
    if (canonical !== (id as PanelType) || seen.has(canonical)) continue
    seen.add(canonical)
    if (canonical === 'landing') continue
    rows.push({
      key: canonical, kind: 'apps', name: meta.label, sub: meta.description,
      icon: meta.icon, refId: canonical,
    })
  }
  return rows
})

const collectionRows = computed<OmniRow[]>(() =>
  store.collections.map((c) => ({
    key: c.id, kind: 'collections' as const, name: c.name,
    sub: `${c.collectionType.toUpperCase()} · ${c.itemIds.length} ITEMS`,
    icon: 'solar:library-bold', refId: c.id,
  })),
)

const peopleRows = computed<OmniRow[]>(() =>
  store.faceGroups.map((g) => ({
    key: g.id, kind: 'people' as const, name: g.name,
    sub: `${g.fileIds.length} FILES${g.detectionEngine ? ` · ${g.detectionEngine.toUpperCase()}` : ''}`,
    icon: 'solar:face-scan-circle-bold', refId: g.id,
  })),
)

const providerRows = computed<OmniRow[]>(() =>
  store.syncConfigs.map((c) => ({
    key: c.id, kind: 'providers' as const,
    name: c.name || `${c.backendType}`,
    sub: `${String(c.backendType).toUpperCase()} · OPEN FILE MANAGER IN PROVIDER`,
    icon: 'solar:cloud-bold', refId: c.id,
  })),
)

const folderRows = computed<OmniRow[]>(() =>
  store.files
    .filter((f) => f.fileType === 'folder')
    .map((f) => ({
      key: f.id, kind: 'folders' as const, name: f.name,
      sub: f.path || 'FOLDER', icon: 'solar:folder-bold', refId: f.id,
    })),
)

const fileRows = computed<OmniRow[]>(() =>
  store.files
    .filter((f) => f.fileType !== 'folder')
    .map((f) => ({
      key: f.id, kind: 'files' as const, name: f.name,
      sub: [f.path, (f.tags ?? []).join(' #')].filter(Boolean).join(' · ') || undefined,
      icon: f.encrypted ? 'solar:lock-bold' : 'solar:file-bold', refId: f.id,
    })),
)

const contentRows = computed<OmniRow[]>(() =>
  store.searchResults.map((r) => ({
    key: r.fileId, kind: 'content' as const, name: r.fileName,
    sub: r.snippet || `${r.matchType ?? 'match'} · score ${r.score.toFixed(2)}`,
    icon: 'solar:file-text-bold', refId: r.fileId,
  })),
)

const sections = computed(() =>
  buildOmniSections(q.value, {
    apps: appRows.value, collections: collectionRows.value, people: peopleRows.value,
    providers: providerRows.value, folders: folderRows.value, files: fileRows.value,
    content: contentRows.value,
  }),
)

const flatRows = computed(() => sections.value.flatMap((s) => s.rows.map((r) => ({ sec: s.id as OmniSectionId, row: r }))))
const totalRows = computed(() => flatRows.value.length)
const empty = computed(() => totalRows.value === 0)

/** BM25 page behind the visible content rows — offer the next page inline. */
const canLoadMoreContent = computed(() =>
  q.value.length >= 2
  && contentRows.value.length > 0
  && store.searchTotalResults > contentRows.value.length,
)

/** Listbox active descendant for screen readers (stable si/ri ids). */
const activeDescendantId = computed(() => {
  if (!activeKey.value) return undefined
  const flat = flatRows.value.findIndex(({ sec, row }) => `${sec}:${row.key}` === activeKey.value)
  if (flat === -1) return undefined
  let n = 0
  for (let si = 0; si < sections.value.length; si++) {
    const rows = sections.value[si].rows
    if (flat < n + rows.length) return `omni-${si}-${flat - n}`
    n += rows.length
  }
  return undefined
})

/** Drop a highlight that no longer exists (new keystroke, new results). */
watch(flatRows, (rows) => {
  if (activeKey.value && !rows.some(({ sec, row }) => `${sec}:${row.key}` === activeKey.value)) {
    activeKey.value = null
  }
})

/* ── search wiring (single source: store.searchQuery) ── */

watch(query, (v) => {
  if (debounce) clearTimeout(debounce)
  if (!v.trim()) {
    // Instant reset — stale BM25 rows must vanish the moment the line is
    // cleared, not linger behind the debounce.
    store.searchQuery = ''
    void store.searchFiles('')
    return
  }
  debounce = setTimeout(() => {
    store.searchQuery = v
    if (v.trim().length >= 2) void store.searchFiles(v.trim())
  }, 250)
})
watch(() => store.searchResults, (results) => {
  if (results.length > 0 && q.value) rememberSearch(q.value)
})

function clearQuery() {
  query.value = ''
  store.searchQuery = ''
  activeKey.value = null
  inputRef.value?.focus()
}

function rerun(sq: string) {
  query.value = sq
  store.searchQuery = sq
  void store.searchFiles(sq)
}

/* ── openers ── */

function openFilesAt(fileId: string) {
  store.selectFile(fileId)
  wm.open('files')
}

function activate(sec: OmniSectionId, row: OmniRow) {
  if (q.value) rememberSearch(q.value)
  if (sec === 'apps') wm.open(row.refId as PanelType)
  else if (sec === 'collections') {
    const col = store.collections.find((c) => c.id === row.refId)
    const first = col?.itemIds[0]
    if (first) openFilesAt(first)
    else wm.open('files')
  } else if (sec === 'people') {
    const g = store.faceGroups.find((x) => x.id === row.refId)
    if (g?.fileIds[0]) openFilesAt(g.fileIds[0])
    else wm.open('files')
  } else if (sec === 'providers') {
    const path = `/providers/${row.refId}`
    store.currentPath = path
    void store.fetchFiles(path)
    wm.open('files')
  } else {
    openFilesAt(row.refId)
  }
}

function rowHint(sec: OmniSectionId, row: OmniRow): string {
  if (sec === 'providers') return `${row.name} — Enter opens the file manager inside this provider · right-click for more`
  return `${row.name} — Enter opens · right-click manages, tags, renames`
}

/* ── rich right-click context menu (store-backed actions only) ── */

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    ta.remove()
  }
}

function fileById(id: string) {
  return store.files.find((f) => f.id === id)
}

function startRename(kind: OmniSectionId, row: OmniRow) {
  if (kind === 'people' || kind === 'folders' || kind === 'files' || kind === 'content') {
    renameTarget.value = { key: `${kind}:${row.key}`, kind, refId: row.refId }
    renameDraft.value = row.name
    void nextTick(() => {
      dropRef.value?.querySelector<HTMLInputElement>('.omni-inline__input')?.focus()
    })
  }
}

async function saveRename() {
  const t = renameTarget.value
  if (!t || !renameDraft.value.trim()) return
  const name = renameDraft.value.trim()
  if (t.kind === 'people') await store.renameFaceGroup(t.refId, name)
  else await store.renameFile(t.refId, name)
  renameTarget.value = null
}

function startTags(row: OmniRow) {
  const f = fileById(row.refId)
  tagTarget.value = { key: `${row.kind}:${row.key}`, fileId: row.refId }
  tagDraft.value = (f?.tags ?? []).join(', ')
  void nextTick(() => {
    dropRef.value?.querySelector<HTMLInputElement>('.omni-inline__input')?.focus()
  })
}

async function saveTags() {
  const t = tagTarget.value
  if (!t) return
  const tags = tagDraft.value.split(',').map((s) => s.trim()).filter(Boolean)
  await store.setFileTags(t.fileId, tags)
  tagTarget.value = null
}

async function createCollectionFromQuery() {
  const name = q.value || 'Untitled'
  await store.createCollection(name, 'custom', '#FFFFFF')
}

function openCtx(e: MouseEvent, sec: OmniSectionId, row: OmniRow) {
  const entries: ContextMenuEntry[] = []
  const fileSubmenu: ContextMenuEntry[] = store.collections.map((c) => ({
    id: `add-to-${c.id}`, label: c.name.toUpperCase().slice(0, 32), icon: 'solar:library-bold',
    action: () => { void store.addToCollection(c.id, row.refId) },
  }))

  if (sec === 'apps') {
    entries.push(
      { id: 'open', label: 'OPEN', icon: 'solar:folder-open-bold', action: () => activate(sec, row) },
      { id: 'div1', label: '', divider: true },
      { id: 'copy', label: 'COPY APP NAME', icon: 'solar:copy-bold', action: () => { void copyText(row.name) } },
    )
  } else if (sec === 'collections') {
    const col = store.collections.find((c) => c.id === row.refId)
    entries.push(
      { id: 'open', label: 'OPEN COLLECTION', icon: 'solar:folder-open-bold', action: () => activate(sec, row) },
      { id: 'copy', label: 'COPY NAME', icon: 'solar:copy-bold', action: () => { void copyText(row.name) } },
      { id: 'div1', label: '', divider: true },
      {
        id: 'add-sel', label: 'ADD SELECTED FILE', icon: 'solar:file-add-bold',
        disabled: !store.selectedFileId,
        action: () => { if (store.selectedFileId) void store.addToCollection(row.refId, store.selectedFileId) },
      },
      ...(col && store.selectedFileId && col.itemIds.includes(store.selectedFileId)
        ? [{ id: 'rm-sel', label: 'REMOVE SELECTED FILE', icon: 'solar:file-minus-bold', action: () => { void store.removeFromCollection(row.refId, store.selectedFileId as string) } } as ContextMenuEntry]
        : []),
      { id: 'new', label: 'NEW COLLECTION FROM QUERY', icon: 'solar:add-bold', action: () => { void createCollectionFromQuery() } },
    )
  } else if (sec === 'people') {
    const others = store.faceGroups.filter((g) => g.id !== row.refId).slice(0, 8)
    entries.push(
      { id: 'open', label: 'OPEN PHOTOS', icon: 'solar:folder-open-bold', action: () => activate(sec, row) },
      { id: 'rename', label: 'RENAME', icon: 'solar:pen-bold', action: () => startRename(sec, row) },
      { id: 'div1', label: '', divider: true },
      {
        id: 'merge', label: 'MERGE INTO…', icon: 'solar:git-branch-bold',
        submenu: others.map((g) => ({
          id: `merge-${g.id}`, label: g.name.toUpperCase().slice(0, 32), icon: 'solar:face-scan-circle-bold',
          action: () => { void store.mergeFaceGroups(row.refId, g.id) },
        })),
        disabled: others.length === 0,
      },
      { id: 'similar', label: 'FIND SIMILAR FACES', icon: 'solar:users-group-two-rounded-bold', action: () => { void store.findSimilarFaces(row.refId) } },
      { id: 'detect', label: 'BATCH DETECT', icon: 'solar:face-scan-circle-bold', action: () => { void store.detectFacesBatch() } },
      { id: 'div2', label: '', divider: true },
      { id: 'delete', label: 'DELETE GROUP', icon: 'solar:trash-bin-trash-bold', action: () => { void store.deleteFaceGroup(row.refId) } },
    )
  } else if (sec === 'providers') {
    entries.push(
      { id: 'open', label: 'OPEN FILE MANAGER IN PROVIDER', icon: 'solar:folder-open-bold', action: () => activate(sec, row) },
      { id: 'sync', label: 'OPEN SYNC PANEL', icon: 'solar:refresh-bold', action: () => wm.open('sync') },
      { id: 'div1', label: '', divider: true },
      { id: 'copy', label: 'COPY PROVIDER ID', icon: 'solar:copy-bold', action: () => { void copyText(row.refId) } },
    )
  } else {
    // folders · files · content — full file management.
    const f = fileById(row.refId)
    entries.push(
      { id: 'open', label: 'OPEN', icon: 'solar:folder-open-bold', action: () => activate(sec, row) },
      { id: 'star', label: f?.isStarred ? 'UNSTAR' : 'STAR', icon: 'solar:star-bold', action: () => { void store.toggleStar(row.refId) } },
      { id: 'div1', label: '', divider: true },
      { id: 'rename', label: 'RENAME', icon: 'solar:pen-bold', action: () => startRename(sec, row) },
      { id: 'tags', label: 'EDIT TAGS', icon: 'solar:tag-bold', action: () => startTags(row) },
      {
        id: 'addto', label: 'ADD TO COLLECTION…', icon: 'solar:library-bold',
        submenu: fileSubmenu.length ? fileSubmenu : undefined,
        disabled: fileSubmenu.length === 0,
        action: fileSubmenu.length ? undefined : () => { void createCollectionFromQuery() },
      },
      { id: 'div2', label: '', divider: true },
      { id: 'copy', label: 'COPY PATH', icon: 'solar:copy-bold', action: () => { void copyText(f?.path ?? row.name) } },
      { id: 'show', label: 'SHOW IN FOLDER', icon: 'solar:folder-search-bold', action: () => openFilesAt(row.refId) },
      { id: 'div3', label: '', divider: true },
      { id: 'delete', label: 'DELETE', icon: 'solar:trash-bin-trash-bold', action: () => { void store.deleteFile(row.refId) } },
    )
  }
  ctx.replaceEntries('omni_result', entries)
  ctx.open(e, 'omni_result', { sec, row })
}

/* ── keyboard ── */

function moveActive(delta: 1 | -1) {
  const order = flatRows.value.map(({ sec, row }) => `${sec}:${row.key}`)
  if (!order.length) return
  const idx = activeKey.value ? order.indexOf(activeKey.value) : -1
  const next = idx === -1 ? (delta === 1 ? 0 : order.length - 1) : (idx + delta + order.length) % order.length
  activeKey.value = order[next]
  void nextTick(() => {
    dropRef.value?.querySelector(`[data-nav="${CSS.escape(activeKey.value as string)}"]`)?.scrollIntoView({ block: 'nearest' })
  })
}

function handleKeydown(e: KeyboardEvent) {
  // Typing inside the rename/tag editors must never steer the list —
  // only the main search line shares keys with navigation.
  if (isEditableTarget(e.target) && e.target !== inputRef.value) return
  if (e.key === 'ArrowDown') { e.preventDefault(); moveActive(1) }
  else if (e.key === 'ArrowUp') { e.preventDefault(); moveActive(-1) }
  else if (e.key === 'Enter' && activeKey.value) {
    e.preventDefault()
    const found = flatRows.value.find(({ sec, row }) => `${sec}:${row.key}` === activeKey.value)
    if (found) activate(found.sec, found.row)
  } else if (e.key === 'Escape' && q.value) { e.preventDefault(); clearQuery() }
}

/* ── highlight ── */

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

function highlight(name: string): string {
  const needle = q.value
  if (!needle) return escapeHtml(name)
  const safe = escapeHtml(needle).replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  return escapeHtml(name).replace(new RegExp(`(${safe})`, 'gi'), '<mark>$1</mark>')
}

onMounted(() => {
  void Promise.allSettled([store.fetchFiles(), store.fetchCollections(), store.fetchFaceGroups(), store.fetchSyncConfigs()])
  // Handoff from another surface (e.g. Code Studio search) may leave a
  // query with stale results — re-fire so content rows match the line.
  if (q.value.length >= 2) void store.searchFiles(q.value)
  void nextTick(() => inputRef.value?.focus())
})

onUnmounted(() => {
  if (debounce) clearTimeout(debounce)
  ctx.unregisterContext('omni_result')
})
</script>

<style scoped>
.omni {
  --omni-accent: #ff3b30;
  width: 100%;
  min-height: 100%;
  display: flex;
  justify-content: center;
  padding: 8vh 18px 16px;
  background:
    radial-gradient(60% 34% at 50% 0%, color-mix(in srgb, var(--omni-accent) 10%, transparent), transparent 70%),
    var(--ui-surface);
  overflow-y: auto;
  font-family: var(--ui-font);
  color: var(--ui-text);
}

.omni-col {
  width: 100%;
  max-width: 560px;
  display: flex;
  flex-direction: column;
  gap: 0;
  margin: 0 auto;
  align-self: flex-start;
}

/* ── the one line ── */
.omni-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 18px;
  height: 56px;
  border-radius: var(--ui-radius-lg);
  background: var(--ui-glass-2);
  border: 1px solid var(--ui-border-strong);
  backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-2), inset 0 1px 0 var(--ui-glass-highlight);
  transition: border-color var(--ui-dur) var(--ui-ease-out), box-shadow var(--ui-dur) var(--ui-ease-out);
}

.omni-bar.focused {
  border-color: color-mix(in srgb, var(--omni-accent) 65%, transparent);
  box-shadow:
    var(--ui-shadow-2),
    0 0 0 3px color-mix(in srgb, var(--omni-accent) 18%, transparent),
    0 0 32px color-mix(in srgb, var(--omni-accent) 12%, transparent),
    inset 0 1px 0 var(--ui-glass-highlight);
}

.omni-bar__icon { color: var(--ui-text-3); flex-shrink: 0; }
.omni-bar.focused .omni-bar__icon { color: var(--omni-accent); }
.omni-bar__input {
  flex: 1; min-width: 0; background: transparent; border: none; outline: none;
  color: var(--ui-text); font-family: var(--ui-font); font-size: 17px; font-weight: 550; height: 100%;
}
.omni-bar__input::placeholder { color: var(--ui-text-faint); }
.omni-bar__clear {
  display: inline-flex; align-items: center; justify-content: center;
  width: 44px; height: 44px; border-radius: 50%;
  border: 1px solid var(--ui-border);
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  color: var(--ui-text-2); cursor: pointer; flex-shrink: 0;
}
.omni-bar__clear:hover { color: var(--omni-accent); border-color: color-mix(in srgb, var(--omni-accent) 55%, transparent); }
.omni-bar__hint {
  font-family: var(--ui-font-mono); font-size: 9px; font-weight: 700; letter-spacing: 0.12em;
  color: var(--ui-text-faint); border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs); padding: 2px 6px; flex-shrink: 0;
}

/* ── dropdown: animated glassmorphy, red accent, freeze ── */
.omni-drop {
  margin-top: 10px;
  border-radius: var(--ui-radius-lg);
  background: color-mix(in srgb, var(--ui-glass-2) 82%, transparent);
  border: 1px solid color-mix(in srgb, var(--omni-accent) 28%, var(--ui-border-strong));
  backdrop-filter: blur(calc(var(--ui-blur-strong) * 1.4)) saturate(calc(var(--ui-saturate) * 1.2));
  -webkit-backdrop-filter: blur(calc(var(--ui-blur-strong) * 1.4)) saturate(calc(var(--ui-saturate) * 1.2));
  box-shadow:
    var(--ui-shadow-menu),
    0 0 48px color-mix(in srgb, var(--omni-accent) 10%, transparent),
    inset 0 1px 0 var(--ui-glass-highlight);
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-height: min(62vh, 560px);
  overflow-y: auto;
  overscroll-behavior: contain;
  transform-origin: top center;
}

.omni-drop-enter-active { animation: omni-in var(--ui-dur) var(--ui-ease-spring) both; }
.omni-drop-leave-active { animation: omni-out 120ms ease-in both; }
@keyframes omni-in {
  from { opacity: 0; transform: translateY(-10px) scale(0.98); filter: blur(6px); }
  to { opacity: 1; transform: translateY(0) scale(1); filter: blur(0); }
}
@keyframes omni-out {
  from { opacity: 1; transform: translateY(0) scale(1); }
  to { opacity: 0; transform: translateY(-6px) scale(0.99); }
}

.omni-loading { display: flex; justify-content: center; padding: 10px 0; }

.omni-sec { display: flex; flex-direction: column; gap: 4px; }
.omni-sec__head { display: flex; align-items: center; gap: 8px; padding: 2px 4px; }
.omni-tag {
  font-family: var(--ui-font-mono); font-size: 9px; font-weight: 800; letter-spacing: 0.14em;
  color: var(--omni-accent);
  background: color-mix(in srgb, var(--omni-accent) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--omni-accent) 35%, transparent);
  border-radius: var(--ui-radius-full); padding: 2px 9px; white-space: nowrap; flex-shrink: 0;
}
.omni-div { flex: 1; height: 1px; background: linear-gradient(90deg, color-mix(in srgb, var(--omni-accent) 35%, transparent), var(--ui-hairline)); }

.omni-row {
  display: flex; align-items: center; gap: 10px; width: 100%;
  min-height: 44px; padding: 8px 10px; border-radius: var(--ui-radius-md);
  background: transparent; border: 1px solid transparent;
  cursor: pointer; text-align: left; color: var(--ui-text); font-family: var(--ui-font);
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), background-color var(--ui-dur-fast) var(--ui-ease-out);
  animation: omni-row-in 160ms var(--ui-ease-out) both;
  animation-delay: min(calc(var(--i, 0) * 18ms), 150ms);
}
@keyframes omni-row-in {
  from { opacity: 0; transform: translateY(-4px); }
  to { opacity: 1; transform: translateY(0); }
}
.omni-row:hover, .omni-row.active {
  border-color: color-mix(in srgb, var(--omni-accent) 45%, transparent);
  background: color-mix(in srgb, var(--omni-accent) 9%, var(--ui-accent-softer));
}
.omni-row:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--omni-accent) 80%, transparent);
  outline-offset: 1px;
}
.omni-row__icon { display: inline-flex; color: var(--ui-text-3); flex-shrink: 0; }
.omni-row:hover .omni-row__icon, .omni-row.active .omni-row__icon { color: var(--omni-accent); }
.omni-row__body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.omni-row__name { font-size: 12.5px; font-weight: 650; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.omni-row__name :deep(mark) {
  background: color-mix(in srgb, var(--omni-accent) 38%, transparent);
  color: var(--ui-text); border-radius: 2px; padding: 0 2px;
}
.omni-row__sub {
  font-family: var(--ui-font-mono); font-size: 9px; letter-spacing: 0.05em; color: var(--ui-text-3);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.omni-row__enter { font-family: var(--ui-font-mono); font-size: 11px; color: var(--ui-text-faint); flex-shrink: 0; }
.omni-row.active .omni-row__enter { color: var(--omni-accent); }

.omni-inline { display: flex; gap: 6px; flex: 1; min-width: 0; }
.omni-inline__input {
  flex: 1; min-width: 0; min-height: 44px;
  background: color-mix(in srgb, var(--ui-surface) 85%, transparent);
  border: 1px solid color-mix(in srgb, var(--omni-accent) 55%, transparent);
  border-radius: var(--ui-radius-sm); color: var(--ui-text);
  font-family: var(--ui-font); font-size: 13px; padding: 5px 9px; outline: none;
}
.omni-inline__btn {
  min-height: 44px; padding: 6px 12px; border-radius: var(--ui-radius-sm);
  border: 1px solid color-mix(in srgb, var(--omni-accent) 55%, transparent);
  background: var(--omni-accent); color: #fff; font-size: 11px; font-weight: 700; cursor: pointer;
}
.omni-inline__btn--ghost { background: transparent; color: var(--ui-text-2); border-color: var(--ui-border); }
.omni-inline__btn:disabled { opacity: 0.45; cursor: default; }

.omni-empty { display: flex; justify-content: center; }

.omni-more { display: flex; justify-content: center; padding: 2px 0; }

.omni-foot {
  display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
  border-top: 1px solid var(--ui-hairline); padding-top: 8px;
}
.omni-foot__counts {
  font-family: var(--ui-font-mono); font-size: 9px; font-weight: 800; letter-spacing: 0.1em;
  color: var(--omni-accent);
}
.omni-foot__keys {
  margin-left: auto; font-family: var(--ui-font-mono); font-size: 8.5px;
  letter-spacing: 0.06em; color: var(--ui-text-3);
}

@media (max-width: 560px) {
  .omni { padding: max(14px, env(safe-area-inset-top, 0px)) 12px 16px; }
  .omni-foot__keys { display: none; }
}

@media (prefers-reduced-motion: reduce) {
  .omni-drop-enter-active, .omni-drop-leave-active { animation: none; }
  .omni-row { animation: none; }
}
</style>
