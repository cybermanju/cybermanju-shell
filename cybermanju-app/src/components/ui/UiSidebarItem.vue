<!-- CyberManju OS — UiSidebarItem (src/components/ui/UiSidebarItem.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <button
    type="button"
    class="ui-sidebar-item"
    :class="{ 'is-active': active }"
    :aria-current="active ? 'true' : undefined"
    @click="emit('click', $event)"
  >
    <AppIcon v-if="icon" :name="icon" :size="15" class="ui-sidebar-item__icon" />
    <span class="ui-sidebar-item__label"><slot>{{ label }}</slot></span>
    <span v-if="count !== undefined" class="ui-sidebar-item__count">{{ count }}</span>
  </button>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

withDefaults(
  defineProps<{
    label?: string
    icon?: string
    count?: number
    /** Selected row. Renders accent fill when the host window is focused,
     * grey fill when not — put `.is-unfocused` on an ancestor (e.g. the
     * blurred window) for the unfocused treatment. */
    active?: boolean
  }>(),
  { label: '', icon: '', count: undefined, active: false },
)

const emit = defineEmits<{ click: [event: MouseEvent] }>()
</script>

<style scoped>
.ui-sidebar-item {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 100%;
  min-height: 28px;
  padding: 0 8px;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-2);
  background: transparent;
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 400;
  text-align: left;
  user-select: none;
  transition:
    background-color var(--ui-dur-fast) ease-out,
    color var(--ui-dur-fast) ease-out;
}

.ui-sidebar-item:hover {
  background: color-mix(in srgb, var(--ui-text) 6%, transparent);
  color: var(--ui-text);
}

.ui-sidebar-item__icon {
  flex-shrink: 0;
  color: var(--ui-text-3);
}

.ui-sidebar-item:hover .ui-sidebar-item__icon {
  color: var(--ui-text-2);
}

.ui-sidebar-item__label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-sidebar-item__count {
  font-size: 11px;
  font-weight: 500;
  color: var(--ui-text-3);
  background: color-mix(in srgb, var(--ui-text) 8%, transparent);
  border-radius: var(--ui-radius-full);
  padding: 0 6px;
  line-height: 18px;
}

/* Selected = accent fill with white text (focused window). */
.ui-sidebar-item.is-active {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
  font-weight: 500;
}

.ui-sidebar-item.is-active .ui-sidebar-item__icon,
.ui-sidebar-item.is-active .ui-sidebar-item__count {
  color: var(--ui-on-accent);
}

.ui-sidebar-item.is-active .ui-sidebar-item__count {
  background: rgba(255, 255, 255, 0.22);
}

/* Unfocused window: grey fill instead of accent. */
.is-unfocused .ui-sidebar-item.is-active {
  background: color-mix(in srgb, var(--ui-text) 14%, transparent);
  color: var(--ui-text);
}

.is-unfocused .ui-sidebar-item.is-active .ui-sidebar-item__icon,
.is-unfocused .ui-sidebar-item.is-active .ui-sidebar-item__count {
  color: var(--ui-text-2);
}

.ui-sidebar-item:focus-visible {
  outline: none;
  box-shadow: var(--ui-focus-ring);
}
</style>
