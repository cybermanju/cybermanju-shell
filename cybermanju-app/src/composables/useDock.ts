// CyberManju OS — useDock (src/composables/useDock.ts).
// Role: Vue shared state: macOS-style dock layout + visibility preferences.
// Exports: useDock, DockLayout, DockVisibility, DOCK_HEIGHT.
// Integrates: vue (module-level singleton, like useTheme).
// Contracts: persisted in localStorage (`cybermanju_dock_v1`); defaults are
// centered + fixed (the classic floating pill that overlaps windows).

import { ref, watch, type Ref } from 'vue'

/** Dock surface shape: `centered` floating pill vs `full` edge-to-edge bar. */
export type DockLayout = 'centered' | 'full'

/** Dock behavior: `fixed` always shown vs `autohide` slides in from the edge. */
export type DockVisibility = 'fixed' | 'autohide'

/**
 * Reserved height (px) of the dock overlay inside `.desktop-area`. Windows
 * in the full+fixed combo inset their workspace by this much so the opaque
 * bar can never swallow them; centered/autohide overlap windows on purpose.
 */
export const DOCK_HEIGHT = 72

const STORAGE_KEY = 'cybermanju_dock_v1'

function loadLayout(): DockLayout {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return 'centered'
    const parsed = JSON.parse(raw) as { layout?: string }
    if (parsed.layout === 'full' || parsed.layout === 'centered') return parsed.layout
  } catch { /* default stands */ }
  return 'centered'
}

function loadVisibility(): DockVisibility {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return 'fixed'
    const parsed = JSON.parse(raw) as { visibility?: string }
    if (parsed.visibility === 'autohide' || parsed.visibility === 'fixed') return parsed.visibility
  } catch { /* default stands */ }
  return 'fixed'
}

function persist(layout: DockLayout, visibility: DockVisibility) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ layout, visibility }))
  } catch {
    /* private mode / quota — the dock still applies for this session */
  }
}

/**
 * Module-level singleton: every caller shares one reactive dock preference
 * state (Dock.vue renders from it, DesktopShell insets windows from it).
 */
const layoutRef = ref<DockLayout>(loadLayout())
const visibilityRef = ref<DockVisibility>(loadVisibility())

watch([layoutRef, visibilityRef], () => persist(layoutRef.value, visibilityRef.value))

export interface DockApi {
  layout: Ref<DockLayout>
  visibility: Ref<DockVisibility>
  setLayout: (layout: DockLayout) => void
  setVisibility: (visibility: DockVisibility) => void
}

export function useDock(): DockApi {
  return {
    layout: layoutRef,
    visibility: visibilityRef,
    setLayout: (layout) => { layoutRef.value = layout },
    setVisibility: (visibility) => { visibilityRef.value = visibility },
  }
}
