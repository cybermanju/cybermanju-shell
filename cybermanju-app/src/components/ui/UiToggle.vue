<!-- CyberManju OS — UiToggle (src/components/ui/UiToggle.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: standalone (no imports). -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <button
    class="ui-toggle"
    :class="{ 'ui-toggle--on': modelValue, 'ui-toggle--sm': size === 'sm' }"
    type="button"
    role="switch"
    :aria-checked="modelValue"
    :disabled="disabled"
    :aria-label="label || undefined"
    @click="toggle"
  >
    <span class="ui-toggle__track">
      <span class="ui-toggle__thumb" />
    </span>
    <span v-if="label || $slots.default" class="ui-toggle__label">
      <slot>{{ label }}</slot>
    </span>
  </button>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    modelValue?: boolean
    label?: string
    disabled?: boolean
    size?: 'sm' | 'md'
  }>(),
  { modelValue: false, label: '', disabled: false, size: 'md' }
)

const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

function toggle() {
  emit('update:modelValue', !props.modelValue)
}
</script>

<style scoped>
.ui-toggle {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  user-select: none;
  color: var(--ui-text-2);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
}

.ui-toggle:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ui-toggle:focus-visible .ui-toggle__track {
  box-shadow: var(--ui-focus-ring);
  border-color: var(--ui-accent);
}

.ui-toggle__track {
  position: relative;
  display: flex;
  align-items: center;
  width: 40px;
  height: 22px;
  padding: 2px;
  border-radius: var(--ui-radius-full);
  background: var(--ui-surface-3);
  border: 1px solid var(--ui-border-strong);
  transition:
    background-color var(--ui-dur) var(--ui-ease-out),
    border-color var(--ui-dur) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out);
}

.ui-toggle--sm .ui-toggle__track {
  width: 32px;
  height: 18px;
}

.ui-toggle__thumb {
  display: block;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  /* Fixed white knob: correct on every theme's accent track. */
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.35);
  transform: translateX(0);
  transition: transform var(--ui-dur) var(--ui-ease-out);
}

.ui-toggle--sm .ui-toggle__thumb {
  width: 13px;
  height: 13px;
}

.ui-toggle--on .ui-toggle__track {
  background: var(--ui-accent);
  border-color: transparent;
  box-shadow: none;
}

.ui-toggle--on .ui-toggle__thumb {
  transform: translateX(18px);
}

.ui-toggle--sm.ui-toggle--on .ui-toggle__thumb {
  transform: translateX(14px);
}

.ui-toggle--on .ui-toggle__label {
  color: var(--ui-text);
}

.ui-toggle__label {
  transition: color var(--ui-dur) var(--ui-ease-out);
}
</style>
