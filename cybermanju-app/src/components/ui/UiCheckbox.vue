<!-- CyberManju OS — UiCheckbox (src/components/ui/UiCheckbox.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <button
    class="ui-checkbox"
    :class="{ 'ui-checkbox--on': modelValue, 'ui-checkbox--mixed': indeterminate }"
    type="button"
    role="checkbox"
    :aria-checked="indeterminate ? 'mixed' : modelValue"
    :disabled="disabled"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="ui-checkbox__box">
      <AppIcon :name="indeterminate ? 'solar:minus-bold' : 'solar:check-bold'" :size="11" />
    </span>
    <span v-if="label || $slots.default" class="ui-checkbox__label">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

withDefaults(
  defineProps<{
    modelValue?: boolean
    label?: string
    disabled?: boolean
    indeterminate?: boolean
  }>(),
  { modelValue: false, label: '', disabled: false, indeterminate: false }
)

const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()
</script>

<style scoped>
.ui-checkbox {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  user-select: none;
  color: var(--ui-text-2);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  transition: color var(--ui-dur-fast) var(--ui-ease-out);
}

.ui-checkbox:hover {
  color: var(--ui-text);
}

.ui-checkbox:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ui-checkbox:focus-visible .ui-checkbox__box {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}

.ui-checkbox__box {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  border-radius: var(--ui-radius-xs);
  border: 1px solid var(--ui-border-strong);
  background: color-mix(in srgb, var(--ui-surface) 75%, transparent);
  color: transparent;
  transition:
    background-color var(--ui-dur) var(--ui-ease-out),
    border-color var(--ui-dur) var(--ui-ease-out),
    color var(--ui-dur) var(--ui-ease-out),
    transform var(--ui-dur-fast) var(--ui-ease-spring),
    box-shadow var(--ui-dur) var(--ui-ease-out);
}

.ui-checkbox:hover .ui-checkbox__box {
  border-color: var(--ui-border-hover);
}

.ui-checkbox:active .ui-checkbox__box {
  transform: scale(0.9);
}

.ui-checkbox--on .ui-checkbox__box,
.ui-checkbox--mixed .ui-checkbox__box {
  background: var(--ui-accent);
  border-color: transparent;
  color: var(--ui-on-accent);
  box-shadow: none;
}

.ui-checkbox--on .ui-checkbox__label {
  color: var(--ui-text);
}
</style>
