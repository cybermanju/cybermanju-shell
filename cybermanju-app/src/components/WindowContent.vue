<!-- CyberManju OS — WindowContent (src/components/WindowContent.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="window-content-panel">
    <!-- Unified search (merged FACES + COLLECTIONS + SEARCH — opens targets itself) -->
    <div v-if="panelType === 'search'" class="panel-search-host">
      <OmniSearchPanel />
    </div>

    <!-- Trash Panel -->
    <div v-if="panelType === 'trash'" class="panel-page">
      <div class="panel-card">
        <UiToolbar divided>
          <template #lead>
            <div class="panel-title">TRASH</div>
          </template>
          <template #trail>
            <UiButton size="sm" icon="solar:refresh-bold" icon-only title="Refresh trash" aria-label="Refresh trash" @click="store.fetchTrashItems()" />
            <UiButton size="sm" variant="danger" icon="solar:trash-bin-trash-bold" icon-only title="Empty trash (asks first)" aria-label="Empty trash" :disabled="store.trashItems.length === 0" @click="confirmEmptyTrashVisible = true" />
          </template>
        </UiToolbar>
        <p class="panel-hint">Deleted files rest here. Restore them, or delete permanently.</p>
        <UiEmpty
          v-if="store.trashItems.length === 0"
          size="sm"
          icon="solar:trash-bin-trash-bold"
          title="Trash is empty"
          description="Files you delete will appear here so you can restore them."
        />
        <div v-else class="trash-list">
          <div v-for="item in store.trashItems" :key="item.id" class="trash-item">
            <span class="trash-icon"><AppIcon :name="item.originalFile.fileType === 'folder' ? 'solar:folder-bold' : 'solar:file-bold'" :size="14" /></span>
            <div class="trash-info">
              <span class="trash-name truncate">{{ item.originalFile.name }}</span>
              <span class="trash-date text-muted">{{ new Date(item.deletedAt).toLocaleDateString() }}</span>
            </div>
            <div class="trash-actions">
              <UiButton size="xs" icon="solar:undo-left-round-bold" icon-only :title="`Restore ${item.originalFile.name}`" :aria-label="`Restore ${item.originalFile.name}`" @click="store.restoreTrashItem(item.originalFile.id)" />
              <UiButton size="xs" variant="danger" icon="solar:trash-bin-trash-bold" icon-only :title="`Delete ${item.originalFile.name} permanently (asks first)`" :aria-label="`Delete ${item.originalFile.name} permanently`" @click="askDeleteTrashItem(item.originalFile.id, item.originalFile.name)" />
            </div>
          </div>
        </div>
        <UiModal
          v-model:visible="confirmEmptyTrashVisible"
          title="Empty trash?"
          :subtitle="`${store.trashItems.length} item${store.trashItems.length === 1 ? '' : 's'} will be permanently deleted`"
          icon="solar:trash-bin-trash-bold"
          size="sm"
          danger
        >
          <p class="modal-text">This cannot be undone. Files in trash will be permanently deleted.</p>
          <template #footer>
            <UiButton variant="ghost" @click="confirmEmptyTrashVisible = false">Keep files</UiButton>
            <UiButton variant="danger" icon="solar:trash-bin-trash-bold" :loading="trashBusy" @click="doEmptyTrash()">Delete permanently</UiButton>
          </template>
        </UiModal>
        <UiModal
          v-model:visible="confirmDeleteTrashVisible"
          title="Delete permanently?"
          :subtitle="pendingTrashName"
          icon="solar:trash-bin-trash-bold"
          size="sm"
          danger
        >
          <p class="modal-text">“{{ pendingTrashName }}” will be permanently deleted. This cannot be undone.</p>
          <template #footer>
            <UiButton variant="ghost" @click="confirmDeleteTrashVisible = false">Keep file</UiButton>
            <UiButton variant="danger" icon="solar:trash-bin-trash-bold" :loading="trashBusy" @click="doDeleteTrashItem()">Delete permanently</UiButton>
          </template>
        </UiModal>
      </div>
    </div>

    <!-- Activity Panel -->
    <div v-if="panelType === 'activity'" class="panel-page">
      <div class="panel-card">
        <UiToolbar divided>
          <template #lead>
            <div class="panel-title">ACTIVITY LOG</div>
          </template>
          <template #trail>
            <UiButton size="sm" icon="solar:refresh-bold" icon-only title="Refresh activity" aria-label="Refresh activity" @click="store.fetchAuditLog()" />
          </template>
        </UiToolbar>
        <p class="panel-hint">Timeline of file operations, newest first.</p>
        <UiEmpty
          v-if="store.auditLog.length === 0"
          size="sm"
          icon="solar:pulse-bold"
          title="No activity yet"
          description="Actions like uploads, moves, and deletes will show up here."
        />
        <div v-else class="activity-list">
          <div v-for="entry in store.auditLog" :key="entry.id" class="activity-item">
            <span class="activity-action">{{ entry.action.toUpperCase() }}</span>
            <span class="activity-entity text-muted">{{ entry.entityType }}</span>
            <span class="activity-date text-muted">{{ new Date(entry.timestamp).toLocaleString() }}</span>
            <span v-if="entry.details && Object.keys(entry.details).length" class="activity-detail text-muted">{{ JSON.stringify(entry.details).substring(0, 40) }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Recent Panel -->
    <div v-if="panelType === 'recent'" class="panel-page">
      <div class="panel-card">
        <UiToolbar divided>
          <template #lead>
            <div class="panel-title">RECENT FILES</div>
          </template>
          <template #trail>
            <UiButton size="sm" icon="solar:refresh-bold" icon-only title="Refresh recent files" aria-label="Refresh recent files" :loading="store.isLoading" @click="refreshPanel()" />
          </template>
        </UiToolbar>
        <p class="panel-hint">Newest first. Open a file to see it here.</p>
        <UiError
          v-if="store.lastError"
          size="sm"
          title="Could not load recent files"
          :message="store.lastError"
          retryable
          @retry="refreshPanel()"
        />
        <div v-else-if="store.isLoading && recentFiles.length === 0" class="panel-loading">
          <UiSpinner size="sm" show-label label="Loading recent files" />
        </div>
        <UiEmpty
          v-else-if="recentFiles.length === 0"
          size="sm"
          icon="solar:history-bold"
          title="No files yet"
          description="Upload or create a file and it will appear here."
        >
          <template #actions>
            <UiButton size="sm" variant="ghost" icon="solar:folder-bold" @click="wm.open('files')">Open files</UiButton>
          </template>
        </UiEmpty>
        <div v-else class="recent-list">
          <div
            v-for="f in recentFiles"
            :key="f.id"
            class="recent-item"
            @click="store.selectFile(f.id)"
          >
            <span class="recent-icon"><AppIcon :name="f.fileType === 'folder' ? 'solar:folder-bold' : 'solar:file-bold'" :size="14" /></span>
            <div class="recent-info">
              <span class="recent-name truncate">{{ f.name }}</span>
              <span class="recent-date text-muted">{{ new Date(f.modifiedAt).toLocaleDateString() }}</span>
            </div>
            <UiButton size="xs" variant="ghost" :icon="f.isStarred ? 'solar:star-bold' : 'solar:star-linear'" icon-only :title="f.isStarred ? `Remove ${f.name} from favorites` : `Star ${f.name}`" :aria-label="f.isStarred ? `Remove ${f.name} from favorites` : `Star ${f.name}`" @click.stop="store.toggleStar(f.id)" />
          </div>
        </div>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import OmniSearchPanel from '@/components/OmniSearchPanel.vue'
import UiButton from '@/components/ui/UiButton.vue'
import UiEmpty from '@/components/ui/UiEmpty.vue'
import UiError from '@/components/ui/UiError.vue'
import UiModal from '@/components/ui/UiModal.vue'
import UiSpinner from '@/components/ui/UiSpinner.vue'
import UiToolbar from '@/components/ui/UiToolbar.vue'
import { ref, computed, onMounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import type { PanelType } from '@/types'

const props = defineProps<{
  panelType: PanelType
}>()

const store = useAppStore()
const wm = useWindowManager()

async function refreshPanel() {
  store.clearError()
  await Promise.allSettled([store.fetchFiles(), store.fetchLooseGroups()])
}

onMounted(() => {
  if (props.panelType === 'recent') {
    void refreshPanel()
  }
})

/** The unified search panel opens its own targets (no host callback needed). */

// ── Trash confirmations (destructive actions always ask first) ──
const confirmEmptyTrashVisible = ref(false)
const confirmDeleteTrashVisible = ref(false)
const pendingTrashId = ref<string | null>(null)
const pendingTrashName = ref('')
const trashBusy = ref(false)

function askDeleteTrashItem(fileId: string, name: string) {
  pendingTrashId.value = fileId
  pendingTrashName.value = name
  confirmDeleteTrashVisible.value = true
}

async function doEmptyTrash() {
  trashBusy.value = true
  try {
    await store.emptyTrash()
    confirmEmptyTrashVisible.value = false
  } finally {
    trashBusy.value = false
  }
}

async function doDeleteTrashItem() {
  if (!pendingTrashId.value) return
  trashBusy.value = true
  try {
    await store.deleteFromTrash(pendingTrashId.value)
    confirmDeleteTrashVisible.value = false
    pendingTrashId.value = null
  } finally {
    trashBusy.value = false
  }
}

const recentFiles = computed(() =>
  [...store.files]
    .sort((a, b) => new Date(b.modifiedAt).getTime() - new Date(a.modifiedAt).getTime())
    .slice(0, 20)
)
</script>

<style scoped>
.window-content-panel {
  height: 100%;
  min-height: 100%;
  overflow-y: auto;
  overscroll-behavior: contain;
  touch-action: pan-x pan-y;
  scrollbar-gutter: stable;
  padding: 0;
  background: var(--ui-surface);
}
.window-content-panel::-webkit-scrollbar {
  width: 10px;
}
.window-content-panel::-webkit-scrollbar-thumb {
  background: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  border-radius: var(--ui-radius-full);
  border: 3px solid transparent;
  background-clip: content-box;
}
.window-content-panel::-webkit-scrollbar-thumb:hover {
  background: var(--ui-accent);
  background-clip: content-box;
  border: 2px solid transparent;
}

.panel-page {
  padding: 12px;
}

.panel-card {
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 16px;
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-1);
}

.panel-card-row {
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  padding: 10px 12px;
  cursor: pointer;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out);
}

.panel-card-row:hover {
  border-color: var(--ui-border-hover);
}

.panel-card-row.active {
  border-color: color-mix(in srgb, var(--ui-accent) 40%, transparent);
  box-shadow: var(--ui-glow-soft);
}

.panel-title {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 700;
  color: var(--ui-text);
  letter-spacing: 1px;
  margin-bottom: 8px;
}

.panel-hint {
  font-family: var(--ui-font);
  font-size: 9px;
  color: var(--ui-text-3);
  margin-bottom: 12px;
}

.bw-title {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 700;
  color: var(--ui-text);
  margin-bottom: 10px;
  letter-spacing: 1px;
}

.bw-card {
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  padding: 12px;
  color: var(--ui-text);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
}

.search-card { margin-bottom: 12px; }
.search-summary { margin: 0; font-size: 10px; flex: 1; min-width: 0; }
.search-sub { margin: -6px 0 10px; }
.modal-text { margin: 0; font-size: 13px; line-height: 1.5; }

.recent-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.recent-head .bw-title { margin-bottom: 8px; }

.text-muted {
  color: var(--ui-text-3) !important;
}

/* Search panel styles */
.panel-search {
  padding: 12px;
}

.recent-searches {
  margin-bottom: 10px;
}

.recent-search-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 10px;
  cursor: pointer;
  font-family: var(--ui-font);
  font-size: 11px;
  color: var(--ui-text);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  margin-bottom: 4px;
  background: var(--ui-glass);
  text-align: left;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out);
}

.recent-search-item:hover {
  border-color: var(--ui-border-hover);
  background: var(--ui-glass-2);
}

.search-results-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.search-result-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px;
  cursor: pointer;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out);
  border-radius: var(--ui-radius-md);
}

.search-result-item:hover {
  border-color: var(--ui-border-hover);
}

.search-match-type {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 6px;
  background: var(--ui-surface-3);
  color: var(--ui-text-2);
  white-space: nowrap;
  border-radius: var(--ui-radius-xs);
}

.search-result-body {
  flex: 1;
  min-width: 0;
}

.search-result-name {
  font-size: 11px;
  font-weight: 700;
  margin-bottom: 2px;
}

.search-result-snippet {
  font-size: 10px;
  word-break: break-word;
}

.search-result-score {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  color: var(--ui-text-3);
}

.scene-strip {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 10px;
}

.scene-strip__label {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.12em;
  color: var(--ui-text-3);
}

.scene-chip {
  font-family: var(--ui-font-mono);
  font-size: 9.5px;
  font-weight: 700;
  letter-spacing: 0.04em;
  padding: 3px 10px;
  border-radius: var(--ui-radius-full);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 40%, transparent);
  background: var(--ui-accent-softer);
  color: var(--ui-accent);
  cursor: pointer;
}

.scene-chip:hover {
  background: color-mix(in srgb, var(--ui-accent) 20%, transparent);
}

.search-result-scene {
  display: inline-block;
  margin-top: 3px;
  font-family: var(--ui-font-mono);
  font-size: 8.5px;
  font-weight: 700;
  letter-spacing: 0.06em;
  color: var(--ui-accent);
  background: var(--ui-accent-softer);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 30%, transparent);
  border-radius: var(--ui-radius-full);
  padding: 1px 7px;
}

.search-result-name :deep(mark),
.search-result-snippet :deep(mark) {
  background: color-mix(in srgb, var(--ui-accent) 35%, transparent);
  color: var(--ui-text);
  border-radius: 2px;
  padding: 0 2px;
}

.trash-list,
.activity-list,
.fav-list,
.recent-list,
.loose-groups-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.trash-item,
.fav-item,
.recent-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-glass);
  cursor: pointer;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out);
}

.trash-item:hover,
.fav-item:hover,
.recent-item:hover {
  border-color: var(--ui-border-hover);
}

.trash-icon,
.fav-icon,
.recent-icon {
  color: var(--ui-text-2);
  display: flex;
  flex-shrink: 0;
}

.trash-info,
.recent-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.trash-name,
.fav-name,
.recent-name {
  font-size: 11px;
  font-weight: 600;
}

.trash-date,
.recent-date {
  font-size: 9px;
}

.trash-actions {
  display: flex;
  gap: 4px;
}

.activity-item {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--ui-hairline);
  font-size: 10px;
  align-items: baseline;
}

.activity-action {
  font-weight: 700;
  min-width: 72px;
}

.activity-entity {
  min-width: 70px;
}

.activity-date {
  font-family: var(--ui-font-mono);
  font-size: 9px;
}

.activity-detail {
  font-family: var(--ui-font-mono);
  font-size: 9px;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.lg-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.lg-name {
  font-size: 12px;
  font-weight: 700;
}

.lg-count {
  font-size: 9px;
  color: var(--ui-text-3);
}

.style-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.style-tag {
  font-family: var(--ui-font-mono);
  font-size: 10px;
  padding: 4px 10px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass);
  color: var(--ui-text-2);
  cursor: pointer;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), color var(--ui-dur-fast) var(--ui-ease-out);
}

.style-tag:hover {
  border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent);
  color: var(--ui-accent);
}

.truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.panel-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px 0;
}

.loose-create {
  display: flex;
  gap: 8px;
  align-items: flex-end;
  margin-bottom: 12px;
}

.loose-create > :first-child {
  flex: 1;
  min-width: 0;
}

.lg-members {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 6px;
}

.lg-member {
  font-family: var(--ui-font-mono);
  font-size: 9.5px;
  padding: 2px 8px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border);
  background: var(--ui-glass);
  color: var(--ui-text-2);
  cursor: pointer;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: border-color var(--ui-dur-fast) var(--ui-ease-out), color var(--ui-dur-fast) var(--ui-ease-out);
}

.lg-member:hover {
  border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent);
  color: var(--ui-accent);
}

.lg-add {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-top: 8px;
}

.lg-add > :first-child {
  flex: 1;
  min-width: 0;
}

@media (prefers-reduced-motion: reduce) {
  .window-content-panel .ui-empty,
  .window-content-panel .ui-error {
    animation: none;
  }
}
</style>
