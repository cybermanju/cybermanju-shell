<!-- CyberManju OS — UiText (src/components/ui/UiText.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <component :is="as" class="ui-text" :class="[`ui-text--${variant}`, `ui-text--${tone}`, classes]">
    <slot />
  </component>
</template>

<script setup lang="ts">
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** Element to render — semantic tags keep headings/landmarks intact. */
    as?: string
    variant?:
      | 'display'
      | 'h1'
      | 'h2'
      | 'h3'
      | 'h4'
      | 'body'
      | 'small'
      | 'caption'
      | 'overline'
      | 'label'
      | 'mono'
    tone?: 'default' | 'muted' | 'faint' | 'accent' | 'success' | 'warning' | 'danger' | 'info' | 'inverse'
    weight?: 400 | 500 | 600 | 700 | 800
    align?: 'left' | 'center' | 'right'
    truncate?: boolean
    gradient?: boolean
    block?: boolean
  }>(),
  {
    as: 'span',
    variant: 'body',
    tone: 'default',
    weight: undefined,
    align: undefined,
    truncate: false,
    gradient: false,
    block: false,
  }
)

const classes = computed(() => ({
  'ui-text--truncate': props.truncate,
  'ui-text--gradient': props.gradient,
  'ui-text--block': props.block,
  [`ui-text--w${props.weight}`]: props.weight !== undefined,
  [`ui-text--align-${props.align}`]: props.align !== undefined,
}))
</script>

<style scoped>
.ui-text {
  font-family: var(--ui-font);
  color: var(--ui-text);
  letter-spacing: var(--ui-tracking);
  transition: color var(--ui-dur) var(--ui-ease-out);
}

.ui-text--block {
  display: block;
}

.ui-text--truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-text--align-left { text-align: left; }
.ui-text--align-center { text-align: center; }
.ui-text--align-right { text-align: right; }

.ui-text--w400 { font-weight: 400; }
.ui-text--w500 { font-weight: 500; }
.ui-text--w600 { font-weight: 600; }
.ui-text--w700 { font-weight: 700; }
.ui-text--w800 { font-weight: 800; }

.ui-text--display {
  font-size: var(--ui-fs-3xl);
  font-weight: 800;
  letter-spacing: -0.02em;
  line-height: 1.05;
}

.ui-text--h1 { font-size: var(--ui-fs-2xl); font-weight: 750; letter-spacing: -0.015em; line-height: 1.15; }
.ui-text--h2 { font-size: var(--ui-fs-xl); font-weight: 700; letter-spacing: -0.01em; line-height: 1.2; }
.ui-text--h3 { font-size: var(--ui-fs-lg); font-weight: 650; line-height: 1.3; }
.ui-text--h4 { font-size: var(--ui-fs-md); font-weight: 650; line-height: 1.35; }
.ui-text--body { font-size: var(--ui-fs-md); font-weight: 400; line-height: 1.55; }
.ui-text--small { font-size: var(--ui-fs-sm); line-height: 1.5; }
.ui-text--caption { font-size: var(--ui-fs-xs); line-height: 1.45; color: var(--ui-text-3); }

.ui-text--overline {
  font-family: var(--ui-font);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0;
  text-transform: none;
  color: var(--ui-text-2);
}

.ui-text--label {
  font-family: var(--ui-font);
  font-size: var(--ui-fs-xs);
  font-weight: 600;
  color: var(--ui-text-2);
}

.ui-text--mono {
  font-family: var(--ui-font-mono);
  font-size: var(--ui-fs-sm);
}

.ui-text--muted { color: var(--ui-text-2); }
.ui-text--faint { color: var(--ui-text-3); }
.ui-text--accent { color: var(--ui-accent); }
.ui-text--success { color: var(--ui-success); }
.ui-text--warning { color: var(--ui-warning); }
.ui-text--danger { color: var(--ui-danger); }
.ui-text--info { color: var(--ui-info); }
.ui-text--inverse { color: var(--ui-on-accent); }

.ui-text--gradient {
  color: var(--ui-text);
  font-weight: 600;
}
</style>
