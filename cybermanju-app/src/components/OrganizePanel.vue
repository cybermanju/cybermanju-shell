<!-- CyberManju OS — OrganizePanel (src/components/OrganizePanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="org-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-org"><AppIcon name="solar:library-bold" /></span>
        <h2 class="panel-title">ORGANIZE</h2>
      </div>
      <UiButton size="sm" icon="solar:refresh-bold" icon-only title="Refresh" aria-label="Refresh organize" :loading="store.isLoading" @click="refresh()" />
    </div>

    <div class="org-tabs" role="tablist" aria-label="Organize views">
      <button
        v-for="t in TABS"
        :key="t.id"
        role="tab"
        :aria-selected="activeTab === t.id"
        :class="{ on: activeTab === t.id }"
        type="button"
        @click="activeTab = t.id"
      >{{ t.label }}</button>
    </div>

    <!-- ══ COLLECTIONS (merged collections panel) ══ -->
    <div v-if="activeTab === 'collections'" class="org-section">
      <div
        v-for="col in collections"
        :key="col.id"
        class="collection-card"
        :class="{ 'drop-target': dragOverCollectionId === col.id }"
        @dragover.prevent="dragOverCollectionId = col.id"
        @dragleave.prevent="dragOverCollectionId = null"
        @drop.prevent="handleDrop(col.id)"
      >
        <div class="col-header">
          <span class="col-name">{{ col.name }}</span>
          <span class="col-type text-muted">{{ col.collectionType }}</span>
        </div>
        <div class="col-meta text-muted">{{ col.itemIds.length }} ITEMS</div>
      </div>
      <UiEmpty
        v-if="!collections.length"
        size="sm"
        icon="solar:library-bold"
        title="No collections yet"
        description="Create one below to group files. Drop the selected file onto a card to add it."
      />
      <div class="create-row">
        <UiInput v-model="newColName" placeholder="COLLECTION NAME" aria-label="Collection name" @enter="handleCreateCollection" />
        <UiSelect v-model="newColType" :options="COLLECTION_TYPE_OPTIONS" aria-label="Collection type" title="Collection type" />
        <UiButton size="sm" icon="solar:add-bold" :disabled="!newColName.trim()" @click="handleCreateCollection">CREATE</UiButton>
      </div>
    </div>

    <!-- ══ LOOSE GROUPS ══ -->
    <div v-else-if="activeTab === 'loose'" class="org-section">
      <p class="panel-hint">Ad-hoc groups for gathering files together.</p>
      <UiError
        v-if="store.lastError"
        size="sm"
        title="Could not load loose groups"
        :message="store.lastError"
        retryable
        @retry="refresh()"
      />
      <form class="loose-create" @submit.prevent="createGroup()">
        <UiInput v-model="looseName" placeholder="New group name…" aria-label="New loose group name" clearable />
        <UiButton size="sm" icon="solar:add-circle-bold" type="submit" :loading="looseBusy" :disabled="!looseName.trim()">Create</UiButton>
      </form>
      <UiEmpty
        v-if="!store.lastError && !store.looseGroups.length"
        size="sm"
        icon="solar:users-group-two-rounded-bold"
        title="No loose groups yet"
        description="Create one above, then add files from the picker on each card."
      />
      <div v-else class="loose-groups-list">
        <div v-for="group in store.looseGroups" :key="group.id" class="loose-group-item">
          <div class="lg-info">
            <div class="lg-name">{{ group.name }}</div>
            <div class="lg-count">{{ group.fileIds.length }} FILES</div>
            <div v-if="group.fileIds.length" class="lg-members">
              <button
                v-for="fid in group.fileIds"
                :key="fid"
                type="button"
                class="lg-member"
                :title="fileName(fid)"
                @click="store.selectFile(fid)"
              >{{ fileName(fid) }}</button>
            </div>
            <div class="lg-add">
              <UiSelect
                v-model="addTarget[group.id]"
                :options="fileOptions"
                inline
                aria-label="Pick a file to add"
                title="Pick a file to add"
              />
              <UiButton size="xs" icon="solar:add-circle-bold" :disabled="!addTarget[group.id]" @click="addFile(group.id)">Add</UiButton>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- ══ STYLE TAGS ══ -->
    <div v-else-if="activeTab === 'tags'" class="org-section">
      <p class="panel-hint">Files grouped by visual style. Click a tag to search it.</p>
      <UiError
        v-if="store.lastError"
        size="sm"
        title="Could not load tags"
        :message="store.lastError"
        retryable
        @retry="refresh()"
      />
      <div v-else-if="store.isLoading && !allStyleTags.length" class="panel-loading">
        <UiSpinner size="sm" show-label label="Loading tags" />
      </div>
      <UiEmpty
        v-else-if="!allStyleTags.length"
        size="sm"
        icon="solar:tag-bold"
        title="No style tags yet"
        description="Tags appear here once files are tagged."
      />
      <div v-else class="style-tags">
        <button v-for="tag in allStyleTags" :key="tag" type="button" class="style-tag" @click="searchTag(tag)">{{ tag }}</button>
      </div>
    </div>

    <!-- ══ FAVORITES ══ -->
    <div v-else class="org-section">
      <p class="panel-hint">Starred files live here for quick access.</p>
      <UiError
        v-if="store.lastError"
        size="sm"
        title="Could not load favorites"
        :message="store.lastError"
        retryable
        @retry="refresh()"
      />
      <div v-else-if="store.isLoading && !store.starredFiles.length" class="panel-loading">
        <UiSpinner size="sm" show-label label="Loading favorites" />
      </div>
      <UiEmpty
        v-else-if="!store.starredFiles.length"
        size="sm"
        icon="solar:star-bold"
        title="No starred files"
        description="Star anything in Files to pin it here."
      />
      <div v-else class="fav-list">
        <div v-for="f in store.starredFiles" :key="f.id" class="fav-item" @click="store.selectFile(f.id)">
          <span class="fav-icon"><AppIcon :name="f.fileType === 'folder' ? 'solar:folder-bold' : 'solar:file-bold'" :size="14" /></span>
          <span class="fav-name truncate">{{ f.name }}</span>
          <UiButton size="xs" variant="ghost" icon="solar:star-bold" icon-only :title="`Remove ${f.name} from favorites`" :aria-label="`Remove ${f.name} from favorites`" @click.stop="store.toggleStar(f.id)" />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiInput from '@/components/ui/UiInput.vue'
import UiSelect from '@/components/ui/UiSelect.vue'
import UiSpinner from '@/components/ui/UiSpinner.vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import type { CollectionType } from '@/types'

const store = useAppStore()
const wm = useWindowManager()

type OrgTab = 'collections' | 'loose' | 'tags' | 'favorites'
const TABS: Array<{ id: OrgTab; label: string }> = [
  { id: 'collections', label: 'COLLECTIONS' },
  { id: 'loose', label: 'LOOSE GROUPS' },
  { id: 'tags', label: 'TAGS' },
  { id: 'favorites', label: 'FAVORITES' },
]

// Merged organize window: favorites / loose-groups / style open here.
const props = defineProps<{ tab?: string }>()
function coerceTab(t: unknown): OrgTab | null {
  return t === 'collections' || t === 'loose' || t === 'tags' || t === 'favorites' ? t : null
}
const activeTab = ref<OrgTab>(coerceTab(props.tab) ?? 'collections')
watch(() => props.tab, (t) => {
  const c = coerceTab(t)
  if (c) activeTab.value = c
})

async function refresh() {
  store.clearError()
  await Promise.allSettled([store.fetchFiles(), store.fetchLooseGroups(), store.fetchCollections()])
}

/* ── collections (merged collections panel) ── */
const collections = computed(() => store.collections)
const newColName = ref('')
const newColType = ref<CollectionType>('custom')
const dragOverCollectionId = ref<string | null>(null)
const COLLECTION_TYPE_OPTIONS = [
  { label: 'CUSTOM', value: 'custom' },
  { label: 'HIGHLIGHTS', value: 'highlights' },
  { label: 'BEST MOMENTS', value: 'best_moments' },
]

async function handleCreateCollection() {
  if (!newColName.value.trim()) return
  await store.createCollection(newColName.value.trim(), newColType.value, '#FFFFFF')
  newColName.value = ''
}

function handleDrop(collectionId: string) {
  dragOverCollectionId.value = null
  const fileId = store.selectedFileId
  if (fileId) void store.addToCollection(collectionId, fileId)
}

/* ── loose groups ── */
const looseName = ref('')
const looseBusy = ref(false)
const addTarget = ref<Record<string, string>>({})

async function createGroup() {
  const name = looseName.value.trim()
  if (!name) return
  looseBusy.value = true
  try {
    const created = await store.createLooseGroup(name)
    if (created) looseName.value = ''
  } finally {
    looseBusy.value = false
  }
}

async function addFile(groupId: string) {
  const fid = addTarget.value[groupId]
  if (!fid) return
  await store.addFileToLooseGroup(groupId, fid)
}

function fileName(fileId: string): string {
  return store.files.find(f => f.id === fileId)?.name ?? fileId.slice(0, 8)
}

const fileOptions = computed(() =>
  store.files.map(f => ({ label: f.name, value: f.id }))
)

/* ── style tags ── */
const allStyleTags = computed(() =>
  [...new Set(store.files.flatMap(f => f.tags || []))].sort()
)

function searchTag(tag: string) {
  store.searchQuery = tag
  wm.open('search')
  void store.searchFiles(tag)
}

onMounted(() => {
  void refresh()
})
</script>

<style scoped>
.org-panel {
  width: 100%;
  min-width: 0;
  height: 100%;
  min-height: 100%;
  box-sizing: border-box;
  background: var(--ui-surface);
  overflow-y: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  padding: max(16px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right)) max(16px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left));
  font-family: var(--ui-font);
  color: var(--ui-text);
}
.org-panel::-webkit-scrollbar { width: 10px; }
.org-panel::-webkit-scrollbar-track { background: transparent; }
.org-panel::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  border-radius: var(--ui-radius-full); border: 3px solid transparent; background-clip: content-box; }
.org-panel::-webkit-scrollbar-thumb:hover { background: var(--ui-accent); background-clip: content-box; border: 2px solid transparent; }

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ui-border);
  margin-bottom: 12px;
}
.header-left { display: flex; align-items: center; gap: 8px; }
.icon-org { font-size: 16px; }
.panel-title { font-size: 14px; font-weight: 800; letter-spacing: 1px; margin: 0; }

.org-tabs { display: flex; gap: 4px; margin-bottom: 12px; }
.org-tabs button {
  flex: 1; min-width: 0; min-height: 44px; padding: 6px 4px; font-size: 9.5px; font-weight: 800; letter-spacing: 0.08em;
  background: transparent; border: 1px solid var(--ui-hairline); border-radius: 8px;
  color: var(--ui-text-3); cursor: pointer; white-space: normal; line-height: 1.25;
}
.org-tabs button.on {
  color: var(--ui-accent);
  border-color: color-mix(in srgb, var(--ui-accent) 50%, transparent);
  background: var(--ui-accent-softer);
}

.org-section { display: flex; flex-direction: column; gap: 8px; }
.panel-hint { font-family: var(--ui-font); font-size: 9px; color: var(--ui-text-3); margin: 0; }
.panel-loading { display: flex; align-items: center; justify-content: center; padding: 22px 0; }
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* collections */
.collection-card {
  min-width: 0;
  min-height: 56px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 10px 12px;
  background: var(--ui-glass);
}
.collection-card.drop-target {
  border-color: color-mix(in srgb, var(--ui-accent) 65%, transparent);
  box-shadow: var(--ui-glow-soft);
}
.col-header { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; min-width: 0; }
.col-name { font-size: 12px; font-weight: 700; }
.col-name, .col-type { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.col-type { font-size: 9px; white-space: nowrap; }
.col-meta { font-size: 9px; margin-top: 2px; }
.create-row { display: flex; gap: 6px; align-items: center; }
.create-row > :first-child { flex: 1; min-width: 0; }

/* loose groups */
.loose-create { display: flex; gap: 8px; align-items: flex-end; }
.loose-create > :first-child { flex: 1; min-width: 0; }
.loose-groups-list { display: flex; flex-direction: column; gap: 6px; }
.loose-group-item {
  background: var(--ui-glass); border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm); padding: 10px 12px;
}
.lg-info { display: flex; flex-direction: column; gap: 2px; }
.lg-name { font-size: 12px; font-weight: 700; }
.lg-count { font-size: 9px; color: var(--ui-text-3); }
.lg-members { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 6px; }
.lg-member {
  font-family: var(--ui-font-mono); font-size: 9.5px; padding: 2px 8px;
  border-radius: var(--ui-radius-full); border: 1px solid var(--ui-border);
  background: var(--ui-glass); color: var(--ui-text-2); cursor: pointer;
  max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.lg-member:hover { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); color: var(--ui-accent); }
.lg-add { display: flex; gap: 6px; align-items: center; margin-top: 8px; }
.lg-add > :first-child { flex: 1; min-width: 0; }

/* tags */
.style-tags { display: flex; flex-wrap: wrap; gap: 6px; }
.style-tag {
  min-height: 44px;
  font-family: var(--ui-font-mono); font-size: 10px; padding: 4px 10px;
  border-radius: var(--ui-radius-full); border: 1px solid var(--ui-border);
  background: var(--ui-glass); color: var(--ui-text-2); cursor: pointer; line-height: 1.2;
}
.style-tag:hover { border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent); color: var(--ui-accent); }

/* favorites */
.fav-list { display: flex; flex-direction: column; gap: 6px; }
.fav-item {
  display: flex; align-items: center; gap: 8px; min-height: 44px; padding: 6px 8px;
  border: 1px solid var(--ui-border); border-radius: var(--ui-radius-sm);
  background: var(--ui-glass); cursor: pointer;
}
.fav-item:hover { border-color: var(--ui-border-hover); }
.fav-icon { color: var(--ui-text-2); display: flex; flex-shrink: 0; }
.fav-name { font-size: 11px; font-weight: 600; flex: 1; min-width: 0; }

.text-muted { color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important; }

@media (max-width: 600px) {
  .org-panel { padding-top: max(12px, env(safe-area-inset-top)); padding-bottom: max(16px, env(safe-area-inset-bottom)); }
  .org-tabs { gap: 6px; margin-inline: -2px; }
  .org-tabs button { font-size: 9px; }
  .create-row, .loose-create { align-items: stretch; flex-wrap: wrap; }
  .create-row > :first-child, .loose-create > :first-child { flex: 1 1 100%; }
  .create-row > :not(:first-child) { flex: 1 1 calc(50% - 3px); min-width: 0; }
  .create-row > :last-child, .loose-create > :last-child { min-height: 44px; }
  .lg-add { align-items: stretch; flex-wrap: wrap; }
  .lg-add > :first-child { flex: 1 1 100%; }
  .lg-add > :last-child { min-height: 44px; align-self: stretch; }
  .lg-member { min-height: 36px; max-width: min(100%, 220px); }
  .style-tags { gap: 8px; }
  .fav-item { padding-block: 8px; }
}

@media (prefers-reduced-motion: reduce) {
  .org-panel *, .org-panel *::before, .org-panel *::after { scroll-behavior: auto !important; transition-duration: 0.01ms !important; animation-duration: 0.01ms !important; animation-iteration-count: 1 !important; }
}
</style>
