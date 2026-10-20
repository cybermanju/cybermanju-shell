<!-- CyberManju OS — UiInput (src/components/ui/UiInput.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: native attrs/listeners target the input; caller class/style stay on the label. -->
<!-- Visible help/error text supplies default ARIA description/validity; explicit caller attrs win. -->
<template>
  <label
    class="ui-field"
    :class="[$attrs.class, { 'ui-field--inline': inline, 'ui-field--error': Boolean(error) }]"
    :style="$attrs.style"
  >
    <span v-if="label || $slots.label" class="ui-field__label">
      <slot name="label">{{ label }}</slot>
      <span v-if="required" class="ui-field__required">*</span>
    </span>

    <span class="ui-field__control" :class="{ 'ui-field__control--focused': focused }">
      <AppIcon v-if="prefixIcon" :name="prefixIcon" :size="14" class="ui-field__prefix" />
      <input
        :aria-describedby="error || hint ? hintId : undefined"
        :aria-invalid="error ? true : undefined"
        v-bind="inputAttrs()"
        ref="inputRef"
        class="ui-field__input"
        :type="type"
        :value="modelValue"
        :placeholder="placeholder"
        :disabled="disabled"
        :readonly="readonly"
        :min="min"
        :max="max"
        :step="step"
        :autocomplete="autocomplete"
        :list="list || undefined"
        :spellcheck="false"
        @input="onInput"
        @focus="focused = true"
        @blur="focused = false"
        @keydown.enter="$emit('enter', $event)"
      />
      <span v-if="$slots.suffix" class="ui-field__suffix"><slot name="suffix" /></span>
      <button
        v-else-if="clearable && modelValue"
        class="ui-field__clear"
        type="button"
        aria-label="Clear"
        @click.prevent="clear"
      >
        <AppIcon name="solar:close-bold" :size="12" />
      </button>
    </span>

    <span v-if="error || hint" :id="hintId" class="ui-field__hint" :class="{ 'ui-field__hint--error': Boolean(error) }">
      {{ error || hint }}
    </span>
  </label>
</template>

<script setup lang="ts">
import { ref, useAttrs, useId } from 'vue'
import AppIcon from '@/components/AppIcon.vue'

defineOptions({ inheritAttrs: false })

const hintId = useId()
const attrs = useAttrs()
// Layout belongs to the wrapper; native attributes/listeners belong to the input.
function inputAttrs() {
  const forwarded = { ...attrs }
  delete forwarded.class
  delete forwarded.style
  return forwarded
}

withDefaults(
  defineProps<{
    modelValue?: string | number
    label?: string
    type?: string
    placeholder?: string
    hint?: string
    error?: string
    disabled?: boolean
    readonly?: boolean
    required?: boolean
    inline?: boolean
    prefixIcon?: string
    clearable?: boolean
    min?: number
    max?: number
    step?: number
    autocomplete?: string
    list?: string
  }>(),
  {
    modelValue: '',
    label: '',
    type: 'text',
    placeholder: '',
    hint: '',
    error: '',
    disabled: false,
    readonly: false,
    required: false,
    inline: false,
    prefixIcon: '',
    clearable: false,
    min: undefined,
    max: undefined,
    step: undefined,
    autocomplete: 'off',
    list: '',
  }
)

const emit = defineEmits<{
  'update:modelValue': [value: string]
  enter: [event: KeyboardEvent]
}>()

const inputRef = ref<HTMLInputElement | null>(null)
const focused = ref(false)

function onInput(event: Event) {
  emit('update:modelValue', (event.target as HTMLInputElement).value)
}

function clear() {
  emit('update:modelValue', '')
  inputRef.value?.focus()
}

defineExpose({ focus: () => inputRef.value?.focus(), el: inputRef })
</script>

<style scoped>
.ui-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.ui-field--inline {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.ui-field__label {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 600;
  color: var(--ui-text-2);
}

.ui-field__required {
  color: var(--ui-danger);
  margin-left: 2px;
}

.ui-field__control {
  position: relative;
  display: flex;
  align-items: center;
  gap: 7px;
  flex: 1;
  min-width: 0;
  min-height: var(--ui-control-h);
  padding: 0 10px;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  transition:
    border-color var(--ui-dur) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out),
    background-color var(--ui-dur) var(--ui-ease-out);
}

.ui-field__control:hover {
  border-color: var(--ui-border-hover);
}

.ui-field__control--focused {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
  background: var(--ui-surface-2);
}

.ui-field--error .ui-field__control {
  border-color: color-mix(in srgb, var(--ui-danger) 65%, transparent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--ui-danger) 14%, transparent);
}

.ui-field__prefix {
  color: var(--ui-text-3);
  flex-shrink: 0;
  transition: color var(--ui-dur) var(--ui-ease-out);
}

.ui-field__control--focused .ui-field__prefix {
  color: var(--ui-accent);
}

.ui-field__input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  outline: none;
  color: var(--ui-text);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  padding: 7px 0;
}

.ui-field__input::placeholder {
  color: var(--ui-text-faint);
}

.ui-field__suffix {
  color: var(--ui-text-3);
  font-family: var(--ui-font-mono);
  font-size: var(--ui-fs-xs);
  flex-shrink: 0;
}

.ui-field__clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  color: var(--ui-text-3);
  background: color-mix(in srgb, var(--ui-text) 8%, transparent);
  flex-shrink: 0;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out);
}

.ui-field__clear:hover {
  background: color-mix(in srgb, var(--ui-danger) 22%, transparent);
  color: var(--ui-danger);
}

.ui-field__hint {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  color: var(--ui-text-3);
}

.ui-field__hint--error {
  color: var(--ui-danger);
}

.ui-field--inline .ui-field__label {
  flex-shrink: 0;
}
</style>
