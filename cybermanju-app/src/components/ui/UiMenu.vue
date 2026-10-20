<!-- CyberManju OS — UiMenu (src/components/ui/UiMenu.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: MenuItem. -->
<!-- Integrates: @/…. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="ui-menu" role="menu" :aria-label="ariaLabel">
    <template v-for="item in items" :key="item.id">
      <div v-if="item.divider" class="ui-menu__divider" role="separator" />
      <button
        v-else
        type="button"
        class="ui-menu__row"
        :class="{ 'is-danger': item.danger, 'is-active': activeId === item.id }"
        role="menuitem"
        :disabled="item.disabled"
        @click="emit('select', item.id)"
        @mouseenter="emit('hover', item.id)"
      >
        <AppIcon v-if="item.icon" :name="item.icon" :size="14" class="ui-menu__icon" />
        <span class="ui-menu__label">{{ item.label }}</span>
        <span v-if="item.shortcut" class="ui-menu__shortcut">{{ item.shortcut }}</span>
        <AppIcon v-if="item.checked" name="solar:check-bold" :size="12" class="ui-menu__check" />
      </button>
    </template>
    <div v-if="items.length === 0" class="ui-menu__empty">{{ emptyText }}</div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'

export interface MenuItem {
  id: string
  label?: string
  icon?: string
  shortcut?: string
  checked?: boolean
  danger?: boolean
  disabled?: boolean
  divider?: boolean
}

withDefaults(
  defineProps<{
    items?: MenuItem[]
    activeId?: string
    ariaLabel?: string
    emptyText?: string
  }>(),
  { items: () => [], activeId: '', ariaLabel: 'Menu', emptyText: 'No items' },
)

const emit = defineEmits<{ select: [id: string]; hover: [id: string] }>()
</script>

<style scoped>
.ui-menu {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 200px;
  padding: 4px;
  background: var(--ui-glass-2);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-menu);
  font-family: var(--ui-font);
}

.ui-menu__row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 22px;
  padding: 3px 8px;
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text);
  font-size: 13px;
  font-weight: 400;
  text-align: left;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.ui-menu__row:hover:not(:disabled),
.ui-menu__row.is-active:not(:disabled) {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
}

.ui-menu__row:hover:not(:disabled) .ui-menu__shortcut,
.ui-menu__row.is-active:not(:disabled) .ui-menu__shortcut,
.ui-menu__row:hover:not(:disabled) .ui-menu__icon,
.ui-menu__row.is-active:not(:disabled) .ui-menu__icon {
  color: var(--ui-on-accent);
}

.ui-menu__row:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.ui-menu__row.is-danger {
  color: var(--ui-danger);
}

.ui-menu__row.is-danger:hover:not(:disabled) {
  background: var(--ui-danger);
  /* Fixed white: clears 3:1 on every theme's danger fill. */
  color: #fff;
}

.ui-menu__icon {
  flex-shrink: 0;
  color: var(--ui-text-2);
}

.ui-menu__label {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ui-menu__shortcut {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--ui-text-3);
}

.ui-menu__check {
  flex-shrink: 0;
}

.ui-menu__divider {
  height: 1px;
  margin: 4px 8px;
  background: var(--ui-separator);
}

.ui-menu__empty {
  padding: 8px;
  font-size: 12px;
  color: var(--ui-text-3);
  text-align: center;
}
</style>
