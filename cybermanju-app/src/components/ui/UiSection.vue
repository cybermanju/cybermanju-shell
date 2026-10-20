<!-- CyberManju OS — UiSection (src/components/ui/UiSection.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <section class="ui-section" :class="[`ui-section--${size}`, { 'ui-section--card': card }]">
    <header v-if="title || $slots.header" class="ui-section__header">
      <slot name="header">
        <div class="ui-section__titles">
          <UiText variant="overline" tone="accent" v-if="eyebrow">{{ eyebrow }}</UiText>
          <h3 class="ui-section__title">{{ title }}</h3>
          <p v-if="description" class="ui-section__desc">{{ description }}</p>
        </div>
        <div v-if="$slots.actions" class="ui-section__actions">
          <slot name="actions" />
        </div>
      </slot>
    </header>
    <div class="ui-section__body">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
import UiText from '@/components/ui/UiText.vue'

withDefaults(
  defineProps<{
    title?: string
    eyebrow?: string
    description?: string
    size?: 'sm' | 'md' | 'lg'
    /** Wrap the section in a glass card. */
    card?: boolean
  }>(),
  { title: '', eyebrow: '', description: '', size: 'md', card: false }
)
</script>

<style scoped>
.ui-section {
  display: flex;
  flex-direction: column;
  gap: calc(10px * var(--ui-unit, 1));
  min-width: 0;
}

.ui-section--card {
  padding: calc(14px * var(--ui-unit, 1));
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
}

.ui-section--sm { gap: 6px; }
.ui-section--lg { gap: calc(16px * var(--ui-unit, 1)); }

.ui-section__header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}

.ui-section__titles {
  min-width: 0;
}

.ui-section__title {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-lg);
  font-weight: 700;
  letter-spacing: -0.01em;
  color: var(--ui-text);
  margin-top: 2px;
}

.ui-section--sm .ui-section__title {
  font-size: var(--ui-fs-md);
}

.ui-section__desc {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  color: var(--ui-text-3);
  margin-top: 2px;
}

.ui-section__actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.ui-section__body {
  min-width: 0;
}
</style>
