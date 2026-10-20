<!-- CyberManju OS — UiSegmented (src/components/ui/UiSegmented.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: SegmentOption. -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-segmented" role="group" :aria-label="ariaLabel">
    <button
      v-for="opt in options"
      :key="String(opt.value)"
      type="button"
      class="ui-segmented__item"
      :class="{ 'is-active': modelValue === opt.value }"
      :aria-pressed="modelValue === opt.value"
      :disabled="disabled"
      @click="emit('update:modelValue', opt.value)"
    >
      <AppIcon v-if="opt.icon" :name="opt.icon" :size="13" />
      <span>{{ opt.label }}</span>
    </button>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

export interface SegmentOption {
  label: string
  value: string | number
  icon?: string
}

withDefaults(
  defineProps<{
    modelValue?: string | number
    options?: SegmentOption[]
    ariaLabel?: string
    disabled?: boolean
  }>(),
  { modelValue: '', options: () => [], ariaLabel: 'Options', disabled: false },
)

const emit = defineEmits<{ 'update:modelValue': [value: string | number] }>()
</script>

<style scoped>
.ui-segmented {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
}

.ui-segmented__item {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-height: 22px;
  padding: 0 10px;
  border-radius: 4px;
  color: var(--ui-text-2);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 500;
  white-space: nowrap;
  transition:
    background-color var(--ui-dur-fast) ease-out,
    color var(--ui-dur-fast) ease-out;
}

.ui-segmented__item:hover:not(:disabled) {
  color: var(--ui-text);
}

.ui-segmented__item.is-active {
  background: color-mix(in srgb, var(--ui-text) 10%, var(--ui-surface-3));
  color: var(--ui-text);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
}

.ui-segmented__item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ui-segmented__item:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}
</style>
