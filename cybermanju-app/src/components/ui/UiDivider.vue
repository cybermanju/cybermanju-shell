<!-- CyberManju OS — UiDivider (src/components/ui/UiDivider.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: standalone (no imports). -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-divider" :class="[`ui-divider--${align}`, { 'ui-divider--spaced': spaced }]">
    <span class="ui-divider__line" />
    <span v-if="label || $slots.label" class="ui-divider__label">
      <slot name="label">{{ label }}</slot>
    </span>
    <span class="ui-divider__line" />
  </div>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    label?: string
    align?: 'start' | 'center' | 'end'
    spaced?: boolean
  }>(),
  { label: '', align: 'center', spaced: false }
)
</script>

<style scoped>
.ui-divider {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-width: 0;
}

.ui-divider--spaced {
  margin: calc(12px * var(--ui-unit, 1)) 0;
}

.ui-divider__line {
  flex: 1;
  height: 1px;
  min-width: 12px;
  background: linear-gradient(
    90deg,
    transparent,
    var(--ui-border-strong) 20%,
    var(--ui-border-strong) 80%,
    transparent
  );
  transform-origin: center;
  animation: ui-sweep-in var(--ui-dur-slow) var(--ui-ease-out) both;
}

.ui-divider__label {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 600;
  color: var(--ui-text-3);
  white-space: nowrap;
  flex-shrink: 0;
}

.ui-divider--start { justify-content: flex-start; }
.ui-divider--start .ui-divider__line:last-child { flex: 0 1 auto; min-width: 12px; }
.ui-divider--end { justify-content: flex-end; }
.ui-divider--end .ui-divider__line:first-child { flex: 0 1 auto; min-width: 12px; }
</style>
