<!-- CyberManju OS — UiModal (src/components/ui/UiModal.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: caller attrs target the dialog and override defaults; subtitles are its default description. -->
<template>
  <Teleport to="body">
    <Transition name="ui-modal">
      <div v-if="visible" class="ui-modal-layer" @click.self="onBackdrop">
        <div
          ref="dialogRef"
          class="ui-modal"
          :class="[`ui-modal--${size}`, { 'ui-modal--danger': danger, 'ui-modal--sheet': variant === 'sheet' }]"
          role="dialog"
          aria-modal="true"
          :aria-label="title || undefined"
          :aria-describedby="subtitle ? subtitleId : undefined"
          v-bind="$attrs"
          @keydown.esc="onClose"
        >
          <header class="ui-modal__header">
            <div class="ui-modal__heading">
              <span v-if="icon" class="ui-modal__icon">
                <AppIcon :name="icon" :size="16" />
              </span>
              <div class="ui-modal__titles">
                <h2 class="ui-modal__title">{{ title }}</h2>
                <p v-if="subtitle" :id="subtitleId" class="ui-modal__subtitle">{{ subtitle }}</p>
              </div>
            </div>
            <UiButton
              v-if="closable"
              variant="ghost"
              size="sm"
              icon="solar:close-bold"
              aria-label="Close"
              @click="onClose"
            />
          </header>

          <div class="ui-modal__body">
            <slot />
          </div>

          <footer v-if="$slots.footer" class="ui-modal__footer">
            <slot name="footer" />
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch, toRef, useId } from 'vue'
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'
import { useFocusTrap } from '@/composables/useFocusTrap'

defineOptions({ inheritAttrs: false })
const subtitleId = useId()

const props = withDefaults(
  defineProps<{
    visible?: boolean
    title?: string
    subtitle?: string
    icon?: string
    size?: 'sm' | 'md' | 'lg' | 'xl'
    danger?: boolean
    closable?: boolean
    /** Close when the backdrop is clicked. */
    dismissable?: boolean
    /**
     * `dialog` (centered, default) or `sheet` (slides down from the top
     * edge, macOS-sheet style for auth/setup flows).
     */
    variant?: 'dialog' | 'sheet'
  }>(),
  {
    visible: false,
    title: '',
    subtitle: '',
    icon: '',
    size: 'md',
    danger: false,
    closable: true,
    dismissable: true,
    variant: 'dialog',
  }
)

const emit = defineEmits<{
  'update:visible': [value: boolean]
  close: []
}>()

const dialogRef = ref<HTMLElement | null>(null)
const isActive = toRef(props, 'visible')
useFocusTrap(dialogRef, isActive)

function onClose() {
  emit('update:visible', false)
  emit('close')
}

function onBackdrop() {
  if (props.dismissable) onClose()
}

watch(
  () => props.visible,
  (open) => {
    if (typeof document === 'undefined') return
    document.documentElement.style.overflow = open ? 'hidden' : ''
  }
)
</script>

<style scoped>
.ui-modal-layer {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  background: color-mix(in srgb, var(--ui-bg-deep) 62%, transparent);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
}

.ui-modal {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  max-height: calc(100vh - 48px);
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur-strong)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-3);
  overflow: hidden;
}

.ui-modal--sm { max-width: 380px; }
.ui-modal--md { max-width: 520px; }
.ui-modal--lg { max-width: 720px; }
.ui-modal--xl { max-width: 960px; }

.ui-modal--danger {
  border-color: color-mix(in srgb, var(--ui-danger) 45%, transparent);
}

.ui-modal__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: calc(14px * var(--ui-unit, 1)) calc(16px * var(--ui-unit, 1));
  border-bottom: 1px solid var(--ui-hairline);
}

.ui-modal__heading {
  display: flex;
  align-items: center;
  gap: 11px;
  min-width: 0;
}

.ui-modal__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-accent);
  background: var(--ui-accent-softer);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 28%, transparent);
}

.ui-modal--danger .ui-modal__icon {
  color: var(--ui-danger);
  background: color-mix(in srgb, var(--ui-danger) 12%, transparent);
  border-color: color-mix(in srgb, var(--ui-danger) 32%, transparent);
}

.ui-modal__titles {
  min-width: 0;
}

.ui-modal__title {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-md);
  font-weight: 600;
  letter-spacing: 0;
  color: var(--ui-text);
}

.ui-modal__subtitle {
  font-family: var(--ui-font);
  font-size: 12px;
  color: var(--ui-text-2);
  margin-top: 2px;
}

.ui-modal__body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: calc(16px * var(--ui-unit, 1));
  color: var(--ui-text-2);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-md);
}

.ui-modal__footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: calc(12px * var(--ui-unit, 1)) calc(16px * var(--ui-unit, 1));
  border-top: 1px solid var(--ui-hairline);
  background: color-mix(in srgb, var(--ui-surface) 55%, transparent);
}

/* transition */
.ui-modal-enter-active,
.ui-modal-leave-active {
  transition: opacity var(--ui-dur) var(--ui-ease-out);
}

.ui-modal-enter-active .ui-modal,
.ui-modal-leave-active .ui-modal {
  transition:
    transform var(--ui-dur-slow) var(--ui-ease-out),
    opacity var(--ui-dur) var(--ui-ease-out);
}

.ui-modal-enter-from,
.ui-modal-leave-to {
  opacity: 0;
}

.ui-modal-enter-from .ui-modal,
.ui-modal-leave-to .ui-modal {
  transform: translateY(14px);
  opacity: 0;
}

/* sheet variant: slides down from the top edge (macOS sheet style) */
.ui-modal-layer:has(.ui-modal--sheet) {
  align-items: flex-start;
  justify-content: center;
  padding-top: 8vh;
}

.ui-modal--sheet {
  max-width: 480px;
  box-shadow: var(--ui-shadow-menu);
}

.ui-modal-enter-from .ui-modal--sheet,
.ui-modal-leave-to .ui-modal--sheet {
  transform: translateY(-16px);
  opacity: 0;
}

/* phones: bottom sheet instead of a floating dialog */
@media (max-width: 560px) {
  .ui-modal-layer {
    padding: 0;
    align-items: flex-end;
  }
  .ui-modal {
    max-width: 100%;
    max-height: calc(100dvh - 40px);
    border-radius: var(--ui-radius-lg) var(--ui-radius-lg) 0 0;
    border-left: none;
    border-right: none;
    border-bottom: none;
  }
  .ui-modal__footer {
    flex-wrap: wrap;
    padding-bottom: calc(12px + env(safe-area-inset-bottom, 0px));
  }
  /* Footer-less sheets sit flush to the bottom edge — keep their last row
     above the home indicator (footers already carry their own inset). */
  .ui-modal:not(:has(.ui-modal__footer)) .ui-modal__body {
    padding-bottom: calc(16px + env(safe-area-inset-bottom, 0px));
  }
}
</style>
