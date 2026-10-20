<!-- CyberManju OS — MapView (src/components/MapView.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue, maplibre-gl. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <div class="map-view">
    <div class="panel-header">
      <div class="header-left">
        <span class="icon-map"><AppIcon name="solar:map-point-bold" /></span>
        <h2 class="panel-title">GEOGRAPHY VIEW</h2>
      </div>
      <div class="header-actions">
        <button class="refresh-btn" @click="toggleFullscreen" title="FULLSCREEN (F)" aria-label="FULLSCREEN"><AppIcon name="solar:maximize-bold" :size="13" /></button>
        <button class="refresh-btn" :class="{ active: mapStyle === 'satellite' }" @click="toggleMapStyle" title="TOGGLE MAP STYLE">[{{ mapStyle === 'satellite' ? 'SAT' : 'OSM' }}]</button>
        <button class="refresh-btn" @click="handleRefresh" title="REFRESH" aria-label="REFRESH MAP"><AppIcon name="solar:refresh-bold" :size="13" /></button>
      </div>
    </div>

    <div class="search-location-bar">
      <input
        v-model="locationQuery"
        class="loc-search-input"
        placeholder="SEARCH LOCATION (E.G. TOKYO, 35.68,139.76)..."
        @keyup.enter="handleLocationSearch"
        aria-label="SEARCH LOCATION"
      />
      <button class="refresh-btn" @click="handleLocationSearch" title="SEARCH LOCATION" aria-label="SEARCH LOCATION"><AppIcon name="solar:target-bold" :size="13" /></button>
    </div>

    <div class="map-container" v-if="geoMarkers.length > 0">
      <div ref="mapContainer" class="maplibre-map"></div>
      <div class="map-stats-overlay" aria-label="Map location count">
        <span>{{ geoMarkers.length }} LOCATIONS</span>
      </div>
    </div>

    <div class="empty-state" v-if="geoMarkers.length === 0 && !isLoading">
      <p>NO GEOTAGGED FILES FOUND. PHOTOS WITH GPS EXIF DATA WILL APPEAR HERE.</p>
    </div>

    <div class="empty-state" v-if="isLoading">
      <div class="loading-spinner"></div>
      <p>LOADING GEO DATA..</p>
    </div>

    <div class="section" v-if="geoMarkers.length > 0">
      <h3 class="section-title"><AppIcon name="solar:list-bold" :size="13" /> GEOTAGGED FILES ({{ geoMarkers.length }})</h3>
      <div class="geo-list">
        <div v-for="marker in geoMarkers" :key="'list-' + marker.fileId" class="geo-list-item" @click="flyToMarker(marker)">
          <span class="geo-list-pin"><AppIcon name="solar:map-point-bold" :size="14" /></span>
          <div class="geo-list-info">
            <span class="geo-list-name">{{ marker.fileName }}</span>
            <span class="geo-list-address text-muted" v-if="marker.address">{{ marker.address }}</span>
          </div>
          <span class="geo-list-coords mono">{{ marker.lat.toFixed(3) }}, {{ marker.lng.toFixed(3) }}</span>
        </div>
      </div>
    </div>

    <div class="status-footer">
      <span>GPS EXTRACTION VIA KAMADAK-EXIF (RUST) | MAPLIBRE GL</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useAppStore } from '@/stores/app'
import type { GeoMarker } from '@/types'
// maplibre-gl v6 is ESM-only and loads its worker from a file next to the
// bundled module, which Vite does not emit. Bundle the worker ourselves and
// point maplibre at it (must happen before the first Map is constructed).
import maplibreWorkerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url'
import 'maplibre-gl/dist/maplibre-gl.css'

const store = useAppStore()
const emit = defineEmits<{ close: [] }>()

const geoMarkers = computed(() => store.geoMarkers)
const isLoading = computed(() => store.isLoading)
const mapContainer = ref<HTMLDivElement | null>(null)
const locationQuery = ref('')
const mapStyle = ref<'osm' | 'satellite'>('osm')

let map: any = null
let maplibreglModule: any = null
let markers: any[] = []

onMounted(async () => {
  await store.fetchGeoFiles()
  await nextTick()
  if (geoMarkers.value.length > 0) initMap()
})

onUnmounted(() => destroyMap())

watch(geoMarkers, async (newMarkers) => {
  if (newMarkers.length > 0 && !map) {
    await nextTick()
    initMap()
  } else if (map) {
    updateMarkers()
  }
})

async function initMap() {
  if (!mapContainer.value || map) return
  try {
    maplibreglModule = await import('maplibre-gl')
    maplibreglModule.setWorkerUrl(maplibreWorkerUrl)
    const center = getMapCenter()
    map = new maplibreglModule.Map({
      container: mapContainer.value,
      style: getMapStyle(),
      center: [center.lng, center.lat],
      zoom: center.zoom,
      attributionControl: false,
    })
    map.on('load', () => addMarkers())
  } catch (e) { console.warn('MapLibre GL failed:', e) }
}

function getMapStyle() {
  if (mapStyle.value === 'satellite') {
    return {
      version: 8,
      sources: {
        satellite: { type: 'raster', tiles: ['https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'], tileSize: 256, attribution: '&copy; Esri' },
        labels: { type: 'raster', tiles: ['https://server.arcgisonline.com/ArcGIS/rest/services/Reference/World_Boundaries_and_Places/MapServer/tile/{z}/{y}/{x}'], tileSize: 256, attribution: '&copy; Esri' },
      },
      layers: [
        { id: 'satellite', type: 'raster', source: 'satellite' },
        { id: 'labels', type: 'raster', source: 'labels' },
      ],
    }
  }
  return {
    version: 8,
    sources: {
      osm: { type: 'raster', tiles: ['https://tile.openstreetmap.org/{z}/{x}/{y}.png'], tileSize: 256, attribution: '&copy; OpenStreetMap contributors' },
    },
    layers: [{ id: 'osm', type: 'raster', source: 'osm' }],
  }
}

function toggleMapStyle() {
  mapStyle.value = mapStyle.value === 'osm' ? 'satellite' : 'osm'
  if (map) {
    destroyMap()
    setTimeout(() => initMap(), 100)
  }
}

function toggleFullscreen() {
  try {
    if (!document.fullscreenElement) {
      void document.documentElement.requestFullscreen().catch(() => {})
    } else {
      void document.exitFullscreen().catch(() => {})
    }
  } catch { /* fullscreen unsupported — stay embedded */ }
}

async function handleLocationSearch() {
  const q = locationQuery.value.trim()
  if (!q) return
  const coordsMatch = q.match(/^(-?\d+\.?\d*)\s*[,;]\s*(-?\d+\.?\d*)$/)
  if (coordsMatch) {
    const lat = parseFloat(coordsMatch[1])
    const lng = parseFloat(coordsMatch[2])
    if (!Number.isFinite(lat) || !Number.isFinite(lng) || Math.abs(lat) > 90 || Math.abs(lng) > 180) return
    if (map) map.flyTo({ center: [lng, lat], zoom: 12, essential: true })
    return
  }
  try {
    const res = await fetch(`https://nominatim.openstreetmap.org/search?format=json&q=${encodeURIComponent(q)}&limit=1`)
    if (!res.ok) return
    const data = await res.json()
    if (data && data.length > 0) {
      const { lat, lon } = data[0]
      const flat = parseFloat(lat)
      const flon = parseFloat(lon)
      if (!Number.isFinite(flat) || !Number.isFinite(flon)) return
      if (map) map.flyTo({ center: [flon, flat], zoom: 12, essential: true })
    }
  } catch { /* offline / blocked geocoder — stay on current view */ }
}

function destroyMap() {
  if (map) {
    markers.forEach((m: any) => m.remove())
    markers = []
    map.remove()
    map = null
  }
}

function getMapCenter() {
  if (geoMarkers.value.length === 0) return { lat: 20, lng: 0, zoom: 2 }
  const lats = geoMarkers.value.map(m => m.lat)
  const lngs = geoMarkers.value.map(m => m.lng)
  const avgLat = lats.reduce((a, b) => a + b, 0) / lats.length
  const avgLng = lngs.reduce((a, b) => a + b, 0) / lngs.length
  const latSpan = Math.max(...lats) - Math.min(...lats)
  const lngSpan = Math.max(...lngs) - Math.min(...lngs)
  const span = Math.max(latSpan, lngSpan)
  const zoom = span > 100 ? 2 : span > 50 ? 3 : span > 20 ? 4 : span > 5 ? 6 : 8
  return { lat: avgLat, lng: avgLng, zoom }
}

function addMarkers() {
  if (!map) return
  markers.forEach((m: any) => m.remove())
  markers = []
  geoMarkers.value.forEach((marker) => {
    const el = document.createElement('div')
    el.className = 'bw-marker'
    el.style.cssText = 'width:16px;height:16px;border:2px solid #000;background:#fff;cursor:pointer;'
    el.addEventListener('mouseenter', () => { el.style.transform = 'scale(1.4)' })
    el.addEventListener('mouseleave', () => { el.style.transform = 'scale(1)' })
    const m = maplibreglModule ? new maplibreglModule.Marker({ element: el }).setLngLat([marker.lng, marker.lat]).addTo(map) : null
    if (m) markers.push(m)
  })
}

function updateMarkers() { addMarkers() }

function flyToMarker(marker: GeoMarker) {
  if (map) map.flyTo({ center: [marker.lng, marker.lat], zoom: 12, essential: true })
}

async function handleRefresh() { await store.fetchGeoFiles() }
</script>

<style scoped>
.map-view {
  width: 100%;
  height: 100%;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  overflow-y: auto;
  padding: max(16px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right)) max(16px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left));
  display: flex;
  flex-direction: column;
  gap: 12px;
  font-family: var(--ui-font-mono);
  color: var(--ui-text);
}

.map-view::-webkit-scrollbar { width: 10px; }
.map-view::-webkit-scrollbar-track { background: transparent; }
.map-view::-webkit-scrollbar-thumb { background: color-mix(in srgb, var(--ui-accent) 45%, transparent);
  border-radius: var(--ui-radius-full); border: 3px solid transparent; background-clip: content-box; }
.map-view::-webkit-scrollbar-thumb:hover { background: var(--ui-accent); background-clip: content-box; border: 2px solid transparent; }

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--ui-border);
  gap: 12px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 8px;
}

.icon-map { font-size: 16px; color: var(--ui-text); }

.panel-title {
  font-size: 14px;
  font-weight: 800;
  letter-spacing: 1px;
  color: var(--ui-text);
  margin: 0;
}

.header-actions { display: flex; gap: 6px; }

.search-location-bar {
  display: flex;
  gap: 6px;
  align-items: center;
}

.loc-search-input {
  flex: 1;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  font-family: var(--ui-font-mono);
  font-size: 10px;
  padding: 4px 8px;
  min-width: 0;
  min-height: 44px;
}

.loc-search-input::placeholder {
  color: color-mix(in srgb, var(--ui-text) 35%, transparent);
}

.refresh-btn {
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  color: var(--ui-text);
  cursor: pointer;
  padding: 2px 6px;
  min-width: 44px;
  min-height: 44px;
  border-radius: var(--ui-radius-sm, 6px);
  font-family: var(--ui-font-mono);
  font-size: 10px;
  font-weight: 700;
}

.refresh-btn:hover { background: var(--ui-glass-2); color: var(--ui-text); }
.refresh-btn.active { background: var(--ui-glass-2); color: var(--ui-text); }

.map-container {
  width: 100%;
  position: relative;
  min-height: 300px;
  border: 1px solid var(--ui-border);
  overflow: hidden;
  border-radius: var(--ui-radius-md, 10px);
}

.maplibre-map { width: 100%; height: 300px; touch-action: none; }

.map-stats-overlay {
  position: absolute;
  top: 8px;
  left: 8px;
  background: var(--ui-surface);
  border: 1px solid var(--ui-border);
  padding: 2px 6px;
  font-size: 9px;
  color: var(--ui-text);
  font-family: var(--ui-font-mono);
  z-index: 5;
  pointer-events: none;
  border-radius: var(--ui-radius-sm, 6px);
}

.section { display: flex; flex-direction: column; gap: 8px; }

.section-title {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 1px;
  color: color-mix(in srgb, var(--ui-text) 60%, transparent);
  margin: 0;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--ui-hairline);
  display: flex;
  align-items: center;
  gap: 6px;
}

.geo-list { display: flex; flex-direction: column; gap: 4px; }

.geo-list-item {
  border: 1px solid var(--ui-border);
  padding: 6px 10px;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  min-height: 44px;
  border-radius: var(--ui-radius-sm, 6px);
}

.geo-list-item:hover { background: color-mix(in srgb, var(--ui-text) 10%, transparent); }

.geo-list-pin { flex-shrink: 0; font-size: 11px; }

.geo-list-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.geo-list-name { font-size: 11px; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.geo-list-address { font-size: 9px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.geo-list-coords { font-size: 9px; color: color-mix(in srgb, var(--ui-text) 50%, transparent); flex-shrink: 0; }

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 40px;
  text-align: center;
}

.empty-state p { font-size: 11px; color: color-mix(in srgb, var(--ui-text) 50%, transparent); margin: 0; }

.loading-spinner {
  width: 24px;
  height: 24px;
  border: 3px solid var(--ui-border);
  border-top-color: var(--ui-border-strong);
  animation: spin 0.8s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

@media (prefers-reduced-motion: reduce) {
  .loading-spinner { animation: none; }
}

.status-footer {
  margin-top: auto;
  padding-top: 10px;
  border-top: 1px solid var(--ui-hairline);
  font-size: 9px;
  color: color-mix(in srgb, var(--ui-text) 35%, transparent);
  text-align: center;
}

.mono { font-family: var(--ui-font-mono); }
.text-muted { color: color-mix(in srgb, var(--ui-text) 50%, transparent) !important; }

@media (max-width: 640px) {
  .map-view {
    gap: 10px;
    padding: max(12px, env(safe-area-inset-top)) max(12px, env(safe-area-inset-right)) max(12px, env(safe-area-inset-bottom)) max(12px, env(safe-area-inset-left));
  }

  .panel-header { align-items: flex-start; }
  .panel-title { font-size: 13px; line-height: 44px; }
  .header-actions { flex-shrink: 0; gap: 4px; }
  .header-actions .refresh-btn { padding-inline: 4px; }
  .search-location-bar { gap: 8px; }
  .loc-search-input { font-size: 11px; }

  .map-container { min-height: clamp(260px, 48vh, 420px); }
  .maplibre-map { height: clamp(260px, 48vh, 420px); }
  .map-stats-overlay { top: 10px; left: 10px; }

  .geo-list { gap: 6px; }
  .geo-list-item {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: 4px 8px;
    padding: 8px 10px;
  }
  .geo-list-info { min-width: 0; }
  .geo-list-coords {
    grid-column: 2;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-footer { padding-bottom: 2px; line-height: 1.4; }
}
</style>

<style>
.maplibregl-popup-content {
  background: var(--ui-surface) !important;
  border: 1px solid var(--ui-border) !important;
  box-shadow: var(--ui-shadow-2);
  padding: 6px 10px !important;
  color: var(--ui-text) !important;
  font-family: var(--ui-font-mono);
  font-size: 11px !important;
}
.maplibregl-popup-tip { border-top-color: var(--ui-border-strong) !important; }
</style>
