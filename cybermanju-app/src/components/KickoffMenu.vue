<!-- CyberManju OS — KickoffMenu (src/components/KickoffMenu.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="kickoff-layer" @mousedown.self="kickoff.close()">
    <div ref="menuRef" class="kickoff" role="dialog" aria-label="Application launcher">
      <div class="k-search">
        <AppIcon name="solar:magnifier-bold" :size="14" class="k-search-icon" />
        <input
          ref="inputRef"
          v-model="query"
          class="k-input"
          type="text"
          placeholder="Search applications…"
          aria-label="Search applications"
          @keydown.esc="kickoff.close()"
          @keydown.enter="openFirst()"
        />
      </div>

      <div class="k-apps">
        <button
          v-for="app in filtered"
          :key="app.id"
          type="button"
          class="k-app"
          :title="app.description"
          @click="launch(app.id)"
        >
          <span class="k-app-icon"><AppIcon :name="app.icon" :size="22" /></span>
          <span class="k-app-label">{{ app.label }}</span>
        </button>
        <div v-if="filtered.length === 0" class="k-empty">No applications match “{{ query }}”</div>
      </div>

      <div class="k-foot">
        <div class="k-tabs" role="tablist" aria-label="Launcher sections">
          <button
            v-for="t in tabs"
            :key="t.id"
            type="button"
            role="tab"
            class="k-tab"
            :class="{ active: tab === t.id }"
            :aria-selected="tab === t.id"
            @click="tab = t.id"
          >{{ t.label }}</button>
        </div>
        <div class="k-user">
          <span class="k-avatar"><AppIcon name="solar:user-circle-bold" :size="18" /></span>
          <span class="k-username">{{ username }}</span>
        </div>
        <div class="k-power" role="group" aria-label="Power actions">
          <button type="button" class="k-pow" title="Lock (back to welcome)" @click="power('lock')">
            <AppIcon name="solar:lock-bold" :size="15" />
          </button>
          <button type="button" class="k-pow" title="Log out" @click="power('logout')">
            <AppIcon name="solar:login-bold" :size="15" />
          </button>
          <button type="button" class="k-pow" title="Restart app" @click="power('restart')">
            <AppIcon name="solar:restart-bold" :size="15" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { MODULE_METADATA, type PanelType } from '@/types'
import { resolvePanel } from '@/utils/panels'
import { useWindowManager } from '@/composables/useWindowManager'
import { useKickoff } from '@/composables/useKickoff'
import { useAppStore } from '@/stores/app'

const wm = useWindowManager()
const kickoff = useKickoff()
const store = useAppStore()

const query = ref('')
const tab = ref<'favorites' | 'apps' | 'recent' | 'power'>('apps')
const inputRef = ref<HTMLInputElement | null>(null)
const menuRef = ref<HTMLElement | null>(null)

interface Entry {
  id: PanelType
  label: string
  icon: string
  description: string
}

/** Canonical launchable panels (aliases collapse via resolvePanel). */
const allApps = computed<Entry[]>(() => {
  const seen = new Set<PanelType>()
  const out: Entry[] = []
  for (const [id, meta] of Object.entries(MODULE_METADATA) as [PanelType, (typeof MODULE_METADATA)[PanelType]][]) {
    const canonical = resolvePanel(id)
    if (canonical === 'landing' || seen.has(canonical)) continue
    seen.add(canonical)
    const target = MODULE_METADATA[canonical]
    out.push({
      id: canonical,
      label: sentence(target.label),
      icon: target.icon,
      description: target.description,
    })
  }
  return out.sort((a, b) => a.label.localeCompare(b.label))
})

function sentence(s: string): string {
  const lower = s.toLowerCase()
  return lower.charAt(0).toUpperCase() + lower.slice(1)
}

const favoriteIds = computed<PanelType[]>(() => ['files', 'editor', 'terminal', 'agent', 'settings'])
const recentIds = computed<PanelType[]>(() =>
  wm.windows.value.map((w) => w.panelType).filter((id, i, arr) => arr.indexOf(id) === i),
)

const tabs = [
  { id: 'favorites', label: 'Favorites' },
  { id: 'apps', label: 'Apps' },
  { id: 'recent', label: 'Recent' },
  { id: 'power', label: 'Power' },
] as const

const scoped = computed<Entry[]>(() => {
  if (tab.value === 'favorites') {
    return favoriteIds.value
      .map((id) => allApps.value.find((a) => a.id === id))
      .filter((a): a is Entry => Boolean(a))
  }
  if (tab.value === 'recent') {
    const rec = recentIds.value
      .map((id) => allApps.value.find((a) => a.id === id))
      .filter((a): a is Entry => Boolean(a))
    return rec.length > 0 ? rec : allApps.value.slice(0, 6)
  }
  if (tab.value === 'power') return []
  return allApps.value
})

const filtered = computed<Entry[]>(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return scoped.value
  return allApps.value.filter(
    (a) => a.label.toLowerCase().includes(q) || a.description.toLowerCase().includes(q),
  )
})

const username = computed(() => store.currentUser?.username || store.currentUser?.displayName || 'user')

function launch(id: PanelType) {
  wm.open(id)
  kickoff.close()
}

function openFirst() {
  if (filtered.value.length > 0) launch(filtered.value[0].id)
}

async function power(action: 'lock' | 'logout' | 'restart') {
  kickoff.close()
  if (action === 'restart') {
    window.location.reload()
    return
  }
  if (action === 'logout') {
    await store.logout().catch(() => {})
    return
  }
  // Lock: park all windows and return to the welcome screen.
  wm.closeAll()
  store.currentPanel = 'landing'
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') kickoff.close()
}

onMounted(() => {
  inputRef.value?.focus()
  document.addEventListener('keydown', onKey)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKey)
})
</script>

<style scoped>
.kickoff-layer {
  position: fixed;
  inset: 0;
  z-index: 9000;
  background: transparent;
}

.kickoff {
  position: fixed;
  left: 8px;
  bottom: calc(var(--ui-panel-h, 44px) + 16px);
  width: 420px;
  max-width: calc(100vw - 32px);
  max-height: min(560px, calc(100vh - 120px));
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--ui-panel);
  backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  -webkit-backdrop-filter: blur(20px) saturate(var(--ui-saturate));
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  box-shadow: var(--ui-shadow-popup);
  font-family: var(--ui-font);
  animation: ui-fade-in var(--ui-dur) ease-out both;
}

.k-search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--ui-separator);
}

.k-search-icon {
  color: var(--ui-text-3);
  flex-shrink: 0;
}

.k-input {
  flex: 1;
  min-width: 0;
  background: var(--ui-surface-2);
  border: 1px solid var(--ui-border-strong);
  border-radius: var(--ui-radius-sm);
  min-height: var(--ui-control-h);
  padding: 0 10px;
  color: var(--ui-text);
  font-size: 13px;
  outline: none;
}

.k-input:focus {
  border-color: var(--ui-accent);
  box-shadow: var(--ui-focus-ring);
}

.k-input::placeholder {
  color: var(--ui-text-3);
}

.k-apps {
  flex: 1;
  min-height: 200px;
  overflow-y: auto;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 4px;
  align-content: start;
  padding: 10px;
}

.k-app {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 10px 6px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  color: var(--ui-text-2);
  cursor: pointer;
  transition: background-color var(--ui-dur-fast) ease-out;
}

.k-app:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.k-app-icon {
  display: inline-flex;
  color: var(--ui-text-2);
}

.k-app:hover .k-app-icon {
  color: var(--ui-text);
}

.k-app-label {
  font-size: 12px;
  text-align: center;
  line-height: 1.3;
  overflow: hidden;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
}

.k-empty {
  grid-column: 1 / -1;
  padding: 24px;
  text-align: center;
  font-size: 13px;
  color: var(--ui-text-3);
}

.k-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-top: 1px solid var(--ui-separator);
}

.k-tabs {
  display: flex;
  gap: 2px;
  flex: 1;
  min-width: 0;
}

.k-tab {
  padding: 4px 10px;
  min-height: 26px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  color: var(--ui-text-2);
  font-size: 12px;
  cursor: pointer;
}

.k-tab:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}

.k-tab.active {
  background: var(--ui-hover);
  color: var(--ui-text);
  font-weight: 600;
}

.k-user {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--ui-text-2);
  font-size: 12px;
  max-width: 130px;
}

.k-username {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.k-power {
  display: flex;
  gap: 2px;
}

.k-pow {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: var(--ui-radius-sm);
  background: transparent;
  border: none;
  color: var(--ui-text-2);
  cursor: pointer;
}

.k-pow:hover {
  background: var(--ui-hover);
  color: var(--ui-text);
}
</style>
