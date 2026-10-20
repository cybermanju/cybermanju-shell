<!-- CyberManju OS — AppDrawer (src/components/AppDrawer.vue). -->
<!-- Role: Vue desktop UI: windows, panels, overlays. -->
<!-- Exports: module root (see items below). -->
<!-- Integrates: @/…, vue. -->
<!-- Contracts: none special (pure logic; errors bubble to caller). -->
<template>
  <Teleport to="body">
    <Transition name="drawer">
      <div
        v-if="launcher.drawerOpen"
        class="app-drawer-veil"
        role="dialog"
        aria-modal="true"
        aria-label="All apps"
        @click.self="close"
      >
        <section class="app-drawer" @touchstart.passive="onSheetTouchStart" @touchend.passive="onSheetTouchEnd">
          <div class="drawer-handle" aria-hidden="true" />
          <header class="drawer-head">
            <div class="drawer-title">
              <h2>Apps</h2>
              <span class="drawer-count">{{ ordered.length }}</span>
            </div>
            <button class="drawer-close" type="button" aria-label="Close app drawer" @click="close">
              <AppIcon name="solar:close-circle-bold" :size="20" />
            </button>
          </header>

          <label class="drawer-search">
            <AppIcon name="solar:magnifier-bold" :size="16" />
            <input
              v-model="query"
              class="drawer-search-input"
              type="search"
              placeholder="Search apps"
              aria-label="Search installed apps"
              autocomplete="off"
            />
            <button v-if="query" class="drawer-clear" type="button" aria-label="Clear search" @click="query = ''">
              <AppIcon name="solar:close-circle-bold" :size="16" />
            </button>
          </label>

          <p v-if="!browser.supported" class="drawer-note">
            Android apps list in the native Android build. This device shows the CyberManju panels instead.
            <button class="drawer-btn" type="button" @click="openInstall">Install options</button>
          </p>

          <div v-else-if="browser.status.value === 'loading'" class="drawer-state" role="status">
            <span class="drawer-spinner" aria-hidden="true" />
            <p>Loading installed apps…</p>
          </div>

          <div v-else-if="browser.status.value === 'error'" class="drawer-state" role="alert">
            <p class="drawer-error">{{ browserErrorHint }}</p>
            <button class="drawer-btn" type="button" @click="retry">
              <AppIcon name="solar:refresh-bold" :size="15" /> Retry
            </button>
            <button v-if="errorNeedsInstall" class="drawer-btn" type="button" @click="openInstall">
              Install options
            </button>
          </div>

          <div
            v-else-if="visible.length"
            ref="gridRef"
            class="drawer-grid"
            role="listbox"
            aria-label="Installed apps"
          >
            <div
              v-for="row in visible"
              :key="row.key"
              class="drawer-cell"
              role="option"
              :aria-selected="false"
              :aria-label="row.name"
            >
              <button
                class="drawer-app"
                type="button"
                :title="row.app.packageName"
                @click="onTap(row)"
                @pointerdown="onPressStart(row, $event)"
                @pointerup="onPressEnd"
                @pointerleave="onPressEnd"
                @pointercancel="onPressEnd"
                @contextmenu.prevent="onContextMenu(row)"
              >
                <span class="drawer-tile">
                  <img
                    v-if="tileFor(row).kind === 'image'"
                    class="drawer-img"
                    :src="tileFor(row).src"
                    :alt="`${row.name} icon`"
                    loading="lazy"
                    draggable="false"
                  />
                  <span v-else class="drawer-letter" aria-hidden="true">{{ tileFor(row).src }}</span>
                </span>
                <span class="drawer-name">{{ row.name }}</span>
              </button>
              <span v-if="row.app.systemApp" class="drawer-sys" title="System app">sys</span>
            </div>
          </div>
          <p v-else class="drawer-state">
            {{ query ? `No apps match “${query}”.` : 'No launchable apps found.' }}
          </p>

          <!-- Long-press customize sheet -->
          <div v-if="active" class="custom-sheet" role="dialog" aria-label="Customize app">
            <div class="custom-head">
              <strong class="custom-title">{{ active.name }}</strong>
              <span class="custom-pkg">{{ active.app.packageName }}</span>
            </div>
            <label class="custom-row">
              <span class="custom-label">Name</span>
              <input
                v-model="aliasDraft"
                class="custom-input"
                type="text"
                maxlength="48"
                placeholder="Custom name"
                aria-label="Custom app name"
              />
            </label>
            <div class="custom-row">
              <span class="custom-label">Icon</span>
              <div class="custom-icon-row">
                <span class="drawer-tile sm">
                  <img v-if="sheetTile.kind === 'image'" class="drawer-img" :src="sheetTile.src" alt="" draggable="false" />
                  <span v-else class="drawer-letter" aria-hidden="true">{{ sheetTile.src }}</span>
                </span>
                <button class="drawer-btn" type="button" @click="pickGallery">
                  <AppIcon name="solar:folder-bold" :size="15" /> Gallery
                </button>
                <input
                  ref="fileRef"
                  type="file"
                  accept="image/*"
                  class="custom-file"
                  aria-label="Pick a custom icon from the gallery"
                  @change="onGalleryFile"
                />
              </div>
            </div>
            <div v-if="packs.length" class="custom-row">
              <span class="custom-label">Icon pack</span>
              <div class="custom-pack-row">
                <select v-model="packDraft" class="custom-input" aria-label="Icon pack">
                  <option value="">System icons</option>
                  <option v-for="p in packs" :key="p.packageName" :value="p.packageName">
                    {{ p.label }}
                  </option>
                </select>
                <button class="drawer-btn" type="button" :disabled="!packDraft || packBusy" @click="applyPackIcon">
                  {{ packBusy ? '…' : 'Apply' }}
                </button>
              </div>
            </div>
            <div class="custom-actions">
              <button class="drawer-btn" type="button" @click="moveActive(-1)" aria-label="Move earlier">← Move</button>
              <button class="drawer-btn" type="button" @click="moveActive(1)" aria-label="Move later">Move →</button>
              <button class="drawer-btn" type="button" @click="hideActive">
                <AppIcon name="solar:eye-bold" :size="15" /> Hide
              </button>
              <button class="drawer-btn danger" type="button" @click="uninstallActive">
                <AppIcon name="solar:trash-bin-trash-bold" :size="15" /> Uninstall
              </button>
            </div>
            <div class="custom-actions">
              <button class="drawer-btn primary" type="button" @click="saveActive">
                <AppIcon name="solar:pen-bold" :size="15" /> Save
              </button>
              <button v-if="hasOverride" class="drawer-btn" type="button" @click="resetActive">Reset</button>
              <button class="drawer-btn" type="button" @click="active = null">Close</button>
            </div>
            <p v-if="sheetMsg" class="custom-msg" role="status">{{ sheetMsg }}</p>
          </div>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import AppIcon from '@/components/AppIcon.vue'
import { computed, onMounted, ref, watch } from 'vue'
import { useAndroidApps } from '@/composables/useAndroidApps'
import { useLauncherStore } from '@/stores/launcher'
import { useAppStore } from '@/stores/app'
import {
  displayIcon,
  fileToLauncherIcon,
  filterOrderedApps,
  orderAndroidApps,
  type OrderedApp,
} from '@/utils/launcher'
import { requestInstallWindow, isInstallNeededError } from '@/utils/installLinks'
import { describeSyncError, type IconPack } from '@/types'

const browser = useAndroidApps()
const launcher = useLauncherStore()
const store = useAppStore()

/** Open the Install window (native builds per platform, App × CLI). */
function openInstall() {
  requestInstallWindow('needs-app')
}

const query = ref('')
const active = ref<OrderedApp | null>(null)
const aliasDraft = ref('')
const packDraft = ref('')
const packBusy = ref(false)
const sheetMsg = ref('')
const packs = ref<IconPack[]>([])
const fileRef = ref<HTMLInputElement | null>(null)
const gridRef = ref<HTMLElement | null>(null)

let pressTimer: ReturnType<typeof setTimeout> | null = null
let longPressed = false
/** Android WebView fires `contextmenu` at ~500ms — BEFORE the 550ms
 * long-press timer. Without this flag a release in the 500-550ms window lets
 * the follow-up `click` run `onTap` and launch the app under the sheet. */
let contextMenuFired = false
let sheetStartY: number | null = null

const ordered = computed(() => orderAndroidApps(browser.apps.value, launcher.overrides))
const visible = computed(() => filterOrderedApps(ordered.value, query.value))

/** AGENT-1 hint for the app-list error — the raw `network:`/`unsupported:`
 * prefix is never rendered alone (same `describeSyncError` mapping the
 * central `notifyError` toast uses). */
const browserErrorHint = computed(() => {
  const detail = browser.lastError.value || ''
  if (!detail) return 'Could not list apps.'
  const { hint } = describeSyncError(detail)
  return hint === 'See logs for detail.' ? detail : `${detail} — ${hint}`
})

/** Install-shaped failures (native-build-only capability) get the same
 * `isInstallNeededError` remedy as the toast net, not just text. */
const errorNeedsInstall = computed(() => isInstallNeededError(browser.lastError.value || ''))

function tileFor(row: OrderedApp) {
  const ov = launcher.overrides[row.key]
  return displayIcon(row.app.iconBase64, row.app.label, ov)
}

const sheetTile = computed(() => {
  if (!active.value) return { kind: 'letter' as const, src: '?' }
  const ov = launcher.overrides[active.value.key]
  const preview = stagedIcon.value.trim()
  if (preview) return { kind: 'image' as const, src: preview }
  const base = ov?.customIcon?.trim() || active.value.app.iconBase64
  return displayIcon(base, active.value.app.label, undefined)
})

const hasOverride = computed(() => {
  if (!active.value) return false
  return active.value.key in launcher.overrides
})

/** Staged icon — a gallery pick OR a pack-apply preview. Component-local
 * only: `saveActive()` is the single commit point into the launcher store
 * (the store deep-watch persists on write), so closing without Save discards. */
const stagedIcon = ref('')
watch(active, () => {
  stagedIcon.value = ''
  sheetMsg.value = ''
  if (active.value) {
    const ov = launcher.overrides[active.value.key]
    aliasDraft.value = ov?.alias ?? ''
    packDraft.value = ov?.iconPack ?? ''
  }
})

function close() {
  active.value = null
  launcher.setDrawer(false)
}

async function retry() {
  try {
    await browser.refresh(true)
  } catch (e) {
    store.notifyError('Could not list Android apps', e)
  }
}

onMounted(() => {
  if (!browser.supported) return
  void browser.refresh().catch(() => {})
  void browser.refreshIconPacks().then((rows) => { packs.value = rows }).catch(() => {})
})

watch(
  () => launcher.drawerOpen,
  (open) => {
    if (open && browser.supported) {
      query.value = ''
      void browser.refresh().catch(() => {})
      void browser.refreshIconPacks().then((rows) => { packs.value = rows }).catch(() => {})
    }
    if (!open) active.value = null
  },
)

/** Swipe-down on the sheet dismisses the drawer (mirror of swipe-up open). */
function onSheetTouchStart(e: TouchEvent) {
  sheetStartY = e.touches[0]?.clientY ?? null
}
function onSheetTouchEnd(e: TouchEvent) {
  if (sheetStartY == null || active.value) return
  const y = e.changedTouches[0]?.clientY ?? sheetStartY
  // Only dismiss when the grid is already at the top — otherwise a normal
  // downward scroll of the app list would slam the drawer shut.
  if (y - sheetStartY > 96 && (gridRef.value?.scrollTop ?? 0) <= 8) close()
  sheetStartY = null
}

async function onTap(row: OrderedApp) {
  if (contextMenuFired) {
    // The contextmenu already opened the sheet for this gesture — swallow the
    // click that follows it instead of launching the app under the sheet.
    contextMenuFired = false
    return
  }
  if (longPressed) {
    longPressed = false
    return
  }
  try {
    await browser.openApp(row.app.packageName)
  } catch (e) {
    store.notifyError(`Could not open ${row.name}`, e)
  }
}

function onPressStart(row: OrderedApp, e: PointerEvent) {
  if (e.pointerType === 'mouse' && e.button !== 0) return
  longPressed = false
  contextMenuFired = false
  if (pressTimer) clearTimeout(pressTimer)
  pressTimer = setTimeout(() => {
    longPressed = true
    try {
      ;(navigator as Navigator & { vibrate?: (p: number) => boolean }).vibrate?.(15)
    } catch { /* optional */ }
    openSheet(row)
  }, 550)
}

function onPressEnd() {
  if (pressTimer) {
    clearTimeout(pressTimer)
    pressTimer = null
  }
}

/** Right-click / Android touch long-press — the WebView's `contextmenu`
 * arrives at ~500ms, before the 550ms timer: cancel the timer (so it cannot
 * arm `longPressed` for the next click) and flag the gesture as consumed. */
function onContextMenu(row: OrderedApp) {
  if (pressTimer) {
    clearTimeout(pressTimer)
    pressTimer = null
  }
  longPressed = false
  contextMenuFired = true
  openSheet(row)
}

function openSheet(row: OrderedApp) {
  active.value = row
}

function pickGallery() {
  fileRef.value?.click()
}

async function onGalleryFile(e: Event) {
  const input = e.target as HTMLInputElement | null
  const file = input?.files?.[0]
  if (!file || !active.value) return
  try {
    const url = await fileToLauncherIcon(file)
    // Staged only — the store deep-watch would persist it instantly, which
    // would make the "Save to keep it" copy a lie. Save commits it.
    stagedIcon.value = url
    sheetMsg.value = 'Gallery icon staged — Save to keep it.'
  } catch (err) {
    store.notifyError('Could not read that image', err)
  } finally {
    if (input) input.value = ''
  }
}

async function applyPackIcon() {
  if (!active.value || !packDraft.value) return
  packBusy.value = true
  sheetMsg.value = ''
  try {
    const url = await browser.packIcon(packDraft.value, active.value.app.packageName)
    // Both halves (resolved image + pack name) stay local until Save — an
    // early `setIconPack` would store an `iconPack` with no `customIcon`.
    stagedIcon.value = url
    sheetMsg.value = 'Pack icon staged — Save to keep it.'
  } catch (e) {
    store.notifyError('That pack has no icon for this app', e)
  } finally {
    packBusy.value = false
  }
}

function saveActive() {
  if (!active.value) return
  const key = active.value.key
  if (aliasDraft.value.trim()) launcher.rename(key, aliasDraft.value)
  else launcher.patch(key, { alias: '' })
  // Single commit point for everything staged in this sheet (icon, pack
  // name) — closing without Save discards it all.
  if (stagedIcon.value) launcher.setIcon(key, stagedIcon.value)
  launcher.setIconPack(key, packDraft.value)
  launcher.pinToEnd(key)
  // The sheet closes right away, so the confirmation has to leave the sheet
  // with it — a transient toast (same pattern as hideActive).
  store.notifySuccess(`Saved “${launcher.get(key).alias || active.value.app.label}”.`)
  active.value = null
}

function moveActive(dir: -1 | 1) {
  if (!active.value) return
  launcher.pinToEnd(active.value.key)
  launcher.move(active.value.key, dir)
  sheetMsg.value = dir < 0 ? 'Moved earlier.' : 'Moved later.'
}

function hideActive() {
  if (!active.value) return
  launcher.hide(active.value.key, true)
  store.notifySuccess(`${active.value.name} hidden — Reset in the store to bring it back`)
  active.value = null
}

async function uninstallActive() {
  if (!active.value) return
  const { packageName, label } = active.value.app
  try {
    await browser.uninstallApp(packageName)
  } catch (e) {
    store.notifyError(`Could not uninstall ${label}`, e)
  }
}

function resetActive() {
  if (!active.value) return
  launcher.reset(active.value.key)
  active.value = null
}
</script>

<style scoped>
.app-drawer-veil {
  position: fixed;
  inset: 0;
  z-index: 9000;
  background: color-mix(in srgb, var(--ui-bg-deep) 55%, transparent);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  display: flex;
  align-items: flex-end;
  justify-content: center;
}
.app-drawer {
  width: min(560px, 100vw);
  max-height: min(86dvh, 720px);
  min-height: 40dvh;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 8px 16px calc(14px + env(safe-area-inset-bottom, 0px));
  border-radius: 24px 24px 0 0;
  border: 1px solid var(--ui-border-strong);
  border-bottom: 0;
  background: color-mix(in srgb, var(--ui-surface) 88%, var(--ui-bg));
  color: var(--ui-text);
  box-shadow: var(--ui-shadow-3);
  overflow: hidden;
}
.drawer-handle {
  width: 44px;
  height: 5px;
  margin: 4px auto 0;
  border-radius: 3px;
  background: color-mix(in srgb, var(--ui-text-2) 42%, transparent);
}
.drawer-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.drawer-title {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.drawer-title h2 {
  margin: 0;
  font-size: 20px;
  font-weight: 750;
  letter-spacing: -0.02em;
}
.drawer-count {
  color: var(--ui-text-3);
  font-size: 12px;
  font-weight: 600;
}
.drawer-close {
  width: 44px;
  height: 44px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 50%;
  background: var(--ui-surface-2);
  color: var(--ui-text-2);
  cursor: pointer;
}
.drawer-search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  min-height: 44px;
  border: 1px solid var(--ui-border);
  border-radius: 14px;
  background: var(--ui-surface);
  color: var(--ui-text-3);
}
.drawer-search-input {
  flex: 1;
  min-width: 0;
  border: 0;
  outline: 0;
  background: none;
  color: var(--ui-text);
  font: inherit;
  font-size: 15px;
}
.drawer-clear {
  border: 0;
  background: none;
  color: var(--ui-text-3);
  cursor: pointer;
  display: grid;
  place-items: center;
  /* Phone tap floor — the icon stays small, the hit area doesn't. */
  min-width: 44px;
  min-height: 44px;
}
.drawer-note {
  margin: 0;
  color: var(--ui-text-3);
  font-size: 13px;
  text-align: center;
}
.drawer-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 26px 12px;
  color: var(--ui-text-3);
  font-size: 13px;
  text-align: center;
  overflow-y: auto;
}
.drawer-error {
  color: var(--ui-danger);
  font-size: 13px;
}
.drawer-spinner {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: 3px solid var(--ui-border-strong);
  border-top-color: var(--ui-accent);
  animation: drawer-spin 0.8s linear infinite;
}
@keyframes drawer-spin {
  to { transform: rotate(360deg); }
}
.drawer-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 14px 6px;
  padding: 4px 2px 10px;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  align-content: start;
}
.drawer-cell {
  position: relative;
  min-width: 0;
}
.drawer-app {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 4px 2px;
  border: 0;
  border-radius: 16px;
  background: transparent;
  cursor: pointer;
  -webkit-tap-highlight-color: transparent;
  touch-action: pan-x pan-y;
}
.drawer-app:active {
  transform: scale(0.92);
}
.drawer-tile {
  width: 56px;
  height: 56px;
  display: grid;
  place-items: center;
  border-radius: 16px;
  background: linear-gradient(145deg, var(--ui-surface-2), var(--ui-surface));
  border: 1px solid var(--ui-border);
  overflow: hidden;
  color: var(--ui-text);
  font-size: 24px;
  font-weight: 750;
}
.drawer-tile.sm {
  width: 46px;
  height: 46px;
  font-size: 20px;
}
.drawer-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.drawer-letter {
  user-select: none;
}
.drawer-name {
  max-width: 100%;
  overflow: hidden;
  color: var(--ui-text-2);
  font-size: 11px;
  line-height: 1.25;
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.drawer-sys {
  position: absolute;
  top: 0;
  right: 6px;
  padding: 1px 5px;
  border-radius: 8px;
  background: var(--ui-surface-2);
  color: var(--ui-text-3);
  font-size: 9px;
  font-weight: 700;
}
.custom-sheet {
  border-top: 1px solid var(--ui-border-strong);
  padding: 12px 2px 4px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: color-mix(in srgb, var(--ui-surface-2) 55%, transparent);
  border-radius: 16px 16px 0 0;
  /* The app grid owns the drawer's scroll; in landscape the sheet must
     scroll itself or Save/Close sit below the fold with no way to reach. */
  max-height: min(58dvh, 460px);
  overflow-y: auto;
  overscroll-behavior: contain;
  -webkit-overflow-scrolling: touch;
}
.custom-head {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.custom-title {
  font-size: 15px;
}
.custom-pkg {
  color: var(--ui-text-3);
  font-size: 11px;
  font-family: var(--ui-font-mono);
}
.custom-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.custom-label {
  flex: 0 0 64px;
  color: var(--ui-text-3);
  font-size: 12px;
  font-weight: 600;
}
.custom-input {
  flex: 1;
  min-width: 0;
  min-height: 44px;
  padding: 0 10px;
  border: 1px solid var(--ui-border);
  border-radius: 10px;
  background: var(--ui-surface);
  color: var(--ui-text);
  font: inherit;
  font-size: 14px;
}
.custom-icon-row,
.custom-pack-row {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.custom-file {
  display: none;
}
.custom-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.drawer-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-height: 44px;
  padding: 0 12px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 12px;
  background: var(--ui-surface);
  color: var(--ui-text);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}
.drawer-btn.primary {
  background: var(--ui-accent);
  border-color: var(--ui-accent);
  color: var(--ui-on-accent);
}
.drawer-btn.danger {
  color: var(--ui-danger);
  border-color: color-mix(in srgb, var(--ui-danger) 45%, var(--ui-border));
}
.drawer-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.custom-msg {
  margin: 0;
  color: var(--ui-text-3);
  font-size: 12px;
}
.drawer-enter-active,
.drawer-leave-active {
  transition: opacity 180ms ease;
}
.drawer-enter-active .app-drawer,
.drawer-leave-active .app-drawer {
  transition: transform 220ms cubic-bezier(0.32, 0.9, 0.35, 1);
}
.drawer-enter-from,
.drawer-leave-to {
  opacity: 0;
}
.drawer-enter-from .app-drawer,
.drawer-leave-to .app-drawer {
  transform: translateY(40px);
}
</style>
