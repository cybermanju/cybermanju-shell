<!-- CyberManju OS — KeyboardShortcutsHelp (src/components/KeyboardShortcutsHelp.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue, @vueuse. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <div
      v-if="store.showShortcutsHelp"
      class="ks-help-overlay"
      @click.self="store.showShortcutsHelp = false"
    >
      <div class="ks-help-modal" role="dialog" aria-label="Keyboard shortcuts">
        <div class="ks-help-header">
          <h2>KEYBOARD SHORTCUTS · {{ transportLabel }}</h2>
          <button ref="closeBtnRef" class="close-btn" @click="store.showShortcutsHelp = false" aria-label="Close shortcuts help" title="Close (Esc)"><AppIcon name="solar:close-bold" :size="13" /></button>
        </div>
        <div v-if="inBrowser" class="ks-browser-note">
          <AppIcon name="solar:info-circle-bold" :size="13" />
          <span>Browser tabs own <b>Ctrl+T / Ctrl+W / Ctrl+Tab</b> — use the <b>Alt+</b> fallback shown. Tauri supports both.</span>
        </div>
        <div class="ks-help-body">
          <div v-for="group in groupedShortcuts" :key="group.label" class="ks-group">
            <div class="ks-group-label">{{ group.label }}</div>
            <div v-for="s in group.shortcuts" :key="s.action" class="ks-row">
              <span class="ks-key">{{ s.keys }}</span>
              <span class="ks-desc">{{ s.description }}</span>
              <span v-if="s.blockedInBrowser && inBrowser" class="ks-fb" :title="`Primary ${s.primary} never reaches page JS in a browser — press ${s.fallback} instead`">
                was {{ s.primary }}
              </span>
            </div>
          </div>
          <div v-if="groupedShortcuts.length === 0" class="ks-empty text-muted">
            No shortcuts registered. Press Esc to close.
          </div>
        </div>
        <div class="ks-help-foot text-muted">Press ? to toggle · Esc to close · {{ inBrowser ? 'Browser: Alt+ fallbacks' : 'Ctrl+K for commands · Alt+1-4 layouts' }}</div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, inject, watch, nextTick, ref } from 'vue'
import { onKeyStroke } from '@vueuse/core'
import { useAppStore } from '@/stores/app'
import { ShortcutsKey } from '@/composables/shortcutsKey'
import type { ShortcutEntry } from '@/composables/useShortcuts'
import { isTauri } from '@/composables/useTauri'
import { useTransport } from '@/composables/useTransport'

const store = useAppStore()
const shortcuts = inject(ShortcutsKey)
const closeBtnRef = ref<HTMLElement | null>(null)

// VueUse key handler: Esc closes the modal wherever focus sits.
onKeyStroke('Escape', () => {
  if (store.showShortcutsHelp) store.showShortcutsHelp = false
})

watch(() => store.showShortcutsHelp, async (open) => {
  if (open) {
    await nextTick()
    closeBtnRef.value?.focus()
  }
})

const inBrowser = computed(() => !isTauri())
const transportLabel = computed(() => useTransport().short)

const allShortcuts = computed<ShortcutEntry[]>(() => {
  return shortcuts?.getAllShortcuts() || []
})

const groupLabels: Record<string, string> = {
  'Global Shortcuts': 'GLOBAL',
  'Navigation': 'NAVIGATION',
  'File Operations': 'FILE OPERATIONS',
  'View': 'VIEW',
  'Panels': 'PANELS',
  'Windows': 'WINDOWS',
  'Workspace': 'WORKSPACE · LAYOUT',
}

const groupOrder = ['Windows', 'Workspace', 'Global Shortcuts', 'Navigation', 'File Operations', 'View', 'Panels']

const groupedShortcuts = computed(() => {
  const map = new Map<string, ShortcutEntry[]>()
  for (const s of allShortcuts.value) {
    const group = s.group || 'Other'
    if (!map.has(group)) map.set(group, [])
    map.get(group)!.push(s)
  }
  return Array.from(map.entries())
    .sort((a, b) => groupOrder.indexOf(a[0]) - groupOrder.indexOf(b[0]))
    .map(([group, shortcuts]) => ({
      label: groupLabels[group] || group.toUpperCase(),
      shortcuts,
    }))
})
</script>

<style scoped>
.ks-help-overlay {
  position: fixed;
  inset: 0;
  background: color-mix(in srgb, var(--ui-bg-deep) 62%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10001;
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
}

.ks-help-modal {width: 560px;
  max-width: 92vw;
  max-height: 74vh;
  background: var(--ui-glass-2);
  border: 1px solid var(--ui-border);
  box-shadow: var(--ui-shadow-2);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border-radius: var(--ui-radius-lg);
  backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(var(--ui-blur)) saturate(var(--ui-saturate));
}

.ks-help-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--ui-border);
  background: var(--ui-surface);
  color: var(--ui-text);
}

.ks-help-header h2 {
  font-family: var(--ui-font);
  font-size: 13px;
  font-weight: 800;
  letter-spacing: 1px;
  margin: 0;
}

.ks-browser-note {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin: 10px 14px 0;
  padding: 8px 10px;
  font-size: 11px;
  line-height: 1.45;
  color: var(--ui-text-2);
  border: 1px solid color-mix(in srgb, var(--ui-warning) 50%, transparent);
  background: color-mix(in srgb, var(--ui-warning) 9%, transparent);
  border-radius: var(--ui-radius-md);
}

.close-btn {
  background: transparent;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  color: var(--ui-text-2);
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-family: var(--ui-font);
  transition:
    background-color var(--ui-dur-fast) var(--ui-ease-out),
    border-color var(--ui-dur-fast) var(--ui-ease-out),
    color var(--ui-dur-fast) var(--ui-ease-out);
}

.close-btn:hover {
  background: var(--ui-glass-2);
  border-color: var(--ui-border-hover);
  color: var(--ui-text);
}

.close-btn:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--ui-accent) 75%, transparent);
  outline-offset: 2px;
}

.ks-help-body {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}

.ks-group {
  padding: 6px 0;
}

.ks-group-label {
  padding: 2px 16px;
  font-family: var(--ui-font);
  font-size: 9px;
  font-weight: 700;
  color: color-mix(in srgb, var(--ui-text) 40%, transparent);
  letter-spacing: 1px;
  margin-bottom: 2px;
}

.ks-row {
  display: flex;
  align-items: center;
  padding: 3px 16px;
  gap: 12px;
}

.ks-key {
  font-family: var(--ui-font-mono);
  font-size: 10px;
  font-weight: 700;
  color: var(--ui-text);
  background: color-mix(in srgb, var(--ui-surface) 55%, transparent);
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  box-shadow: inset 0 -1px 0 color-mix(in srgb, var(--ui-text) 8%, transparent);
  padding: 2px 8px;
  min-width: 100px;
  text-align: center;
  letter-spacing: 0.04em;
  flex-shrink: 0;
}

.ks-desc {
  font-family: var(--ui-font);
  font-size: 10px;
  color: color-mix(in srgb, var(--ui-text) 70%, transparent);
  flex: 1;
}

.ks-fb {
  font-family: var(--ui-font-mono);
  font-size: 8.5px;
  color: var(--ui-warning);
  white-space: nowrap;
  flex-shrink: 0;
}

.ks-empty { padding: 16px; text-align: center; font-size: 11px; }

.ks-help-foot {
  padding: 8px 16px;
  border-top: 1px solid var(--ui-border);
  font-family: var(--ui-font-mono);
  font-size: 9px;
  letter-spacing: 0.06em;
  text-align: center;
}
</style>
