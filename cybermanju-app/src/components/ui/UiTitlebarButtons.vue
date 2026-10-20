<!-- CyberManju OS — UiTitlebarButtons (src/components/ui/UiTitlebarButtons.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-wbtns" :class="{ 'is-focused': focused }" @mousedown.stop>
    <button
      class="ui-wbtn"
      type="button"
      aria-label="Minimize window"
      title="Minimize"
      @click.stop="emit('minimize')"
    >
      <AppIcon name="solar:minus-bold" :size="11" />
    </button>
    <button
      class="ui-wbtn"
      type="button"
      :aria-label="maximized ? 'Restore window' : 'Maximize window'"
      :title="maximized ? 'Restore' : 'Maximize'"
      @click.stop="emit('zoom')"
    >
      <AppIcon :name="maximized ? 'solar:minimize-bold' : 'solar:maximize-bold'" :size="11" />
    </button>
    <button
      class="ui-wbtn ui-wbtn--close"
      type="button"
      aria-label="Close window"
      title="Close"
      @click.stop="emit('close')"
    >
      <AppIcon name="solar:close-bold" :size="11" />
    </button>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

withDefaults(
  defineProps<{ focused?: boolean; maximized?: boolean }>(),
  { focused: true, maximized: false },
)

const emit = defineEmits<{ close: []; minimize: []; zoom: [] }>()
</script>

<style scoped>
/* Breeze-style flat glyph buttons: monochrome, close hovers danger fill. */
.ui-wbtns {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.ui-wbtn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 24px;
  padding: 0;
  border-radius: var(--ui-radius-sm);
  border: none;
  background: transparent;
  color: var(--ui-text-2);
  cursor: pointer;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.ui-wbtn:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.ui-wbtn--close:hover {
  background: var(--ui-danger);
  /* Fixed white: clears 3:1 on every theme's danger fill. */
  color: #ffffff;
}

.ui-wbtn:active {
  background: color-mix(in srgb, var(--ui-text) 14%, transparent);
}

.ui-wbtn--close:active {
  background: color-mix(in srgb, var(--ui-danger) 80%, black);
  /* Fixed white: clears 3:1 on every theme's danger fill. */
  color: #ffffff;
}

.ui-wbtn:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}
</style>
