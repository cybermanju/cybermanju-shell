<!-- CyberManju OS — FileUploadDialog (src/components/FileUploadDialog.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <div v-if="visible" class="upload-overlay" @click.self="$emit('close')">
      <div ref="uploadRef" class="upload-modal">
        <div class="upload-header">
          <span>FILE UPLOAD</span>
          <button class="upload-close" @click="$emit('close')" aria-label="CLOSE" title="CLOSE"><AppIcon name="solar:close-bold" :size="14" /></button>
        </div>

        <div
          class="drop-zone"
          :class="{ 'drop-active': isDragging }"
          @dragover.prevent="isDragging = true"
          @dragleave.prevent="isDragging = false"
          @drop.prevent="handleDrop"
        >
          <span class="drop-zone-copy" v-if="!isDragging">DROP FILES HERE</span>
          <span class="drop-zone-copy" v-else>RELEASE TO UPLOAD</span>
          <span v-if="!isDragging" class="drop-zone-hint">OR</span>
          <button type="button" class="file-picker-button" @click="fileInput?.click()">CHOOSE FILES</button>
          <input ref="fileInput" type="file" multiple class="file-input-hidden" @change="handleFileInput" />
        </div>

        <div v-if="files.length > 0" class="upload-files">
          <div v-for="(f, idx) in files" :key="idx" class="upload-file-row" :class="{ done: f.status === 'done', error: f.status === 'error' }">
            <span class="uf-name truncate">{{ f.name }}</span>
            <span class="uf-size text-muted">{{ humanBytes(f.size) }}</span>
            <span class="uf-status">{{ f.status === 'uploading' ? 'UPLOADING..' : f.status === 'done' ? 'DONE' : f.status === 'error' ? 'FAILED' : 'PENDING' }}</span>
            <span v-if="f.error" class="uf-error text-muted">{{ f.error }}</span>
          </div>
        </div>

        <div class="upload-footer" v-if="files.length > 0">
          <span class="upload-progress-text">{{ completedCount }}/{{ files.length }} FILES</span>
          <UiButton
            variant="primary"
            size="sm"
            icon="solar:upload-bold"
            :loading="isUploading"
            :disabled="isUploading"
            @click="startUpload"
          >UPLOAD</UiButton>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { ref, toRef, computed, watch, nextTick } from 'vue'
import { useAppStore } from '@/stores/app'
import { humanBytes } from '@/utils/format'
import { invoke } from '@/composables/useTauri'
import { useFocusTrap } from '@/composables/useFocusTrap'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ close: [] }>()
const uploadRef = ref<HTMLElement | null>(null)
useFocusTrap(uploadRef, toRef(props, 'visible'))

const store = useAppStore()
const fileInput = ref<HTMLInputElement | null>(null)
const isDragging = ref(false)
const isUploading = ref(false)

interface UploadFile {
  name: string
  size: number
  data: ArrayBuffer
  status: 'pending' | 'uploading' | 'done' | 'error'
  error?: string
}

const files = ref<UploadFile[]>([])

const completedCount = computed(() => files.value.filter(f => f.status === 'done').length)

async function readFile(file: File): Promise<ArrayBuffer> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as ArrayBuffer)
    reader.onerror = reject
    reader.readAsArrayBuffer(file)
  })
}

async function handleDrop(e: DragEvent) {
  isDragging.value = false
  const droppedFiles = Array.from(e.dataTransfer?.files || [])
  for (const f of droppedFiles) {
    const data = await readFile(f)
    files.value.push({ name: f.name, size: f.size, data, status: 'pending' })
  }
}

function handleFileInput(e: Event) {
  const input = e.target as HTMLInputElement
  const selectedFiles = Array.from(input.files || [])
  Promise.all(selectedFiles.map(async f => {
    const data = await readFile(f)
    files.value.push({ name: f.name, size: f.size, data, status: 'pending' })
  }))
  if (input) input.value = ''
}

async function startUpload() {
  isUploading.value = true
  for (const f of files.value) {
    if (f.status === 'done' || f.status === 'uploading') continue
    f.status = 'uploading'
    try {
      const uint8 = new Uint8Array(f.data)
      await invoke('upload_file', { fileName: f.name, fileData: Array.from(uint8), parentPath: store.currentPath })
      f.status = 'done'
    } catch (e) {
      f.status = 'error'
      f.error = e instanceof Error ? e.message : String(e)
    }
  }
  isUploading.value = false
  await store.fetchFiles()
  emit('close')
}
</script>

<style scoped>
.upload-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, var(--ui-bg-deep) 62%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
  font-family: var(--ui-font);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  padding: max(12px, env(safe-area-inset-top)) max(12px, env(safe-area-inset-right))
    max(12px, env(safe-area-inset-bottom)) max(12px, env(safe-area-inset-left));
  box-sizing: border-box;
}

.upload-modal {
  background: var(--ui-glass-2);
  border: 1px solid var(--ui-border);
  width: 480px;
  max-width: min(480px, 100%);
  max-height: min(80vh, 100%);
  min-height: 0;
  display: flex;
  flex-direction: column;
  color: var(--ui-text);
  border-radius: var(--ui-radius-lg);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
}

.upload-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-border);
  font-weight: 700;
  font-size: 12px;
  letter-spacing: 1px;
}

.upload-close {
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-2);
  width: 44px;
  height: 44px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-family: var(--ui-font);
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out);
}

.upload-close:hover {
  background: var(--ui-glass-2);
  border-color: var(--ui-border-hover);
  color: var(--ui-text);
}

.drop-zone {
  border: 1px dashed var(--ui-border-strong);
  border-radius: var(--ui-radius-md);
  margin: 12px;
  padding: 24px 16px;
  text-align: center;
  cursor: pointer;
  font-size: 11px;
  min-height: 112px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  box-sizing: border-box;
  color: var(--ui-text-3);
  transition: border-color 0.15s, background 0.15s, color 0.15s;
}

.drop-zone:hover,
.drop-zone.drop-active {
  border-color: color-mix(in srgb, var(--ui-accent) 55%, transparent);
  background: var(--ui-accent-softer);
  color: var(--ui-text);
}

.file-input-hidden {
  display: none;
}

.drop-zone-copy { font-weight: 700; letter-spacing: .04em; }
.drop-zone-hint { color: var(--ui-text-3); font-size: 9px; }

.file-picker-button {
  min-height: 44px;
  padding: 10px 16px;
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-glass-2);
  color: var(--ui-text);
  font: inherit;
  font-weight: 700;
  letter-spacing: .04em;
  cursor: pointer;
}

.file-picker-button:hover,
.file-picker-button:focus-visible {
  border-color: var(--ui-accent);
  background: var(--ui-accent-softer);
}

.upload-files {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 240px;
}

.upload-file-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  font-size: 9px;
  background: color-mix(in srgb, var(--ui-glass) 50%, transparent);
  min-width: 0;
}

.upload-file-row.done {
  border-color: var(--ui-border);
  opacity: 0.6;
}

.upload-file-row.error {
  border-color: color-mix(in srgb, var(--ui-danger) 45%, transparent);
  background: color-mix(in srgb, var(--ui-danger) 10%, transparent);
}

.uf-name { flex: 1; }
.uf-size { flex-shrink: 0; }
.uf-status { flex-shrink: 0; font-weight: 700; }
.uf-error { flex: 1; text-align: right; font-size: 8px; color: var(--ui-danger); }

.upload-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px;
  border-top: 1px solid var(--ui-border);
  flex-shrink: 0;
  gap: 12px;
}

.upload-progress-text {
  font-size: 10px;
  color: var(--ui-text-3);
}

.text-muted { color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important; }
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

@media (max-width: 600px) {
  .upload-modal {
    width: 100%;
    max-height: 100%;
  }

  .upload-header { padding: 8px 12px 8px 16px; }
  .drop-zone { margin: 12px 12px 10px; min-height: 124px; }
  .upload-files { padding: 0 12px; max-height: none; }
  .upload-file-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 5px 8px;
    padding: 9px 10px;
    font-size: 10px;
  }
  .uf-name { grid-column: 1 / -1; min-width: 0; }
  .uf-size { grid-column: 1; justify-self: start; }
  .uf-status { grid-column: 2; justify-self: end; }
  .uf-error {
    grid-column: 1 / -1;
    text-align: left;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .upload-footer { padding: 10px 12px max(10px, env(safe-area-inset-bottom)); }
}

@media (prefers-reduced-motion: reduce) {
  .upload-close, .drop-zone { transition: none; }
}
</style>
