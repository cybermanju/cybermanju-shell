<!-- CyberManju OS — UiTitle (src/components/ui/UiTitle.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <header class="ui-title" :class="{ 'ui-title--compact': compact, 'ui-title--divided': divider }">
    <div class="ui-title__main">
      <div class="ui-title__heading">
        <span v-if="icon || $slots.icon" class="ui-title__icon">
          <slot name="icon"><AppIcon :name="icon" :size="16" /></slot>
        </span>
        <div class="ui-title__text">
          <div class="ui-title__label">
            <slot>{{ title }}</slot>
            <span v-if="badge" class="ui-title__badge">{{ badge }}</span>
          </div>
          <div v-if="subtitle && !compact" class="ui-title__subtitle">{{ subtitle }}</div>
        </div>
      </div>
      <div v-if="$slots.actions" class="ui-title__actions">
        <slot name="actions" />
      </div>
    </div>
    <span class="ui-title__rule" aria-hidden="true" />
  </header>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from '@/components/AppIcon.vue'
import { useWindowUi } from '@/composables/useWindowUi'

withDefaults(
  defineProps<{
    title?: string
    icon?: string
    subtitle?: string
    badge?: string
    /** Hairline rule underneath (panel headers use it, window toolbars don't). */
    divider?: boolean
  }>(),
  { title: '', icon: '', subtitle: '', badge: '', divider: true }
)

/**
 * Window-aware: inside a narrow OS window the subtitle and badge collapse and
 * the heading drops a size, so titles never wrap or steal content space.
 */
const win = useWindowUi()
const compact = computed(() => win.isNarrow.value || win.compact.value)
</script>

<style scoped>
.ui-title {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.ui-title__main {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.ui-title__heading {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex: 1;
}

.ui-title__icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  flex-shrink: 0;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-accent);
  background: var(--ui-accent-softer);
  border: 1px solid color-mix(in srgb, var(--ui-accent) 26%, transparent);
  box-shadow: inset 0 1px 0 color-mix(in srgb, var(--ui-accent) 22%, transparent);
  transition: transform var(--ui-dur) var(--ui-ease-spring);
}

.ui-title:hover .ui-title__icon {
  transform: translateY(-1px) rotate(-4deg);
}

.ui-title__text {
  min-width: 0;
}

.ui-title__label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: var(--ui-font);
  font-size: var(--ui-fs-lg);
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ui-text);
  line-height: 1.2;
}

.ui-title--compact .ui-title__label {
  font-size: var(--ui-fs-md);
}

.ui-title__badge {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 600;
  padding: 1px 8px;
  border-radius: var(--ui-radius-full);
  color: var(--ui-text-2);
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  border: 1px solid var(--ui-border);
}

.ui-title__subtitle {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  color: var(--ui-text-3);
  margin-top: 2px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-title__actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.ui-title__rule {
  display: block;
  height: 1px;
  width: 100%;
  transform-origin: left center;
  background: linear-gradient(
    90deg,
    color-mix(in srgb, var(--ui-accent) 45%, transparent),
    var(--ui-border) 30%,
    transparent
  );
  animation: ui-sweep-in var(--ui-dur-slow) var(--ui-ease-out) both;
}

.ui-title--compact {
  gap: 5px;
}
</style>
