<!-- CyberManju OS — FilePermissionsPanel (src/components/FilePermissionsPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="perms-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-perms"><AppIcon name="solar:key-bold" /></span>
        <h2 class="panel-title">FILE PERMISSIONS</h2>
      </div>
      <button class="close-btn" @click="$emit('close')" aria-label="CLOSE" title="CLOSE"><AppIcon name="solar:close-bold" :size="14" /></button>
    </div>

    <div v-if="!store.selectedFile" class="empty-state">
      <p class="text-muted">SELECT A FILE TO VIEW PERMISSIONS</p>
    </div>

    <template v-else>
      <div class="file-info">
        <span class="fi-name">{{ store.selectedFile.name }}</span>
        <span class="fi-path text-muted">{{ store.selectedFile.path }}</span>
      </div>

      <div class="section">
        <h3 class="section-title"><AppIcon name="solar:key-bold" :size="13" /> ACCESS CONTROL</h3>
        <div v-if="permissions.length === 0" class="text-muted" style="font-size:10px;">NO PERMISSIONS SET</div>
        <div v-else class="perms-list">
          <div v-for="(perm, idx) in permissions" :key="perm.userId + idx" class="perm-row">
            <span class="perm-user">{{ perm.username }}</span>
            <span class="perm-access">{{ perm.access.toUpperCase() }}</span>
            <button class="perm-revoke" @click="handleRevoke(perm)" title="REVOKE ACCESS" aria-label="REVOKE ACCESS"><AppIcon name="solar:close-bold" :size="12" /></button>
          </div>
        </div>
      </div>

      <div class="section grant-section">
        <h3 id="share-link-heading" class="section-title"><AppIcon name="solar:share-bold" :size="13" /> SHARE LINK</h3>
        <div class="share-row">
          <input aria-labelledby="share-link-heading" :value="shareLink" class="bw-input" readonly style="flex:1;" @click="($event.target as HTMLInputElement).select()" />
          <button class="bw-btn" @click="copyShareLink" :disabled="!shareLink" title="COPY LINK" aria-label="COPY LINK"><AppIcon name="solar:copy-bold" :size="13" /></button>
        </div>
        <div v-if="shareCopied" class="share-copied text-muted">LINK COPIED TO CLIPBOARD</div>
      </div>

      <div class="section grant-section">
        <h3 class="section-title"><AppIcon name="solar:add-bold" :size="13" /> GRANT ACCESS</h3>
        <div class="grant-row">
          <select aria-label="User to grant access" v-model="grantUserId" class="bw-input" style="flex:1;">
            <option value="" disabled>SELECT USER</option>
            <option v-for="u in store.users" :key="u.id" :value="u.id">{{ u.username }} ({{ u.role }})</option>
          </select>
        </div>
        <div class="grant-row">
          <select aria-label="Access level" v-model="grantAccess" class="bw-input" style="flex:1;">
            <option value="read">READ</option>
            <option value="write">WRITE</option>
            <option value="admin">ADMIN</option>
          </select>
          <button class="bw-btn" @click="handleGrant" :disabled="!grantUserId" title="GRANT ACCESS" aria-label="GRANT ACCESS"><AppIcon name="solar:user-plus-bold" :size="13" /></button>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, computed, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { invoke, getAuthToken } from '@/composables/useTauri'
import { copyToClipboard } from '@/utils/clipboard'
import type { FilePermission } from '@/types'

defineEmits<{ close: [] }>()

const store = useAppStore()

const permissions = ref<FilePermission[]>([])
const grantUserId = ref('')
const grantAccess = ref<'read' | 'write' | 'admin'>('read')
const shareCopied = ref(false)

const shareLink = computed(() => {
  if (!store.selectedFile) return ''
  const base = window.location.origin || 'http://localhost:3456'
  return `${base}/share/${store.selectedFile.id}`
})

watch(() => store.selectedFileId, async (id) => {
  if (id) {
    await fetchPermissions(id)
  } else {
    permissions.value = []
  }
})

async function fetchPermissions(fileId: string) {
  try {
    const perms = await invoke<FilePermission[]>('get_file_permissions', { fileId })
    permissions.value = perms
  } catch {
    permissions.value = []
  }
}

async function handleGrant() {
  if (!grantUserId.value || !store.selectedFileId) return
  try {
    await invoke('grant_file_permission', {
      fileId: store.selectedFileId,
      userId: grantUserId.value,
      access: grantAccess.value,
      sessionToken: getAuthToken(),
    })
    store.notifySuccess('Permission granted')
    await fetchPermissions(store.selectedFileId)
    grantUserId.value = ''
  } catch (e) {
    store.notifyError('Failed to grant permission', e)
  }
}

async function copyShareLink() {
  if (!shareLink.value) return
  try {
    if (!await copyToClipboard(shareLink.value)) throw new Error('Clipboard unavailable')
    shareCopied.value = true
    setTimeout(() => { shareCopied.value = false }, 2000)
  } catch {
    store.notifyError('Failed to copy link', '')
  }
}

async function handleRevoke(perm: FilePermission) {
  if (!store.selectedFileId) return
  try {
    await invoke('revoke_file_permission', {
      fileId: store.selectedFileId,
      userId: perm.userId,
      sessionToken: getAuthToken(),
    })
    store.notifySuccess('Permission revoked')
    await fetchPermissions(store.selectedFileId)
  } catch (e) {
    store.notifyError('Failed to revoke permission', e)
  }
}
</script>

<style scoped>
.perms-panel {
  width: 100%;
  height: 100%;
  background: var(--ui-surface);
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 16px;
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
.icon-perms { font-size: 16px; }
.panel-title { font-size: 14px; font-weight: 800; letter-spacing: 1px; margin: 0; }

.close-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  padding: 2px 6px;
  cursor: pointer;
  font-family: var(--ui-font);
  font-size: 9px;
}

.close-btn:hover {
  background: var(--ui-glass-2);
  color: var(--ui-text);
}

.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 120px;
}

.file-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 16px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ui-border);
}

.fi-name { font-size: 13px; font-weight: 700; }
.fi-path { font-size: 9px; overflow-wrap: anywhere; }

.section { margin-bottom: 16px; }

.section-title {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1px;
  color: color-mix(in srgb, var(--ui-text) 60%, transparent);
  margin: 0 0 8px;
}

.perms-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.perm-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--ui-border);
  font-size: 10px;
  min-width: 0;
}

.perm-user { flex: 1; min-width: 0; font-weight: 600; overflow-wrap: anywhere; }
.perm-access { font-size: 9px; border: 1px solid var(--ui-border-strong); padding: 0 4px; }

.perm-revoke {
  background: transparent;
  border: 1px solid var(--ui-border-strong);
  color: var(--ui-text);
  padding: 1px 4px;
  cursor: pointer;
  font-family: var(--ui-font);
  font-size: 8px;
}

.perm-revoke:hover {
  background: var(--ui-glass-2);
  color: var(--ui-text);
}

.grant-section {
  padding-top: 12px;
  border-top: 1px solid var(--ui-border);
}

.grant-row {
  display: flex;
  gap: 6px;
  margin-bottom: 6px;
}

.bw-input {
  min-width: 0;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: 10px;
  padding: 4px 6px;
}

.bw-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  padding: 4px 12px;
  cursor: pointer;
  font-family: var(--ui-font);
  font-size: 10px;
  font-weight: 700;
  white-space: nowrap;
}

.bw-btn:hover {
  background: var(--ui-glass-2);
  color: var(--ui-text);
}

.bw-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.share-row {
  display: flex;
  gap: 6px;
  margin-bottom: 6px;
}

.share-row .bw-input { overflow: hidden; text-overflow: ellipsis; }

.share-copied {
  font-size: 9px;
  margin-bottom: 8px;
}

.text-muted { color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important; }

@media (max-width: 640px) {
  .perms-panel {
    box-sizing: border-box;
    min-height: 100%;
    max-height: 100dvh;
    padding: max(12px, env(safe-area-inset-top)) max(12px, env(safe-area-inset-right)) max(16px, env(safe-area-inset-bottom)) max(12px, env(safe-area-inset-left));
  }

  .panel-header {
    position: sticky;
    top: calc(-1 * max(12px, env(safe-area-inset-top)));
    z-index: 1;
    padding: 8px 0 12px;
    background: var(--ui-surface);
  }

  .panel-title { font-size: 13px; }

  .close-btn,
  .perm-revoke,
  .bw-btn {
    min-width: 44px;
    min-height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn { padding: 8px; }

  .file-info {
    margin-bottom: 18px;
    padding: 12px;
    border: 1px solid var(--ui-border);
    border-radius: 14px;
    background: color-mix(in srgb, var(--ui-surface) 88%, var(--ui-glass-2));
  }

  .fi-name { font-size: 14px; line-height: 1.35; }
  .fi-path { font-size: 11px; line-height: 1.4; }

  .section { margin-bottom: 20px; }
  .section-title { margin-bottom: 10px; font-size: 11px; }

  .perm-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas: "user revoke" "access revoke";
    align-items: center;
    gap: 5px 10px;
    min-height: 64px;
    padding: 10px 10px 10px 12px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--ui-surface) 92%, var(--ui-glass-2));
  }

  .perm-user { grid-area: user; font-size: 13px; line-height: 1.3; }
  .perm-access { grid-area: access; justify-self: start; padding: 3px 7px; font-size: 10px; border-radius: 6px; }
  .perm-revoke { grid-area: revoke; border-radius: 10px; }

  .grant-section { padding-top: 16px; }
  .grant-row,
  .share-row { flex-direction: column; gap: 8px; margin-bottom: 8px; }

  .grant-row .bw-input,
  .share-row .bw-input,
  .grant-row .bw-btn,
  .share-row .bw-btn { width: 100%; box-sizing: border-box; }

  .bw-input {
    min-height: 44px;
    padding: 10px 12px;
    border-radius: 10px;
    font-size: 13px;
  }

  .bw-btn { padding: 10px 14px; border-radius: 10px; }
  .share-row .bw-input { text-overflow: ellipsis; }
  .share-copied { font-size: 11px; }
}

@media (prefers-reduced-motion: reduce) {
  .perms-panel { scroll-behavior: auto; }
}
</style>
