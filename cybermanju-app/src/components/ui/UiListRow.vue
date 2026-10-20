<!-- CyberManju OS — UiListRow (src/components/ui/UiListRow.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <component
    :is="clickable ? 'button' : 'div'"
    class="ui-row"
    :class="{ 'ui-row--clickable': clickable, 'ui-row--selected': selected, 'ui-row--danger': danger }"
    :type="clickable ? 'button' : undefined"
    @click="clickable && emit('click', $event)"
  >
    <span v-if="icon || $slots.icon" class="ui-row__icon" :class="{ 'ui-row__icon--square': squareIcon }">
      <slot name="icon">
        <AppIcon :name="icon" :size="15" />
      </slot>
    </span>

    <span class="ui-row__body">
      <span class="ui-row__title">
        <slot name="title">{{ title }}</slot>
      </span>
      <span v-if="subtitle || $slots.subtitle" class="ui-row__subtitle">
        <slot name="subtitle">{{ subtitle }}</slot>
      </span>
    </span>

    <span v-if="badge || $slots.badge" class="ui-row__badge">
      <slot name="badge">
        <UiBadge :label="badge" :tone="badgeTone" />
      </slot>
    </span>

    <span v-if="meta || $slots.meta" class="ui-row__meta">
      <slot name="meta">{{ meta }}</slot>
    </span>

    <span v-if="$slots.actions" class="ui-row__actions" @click.stop>
      <slot name="actions" />
    </span>

    <AppIcon v-if="chevron" name="solar:alt-arrow-right-bold" :size="14" class="ui-row__chevron" />
  </component>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import UiBadge from '@/components/ui/UiBadge.vue'

withDefaults(
  defineProps<{
    title?: string
    subtitle?: string
    icon?: string
    squareIcon?: boolean
    meta?: string
    badge?: string
    badgeTone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'
    clickable?: boolean
    selected?: boolean
    danger?: boolean
    chevron?: boolean
  }>(),
  {
    title: '',
    subtitle: '',
    icon: '',
    squareIcon: false,
    meta: '',
    badge: '',
    badgeTone: 'neutral',
    clickable: true,
    selected: false,
    danger: false,
    chevron: false,
  }
)

const emit = defineEmits<{ click: [event: MouseEvent] }>()
</script>

<style scoped>
.ui-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  min-width: 0;
  padding: calc(8px * var(--ui-unit, 1)) calc(10px * var(--ui-unit, 1));
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text);
  font-family: var(--ui-font);
  text-align: left;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    transform var(--ui-dur-fast) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out);
}

.ui-row--clickable {
  cursor: pointer;
}

.ui-row--clickable:hover {
  background: var(--ui-accent-softer);
  border-color: color-mix(in srgb, var(--ui-accent) 22%, transparent);
  transform: translateX(2px);
}

.ui-row--clickable:active {
  transform: translateX(2px) scale(0.995);
}

.ui-row--clickable:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: -2px;
}

.ui-row--selected {
  background: var(--ui-accent-softer);
  border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent);
}

.ui-row--danger:hover {
  background: color-mix(in srgb, var(--ui-danger) 12%, transparent);
  border-color: color-mix(in srgb, var(--ui-danger) 32%, transparent);
}

.ui-row__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  flex-shrink: 0;
  border-radius: var(--ui-radius-full);
  color: var(--ui-accent);
  background: var(--ui-accent-softer);
}

.ui-row__icon--square {
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border);
  color: var(--ui-text-2);
}

.ui-row--danger .ui-row__icon {
  color: var(--ui-danger);
  background: color-mix(in srgb, var(--ui-danger) 12%, transparent);
}

.ui-row__body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.ui-row__title {
  font-size: var(--ui-fs-sm);
  font-weight: 600;
  color: var(--ui-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-row__subtitle {
  font-family: var(--ui-font-mono);
  font-size: var(--ui-fs-xs);
  color: var(--ui-text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-row__badge,
.ui-row__meta {
  flex-shrink: 0;
}

.ui-row__meta {
  font-family: var(--ui-font-mono);
  font-size: var(--ui-fs-xs);
  color: var(--ui-text-3);
  white-space: nowrap;
}

.ui-row__actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  opacity: 0.65;
  transition: opacity var(--ui-dur) var(--ui-ease-out);
}

.ui-row:hover .ui-row__actions {
  opacity: 1;
}

.ui-row__chevron {
  color: var(--ui-text-faint);
  flex-shrink: 0;
  transition: transform var(--ui-dur) var(--ui-ease-out);
}

.ui-row:hover .ui-row__chevron {
  transform: translateX(3px);
  color: var(--ui-accent);
}
</style>
