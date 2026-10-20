<!-- CyberManju OS — UiEmpty (src/components/ui/UiEmpty.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-empty" :class="[`ui-empty--${size}`]">
    <div class="ui-empty__orb">
      <AppIcon :name="icon" :size="iconSize" />
      <span class="ui-empty__orb-ring" aria-hidden="true" />
      <span class="ui-empty__orb-ring ui-empty__orb-ring--delayed" aria-hidden="true" />
    </div>
    <div class="ui-empty__title">{{ title }}</div>
    <p v-if="description || $slots.description" class="ui-empty__desc">
      <slot name="description">{{ description }}</slot>
    </p>
    <div v-if="$slots.actions" class="ui-empty__actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from '@/components/AppIcon.vue'

const props = withDefaults(
  defineProps<{
    icon?: string
    title?: string
    description?: string
    size?: 'sm' | 'md' | 'lg'
  }>(),
  { icon: 'solar:inbox-bold', title: 'Nothing here yet', description: '', size: 'md' }
)

const iconSize = computed(() => ({ sm: 18, md: 24, lg: 30 }[props.size]))
</script>

<style scoped>
.ui-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: calc(28px * var(--ui-unit, 1)) 20px;
  text-align: center;
  animation: ui-rise var(--ui-dur-slow) var(--ui-ease-out) both;
}

.ui-empty--sm { padding: calc(16px * var(--ui-unit, 1)) 14px; gap: 7px; }
.ui-empty--lg { padding: calc(44px * var(--ui-unit, 1)) 24px; gap: 13px; }

.ui-empty__orb {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  border-radius: 50%;
  color: var(--ui-accent);
  background: radial-gradient(
    circle at 35% 30%,
    color-mix(in srgb, var(--ui-accent) 26%, transparent),
    var(--ui-accent-softer) 65%,
    transparent
  );
  border: 1px solid color-mix(in srgb, var(--ui-accent) 30%, transparent);
}

.ui-empty--sm .ui-empty__orb { width: 42px; height: 42px; }
.ui-empty--lg .ui-empty__orb { width: 72px; height: 72px; }

.ui-empty__orb-ring {
  position: absolute;
  inset: -6px;
  border-radius: 50%;
  border: 1px dashed color-mix(in srgb, var(--ui-accent) 35%, transparent);
  animation: ui-spin 14s linear infinite;
}

.ui-empty__orb-ring--delayed {
  inset: -13px;
  border-style: dotted;
  opacity: 0.55;
  animation-duration: 22s;
  animation-direction: reverse;
}

.ui-empty__title {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-md);
  font-weight: 700;
  color: var(--ui-text);
}

.ui-empty--sm .ui-empty__title { font-size: var(--ui-fs-sm); }

.ui-empty__desc {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-sm);
  color: var(--ui-text-3);
  max-width: 42ch;
}

.ui-empty__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: center;
  gap: 8px;
  margin-top: 4px;
}
</style>
