<!-- CyberManju OS — UiToolbar (src/components/ui/UiToolbar.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div
    class="ui-toolbar"
    :class="[
      `ui-toolbar--${align}`,
      { 'ui-toolbar--glass': glass, 'ui-toolbar--divided': divided, 'ui-toolbar--narrow': narrow },
    ]"
  >
    <div class="ui-toolbar__lead">
      <slot name="lead" />
    </div>
    <div class="ui-toolbar__main">
      <slot />
    </div>
    <div class="ui-toolbar__trail">
      <slot name="trail" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useWindowUi } from '@/composables/useWindowUi'

withDefaults(
  defineProps<{
    align?: 'start' | 'center' | 'between'
    glass?: boolean
    divided?: boolean
  }>(),
  { align: 'between', glass: false, divided: false }
)

/** Responds to the width of the hosting OS window, not the viewport. */
const win = useWindowUi()
const narrow = computed(() => win.isNarrow.value)
</script>

<style scoped>
.ui-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: calc(6px * var(--ui-unit, 1)) calc(8px * var(--ui-unit, 1));
  border-radius: var(--ui-radius-md);
  flex-wrap: wrap;
}

.ui-toolbar--glass {
  background: var(--ui-glass);
  border: 1px solid var(--ui-border);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  box-shadow: var(--ui-shadow-1);
}

.ui-toolbar--divided {
  border-bottom: 1px solid var(--ui-hairline);
  border-radius: var(--ui-radius-md) var(--ui-radius-md) 0 0;
}

.ui-toolbar__lead,
.ui-toolbar__trail {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.ui-toolbar__main {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1;
  flex-wrap: wrap;
}

.ui-toolbar--start .ui-toolbar__main { justify-content: flex-start; }
.ui-toolbar--center .ui-toolbar__main { justify-content: center; }
.ui-toolbar--between .ui-toolbar__main { justify-content: center; }

.ui-toolbar--between .ui-toolbar__trail {
  margin-left: auto;
}

/* Windows squeeze: compress and let the trailing slot wrap below. */
.ui-toolbar--narrow {
  gap: 5px;
  padding: calc(4px * var(--ui-unit, 1)) calc(6px * var(--ui-unit, 1));
}

.ui-toolbar--narrow .ui-toolbar__trail {
  margin-left: auto;
}
</style>
