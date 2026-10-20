<!-- CyberManju OS — CommandPalette (src/components/CommandPalette.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <div
      v-if="store.commandPaletteOpen"
      class="cp-overlay"
      @click.self="close"
    >
      <div ref="cpRef" class="cp-modal" role="dialog" aria-label="Command palette">
        <div class="cp-header">
          <span class="cp-prompt">&gt;</span>
          <input
            ref="inputRef"
            v-model="query"
            class="cp-input"
            placeholder="Type a command… (try “search” or “people”)"
            aria-label="Search commands"
            @keydown="handleKeydown"
            @input="filterCommands"
          />
          <button type="button" class="cp-close" aria-label="Close command palette" @click="close">×</button>
        </div>
        <div class="cp-results">
          <div
            v-for="(group, gi) in filteredGroups"
            :key="gi"
            class="cp-group"
          >
            <div class="cp-group-label">{{ group.label }}</div>
            <div
              v-for="(cmd, ci) in group.items"
              :key="cmd.id"
              class="cp-item"
              :class="{ active: activeIndex === getGlobalIndex(gi, ci) }"
              @click="execute(cmd)"
              @mouseenter="activeIndex = getGlobalIndex(gi, ci)"
            >
              <span class="cp-item-icon"><AppIcon :name="cmd.icon" :size="14" /></span>
              <span class="cp-item-label">{{ cmd.label }}</span>
              <span v-if="cmd.shortcut" class="cp-item-shortcut">{{ cmd.shortcut }}</span>
            </div>
          </div>
          <div v-if="allCommandsFiltered.length === 0" class="cp-empty">
            <div class="cp-empty__title">No matches for “{{ query }}”</div>
            <div class="cp-empty__hint text-muted">Try a different word, or press Esc to clear.</div>
            <button type="button" class="cp-empty__clear" @click="query = ''">Clear search</button>
          </div>
        </div>
        <div class="cp-foot text-muted">↑↓ navigate · ⏎ run · Esc close · Ctrl+K toggles</div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, toRef, computed, nextTick, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { useLauncherStore } from '@/stores/launcher'
import { useWindowManager } from '@/composables/useWindowManager'
import type { PanelType } from '@/types'
import { useFocusTrap } from '@/composables/useFocusTrap'

const store = useAppStore()
const launcher = useLauncherStore()
const wm = useWindowManager()
const cpRef = ref<HTMLElement | null>(null)
useFocusTrap(cpRef, toRef(store, 'commandPaletteOpen'))

interface Command {
  id: string
  label: string
  icon: string
  shortcut?: string
  action: () => void
}

interface CommandGroup {
  label: string
  items: Command[]
}

const query = ref('')
const activeIndex = ref(0)
const inputRef = ref<HTMLInputElement | null>(null)

const commands = computed<CommandGroup[]>(() => [
  {
    label: 'NAVIGATION',
    items: [
      { id: 'nav-files', label: 'Go to Files', icon: 'solar:folder-bold', shortcut: '', action: () => { wm.open('files') } },
      { id: 'nav-search', label: 'Go to Search (apps, collections, people, files)', icon: 'solar:magnifier-bold', action: () => { wm.open('search') } },
      { id: 'nav-map', label: 'Go to Map', icon: 'solar:map-bold', action: () => { wm.open('map'); store.fetchGeoFiles() } },
      { id: 'nav-code', label: 'Open Code Studio', icon: 'solar:code-bold', action: () => { wm.open('editor') } },
      { id: 'nav-sync', label: 'Go to Sync (providers + OAuth)', icon: 'solar:refresh-bold', action: () => { wm.open('sync') } },
      { id: 'nav-transfer', label: 'Open Transfer Board (mv/cp between providers)', icon: 'solar:share-bold', action: () => { wm.open('transfer') } },
      { id: 'nav-accounts', label: 'Go to Account Manager', icon: 'solar:user-circle-bold', action: () => { wm.open('accounts') } },
      { id: 'nav-users', label: 'Go to Accounts & Users', icon: 'solar:users-group-rounded-bold', action: () => { wm.open('accounts', { tab: 'users' }) } },
      { id: 'nav-terminal', label: 'Open Terminal (cybsh)', icon: 'solar:file-terminal-bold', shortcut: 'Ctrl+`', action: () => { wm.open('terminal') } },
      { id: 'nav-tasks', label: 'Open Tasks (ps/top)', icon: 'solar:cpu-bold', action: () => { wm.open('processes') } },
      { id: 'nav-schedules', label: 'Open Schedules (cron)', icon: 'solar:alarm-bold', action: () => { wm.open('cron') } },
      { id: 'nav-disks', label: 'Open Disks & Volume', icon: 'solar:ssd-square-bold', action: () => { wm.open('disks') } },
      { id: 'nav-backup', label: 'Open Vault Backup (export/schedule/restore)', icon: 'solar:archive-bold', action: () => { wm.open('disks', { tab: 'backup' }) } },
      { id: 'nav-storage', label: 'Open Storage & Disks', icon: 'solar:database-bold', action: () => { wm.open('disks') } },
      { id: 'nav-settings', label: 'Go to Settings', icon: 'solar:settings-bold', action: () => { wm.open('settings') as PanelType } },
      { id: 'nav-trash', label: 'Go to Trash', icon: 'solar:trash-bin-trash-bold', action: () => { wm.open('trash'); store.fetchTrashItems() } },
      { id: 'nav-favorites', label: 'Open Search', icon: 'solar:star-bold', action: () => { wm.open('search') } },
      { id: 'nav-recent', label: 'Go to Recent Files', icon: 'solar:history-bold', action: () => { wm.open('recent') as PanelType } },
      { id: 'nav-activity', label: 'Go to Activity Log', icon: 'solar:pulse-bold', action: () => { wm.open('activity'); store.fetchAuditLog() } },
      { id: 'nav-notifications', label: 'Open Notification Center', icon: 'solar:bell-bold', action: () => { launcher.setMessagesOpen(true, 'system') } },
    ],
  },
  {
    label: 'ACTIONS',
    items: [
      { id: 'act-new-folder', label: 'New Folder', icon: 'solar:add-folder-bold', shortcut: 'Ctrl+N', action: () => { store.createFolderPromptOpen = true } },
      { id: 'act-encrypt', label: 'Encrypt Selected File', icon: 'solar:lock-bold', shortcut: 'Ctrl+E', action: () => { wm.open('encryption', { tab: 'shield' }) } },
      { id: 'act-compress', label: 'Compress Selected File', icon: 'solar:archive-bold', shortcut: 'Ctrl+Shift+C', action: () => { wm.open('encryption', { tab: 'compress' }) } },
      { id: 'act-batch-detect', label: 'Batch Face Detection', icon: 'solar:face-scan-circle-bold', action: () => { store.detectFacesBatch() } },
      { id: 'act-refresh', label: 'Refresh Files', icon: 'solar:refresh-bold', action: () => { store.fetchFiles() } },
      { id: 'act-identity', label: 'Accounts — OAuth sign-in & .cybermanju disk', icon: 'solar:login-bold', action: () => { wm.open('accounts') } },
    ],
  },
  {
    label: 'AGENT',
    items: [
      { id: 'agent-open', label: 'Open AI Agent panel', icon: 'solar:bot-bold', action: () => { wm.open('agent') } },
      { id: 'agent-abort', label: 'Agent — abort the current run', icon: 'solar:stop-circle-bold', action: () => {
        const job = store.activeAgentJob
        if (job && (job.status === 'running' || job.status === 'waiting_approval')) void store.abortAgentJob(job.jobId)
        else store.notifyError('No agent run to abort', 'the agent is idle')
      } },
      { id: 'agent-init', label: 'Agent — analyze repo and write AGENTS.md', icon: 'solar:file-text-bold', action: () => {
        wm.open('agent')
        const cfg = store.agentConfigs[0]
        if (!cfg) {
          store.notifyError('No agent config', 'open the Agent panel and save a config first')
          return
        }
        void store.initAgentRun(cfg.id)
      } },
    ],
  },
  {
    label: 'VIEW',
    items: [
      { id: 'view-grid', label: 'Grid View', icon: 'solar:grid-3x3-bold', shortcut: 'Ctrl+G', action: () => { wm.open('files'); store.viewMode = 'grid' } },
      { id: 'view-list', label: 'List View', icon: 'solar:list-bold', shortcut: 'Ctrl+L', action: () => { wm.open('files'); store.viewMode = 'list' } },
      { id: 'view-masonry', label: 'Masonry View', icon: 'solar:columns-3-bold', shortcut: 'Ctrl+M', action: () => { wm.open('files'); store.viewMode = 'masonry' } },
      { id: 'view-toggle-sidebar', label: 'Toggle Sidebar', icon: 'solar:panel-left-bold', shortcut: 'Ctrl+B', action: () => { store.sidebarCollapsed = !store.sidebarCollapsed } },
      { id: 'view-toggle-matrix', label: 'Toggle Matrix Rain', icon: 'solar:barcode-bold', action: () => { store.matrixRainEnabled = !store.matrixRainEnabled } },
    ],
  },
  {
    label: 'PANELS',
    items: [
      { id: 'panel-encryption', label: 'Open File Shield', icon: 'solar:lock-bold', shortcut: 'Ctrl+E', action: () => { wm.open('encryption', { tab: 'shield' }) } },
      { id: 'panel-compression', label: 'Open Compression Engine', icon: 'solar:archive-bold', shortcut: 'Ctrl+Shift+C', action: () => { wm.open('encryption', { tab: 'compress' }) } },
      { id: 'panel-permissions', label: 'Open Permissions Panel', icon: 'solar:key-bold', action: () => { wm.open('permissions' as PanelType) } },
      { id: 'panel-archive', label: 'Open Life Archive (notes, timeline, people, tasks)', icon: 'solar:notebook-bold', action: () => { wm.open('archive') } },
    ],
  },
])

function filterCommands() {
  activeIndex.value = 0
}

interface Cmd { id: string; label: string; icon: string; shortcut?: string; action: () => void }
interface CmdGroup { label: string; items: Cmd[] }

const filteredGroups = computed(() => {
  const q = query.value.toLowerCase().trim()
  if (!q) return commands.value as CmdGroup[]
  return (commands.value as CmdGroup[])
    .map((g: CmdGroup) => ({
      ...g,
      items: g.items.filter((c: Cmd) => c.label.toLowerCase().includes(q) || c.id.toLowerCase().includes(q)),
    }))
    .filter((g: CmdGroup) => g.items.length > 0)
})

const allCommandsFiltered = computed(() => {
  return filteredGroups.value.flatMap((g: CmdGroup) => g.items)
})

function getGlobalIndex(gi: number, ci: number): number {
  let idx = 0
  const groups = filteredGroups.value as CmdGroup[]
  for (let g = 0; g < gi; g++) {
    idx += groups[g].items.length
  }
  return idx + ci
}

function handleKeydown(e: KeyboardEvent) {
  const total = allCommandsFiltered.value.length
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    activeIndex.value = (activeIndex.value + 1) % Math.max(total, 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    activeIndex.value = (activeIndex.value - 1 + Math.max(total, 1)) % Math.max(total, 1)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const cmd = allCommandsFiltered.value[activeIndex.value]
    if (cmd) execute(cmd)
  } else if (e.key === 'Escape') {
    close()
  }
}

function execute(cmd: Command) {
  cmd.action()
  close()
}

function close() {
  store.commandPaletteOpen = false
  query.value = ''
}

watch(() => store.commandPaletteOpen, async (v: boolean) => {
  if (v) {
    await nextTick()
    inputRef.value?.focus()
    query.value = ''
    activeIndex.value = 0
  }
})
</script>

<style scoped>
.cp-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, var(--ui-bg-deep) 45%, transparent);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding: 120px 12px 12px;
  z-index: 10000;
  backdrop-filter: blur(14px) saturate(130%);
  -webkit-backdrop-filter: blur(14px) saturate(130%);
}

.cp-modal {width: 560px;
  max-width: 90vw;
  max-height: 440px;
  background: var(--ui-glass-2);
  border: 1px solid var(--ui-border);
  box-shadow: var(--ui-shadow-menu);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--ui-radius-lg);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  animation: ui-fade-in var(--ui-dur) ease-out both;
}

.cp-header {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--ui-separator);
  flex: 0 0 auto;
}

.cp-prompt {
  color: var(--ui-text-3);
  font-family: var(--ui-font);
  font-size: 17px;
  font-weight: 400;
}

.cp-input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 17px;
  font-weight: 400;
  outline: none;
}

.cp-close {
  flex: 0 0 44px;
  width: 44px;
  height: 44px;
  margin: -7px -8px -7px 0;
  padding: 0;
  border: 0;
  border-radius: var(--ui-radius-full);
  background: transparent;
  color: var(--ui-text-2);
  font-family: var(--ui-font);
  font-size: 25px;
  line-height: 1;
  cursor: pointer;
}

.cp-close:hover,
.cp-close:focus-visible {
  background: var(--ui-surface-2);
  color: var(--ui-text);
  outline: none;
}

.cp-input::placeholder {
  color: var(--ui-text-3);
}

.cp-results {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
  padding: 4px 0;
}

.cp-group {
  padding: 4px 0;
}

.cp-group-label {
  padding: 6px 12px 2px;
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 600;
  color: var(--ui-text-2);
  letter-spacing: 0;
  text-transform: lowercase;
}

.cp-group-label::first-letter {
  text-transform: uppercase;
}

.cp-item {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 22px;
  padding: 3px 8px;
  margin: 1px 4px;
  border-radius: var(--ui-radius-sm);
  cursor: pointer;
  font-family: var(--ui-font);
  font-size: 13px;
  color: var(--ui-text);
  min-width: 0;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.cp-item:hover,
.cp-item.active {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
}

.cp-item-icon {
  width: 20px;
  text-align: center;
  flex-shrink: 0;
  color: var(--ui-text-2);
}

.cp-item:hover .cp-item-icon,
.cp-item.active .cp-item-icon {
  color: inherit;
}

.cp-item-label {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}

.cp-item-shortcut {
  font-family: var(--ui-font);
  font-size: 12px;
  color: var(--ui-text-3);
  margin-left: 12px;
  flex-shrink: 0;
}

.cp-item:hover .cp-item-shortcut,
.cp-item.active .cp-item-shortcut {
  color: var(--ui-on-accent);
}

.cp-empty {
  padding: 24px;
  text-align: center;
  font-size: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
}

.cp-empty__title { font-size: 12px; font-weight: 700; color: var(--ui-text); }
.cp-empty__hint { font-size: 10px; }
.cp-empty__clear {
  margin-top: 4px;
  font-size: 10px;
  font-weight: 700;
  min-height: 44px;
  padding: 8px 12px;
  border-radius: var(--ui-radius-full);
  border: 1px solid var(--ui-border-strong);
  background: transparent;
  color: var(--ui-text-2);
  cursor: pointer;
}
.cp-empty__clear:hover { border-color: var(--ui-border-hover); color: var(--ui-text); }

.cp-foot {
  flex: 0 0 auto;
  padding: 6px 12px;
  border-top: 1px solid var(--ui-separator);
  font-family: var(--ui-font);
  font-size: 11px;
  color: var(--ui-text-3);
  text-align: center;
}

.text-muted { opacity: 0.5; }

@media (prefers-reduced-motion: reduce) {
  .cp-modal,
  .cp-item,
  .cp-close {
    animation: none;
    transition: none;
  }
}

/* KRunner style in plasma: top-center, flat, single input, no heavy border. */
[data-ui-shell='plasma'] .cp-overlay {
  align-items: flex-start;
  justify-content: center;
  padding: 12vh 12px 12px;
  background: transparent;
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
}

[data-ui-shell='plasma'] .cp-modal {
  border: 1px solid var(--ui-border);
  box-shadow: var(--ui-shadow-popup);
}

[data-ui-shell='plasma'] .cp-item {
  border-radius: var(--ui-radius-sm);
}

/* Phone overrides — deliberately LAST: when this block sat before the base
   `.cp-modal` / `.cp-item` / `.cp-input` rules, equal-specificity cascade
   made it dead code: rows stayed 22px, the modal stayed 90vw, and the input
   kept its desktop font size. */
@media (max-width: 560px) {
  .cp-overlay {
    align-items: center;
    padding: max(8px, env(safe-area-inset-top, 0px)) max(8px, env(safe-area-inset-right, 0px)) max(8px, env(safe-area-inset-bottom, 0px)) max(8px, env(safe-area-inset-left, 0px));
  }
  .cp-modal {
    width: 100%;
    max-width: 100%;
    max-height: calc(100dvh - max(16px, env(safe-area-inset-top, 0px) + env(safe-area-inset-bottom, 0px)));
  }
  .cp-input {
    font-size: 16px;
  }
  .cp-item {
    min-height: 44px;
    padding-block: 8px;
  }
}
</style>
