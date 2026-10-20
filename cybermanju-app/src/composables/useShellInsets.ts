// CyberManju OS — UseShellInsets (src/composables/useShellInsets.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useShellInsets, ShellInsets.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { computed, ref } from 'vue'

/**
 * Layout insets owned by the active shell, read live from the `--ui-*`
 * custom properties published by `buildCssVars()`:
 * - macOS: `--ui-menubar-h: 24px`, `--ui-panel-h: 0px`
 * - plasma: `--ui-menubar-h: 0px`, `--ui-panel-h: 44px`
 *
 * `useWindowManager()` consults these whenever the measured workspace box
 * is unavailable, so tiled/strip/overview math respects the bottom panel.
 */
const version = ref(0)

if (typeof window !== 'undefined' && typeof MutationObserver !== 'undefined') {
  try {
    const mo = new MutationObserver(() => {
      version.value += 1
    })
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-ui-shell', 'data-ui-theme'],
    })
  } catch {
    /* non-DOM environment — vars read once */
  }
}

function readVarPx(name: string, fallback: number): number {
  try {
    // Depend on the version so computed() re-evaluates on shell change.
    void version.value
    if (typeof document === 'undefined') return fallback
    const raw = getComputedStyle(document.documentElement).getPropertyValue(name)
    const px = parseFloat(raw)
    return Number.isFinite(px) ? px : fallback
  } catch {
    return fallback
  }
}

export function useShellInsets() {
  /** Top inset: menu-bar height (0 in plasma). */
  const top = computed(() => readVarPx('--ui-menubar-h', 0))
  /** Bottom inset: panel height (0 in macOS, Dock floats). */
  const bottom = computed(() => readVarPx('--ui-panel-h', 0))
  /** Titlebar height for snap/dock math (28 macOS / 30 plasma). */
  const titlebar = computed(() => readVarPx('--ui-titlebar-h', 28))
  return { top, bottom, titlebar }
}

export type ShellInsets = ReturnType<typeof useShellInsets>
