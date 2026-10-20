<!-- CyberManju OS — UiCard (src/components/ui/UiCard.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <component
    :is="interactive ? 'button' : 'div'"
    class="ui-card"
    :class="[
      `ui-card--${pad}`,
      {
        'ui-card--interactive': interactive,
        'ui-card--flush': flush,
        'ui-card--accent': accent,
        'ui-card--danger': danger,
        'ui-card--selected': selected,
      },
    ]"
    :type="interactive ? 'button' : undefined"
    @click="onClick"
  >
    <span v-if="accent || danger" class="ui-card__rail" aria-hidden="true" />
    <header v-if="title || $slots.header" class="ui-card__header">
      <slot name="header">
        <div class="ui-card__heading">
          <AppIcon v-if="icon" :name="icon" :size="15" class="ui-card__icon" />
          <span class="ui-card__title">{{ title }}</span>
          <span v-if="meta" class="ui-card__meta">{{ meta }}</span>
        </div>
        <div v-if="$slots.actions" class="ui-card__actions" @click.stop>
          <slot name="actions" />
        </div>
      </slot>
    </header>
    <div class="ui-card__body" :class="{ 'ui-card__body--flush': !hasBodyPadding }">
      <slot />
    </div>
    <footer v-if="$slots.footer" class="ui-card__footer">
      <slot name="footer" />
    </footer>
  </component>
</template>

<script setup lang="ts">
import { computed, useSlots } from 'vue'
import AppIcon from '@/components/AppIcon.vue'

const props = withDefaults(
  defineProps<{
    title?: string
    icon?: string
    meta?: string
    pad?: 'none' | 'sm' | 'md' | 'lg'
    interactive?: boolean
    flush?: boolean
    accent?: boolean
    danger?: boolean
    selected?: boolean
  }>(),
  {
    title: '',
    icon: '',
    meta: '',
    pad: 'md',
    interactive: false,
    flush: false,
    accent: false,
    danger: false,
    selected: false,
  }
)

const emit = defineEmits<{ click: [event: MouseEvent] }>()
const slots = useSlots()

const hasBodyPadding = computed(() => props.pad !== 'none')
const onClick = (e: MouseEvent) => {
  if (props.interactive) emit('click', e)
}
</script>

<style scoped>
.ui-card {
  position: relative;
  display: flex;
  flex-direction: column;
  min-width: 0;
  width: 100%;
  text-align: left;
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  color: var(--ui-text);
  overflow: hidden;
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-1);
  transition:
    transform var(--ui-dur) var(--ui-ease-out),
    box-shadow var(--ui-dur) var(--ui-ease-out),
    border-color var(--ui-dur) var(--ui-ease-out),
    background-color var(--ui-dur) var(--ui-ease-out);
}

.ui-card::before {
  content: none;
}

.ui-card--interactive {
  cursor: pointer;
}

.ui-card--interactive:hover {
  border-color: var(--ui-border-hover);
  box-shadow: var(--ui-shadow-2);
}

.ui-card--interactive:active {
  transform: scale(0.995);
}

.ui-card--interactive:focus-visible {
  outline: none;
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}

.ui-card--selected {
  border-color: var(--ui-accent);
  background: color-mix(in srgb, var(--ui-accent) 7%, var(--ui-glass));
}

.ui-card--accent {
  border-color: var(--ui-border-strong);
}

.ui-card--danger {
  border-color: color-mix(in srgb, var(--ui-danger) 35%, var(--ui-border));
}

/* Quiet 2px selection edge — no glow. */
.ui-card__rail {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 0;
  width: 2px;
  background: var(--ui-accent);
}

.ui-card--danger .ui-card__rail {
  background: var(--ui-danger);
}

.ui-card__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: calc(10px * var(--ui-unit, 1)) calc(12px * var(--ui-unit, 1));
  border-bottom: 1px solid var(--ui-hairline);
  min-width: 0;
}

.ui-card__heading {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.ui-card__icon {
  color: var(--ui-accent);
  flex-shrink: 0;
}

.ui-card__title {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  font-weight: 600;
  color: var(--ui-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-card__meta {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  color: var(--ui-text-3);
  white-space: nowrap;
}

.ui-card__actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.ui-card__body {
  flex: 1;
  min-height: 0;
  padding: calc(12px * var(--ui-unit, 1));
}

.ui-card__body--flush {
  padding: 0;
}

.ui-card--pad-none .ui-card__body {
  padding: 0;
}

.ui-card--pad-sm .ui-card__body {
  padding: calc(8px * var(--ui-unit, 1));
}

.ui-card--pad-lg .ui-card__body {
  padding: calc(18px * var(--ui-unit, 1));
}

.ui-card__footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: calc(10px * var(--ui-unit, 1)) calc(12px * var(--ui-unit, 1));
  border-top: 1px solid var(--ui-hairline);
  background: color-mix(in srgb, var(--ui-surface) 55%, transparent);
}
</style>
