<!-- CyberManju OS — UiButton (src/components/ui/UiButton.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: vue, @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <button
    class="ui-btn"
    :class="[
      `ui-btn--${variant}`,
      `ui-btn--${size}`,
      {
        'ui-btn--block': block,
        'ui-btn--active': active,
        'ui-btn--loading': loading,
        'ui-btn--icon-only': iconOnly,
      },
    ]"
    :type="type"
    :disabled="disabled || loading"
    :aria-busy="loading || undefined"
    :aria-pressed="active"
    @click="onClick"
  >
    <span v-if="showRipple" class="ui-btn__ripple" :style="rippleStyle" aria-hidden="true" />
    <span class="ui-btn__content">
      <AppIcon v-if="loading && !icon" name="solar:loader-bold" class="ui-btn__spinner" :size="iconSize" />
      <AppIcon v-else-if="icon" :name="icon" :size="iconSize" />
      <span v-if="!iconOnly" class="ui-btn__label"><slot /></span>
      <AppIcon v-if="iconRight && !iconOnly" :name="iconRight" :size="iconSize" />
    </span>
  </button>
</template>

<script setup lang="ts">
import { computed, ref, useSlots } from 'vue'
import AppIcon from '@/components/AppIcon.vue'

const props = withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary' | 'ghost' | 'outline' | 'danger' | 'subtle'
    size?: 'xs' | 'sm' | 'md' | 'lg'
    icon?: string
    iconRight?: string
    loading?: boolean
    disabled?: boolean
    block?: boolean
    active?: boolean
    type?: 'button' | 'submit' | 'reset'
    /**
     * Press ripple — off by default (flat controls). Enable per-button
     * only where press feedback needs emphasis.
     */
    ripple?: boolean
  }>(),
  {
    variant: 'secondary',
    size: 'md',
    icon: '',
    iconRight: '',
    loading: false,
    disabled: false,
    block: false,
    active: false,
    type: 'button',
    ripple: false,
  }
)

const emit = defineEmits<{ click: [event: MouseEvent] }>()

const slots = useSlots()
const iconSize = computed(() => ({ xs: 12, sm: 13, md: 15, lg: 17 }[props.size]))
/** Icon-only mode when the button carries an icon and no text content. */
const iconOnly = computed(() => Boolean(props.icon) && !slots.default)

const rippleStyle = ref<Record<string, string> | null>(null)
const showRipple = ref(false)

function onClick(event: MouseEvent) {
  if (props.ripple && !props.loading) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
    rippleStyle.value = {
      left: `${event.clientX - rect.left}px`,
      top: `${event.clientY - rect.top}px`,
    }
    showRipple.value = false
    requestAnimationFrame(() => {
      showRipple.value = true
      setTimeout(() => {
        showRipple.value = false
      }, 520)
    })
  }
  emit('click', event)
}
</script>

<style scoped>
.ui-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  isolation: isolate;
  border-radius: var(--ui-radius-sm);
  border: 1px solid transparent;
  font-family: var(--ui-font);
  font-weight: 500;
  user-select: none;
  white-space: nowrap;
  cursor: pointer;
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out),
    box-shadow var(--ui-dur-fast) var(--ui-ease-out);
}

.ui-btn:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
  border-color: var(--ui-accent);
}

.ui-btn:active:not(:disabled) {
  transform: scale(0.98);
}

.ui-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ui-btn--block {
  width: 100%;
}

/* sizes */
.ui-btn--xs { min-height: 22px; padding: 0 8px; font-size: var(--ui-fs-xs); gap: 4px; }
.ui-btn--sm { min-height: calc(var(--ui-control-h) * 0.8); padding: 0 10px; font-size: var(--ui-fs-xs); gap: 5px; }
.ui-btn--md { min-height: var(--ui-control-h); padding: 0 14px; font-size: var(--ui-fs-sm); gap: 6px; }
.ui-btn--lg { min-height: calc(var(--ui-control-h) * 1.25); padding: 0 20px; font-size: var(--ui-fs-md); gap: 8px; }

.ui-btn__content {
  position: relative;
  z-index: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: inherit;
}

.ui-btn__label {
  display: inline-block;
}

/* variants */
.ui-btn--primary {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
  border-color: transparent;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
}

.ui-btn--primary:hover:not(:disabled) {
  background: var(--ui-accent-strong);
}

.ui-btn--secondary {
  background: var(--ui-surface-2);
  border-color: var(--ui-border-strong);
  color: var(--ui-text);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.06);
}

.ui-btn--secondary:hover:not(:disabled) {
  background: var(--ui-surface-3);
  border-color: var(--ui-border-hover);
  color: var(--ui-text);
}

.ui-btn--ghost {
  background: transparent;
  color: var(--ui-text-2);
}

.ui-btn--ghost:hover:not(:disabled) {
  background: color-mix(in srgb, var(--ui-text) 7%, transparent);
  color: var(--ui-text);
}

.ui-btn--outline {
  background: transparent;
  border-color: var(--ui-border-strong);
  color: var(--ui-text);
}

.ui-btn--outline:hover:not(:disabled) {
  background: color-mix(in srgb, var(--ui-text) 5%, transparent);
  border-color: var(--ui-border-hover);
}

.ui-btn--danger {
  background: color-mix(in srgb, var(--ui-danger) 12%, transparent);
  border-color: transparent;
  color: var(--ui-danger);
}

.ui-btn--danger:hover:not(:disabled) {
  background: var(--ui-danger);
  color: white;
}

.ui-btn--subtle {
  background: color-mix(in srgb, var(--ui-text) 5%, transparent);
  border-color: transparent;
  color: var(--ui-text-2);
}

.ui-btn--subtle:hover:not(:disabled) {
  background: color-mix(in srgb, var(--ui-text) 9%, transparent);
  border-color: transparent;
  color: var(--ui-text);
}

.ui-btn--active {
  background: var(--ui-accent-softer);
  border-color: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  color: var(--ui-accent);
}

.ui-btn--icon-only.ui-btn--active {
  color: var(--ui-accent);
}

/* loading */
.ui-btn--loading .ui-btn__spinner {
  animation: ui-spin 0.8s linear infinite;
}

/* ripple: quiet neutral press feedback (opt-in per button) */
.ui-btn__ripple {
  position: absolute;
  width: 12px;
  height: 12px;
  margin: -6px 0 0 -6px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--ui-text) 18%, transparent);
  transform: scale(0);
  opacity: 0.6;
  pointer-events: none;
  z-index: 0;
  animation: ui-ripple 420ms var(--ui-ease-out) forwards;
}

@keyframes ui-ripple {
  to {
    transform: scale(26);
    opacity: 0;
  }
}
</style>
