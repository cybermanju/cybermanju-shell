<!-- CyberManju OS — UiProgress (src/components/ui/UiProgress.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-progress" :class="[`ui-progress--${tone}`, { 'ui-progress--indeterminate': indeterminate }]">
    <div v-if="label || $slots.label || showValue" class="ui-progress__meta">
      <span class="ui-progress__label"><slot name="label">{{ label }}</slot></span>
      <span v-if="showValue && !indeterminate" class="ui-progress__value">{{ clamped }}%</span>
    </div>
    <div class="ui-progress__track">
      <div
        class="ui-progress__fill"
        :style="indeterminate ? undefined : { width: `${clamped}%` }"
      />
      <div v-if="indeterminate" class="ui-progress__shine" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 0 – 100 */
    value?: number
    label?: string
    tone?: 'accent' | 'success' | 'warning' | 'danger' | 'info'
    indeterminate?: boolean
    showValue?: boolean
  }>(),
  { value: 0, label: '', tone: 'accent', indeterminate: false, showValue: false }
)

const clamped = computed(() => Math.max(0, Math.min(100, Math.round(props.value))))
</script>

<style scoped>
.ui-progress {
  display: flex;
  flex-direction: column;
  gap: 5px;
  width: 100%;
  min-width: 0;
}

.ui-progress__meta {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}

.ui-progress__label {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 600;
  color: var(--ui-text-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-progress__value {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 600;
  color: var(--ui-text-2);
  font-variant-numeric: tabular-nums;
}

.ui-progress__track {
  position: relative;
  height: 6px;
  border-radius: var(--ui-radius-full);
  background: color-mix(in srgb, var(--ui-text) 10%, transparent);
  border: 1px solid var(--ui-hairline);
  overflow: hidden;
}

.ui-progress__fill {
  height: 100%;
  border-radius: inherit;
  background: var(--ui-accent);
  transition: width var(--ui-dur-slow) ease-out;
}

.ui-progress--success .ui-progress__fill {
  background: var(--ui-success);
}

.ui-progress--warning .ui-progress__fill {
  background: var(--ui-warning);
}

.ui-progress--danger .ui-progress__fill {
  background: var(--ui-danger);
}

.ui-progress--info .ui-progress__fill {
  background: var(--ui-info);
}

.ui-progress--indeterminate .ui-progress__fill {
  width: 35%;
  animation: ui-progress-slide 1.4s var(--ui-ease) infinite;
}

@keyframes ui-progress-slide {
  0% { transform: translateX(-110%); }
  100% { transform: translateX(330%); }
}

.ui-progress__shine {
  display: none;
}
</style>
