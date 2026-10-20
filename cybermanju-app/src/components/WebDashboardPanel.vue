<!-- CyberManju OS — WebDashboardPanel (src/components/WebDashboardPanel.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="dash-panel">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-dash"><AppIcon name="solar:monitor-bold" /></span>
        <h2 class="panel-title">REMOTE DASHBOARD</h2>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:pulse-bold" :size="13" /> DASHBOARD STATUS</h3>
      <div class="status-card" aria-live="polite">
        <div class="s-row"><span class="s-key text-muted">STATUS</span><span class="s-value" :class="{ 'is-running': store.dashboardStatus?.running }">{{ store.dashboardStatus ? (store.dashboardStatus.running ? 'RUNNING' : 'STOPPED') : 'UNAVAILABLE' }}</span></div>
        <div class="s-row"><span class="s-key text-muted">PORT</span><span class="s-value">{{ store.dashboardStatus?.port ?? '—' }}</span></div>
        <div class="s-row"><span class="s-key text-muted">URL</span><span class="s-value mono s-value-wrap">{{ store.dashboardStatus?.url ?? '—' }}</span></div>
        <div class="s-row"><span class="s-key text-muted">CONNECTIONS</span><span class="s-value">{{ store.dashboardStatus?.activeConnections ?? '—' }}</span></div>
      </div>
      <div class="dashboard-actions">
        <button class="bw-btn" @click="store.startDashboard()" :disabled="store.dashboardStatus?.running" title="START DASHBOARD"><AppIcon name="solar:play-bold" :size="12" /> START</button>
        <button class="bw-btn" @click="store.stopDashboard()" :disabled="!store.dashboardStatus?.running" title="STOP DASHBOARD"><AppIcon name="solar:close-square-bold" :size="12" /> STOP</button>
        <button class="bw-btn" @click="store.fetchDashboardStatus()" title="REFRESH DASHBOARD"><AppIcon name="solar:refresh-bold" :size="13" /> REFRESH</button>
      </div>
    </div>

    <div class="section">
      <h3 class="section-title"><AppIcon name="solar:brackets-bold" :size="13" /> API ENDPOINTS</h3>
      <div class="api-list">
        <div v-for="ep in apiEndpoints" :key="ep.path + ep.method" class="api-row">
          <span class="api-method">{{ ep.method }}</span>
          <span class="api-path">{{ ep.path }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, computed, onMounted } from 'vue'
import { useAppStore } from '@/stores/app'
import type { DashboardStatus, ApiEndpoint } from '@/types'

const store = useAppStore()

const apiEndpoints = ref<ApiEndpoint[]>([
  { method: 'GET', path: '/api/files', description: 'LIST FILES' },
  { method: 'GET', path: '/api/search', description: 'FULL-TEXT SEARCH' },
  { method: 'GET', path: '/api/health', description: 'HEALTH CHECK' },
  { method: 'GET', path: '/api/accounts', description: 'LIST ACCOUNTS' },
  { method: 'GET', path: '/api/collections', description: 'LIST COLLECTIONS' },
  { method: 'GET', path: '/api/face-groups', description: 'LIST FACE GROUPS' },
  { method: 'GET', path: '/api/encryption/status', description: 'ENCRYPTION STATUS' },
])

onMounted(() => {
  store.fetchDashboardStatus()
})
</script>

<style scoped>
.dash-panel {
  width: 100%;
  height: 100%;
  background: var(--ui-surface);
  overflow-y: auto;
  padding: max(16px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right)) max(16px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left));
  font-family: var(--ui-font);
  color: var(--ui-text);
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ui-border);
  margin-bottom: 16px;
}

.header-left { display: flex; align-items: center; gap: 8px; }
.icon-dash { font-size: 16px; }
.panel-title { font-size: 14px; font-weight: 800; letter-spacing: 1px; margin: 0; }

.section { margin-bottom: 16px; }

.section:last-child { margin-bottom: 0; }

.section-title {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1px;
  color: color-mix(in srgb, var(--ui-text) 60%, transparent);
  margin: 0 0 8px;
}

.status-card {
  border: 1px solid var(--ui-border);
  border-radius: 14px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 0;
  background: color-mix(in srgb, var(--ui-surface) 86%, var(--ui-text));
}

.s-row { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; min-height: 28px; }
.s-row + .s-row { border-top: 1px solid color-mix(in srgb, var(--ui-border) 65%, transparent); }
.s-key { flex: 0 0 auto; font-size: 10px; letter-spacing: .5px; }
.s-value { min-width: 0; font-size: 11px; font-weight: 700; text-align: right; }
.s-value.is-running { color: color-mix(in srgb, var(--ui-accent, #4ade80) 75%, var(--ui-text)); }
.s-value-wrap { overflow-wrap: anywhere; }
.mono { font-family: var(--ui-font); }

.dashboard-actions { display: flex; gap: 6px; margin-top: 8px; }
.dashboard-actions .bw-btn { flex: 1 1 0; min-width: 0; min-height: 44px; border-radius: 12px; }

.api-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  border: 1px solid var(--ui-border);
  border-radius: 14px;
  overflow: hidden;
}

.api-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 44px;
  padding: 6px 10px;
  font-size: 11px;
  border-bottom: 1px solid var(--ui-border);
}

.api-row:last-child { border-bottom: 0; }

.api-method { flex: 0 0 36px; font-weight: 700; }
.api-path { min-width: 0; color: color-mix(in srgb, var(--ui-text) 70%, transparent); overflow-wrap: anywhere; }

.text-muted { color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important; }

@media (max-width: 600px) {
  .dash-panel { padding-top: max(12px, env(safe-area-inset-top)); padding-right: max(12px, env(safe-area-inset-right)); padding-bottom: max(20px, env(safe-area-inset-bottom)); padding-left: max(12px, env(safe-area-inset-left)); }
  .panel-header { margin-bottom: 14px; padding-bottom: 12px; }
  .panel-title { font-size: clamp(12px, 3.5vw, 14px); letter-spacing: .8px; }
  .section { margin-bottom: 20px; }
  .section-title { line-height: 1.4; }
  .dashboard-actions { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .dashboard-actions .bw-btn:last-child { grid-column: 1 / -1; }
}

@media (prefers-reduced-motion: reduce) {
  .dash-panel *, .dash-panel *::before, .dash-panel *::after { scroll-behavior: auto !important; transition-duration: 0.01ms !important; animation-duration: 0.01ms !important; }
}
</style>
