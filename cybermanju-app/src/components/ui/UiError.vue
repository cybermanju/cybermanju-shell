<!-- CyberManju OS — UiError (src/components/ui/UiError.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div
    class="ui-error"
    :class="[`ui-error--${size}`]"
    role="alert"
  >
    <span class="ui-error__icon" aria-hidden="true">
      <AppIcon :name="icon" :size="iconSize" />
    </span>
    <div class="ui-error__body">
      <div class="ui-error__title">{{ title }}</div>
      <p v-if="message" class="ui-error__message">{{ message }}</p>
      <div v-if="$slots.actions" class="ui-error__actions">
        <slot name="actions" />
      </div>
    </div>
    <UiButton
      v-if="retryable"
      size="sm"
      variant="outline"
      icon="solar:refresh-bold"
      @click="$emit('retry')"
    >
      RETRY
    </UiButton>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from '@/components/AppIcon.vue'
import UiButton from '@/components/ui/UiButton.vue'

withDefaults(
  defineProps<{
    icon?: string
    title?: string
    message?: string
    size?: 'sm' | 'md'
    retryable?: boolean
  }>(),
  {
    icon: 'solar:danger-triangle-bold',
    title: 'Something went wrong',
    message: '',
    size: 'md',
    retryable: false,
  }
)

defineEmits<{ retry: [] }>()

const iconSize = computed(() => 16)
</script>

<style scoped>
.ui-error {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  border: 1px solid color-mix(in srgb, var(--ui-danger) 40%, transparent);
  border-radius: var(--ui-radius-md);
  background: color-mix(in srgb, var(--ui-danger) 8%, transparent);
  animation: ui-rise var(--ui-dur-slow) var(--ui-ease-out) both;
}

.ui-error--sm {
  padding: 9px 11px;
  gap: 8px;
}

.ui-error__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-danger);
  background: color-mix(in srgb, var(--ui-danger) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--ui-danger) 30%, transparent);
}

.ui-error__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.ui-error__title {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  font-weight: 600;
  color: var(--ui-text);
}

.ui-error__message {
  margin: 0;
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  color: var(--ui-text-2);
  word-break: break-word;
}

.ui-error__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 4px;
}

@media (prefers-reduced-motion: reduce) {
  .ui-error {
    animation: none;
  }
}
</style>
