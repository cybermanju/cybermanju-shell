<!-- CyberManju OS — UiGrid (src/components/ui/UiGrid.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div
    class="ui-grid"
    :style="style"
    :class="{ 'ui-grid--auto': auto }"
  >
    <slot />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useWindowUi } from '@/composables/useWindowUi'

const props = withDefaults(
  defineProps<{
    /** CSS grid-template-columns; ignored when `min` is set. */
    columns?: string | number
    /** Auto-fit column width — collapses columns in narrow windows. */
    min?: string
    gap?: number | string
    dense?: boolean
    /** Let the hosting window decide how many columns fit. */
    auto?: boolean
  }>(),
  { columns: 'repeat(auto-fill, minmax(160px, 1fr))', min: '', gap: 10, dense: false, auto: false }
)

const win = useWindowUi()

const style = computed(() => {
  const gap = typeof props.gap === 'number' ? `${props.gap}px` : props.gap
  if (props.auto && props.min) {
    // Responsive inside the hosting window: fewer columns when it shrinks.
    const eff = win.isNarrow.value ? Math.max(80, parseInt(props.min, 10) - 40) : parseInt(props.min, 10)
    return {
      display: 'grid',
      gap,
      gridTemplateColumns: `repeat(auto-fill, minmax(${eff}px, 1fr))`,
      gridAutoFlow: props.dense ? ('dense' as const) : undefined,
    }
  }
  const cols = typeof props.columns === 'number' ? `repeat(${props.columns}, 1fr)` : props.columns
  return {
    display: 'grid',
    gap,
    gridTemplateColumns: props.min ? `repeat(auto-fill, minmax(${props.min}, 1fr))` : cols,
    gridAutoFlow: props.dense ? ('dense' as const) : undefined,
  }
})
</script>

<style scoped>
.ui-grid {
  min-width: 0;
}
</style>
