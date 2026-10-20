<!-- CyberManju OS — UiTrafficLights (src/components/ui/UiTrafficLights.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-lights" :class="{ 'is-focused': focused }" @mousedown.stop>
    <button
      class="ui-light ui-light--close"
      type="button"
      aria-label="Close window"
      title="Close"
      @click.stop="emit('close')"
    >
      <AppIcon name="solar:close-bold" :size="7" class="ui-light__glyph" />
    </button>
    <button
      class="ui-light ui-light--min"
      type="button"
      aria-label="Minimize window"
      title="Minimize"
      @click.stop="emit('minimize')"
    >
      <AppIcon name="solar:minus-bold" :size="7" class="ui-light__glyph" />
    </button>
    <button
      class="ui-light ui-light--zoom"
      type="button"
      aria-label="Zoom window"
      title="Zoom"
      @click.stop="emit('zoom')"
    >
      <AppIcon name="solar:maximize-bold" :size="7" class="ui-light__glyph" />
    </button>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

withDefaults(defineProps<{ focused?: boolean }>(), { focused: true })

const emit = defineEmits<{ close: []; minimize: []; zoom: [] }>()
</script>

<style scoped>
.ui-lights {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.ui-light {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  height: 12px;
  padding: 0;
  border-radius: 50%;
  border: 1px solid rgba(0, 0, 0, 0.12);
  color: transparent;
  cursor: pointer;
  transition:
    background-color var(--ui-dur-fast) ease-out,
    color var(--ui-dur-fast) ease-out;
}

/* Inactive window: all grey. */
.ui-lights:not(.is-focused) .ui-light {
  background: color-mix(in srgb, var(--ui-text) 18%, transparent);
  border-color: transparent;
}

/* Focused window: macOS traffic-light colours. */
.ui-lights.is-focused .ui-light--close { background: #FF5F57; }
.ui-lights.is-focused .ui-light--min { background: #FEBC2E; }
.ui-lights.is-focused .ui-light--zoom { background: #28C840; }

.ui-light__glyph {
  opacity: 0;
  transform: scale(0.6);
  color: rgba(0, 0, 0, 0.6);
  transition:
    opacity var(--ui-dur-fast) ease-out,
    transform var(--ui-dur-fast) ease-out;
}

.ui-lights:hover .ui-light__glyph,
.ui-light:focus-visible .ui-light__glyph {
  opacity: 1;
  transform: scale(1);
}

.ui-light:active {
  filter: brightness(0.85);
}

.ui-light:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}
</style>
