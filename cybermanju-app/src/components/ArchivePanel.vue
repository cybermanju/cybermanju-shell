<!-- CyberManju OS — ArchivePanel (src/components/ArchivePanel.vue). -->
<!-- Role: life-archive + daily-use window (ANALYSIS.md §4.2/4.3/4.4/4.9): NOTES
     (vault markdown + backlinks), TIMELINE ("on this day" + day buckets),
     PEOPLE (FaceGroups++ contact records), TASKS (todos), BOOKMARKS (via
     import_from_url), DATA (CSV/TSV preview). All renderers are pure utils;
     persistence is localStorage, server mutations reuse store actions. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/utils/{notes,timeline,contacts,tasks,bookmarks,csv,i18n},
     store files/faceGroups/collections actions, wm.open('editor') for edits. -->
<!-- Contracts: honest limits surfaced in-UI (unloaded note contents excluded
     from backlinks; DATA caps 5000 rows; hashes never guessed). -->
<template>
  <div class="archive-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-archive"><AppIcon name="solar:library-bold" /></span>
        <h2 class="panel-title">{{ tr('archive.title') }}</h2>
      </div>
      <div class="header-right">
        <button class="ghost-btn" type="button" @click="refresh">REFRESH</button>
      </div>
    </div>

    <div class="ar-tabs" role="tablist" aria-label="Archive views">
      <button
        v-for="tb in TABS"
        :key="tb"
        role="tab"
        :aria-selected="view === tb"
        :class="{ on: view === tb }"
        type="button"
        @click="view = tb"
      >{{ tr(`archive.${tb.toLowerCase()}`) }}</button>
    </div>

    <!-- ══ NOTES ══ -->
    <template v-if="view === 'NOTES'">
      <div class="section">
        <div v-if="!noteFiles.length" class="note">{{ tr('archive.noNotes') }}</div>
        <div v-else class="note-list">
          <button
            v-for="f in noteFiles"
            :key="f.id"
            type="button"
            class="note-row"
            :class="{ on: selectedNoteId === f.id }"
            @click="openNote(f.id)"
          >
            <span class="truncate">{{ f.name }}</span>
            <span class="text-muted">{{ backlinksOf(f.id).length }}⇤</span>
          </button>
        </div>
      </div>
      <div v-if="selectedNote" class="section">
        <h3 class="section-title">{{ selectedNote.name }}</h3>
        <div v-if="noteLoading" class="note text-muted">LOADING…</div>
        <pre v-else class="note-body">{{ selectedContent || '(empty)' }}</pre>
        <div v-if="outLinks.length" class="note-links">
          <span class="text-muted">→</span>
          <button v-for="id in outLinks" :key="id" type="button" class="link-chip" @click="openNote(id)">{{ noteName(id) }}</button>
        </div>
        <div class="note-links">
          <span class="text-muted">{{ tr('archive.backlinks') }} ({{ loadedCount }} LOADED)</span>
          <button v-for="id in backlinksOf(selectedNote.id)" :key="id" type="button" class="link-chip" @click="openNote(id)">{{ noteName(id) }}</button>
        </div>
        <div v-if="brokenOfSelected.length" class="note text-muted">{{ tr('archive.brokenLinks') }}: {{ brokenOfSelected.join(', ') }}</div>
        <button class="ghost-btn primary" type="button" @click="editInStudio">OPEN IN EDITOR</button>
        <div class="create-row">
          <select v-model="memoryConfigId" class="input grow" aria-label="Agent for memory">
            <option value="">MEMORY → AGENT…</option>
            <option v-for="c in store.agentConfigs" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
          <button class="ghost-btn" type="button" :disabled="!memoryConfigId || !selectedContent" @click="rememberNote">REMEMBER</button>
        </div>
      </div>
    </template>

    <!-- ══ TIMELINE ══ -->
    <template v-if="view === 'TIMELINE'">
      <div class="section">
        <h3 class="section-title">{{ tr('archive.onThisDay') }}</h3>
        <div class="tl-controls">
          <input v-model="onDay" class="input" type="date" aria-label="On-this-day date" @change="onDayPicked" />
        </div>
        <div v-if="!onThisDayFiles.length" class="note">{{ tr('archive.noTimeline') }}</div>
        <div v-else class="note-list">
          <button v-for="f in onThisDayFiles" :key="f.id" type="button" class="note-row" @click="store.selectFile(f.id)">
            <span class="truncate">{{ f.name }}</span>
            <span class="text-muted">{{ f.createdAt.slice(0, 4) }}</span>
          </button>
        </div>
      </div>
      <div class="section">
        <h3 class="section-title">DAYS {{ rangeLabel }}</h3>
        <div v-for="b in dayBuckets.slice(0, 60)" :key="b.day" class="tl-day">
          <span class="tl-date">{{ b.day }}</span>
          <span>{{ b.count }} FILES{{ b.withGps ? ` · ${b.withGps} GPS` : '' }}{{ b.withFaces ? ` · ${b.withFaces} FACES` : '' }}</span>
        </div>
      </div>
    </template>

    <!-- ══ PEOPLE (FaceGroups++) ══ -->
    <template v-if="view === 'PEOPLE'">
      <div class="section">
        <div v-if="!store.faceGroups.length" class="note">{{ tr('archive.noPeople') }}</div>
        <div v-else class="note-list">
          <button
            v-for="g in store.faceGroups"
            :key="g.id"
            type="button"
            class="note-row"
            :class="{ on: selectedGroupId === g.id }"
            @click="selectedGroupId = g.id"
          >
            <span class="truncate">{{ contactName(g.id, g.name) }}</span>
            <span class="text-muted">{{ g.fileIds.length }}</span>
          </button>
        </div>
      </div>
      <div v-if="activeGroup" class="section">
        <h3 class="section-title">{{ activeContact.name }}</h3>
        <div class="alias-row">
          <span v-for="a in activeContact.aliases" :key="a" class="alias-chip">{{ a }} <button type="button" aria-label="Remove alias" @click="dropAlias(a)">×</button></span>
        </div>
        <div class="create-row">
          <input v-model="aliasDraft" class="input" placeholder="ALIAS" aria-label="New alias" @keyup.enter="pushAlias" />
          <button class="ghost-btn" type="button" @click="pushAlias">ADD</button>
        </div>
        <textarea v-model="notesDraft" class="input notes-area" placeholder="NOTES ABOUT THIS PERSON" aria-label="Contact notes" @change="saveNotes" />
        <div class="note text-muted">{{ activeContact.links.length }} LINKED FILES</div>
        <div class="create-row">
          <button class="ghost-btn" type="button" @click="renaming = !renaming">RENAME</button>
          <button class="ghost-btn danger" type="button" @click="removeGroup">DELETE</button>
        </div>
        <div v-if="renaming" class="create-row">
          <input v-model="renameDraft" class="input" aria-label="New group name" @keyup.enter="applyRename" />
          <button class="ghost-btn primary" type="button" @click="applyRename">APPLY</button>
        </div>
      </div>
    </template>

    <!-- ══ TASKS (local + server shared list, §4.9) ══ -->
    <template v-if="view === 'TASKS'">
      <div class="section">
        <form class="create-row" @submit.prevent="pushTodo">
          <input v-model="todoDraft" class="input grow" :placeholder="tr('archive.taskPlaceholder')" aria-label="New task title" />
          <button class="ghost-btn primary" type="submit">{{ tr('archive.addTask') }}</button>
        </form>
        <p v-if="serverTodos === null" class="note text-muted">LOCAL ONLY — no dashboard (static host).</p>
        <div v-if="!todos.length && !(serverTodos ?? []).length" class="note">{{ tr('archive.noTasks') }}</div>
        <div v-for="td in serverTodos ?? []" :key="`srv-${td.id}`" class="todo-row">
          <input type="checkbox" :checked="td.done" :aria-label="`Done: ${td.title}`" @change="flipServerTodo(td.id, !td.done)" />
          <span class="truncate" :class="{ done: td.done }">{{ td.title }} <span class="text-muted">· {{ td.source }}</span></span>
          <button type="button" class="todo-x" aria-label="Remove task" @click="dropServerTodo(td.id)">×</button>
        </div>
        <div v-for="td in todos" :key="td.id" class="todo-row">
          <input type="checkbox" :checked="td.done" :aria-label="`Done: ${td.title}`" @change="flipTodo(td.id)" />
          <span class="truncate" :class="{ done: td.done }">{{ td.title }}</span>
          <button type="button" class="todo-x" aria-label="Remove task" @click="dropTodo(td.id)">×</button>
        </div>
        <button v-if="todos.some((x) => x.done)" class="ghost-btn" type="button" @click="pruneTodos">{{ tr('archive.clearDone') }}</button>
      </div>
    </template>

    <!-- ══ BOOKMARKS ══ -->
    <template v-if="view === 'BOOKMARKS'">
      <div class="section">
        <form class="create-row" @submit.prevent="pushBookmark">
          <input v-model="urlDraft" class="input grow" :placeholder="tr('archive.bookmarkPlaceholder')" aria-label="Bookmark URL" inputmode="url" />
          <button class="ghost-btn primary" type="submit">ADD</button>
        </form>
        <div v-if="!bookmarks.length" class="note">{{ tr('archive.noBookmarks') }}</div>
        <div v-for="bm in bookmarks" :key="bm.id" class="bm-row">
          <a class="truncate" :href="bm.url" target="_blank" rel="noopener">{{ bm.title }}</a>
          <button v-if="!bm.importedFileId" class="ghost-btn" type="button" :disabled="importingId === bm.id" @click="importBookmark(bm.id)">
            {{ importingId === bm.id ? '…' : tr('archive.import') }}
          </button>
          <span v-else class="text-muted">{{ tr('archive.imported') }}</span>
          <button type="button" class="todo-x" aria-label="Remove bookmark" @click="dropBookmark(bm.id)">×</button>
        </div>
      </div>
    </template>

    <!-- ══ DATA ══ -->
    <template v-if="view === 'DATA'">
      <div class="section">
        <label class="field">
          <span class="text-muted">{{ tr('archive.noData') }}</span>
          <select v-model="datasetId" class="input" aria-label="Dataset file">
            <option value="">—</option>
            <option v-for="f in datasetFiles" :key="f.id" :value="f.id">{{ f.name }}</option>
          </select>
        </label>
        <div v-if="dataset" class="note text-muted">
          {{ dataset.rows.length }} ROWS · {{ dataset.columns.length }} COLS · {{ dataset.truncated ? 'TRUNCATED @5000' : 'FULL' }}
        </div>
      </div>
      <div v-if="dataset" class="section dataset-scroll">
        <table class="dataset-table">
          <thead>
            <tr>
              <th v-for="(c, i) in dataset.columns" :key="i">{{ c || `col${i}` }} <span class="text-muted">{{ colTypes[i] }}</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(r, i) in dataset.rows.slice(0, 100)" :key="i">
              <td v-for="(c, j) in dataset.columns" :key="j">{{ r[j] ?? '' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import { buildNoteIndex, isNoteFile, type NoteDoc } from '@/utils/notes'
import { groupByDay, onThisDay, timelineRange } from '@/utils/timeline'
import {
  addAlias,
  contactFromFaceGroup,
  parseContactsStore,
  removeAlias,
  serializeContactsStore,
  setContactNotes,
  type ContactRecord,
} from '@/utils/contacts'
import { addTodo, clearDone, parseTodosStore, removeTodo, serializeTodosStore, toggleTodo, type TodoItem } from '@/utils/tasks'
import { addBookmark, markImported, parseBookmarksStore, removeBookmark, serializeBookmarksStore, type BookmarkItem } from '@/utils/bookmarks'
import { inferColumnType, parseDelimited } from '@/utils/csv'
import { getLocale, t } from '@/utils/i18n'

const store = useAppStore()
const wm = useWindowManager()

type ArchiveTab = 'NOTES' | 'TIMELINE' | 'PEOPLE' | 'TASKS' | 'BOOKMARKS' | 'DATA'
const TABS: ArchiveTab[] = ['NOTES', 'TIMELINE', 'PEOPLE', 'TASKS', 'BOOKMARKS', 'DATA']
const props = defineProps<{ tab?: string }>()
function coerce(t: unknown): ArchiveTab | null {
  return (TABS as string[]).includes(t as string) ? (t as ArchiveTab) : null
}
const view = ref<ArchiveTab>(coerce(props.tab) ?? 'NOTES')
watch(() => props.tab, (v) => {
  const c = coerce(v)
  if (c) view.value = c
})

const loc = ref(getLocale())
const tr = (key: string) => t(key, loc.value)
function refreshLocale() {
  loc.value = getLocale()
}

async function refresh() {
  refreshLocale()
  await Promise.all([store.fetchFiles(), store.fetchFaceGroups()])
}

/* ── NOTES ── */
const noteFiles = computed(() => store.files.filter((f) => isNoteFile(f.name, f.fileType)))
const noteContents = ref(new Map<string, string>())
const selectedNoteId = ref('')
const noteLoading = ref(false)
const selectedNote = computed(() => noteFiles.value.find((f) => f.id === selectedNoteId.value) ?? null)
const selectedContent = computed(() => (selectedNote.value ? noteContents.value.get(selectedNote.value.id) ?? '' : ''))
const noteIndex = computed(() => {
  const docs: NoteDoc[] = noteFiles.value.map((f) => ({
    id: f.id,
    name: f.name,
    path: f.path,
    contentText: noteContents.value.get(f.id),
  }))
  return buildNoteIndex(docs)
})
const loadedCount = computed(() => noteContents.value.size)
const noteName = (id: string) => noteFiles.value.find((f) => f.id === id)?.name ?? id
const backlinksOf = (id: string) => noteIndex.value.backlinks[id] ?? []
const outLinks = computed(() => (selectedNote.value ? noteIndex.value.links[selectedNote.value.id] ?? [] : []))
const brokenOfSelected = computed(() => {
  if (!selectedNote.value) return []
  return noteIndex.value.broken.filter((b) => b.from === selectedNote.value!.id).map((b) => b.target)
})

async function loadContent(id: string): Promise<string> {
  const cached = noteContents.value.get(id)
  if (cached !== undefined) return cached
  const node = noteFiles.value.find((f) => f.id === id)
  if (node?.contentText) {
    noteContents.value.set(id, node.contentText)
    return node.contentText
  }
  try {
    const res = await store.readManagedContent(id)
    const text = typeof res === 'string' ? res : (res as { content?: string } | null)?.content ?? ''
    noteContents.value.set(id, text)
    return text
  } catch {
    noteContents.value.set(id, '')
    return ''
  }
}

async function openNote(id: string) {
  selectedNoteId.value = id
  noteLoading.value = true
  try {
    await loadContent(id)
  } finally {
    noteLoading.value = false
  }
}

function editInStudio() {
  if (selectedNote.value) wm.open('editor', { fileId: selectedNote.value.id } as never)
}

// ── NOTES → agent memory bridge (§4.2: the agent gets a notepad) ──
const memoryConfigId = ref('')
async function rememberNote() {
  if (!selectedNote.value || !memoryConfigId.value || !selectedContent.value) return
  const text = `Note "${selectedNote.value.name}":\n${selectedContent.value.slice(0, 4000)}`
  await store.storeAgentMemory(memoryConfigId.value, text)
}

/* ── TIMELINE ── */
const today = new Date()
const onDay = ref(today.toISOString().slice(0, 10))
const dayBuckets = computed(() =>
  groupByDay(store.files.map((f) => ({ id: f.id, name: f.name, createdAt: f.createdAt, gpsLat: f.gpsLat, faceGroupIds: f.faceGroupIds }))),
)
const rangeLabel = computed(() => {
  const r = timelineRange(store.files.map((f) => ({ id: f.id, name: f.name, createdAt: f.createdAt })))
  return r ? `${r.min} → ${r.max}` : ''
})
const onThisDayFiles = computed(() => {
  const [y, m, d] = onDay.value.split('-').map(Number)
  if (!y || !m || !d) return []
  void y
  return onThisDay(
    store.files.map((f) => ({ id: f.id, name: f.name, createdAt: f.createdAt })),
    m,
    d,
  ).slice(0, 100)
})
function onDayPicked() {
  /* reactive via onDay — no-op anchor for the change event */
}

/* ── PEOPLE ── */
const CONTACTS_KEY = 'cybermanju.contacts.v1'
const contacts = ref<Record<string, ContactRecord>>({})
const selectedGroupId = ref('')
const aliasDraft = ref('')
const notesDraft = ref('')
const renaming = ref(false)
const renameDraft = ref('')
const activeGroup = computed(() => store.faceGroups.find((g) => g.id === selectedGroupId.value) ?? null)
const activeContact = computed<ContactRecord>(() => {
  const g = activeGroup.value
  if (!g) return { groupId: '', name: '', aliases: [], notes: '', links: [], updatedAt: '' }
  return contacts.value[g.id] ?? contactFromFaceGroup(g)
})
function contactName(groupId: string, fallback: string) {
  return contacts.value[groupId]?.name ?? fallback
}
function persistContacts() {
  try {
    localStorage.setItem(CONTACTS_KEY, serializeContactsStore(contacts.value))
  } catch {
    /* private mode — session-only */
  }
}
function ensureContact(): ContactRecord | null {
  const g = activeGroup.value
  if (!g) return null
  const c = contacts.value[g.id] ?? contactFromFaceGroup({ ...g, name: activeContact.value.name || g.name })
  contacts.value[g.id] = c
  return c
}
watch(selectedGroupId, () => {
  notesDraft.value = activeContact.value.notes
  renameDraft.value = activeContact.value.name
  renaming.value = false
})
function pushAlias() {
  const c = ensureContact()
  if (!c || !aliasDraft.value.trim()) return
  contacts.value[c.groupId] = addAlias(c, aliasDraft.value)
  aliasDraft.value = ''
  persistContacts()
}
function dropAlias(a: string) {
  const c = ensureContact()
  if (!c) return
  contacts.value[c.groupId] = removeAlias(c, a)
  persistContacts()
}
function saveNotes() {
  const c = ensureContact()
  if (!c) return
  contacts.value[c.groupId] = setContactNotes(c, notesDraft.value)
  persistContacts()
}
async function applyRename() {
  if (!activeGroup.value || !renameDraft.value.trim()) return
  await store.renameFaceGroup(activeGroup.value.id, renameDraft.value.trim())
  renaming.value = false
}
async function removeGroup() {
  if (!activeGroup.value) return
  await store.deleteFaceGroup(activeGroup.value.id)
  selectedGroupId.value = ''
}

/* ── TASKS (local + server shared list, §4.9) ── */
const TODOS_KEY = 'cybermanju.todos.v1'
const todos = ref<TodoItem[]>([])
const todoDraft = ref('')
// Server list when a dashboard answers, else null → local-only mode.
const serverTodos = computed(() => store.serverTodos)
function persistTodos() {
  try {
    localStorage.setItem(TODOS_KEY, serializeTodosStore(todos.value))
  } catch {
    /* private mode */
  }
}
async function pushTodo() {
  if (serverTodos.value !== null) {
    if (!todoDraft.value.trim()) return
    await store.createServerTodo(todoDraft.value)
    todoDraft.value = ''
    return
  }
  const res = addTodo(todos.value, todoDraft.value)
  if (!res) return
  todos.value = res.list
  todoDraft.value = ''
  persistTodos()
}
async function flipServerTodo(id: string, done: boolean) {
  await store.toggleServerTodo(id, done)
}
async function dropServerTodo(id: string) {
  await store.deleteServerTodo(id)
}
function flipTodo(id: string) {
  todos.value = toggleTodo(todos.value, id)
  persistTodos()
}
function dropTodo(id: string) {
  todos.value = removeTodo(todos.value, id)
  persistTodos()
}
function pruneTodos() {
  todos.value = clearDone(todos.value).list
  persistTodos()
}

/* ── BOOKMARKS ── */
const BOOKMARKS_KEY = 'cybermanju.bookmarks.v1'
const bookmarks = ref<BookmarkItem[]>([])
const urlDraft = ref('')
const importingId = ref<string | null>(null)
function persistBookmarks() {
  try {
    localStorage.setItem(BOOKMARKS_KEY, serializeBookmarksStore(bookmarks.value))
  } catch {
    /* private mode */
  }
}
function pushBookmark() {
  const res = addBookmark(bookmarks.value, urlDraft.value)
  if (!res) return
  bookmarks.value = res.list
  urlDraft.value = ''
  persistBookmarks()
}
function dropBookmark(id: string) {
  bookmarks.value = removeBookmark(bookmarks.value, id)
  persistBookmarks()
}
async function importBookmark(id: string) {
  const bm = bookmarks.value.find((b) => b.id === id)
  if (!bm) return
  importingId.value = id
  try {
    const file = await store.importFromUrl(bm.url, '/')
    if (file) {
      bookmarks.value = markImported(bookmarks.value, id, file.id)
      persistBookmarks()
    }
  } finally {
    importingId.value = null
  }
}

/* ── DATA ── */
const datasetId = ref('')
const datasetText = ref<string | null>(null)
const datasetFiles = computed(() =>
  store.files.filter((f) => /\.(csv|tsv|txt)$/i.test(f.name) && f.fileType !== 'folder'),
)
const dataset = computed(() => (datasetText.value === null ? null : parseDelimited(datasetText.value)))
const colTypes = computed(() => {
  const d = dataset.value
  if (!d) return []
  return d.columns.map((_, i) => inferColumnType(d.rows.map((r) => r[i] ?? '')))
})
watch(datasetId, async (id) => {
  datasetText.value = null
  if (!id) return
  const node = store.files.find((f) => f.id === id)
  if (node?.contentText) {
    datasetText.value = node.contentText
    return
  }
  try {
    const res = await store.readManagedContent(id)
    datasetText.value = typeof res === 'string' ? res : (res as { content?: string } | null)?.content ?? ''
  } catch {
    datasetText.value = ''
  }
})

function loadPersisted() {
  try {
    todos.value = parseTodosStore(localStorage.getItem(TODOS_KEY))
    bookmarks.value = parseBookmarksStore(localStorage.getItem(BOOKMARKS_KEY))
    contacts.value = parseContactsStore(localStorage.getItem(CONTACTS_KEY))
  } catch {
    /* fresh profile */
  }
}

onMounted(async () => {
  loadPersisted()
  refreshLocale()
  window.addEventListener('focus', refreshLocale)
  await refresh()
  await store.fetchServerTodos()
  if (store.agentConfigs.length === 0) {
    try {
      await store.fetchAgentConfigs()
    } catch {
      /* static hosts have no agent backend */
    }
  }
})
</script>

<style scoped>
.archive-panel {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--ui-surface);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 13px;
  box-sizing: border-box;
}
.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border);
}
.header-left { display: flex; align-items: center; gap: 10px; }
.icon-archive { color: var(--ui-accent); }
.panel-title { margin: 0; font-size: 13px; letter-spacing: 2px; }
.ghost-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  font-family: inherit;
  font-size: 11px;
  padding: 4px 9px;
  cursor: pointer;
}
.ghost-btn:hover:not(:disabled) { color: var(--ui-text); border-color: var(--ui-border-strong); }
.ghost-btn:disabled { opacity: 0.4; cursor: default; }
.ghost-btn.primary { color: var(--ui-accent); border-color: color-mix(in srgb, var(--ui-accent) 60%, transparent); }
.ghost-btn.danger { color: var(--ui-danger); border-color: color-mix(in srgb, var(--ui-danger) 55%, transparent); }
.ar-tabs { display: flex; gap: 4px; padding: 8px 12px 0; flex-wrap: wrap; }
.ar-tabs button {
  flex: 1;
  min-width: 90px;
  padding: 6px 0;
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  color: color-mix(in srgb, var(--ui-text) 55%, transparent);
  font-family: inherit;
  font-size: 11px;
  letter-spacing: 1px;
  cursor: pointer;
}
.ar-tabs button.on { color: var(--ui-text); border-bottom-color: var(--ui-accent); }
.section { padding: 12px; border-bottom: 1px solid var(--ui-border); }
.section-title { margin: 0 0 8px; font-size: 12px; letter-spacing: 1.5px; }
.note { padding: 8px 0; color: color-mix(in srgb, var(--ui-text) 75%, transparent); }
.text-muted { color: color-mix(in srgb, var(--ui-text) 55%, transparent); }
.note-list { display: flex; flex-direction: column; gap: 4px; }
.note-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px;
  background: transparent;
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  cursor: pointer;
  text-align: left;
}
.note-row.on { border-color: var(--ui-accent); }
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
.note-body {
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 12px;
  max-height: 260px;
  overflow-y: auto;
  border: 1px solid var(--ui-border);
  padding: 8px;
  margin: 0 0 8px;
}
.note-links { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; margin: 6px 0; }
.link-chip {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: var(--ui-accent);
  font-family: inherit;
  font-size: 11px;
  padding: 2px 8px;
  cursor: pointer;
}
.tl-controls { display: flex; gap: 8px; margin-bottom: 8px; }
.input {
  background: var(--ui-bg, transparent);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: inherit;
  font-size: 12px;
  padding: 5px 8px;
  min-width: 0;
}
.input.grow { flex: 1; }
.create-row { display: flex; gap: 8px; margin: 8px 0; align-items: center; }
.tl-day {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 0;
  border-bottom: 1px dashed var(--ui-border);
  font-size: 12px;
}
.tl-date { font-variant-numeric: tabular-nums; color: var(--ui-accent); }
.alias-row { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 4px; }
.alias-chip {
  border: 1px solid var(--ui-border);
  padding: 2px 4px 2px 8px;
  font-size: 11px;
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.alias-chip button { background: none; border: none; color: var(--ui-text); cursor: pointer; }
.notes-area { width: 100%; min-height: 56px; resize: vertical; box-sizing: border-box; }
.todo-row, .bm-row { display: flex; align-items: center; gap: 8px; padding: 5px 0; border-bottom: 1px dashed var(--ui-border); font-size: 12px; }
.todo-row .done { text-decoration: line-through; opacity: 0.6; }
.todo-x { background: none; border: none; color: var(--ui-text); cursor: pointer; font-size: 14px; }
.bm-row a { color: var(--ui-accent); flex: 1; }
.field { display: flex; flex-direction: column; gap: 6px; }
.dataset-scroll { overflow-x: auto; }
.dataset-table { border-collapse: collapse; font-size: 11px; width: 100%; }
.dataset-table th, .dataset-table td {
  border: 1px solid var(--ui-border);
  padding: 3px 8px;
  text-align: left;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
