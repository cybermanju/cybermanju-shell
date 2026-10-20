<!-- CyberManju OS — ConfirmDialog (src/components/ConfirmDialog.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <div
      v-if="visible"
      class="confirm-overlay"
      @click.self="handleCancel"
      role="dialog"
      aria-modal="true"
      :aria-label="title"
    >
      <div ref="dialogRef" class="confirm-modal">
        <span v-if="isPlasma" class="confirm-icon"><AppIcon :name="icon || 'solar:info-circle-bold'" :size="28" /></span>
        <div class="confirm-main">
          <div class="confirm-header">{{ title }}</div>
          <div class="confirm-body">{{ message }}</div>
          <div class="confirm-actions">
            <button ref="cancelBtnRef" class="confirm-btn cancel" @click="handleCancel"><AppIcon name="solar:close-bold" :size="13" /> {{ cancelText }}</button>
            <button ref="confirmBtnRef" class="confirm-btn ok" @click="handleConfirm"><AppIcon name="solar:check-bold" :size="13" /> {{ confirmText }}</button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, toRef, watch } from 'vue'
import AppIcon from '@/components/AppIcon.vue'
import { useFocusTrap } from '@/composables/useFocusTrap'
import { useTheme } from '@/composables/useTheme'

const props = withDefaults(defineProps<{
  visible: boolean
  title?: string
  message?: string
  confirmText?: string
  cancelText?: string
  icon?: string
}>(), {
  title: 'Confirm',
  message: 'Are you sure?',
  confirmText: 'OK',
  cancelText: 'Cancel',
  icon: '',
})

const isPlasma = computed(() => useTheme().shellStyle.value === 'plasma')

const emit = defineEmits<{
  confirm: []
  cancel: []
  'update:visible': [value: boolean]
}>()

const dialogRef = ref<HTMLElement | null>(null)
const cancelBtnRef = ref<HTMLElement | null>(null)
const confirmBtnRef = ref<HTMLElement | null>(null)

useFocusTrap(dialogRef, toRef(props, 'visible'))

function handleConfirm() { emit('confirm'); emit('update:visible', false) }
function handleCancel() { emit('cancel'); emit('update:visible', false) }

/** Escape cancels like the Cancel button (document-level: focus can sit
 *  anywhere in the dialog, and focusFirst only lands after its 50ms tick). */
function handleKeydown(e: KeyboardEvent) {
  if (e.key !== 'Escape' || e.defaultPrevented) return
  e.preventDefault()
  handleCancel()
}

watch(() => props.visible, (v) => {
  if (v) document.addEventListener('keydown', handleKeydown)
  else document.removeEventListener('keydown', handleKeydown)
}, { immediate: true })

onUnmounted(() => document.removeEventListener('keydown', handleKeydown))
</script>

<style scoped>
.confirm-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, var(--ui-bg-deep) 62%, transparent);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10002;
  animation: ui-fade-in var(--ui-dur) var(--ui-ease-out);
}

.confirm-modal {
  position: relative;
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-menu);
  padding: 18px;
  max-width: 260px;
  width: calc(100% - 48px);
  font-family: var(--ui-font);
  text-align: center;
  animation: ui-fade-in var(--ui-dur) ease-out both;
}

.confirm-header {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text);
  margin-bottom: 6px;
}

.confirm-body {
  font-size: 12px;
  color: var(--ui-text-2);
  margin-bottom: 16px;
  line-height: 1.4;
}

.confirm-actions {
  display: flex;
  justify-content: center;
  gap: 8px;
}

.confirm-btn {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 26px;
  padding: 4px 12px;
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  border-radius: var(--ui-radius-sm);
  border: 1px solid var(--ui-border-strong);
  background: var(--ui-surface-2);
  color: var(--ui-text);
  transition:
    background-color var(--ui-dur-fast) ease-out,
    border-color var(--ui-dur-fast) ease-out;
}

.confirm-btn:hover {
  background: var(--ui-surface-3);
  border-color: var(--ui-border-hover);
}

.confirm-btn.ok {
  background: var(--ui-accent);
  border-color: transparent;
  color: var(--ui-on-accent);
}

.confirm-btn.ok:hover {
  background: var(--ui-accent-strong);
  color: var(--ui-on-accent);
}

.confirm-btn:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}

/* Touch: the 26px desktop buttons are below the tap floor. */
@media (pointer: coarse) {
  .confirm-btn {
    min-height: 44px;
    font-size: 15px;
  }
}

/* Plasma standard dialog: icon left, text right, buttons right-aligned. */
.confirm-icon {
  display: none;
}

[data-ui-shell='plasma'] .confirm-modal {
  display: flex;
  gap: 14px;
  max-width: 380px;
  text-align: left;
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-popup);
}

[data-ui-shell='plasma'] .confirm-icon {
  display: inline-flex;
  flex-shrink: 0;
  margin-top: 2px;
  color: var(--ui-text-2);
}

[data-ui-shell='plasma'] .confirm-main {
  flex: 1;
  min-width: 0;
}

[data-ui-shell='plasma'] .confirm-actions {
  justify-content: flex-end;
}
</style>
