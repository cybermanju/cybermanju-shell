<!-- CyberManju OS — UiSpinner (src/components/ui/UiSpinner.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: standalone (no imports). -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <span
    class="ui-spinner"
    :class="[`ui-spinner--${variant}`, `ui-spinner--${size}`]"
    role="status"
    :aria-label="label"
  >
    <span class="ui-spinner__ring" />
    <span v-if="variant === 'orbit'" class="ui-spinner__ring ui-spinner__ring--inner" />
    <span v-if="label && showLabel" class="ui-spinner__label">{{ label }}</span>
  </span>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    size?: 'xs' | 'sm' | 'md' | 'lg'
    variant?: 'ring' | 'orbit'
    label?: string
    showLabel?: boolean
  }>(),
  { size: 'md', variant: 'ring', label: 'Loading', showLabel: false }
)
</script>

<style scoped>
.ui-spinner {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  color: var(--ui-accent);
}

.ui-spinner__ring {
  position: relative;
  display: block;
  width: var(--ui-spinner-size, 22px);
  height: var(--ui-spinner-size, 22px);
  border-radius: 50%;
  border: 2px solid color-mix(in srgb, var(--ui-accent) 22%, transparent);
  border-top-color: var(--ui-accent);
  animation: ui-spin 0.85s linear infinite;
  box-shadow: inset 0 0 8px color-mix(in srgb, var(--ui-accent) 22%, transparent);
}

.ui-spinner__ring--inner {
  position: absolute;
  top: 50%;
  left: 50%;
  width: calc(var(--ui-spinner-size, 22px) * 0.55);
  height: calc(var(--ui-spinner-size, 22px) * 0.55);
  margin: calc(var(--ui-spinner-size, 22px) * -0.275) 0 0 calc(var(--ui-spinner-size, 22px) * -0.275);
  border-width: 1.5px;
  border-top-color: transparent;
  border-bottom-color: var(--ui-accent);
  animation-duration: 1.3s;
  animation-direction: reverse;
}

.ui-spinner--ring { --ui-spinner-size: 22px; }
.ui-spinner--xs { --ui-spinner-size: 12px; }
.ui-spinner--sm { --ui-spinner-size: 16px; }
.ui-spinner--md { --ui-spinner-size: 22px; }
.ui-spinner--lg { --ui-spinner-size: 34px; }

.ui-spinner--lg .ui-spinner__ring {
  border-width: 3px;
}

.ui-spinner__label {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  color: var(--ui-text-3);
}
</style>
