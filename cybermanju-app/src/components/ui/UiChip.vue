<!-- CyberManju OS — UiChip (src/components/ui/UiChip.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <span
    class="ui-chip"
    :class="{ 'ui-chip--active': active, 'ui-chip--pill': pill }"
    role="button"
    tabindex="0"
    :aria-pressed="active"
    @click="emit('click', $event)"
    @keydown.enter.prevent="emit('click', $event as unknown as MouseEvent)"
    @keydown.space.prevent="emit('click', $event as unknown as MouseEvent)"
  >
    <AppIcon v-if="icon" :name="icon" :size="13" class="ui-chip__icon" />
    <span class="ui-chip__label"><slot>{{ label }}</slot></span>
    <span v-if="count !== undefined" class="ui-chip__count">{{ count }}</span>
    <button
      v-if="removable"
      class="ui-chip__remove"
      type="button"
      :aria-label="`Remove ${label}`"
      @click.stop="emit('remove', $event)"
    >
      <AppIcon name="solar:close-bold" :size="11" />
    </button>
  </span>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

withDefaults(
  defineProps<{
    label?: string
    icon?: string
    count?: number
    active?: boolean
    removable?: boolean
    /** @deprecated Chips are flat 4px by default now; kept for compat. */
    square?: boolean
    pill?: boolean
  }>(),
  { label: '', icon: '', active: false, removable: false, square: false, pill: false }
)

const emit = defineEmits<{ click: [event: MouseEvent]; remove: [event: MouseEvent] }>()
</script>

<style scoped>
.ui-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: calc(var(--ui-control-h) * 0.78);
  padding: 0 11px;
  background: color-mix(in srgb, var(--ui-text) 6%, var(--ui-surface-2));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-xs);
  color: var(--ui-text-2);
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 500;
  cursor: pointer;
  user-select: none;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out);
}

.ui-chip:hover {
  border-color: var(--ui-border-hover);
  color: var(--ui-text);
}

.ui-chip:active {
  transform: scale(0.98);
}

.ui-chip:focus-visible {
  outline: none;
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}

.ui-chip--pill {
  border-radius: var(--ui-radius-full);
}

.ui-chip--active {
  background: var(--ui-accent);
  border-color: transparent;
  color: var(--ui-on-accent);
}

.ui-chip__count {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 600;
  padding: 1px 5px;
  border-radius: var(--ui-radius-full);
  background: color-mix(in srgb, var(--ui-text) 12%, transparent);
  color: inherit;
}

.ui-chip--active .ui-chip__count {
  background: rgba(255, 255, 255, 0.25);
  color: inherit;
}

.ui-chip__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 15px;
  height: 15px;
  margin-right: -4px;
  border-radius: 50%;
  color: var(--ui-text-3);
  background: transparent;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out);
}

.ui-chip__remove:hover {
  background: color-mix(in srgb, var(--ui-danger) 20%, transparent);
  color: var(--ui-danger);
}
</style>
