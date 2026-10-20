<!-- CyberManju OS — UiStack (src/components/ui/UiStack.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <component
    :is="as"
    class="ui-stack"
    :class="[`ui-stack--${direction}`, `ui-stack--${align}`, `ui-stack--${justify}`]"
    :style="style"
  >
    <slot />
  </component>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    as?: string
    direction?: 'row' | 'column'
    align?: 'start' | 'center' | 'end' | 'stretch' | 'baseline'
    justify?: 'start' | 'center' | 'end' | 'between' | 'around'
    gap?: number | string
    wrap?: boolean
  }>(),
  {
    as: 'div',
    direction: 'column',
    align: 'stretch',
    justify: 'start',
    gap: undefined,
    wrap: false,
  }
)

const style = computed(() => ({
  gap: props.gap !== undefined ? (typeof props.gap === 'number' ? `${props.gap}px` : props.gap) : undefined,
  flexWrap: props.wrap ? ('wrap' as const) : undefined,
}))
</script>

<style scoped>
.ui-stack {
  display: flex;
  min-width: 0;
}

.ui-stack--row { flex-direction: row; }
.ui-stack--column { flex-direction: column; }

.ui-stack--start { align-items: flex-start; }
.ui-stack--center { align-items: center; }
.ui-stack--end { align-items: flex-end; }
.ui-stack--stretch { align-items: stretch; }
.ui-stack--baseline { align-items: baseline; }

.ui-stack.ui-stack--row.ui-stack--start { justify-content: flex-start; }
.ui-stack.ui-stack--row.ui-stack--center { justify-content: center; }
.ui-stack.ui-stack--row.ui-stack--end { justify-content: flex-end; }
.ui-stack.ui-stack--row.ui-stack--between { justify-content: space-between; }
.ui-stack.ui-stack--row.ui-stack--around { justify-content: space-around; }

.ui-stack.ui-stack--column.ui-stack--start { justify-content: flex-start; }
.ui-stack.ui-stack--column.ui-stack--center { justify-content: center; }
.ui-stack.ui-stack--column.ui-stack--end { justify-content: flex-end; }
.ui-stack.ui-stack--column.ui-stack--between { justify-content: space-between; }
.ui-stack.ui-stack--column.ui-stack--around { justify-content: space-around; }
</style>
