<!-- CyberManju OS — MobileNav (src/components/MobileNav.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <!-- Keep app tabs for open sheets; on the home surface the launcher dock
    is the only bottom control, like a native phone home screen. The Home
    tab always coexists with the open sheets, so only app tabs can be
    `active` here (there is no "no-window" state to mark). -->
  <nav v-if="hasOpenWindows" class="mobile-nav" role="navigation" aria-label="Mobile navigation">
    <button
      class="mn-btn"
      type="button"
      aria-label="Home"
      @click="goHome"
    >
      <AppIcon name="solar:home-bold" :size="20" />
      <span class="mn-label">Home</span>
    </button>
    <button
      v-for="t in tabs"
      :key="t.id"
      class="mn-btn"
      :class="{ active: hasOpenWindows && isActive(t) }"
      type="button"
      :aria-label="t.label"
      :aria-current="hasOpenWindows && isActive(t) ? 'page' : undefined"
      @click="go(t)"
    >
      <AppIcon :name="t.icon" :size="20" />
      <span class="mn-label">{{ t.label }}</span>
    </button>
    <!-- iPhone-X home indicator pill under the tab bar. -->
    <span class="mn-home" aria-hidden="true" />
  </nav>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed } from 'vue'
import { useAppStore } from '@/stores/app'
import { useWindowManager } from '@/composables/useWindowManager'
import type { PanelType } from '@/types'

const wm = useWindowManager()
const store = useAppStore()

interface Tab {
  id: string
  label: string
  icon: string
  panel: PanelType
  props?: Record<string, unknown>
}

const tabs: Tab[] = [
  { id: 'files', label: 'Files', icon: 'solar:folder-bold', panel: 'files' },
  { id: 'search', label: 'Search', icon: 'solar:magnifier-bold', panel: 'search' },
  { id: 'agent', label: 'Agent', icon: 'solar:bot-bold', panel: 'agent' },
  { id: 'disks', label: 'Vaults', icon: 'solar:diskette-bold', panel: 'storage' },
  { id: 'settings', label: 'Settings', icon: 'solar:settings-bold', panel: 'settings' },
]

const hasOpenWindows = computed(() => wm.windows.value.some(w => !w.minimized))

const activePanel = computed(() => {
  const focused = wm.activeWindow.value?.panelType as string | undefined
  if (focused) return focused
  if (store.currentPanel === 'landing') return 'files'
  return store.currentPanel
})

function isActive(t: Tab): boolean {
  if (t.panel === 'storage') return activePanel.value === 'storage'
  return activePanel.value === t.panel
}

function goHome() {
  wm.closeAll()
  store.currentPanel = 'files'
  try {
    ;(navigator as Navigator & { vibrate?: (p: number) => boolean }).vibrate?.(8)
  } catch { /* no haptics */ }
}

function go(t: Tab) {
  if (store.currentPanel === 'landing') store.currentPanel = 'files'
  wm.open(t.panel, t.props)
  try {
    ;(navigator as Navigator & { vibrate?: (p: number) => boolean }).vibrate?.(5)
  } catch { /* no haptics */ }
}
</script>

<style scoped>
.mobile-nav {
  display: none;
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  height: calc(64px + env(safe-area-inset-bottom, 0px));
  min-height: 64px;
  box-sizing: border-box;
  padding: 4px 8px calc(4px + env(safe-area-inset-bottom, 0px));
  /* iOS tab bar: hairline top edge + heavy blur, like Cupertino tab bars. */
  background: color-mix(in srgb, var(--ui-surface) 66%, transparent);
  backdrop-filter: blur(28px) saturate(1.6);
  -webkit-backdrop-filter: blur(28px) saturate(1.6);
  border-top: 0.5px solid color-mix(in srgb, var(--ui-border) 68%, transparent);
  box-shadow: 0 -8px 28px rgba(16, 24, 40, .06);
  z-index: 100;
  font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", var(--ui-font), sans-serif;
}
/* iPhone-X home indicator pill. */
.mn-home {
  position: absolute;
  bottom: calc(3px + env(safe-area-inset-bottom, 0px));
  left: 50%;
  width: 134px;
  height: 5px;
  transform: translateX(-50%);
  border-radius: 3px;
  background: color-mix(in srgb, var(--ui-text-2) 36%, transparent);
  pointer-events: none;
}

.mn-btn {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: none;
  color: var(--ui-text-3);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  min-height: 56px;
  padding: 5px 2px;
  border-radius: 14px;
  transition: color 120ms ease-out, transform 150ms cubic-bezier(.34,1.4,.4,1), background 120ms ease-out;
  -webkit-tap-highlight-color: transparent;
}
.mn-btn:active {
  color: var(--ui-text-2);
  background: color-mix(in srgb, var(--ui-accent) 9%, transparent);
  transform: scale(0.92);
}

.mn-label {
  font-size: 10px;
  font-weight: 600;
  white-space: nowrap;
}

.mn-btn.active {
  color: var(--ui-accent);
  background: color-mix(in srgb, var(--ui-accent) 12%, transparent);
}

@media (max-width: 768px) {
  .mobile-nav {
    display: flex;
  }
}
</style>
