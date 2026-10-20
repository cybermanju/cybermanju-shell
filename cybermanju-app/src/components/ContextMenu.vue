<!-- CyberManju OS — ContextMenu (src/components/ContextMenu.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <Transition name="ctx-fade">
      <div
        v-if="ctx.state.visible"
        ref="menuRef"
        class="ctx-menu"
        :style="menuStyle"
        role="menu"
        :aria-label="'Context menu'"
        @click.stop
        @contextmenu.prevent
        @keydown="handleKeydown"
        tabindex="-1"
      >
        <template v-for="(item, i) in visibleItems" :key="item.id">
          <div v-if="item.divider" class="ctx-divider" role="separator" />
          <div
            v-else
            class="ctx-item"
            :class="{
              'ctx-disabled': item.disabled,
              'ctx-focused': focusIndex === i,
              'ctx-has-submenu': !!item.submenu?.length,
            }"
            role="menuitem"
            :aria-disabled="item.disabled"
            :aria-haspopup="!!item.submenu?.length"
            :aria-expanded="!!item.submenu?.length && openSubmenuId === item.id ? 'true' : 'false'"
            @mouseenter="onItemHover(i, item)"
            @click="onItemClick(item)"
          >
            <span class="ctx-icon"><AppIcon :name="item.icon" :size="13" /></span>
            <span class="ctx-label">{{ item.label }}</span>
            <span class="ctx-shortcut text-muted">{{ item.shortcut || '' }}</span>
            <span v-if="item.submenu?.length" class="ctx-arrow"><AppIcon name="solar:alt-arrow-right-bold" :size="11" /></span>
          </div>
          <!-- submenu -->
          <Teleport to="body" v-if="item.submenu?.length && openSubmenuId === item.id">
            <div
              class="ctx-menu ctx-submenu"
              :style="submenuStyle(item.submenu || [])"
              role="menu"
              :aria-label="item.label + ' SUBMENU'"
              @click.stop
              @contextmenu.prevent
            >
              <template v-for="(sub, si) in item.submenu" :key="sub.id">
                <div v-if="sub.divider" class="ctx-divider" role="separator" />
                <div
                  v-else
                  class="ctx-item"
                  :class="{ 'ctx-disabled': sub.disabled }"
                  role="menuitem"
                  :aria-disabled="sub.disabled"
                  @click="onItemClick(sub)"
                >
                  <span class="ctx-icon"><AppIcon :name="sub.icon" :size="13" /></span>
                  <span class="ctx-label">{{ sub.label }}</span>
                  <span class="ctx-shortcut text-muted">{{ sub.shortcut || '' }}</span>
                </div>
              </template>
            </div>
          </Teleport>
        </template>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, ref, watch, nextTick, onMounted, onUnmounted } from 'vue'
import { useContextMenu, type ContextMenuEntry } from '@/composables/useContextMenu'

const ctx = useContextMenu()
const menuRef = ref<HTMLElement | null>(null)
const focusIndex = ref(-1)
const openSubmenuId = ref<string | null>(null)

const visibleItems = computed(() => ctx.state.items)

/** Row height the positioning math must budget for: touch menus use the
 *  44px tap floor, so the viewport clamp below would otherwise let a menu
 *  run off the bottom of a phone screen. */
const ROW_H = (() => {
  try {
    return window.matchMedia?.('(pointer: coarse)').matches ? 44 : 28
  } catch {
    return 28
  }
})()

const menuStyle = computed(() => {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const estW = 200
  const estH = visibleItems.value.length * ROW_H + 8
  let x = ctx.state.x
  let y = ctx.state.y
  if (x + estW > vw) x = vw - estW - 4
  if (y + estH > vh) y = Math.max(4, vh - estH - 4)
  if (x < 4) x = 4
  if (y < 4) y = 4
  return { left: x + 'px', top: y + 'px' }
})

function submenuStyle(entries: ContextMenuEntry[]) {
  const vw = window.innerWidth
  const vh = window.innerHeight
  let x = ctx.state.x + 190
  let y = ctx.state.y
  const estH = entries.length * ROW_H + 8
  if (x + 180 > vw) x = ctx.state.x - 190
  if (y + estH > vh) y = Math.max(4, vh - estH - 4)
  return { left: x + 'px', top: y + 'px' }
}

function onItemHover(i: number, item: ContextMenuEntry) {
  focusIndex.value = i
  if (item.submenu?.length) {
    openSubmenuId.value = item.id
  } else if (openSubmenuId.value) {
    openSubmenuId.value = null
  }
}

function onItemClick(item: ContextMenuEntry) {
  if (item.disabled) return
  if (item.submenu?.length) {
    // Touch has no hover: the first tap opens the submenu (mouse users get
    // the same result from mouseenter), a second tap on the parent folds it
    // back. Rows inside the submenu fall through to their own handler.
    openSubmenuId.value = openSubmenuId.value === item.id ? null : item.id
    return
  }
  ctx.triggerAction(item)
  openSubmenuId.value = null
}

function handleKeydown(e: KeyboardEvent) {
  const items = visibleItems.value.filter(it => !it.divider)
  switch (e.key) {
    case 'ArrowDown':
      e.preventDefault()
      focusIndex.value = focusIndex.value < items.length - 1 ? focusIndex.value + 1 : 0
      break
    case 'ArrowUp':
      e.preventDefault()
      focusIndex.value = focusIndex.value > 0 ? focusIndex.value - 1 : items.length - 1
      break
    case 'ArrowRight': {
      const item = items[focusIndex.value]
      if (item?.submenu?.length) openSubmenuId.value = item.id
      break
    }
    case 'ArrowLeft':
      openSubmenuId.value = null
      break
    case 'Enter':
    case ' ':
      e.preventDefault()
      if (items[focusIndex.value]) onItemClick(items[focusIndex.value])
      break
    case 'Escape':
      if (openSubmenuId.value) {
        openSubmenuId.value = null
      } else {
        ctx.close()
      }
      break
  }
}

function closeOnScroll() {
  if (ctx.state.visible) ctx.close()
}

function closeOnResize() {
  if (ctx.state.visible) ctx.close()
}

/** Named so the paired removal in `onUnmounted` can actually find it. */
function closeOnOutsideClick() {
  if (ctx.state.visible) ctx.close()
}

watch(() => ctx.state.visible, (v) => {
  if (v) {
    nextTick(() => menuRef.value?.focus())
  } else {
    openSubmenuId.value = null
    focusIndex.value = -1
  }
})

onMounted(() => {
  document.addEventListener('scroll', closeOnScroll, true)
  window.addEventListener('resize', closeOnResize)
  document.addEventListener('click', closeOnOutsideClick)
})

onUnmounted(() => {
  document.removeEventListener('scroll', closeOnScroll, true)
  window.removeEventListener('resize', closeOnResize)
  document.removeEventListener('click', closeOnOutsideClick)
})
</script>

<style scoped>.ctx-menu {position: fixed;
  /* Above modals/palette (10000): a menu opened from inside a dialog has
     to be visible and tappable — both teleport to body. */
  z-index: 10100;
  background: var(--ui-glass-2);
  border: 1px solid var(--ui-border);
  min-width: 200px;
  max-width: 300px;
  padding: 4px;
  font-family: var(--ui-font);
  font-size: 13px;
  color: var(--ui-text);
  box-shadow: var(--ui-shadow-menu);
  outline: none;
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  border-radius: var(--ui-radius-lg);
}

.ctx-submenu {position: fixed;
  z-index: 10101;
  border-radius: var(--ui-radius-lg);
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 22px;
  padding: 3px 8px;
  border-radius: var(--ui-radius-sm);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  color: var(--ui-text);
  transition: background-color var(--ui-dur-fast) ease-out;
}

.ctx-item:hover,
.ctx-focused {
  background: var(--ui-accent);
  color: var(--ui-on-accent);
}

.ctx-item:hover .ctx-shortcut,
.ctx-focused .ctx-shortcut {
  color: var(--ui-on-accent);
}

.ctx-disabled {
  opacity: 0.35;
  cursor: default;
  pointer-events: none;
}

.ctx-icon {
  width: 16px;
  flex-shrink: 0;
  display: inline-flex;
  justify-content: center;
  color: var(--ui-text-2);
}

.ctx-item:hover .ctx-icon,
.ctx-focused .ctx-icon {
  color: inherit;
}

.ctx-label {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  /* Legacy labels are registered UPPERCASE — render sentence case. */
  text-transform: lowercase;
}

.ctx-label::first-letter {
  text-transform: uppercase;
}

.ctx-shortcut {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--ui-text-3);
  margin-left: 12px;
}

.ctx-arrow {
  flex-shrink: 0;
  display: flex;
  align-items: center;
}

.ctx-divider {
  height: 1px;
  background: var(--ui-separator);
  margin: 4px 8px;
}

.ctx-fade-enter-active,
.ctx-fade-leave-active {
  transition: opacity 0.1s ease;
}

.ctx-fade-enter-from,
.ctx-fade-leave-to {
  opacity: 0;
}

/* Touch: the 22px desktop row is untappable — hold the 44px floor (the
   script's ROW_H budget uses the same value for viewport clamping). */
@media (pointer: coarse) {
  .ctx-item {
    min-height: 44px;
    padding: 8px 10px;
    font-size: 15px;
  }
}
</style>
