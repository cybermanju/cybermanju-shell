<!-- CyberManju OS — UiSelect (src/components/ui/UiSelect.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: fallthrough attributes such as aria-label/title are applied to the native select. -->
<template>
  <label class="ui-select" :class="{ 'ui-select--inline': inline }">
    <span v-if="label" class="ui-select__label">{{ label }}</span>
    <span class="ui-select__control">
      <select
        v-bind="$attrs"
        ref="selectRef"
        class="ui-select__native"
        :value="modelValue"
        :disabled="disabled"
        @change="onChange"
      >
        <option v-for="opt in normalizedOptions" :key="String(opt.value)" :value="opt.value">
          {{ opt.label }}
        </option>
      </select>
      <AppIcon name="solar:alt-arrow-down-bold" :size="13" class="ui-select__caret" />
    </span>
  </label>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import AppIcon from '@/components/AppIcon.vue'

defineOptions({ inheritAttrs: false })

type Option = string | { label: string; value: string | number }

const props = withDefaults(
  defineProps<{
    modelValue?: string | number
    label?: string
    options?: Option[]
    disabled?: boolean
    inline?: boolean
  }>(),
  { modelValue: '', label: '', options: () => [], disabled: false, inline: false }
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const selectRef = ref<HTMLSelectElement | null>(null)

const normalizedOptions = computed(() =>
  props.options.map((o) => (typeof o === 'string' ? { label: o, value: o } : o))
)

function onChange(event: Event) {
  emit('update:modelValue', (event.target as HTMLSelectElement).value)
}

defineExpose({ el: selectRef })
</script>

<style scoped>
.ui-select {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.ui-select--inline {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.ui-select__label {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 600;
  color: var(--ui-text-2);
}

.ui-select__control {
  position: relative;
  display: flex;
  align-items: center;
  min-width: 0;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  transition:
    border-color var(--ui-dur) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out);
}

.ui-select__control:hover {
  border-color: var(--ui-border-hover);
}

.ui-select__control:focus-within {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}

.ui-select__native {
  appearance: none;
  -webkit-appearance: none;
  flex: 1;
  min-width: 0;
  height: var(--ui-control-h);
  padding: 0 30px 0 10px;
  background: transparent;
  border: none;
  outline: none;
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  cursor: pointer;
}

.ui-select__native:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.ui-select__native option {
  background: var(--ui-surface-3);
  color: var(--ui-text);
}

.ui-select__caret {
  position: absolute;
  right: 9px;
  color: var(--ui-text-3);
  pointer-events: none;
  transition: transform var(--ui-dur) var(--ui-ease-out);
}

.ui-select__control:focus-within .ui-select__caret {
  transform: rotate(180deg);
  color: var(--ui-accent);
}
</style>
