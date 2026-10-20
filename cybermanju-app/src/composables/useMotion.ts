// CyberManju OS — UseMotion (src/composables/useMotion.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useMotion.
// Integrates: vue, @/….
// Contracts: none special (pure logic; errors bubble to caller).
import { computed, ref, watchEffect, type ComputedRef } from 'vue'
import { useTheme } from '@/composables/useTheme'

const mediaQuery =
  typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-reduced-motion: reduce)')
    : null

const systemReduced = ref(Boolean(mediaQuery?.matches))

if (mediaQuery) {
  const onChange = (e: MediaQueryListEvent) => {
    systemReduced.value = e.matches
  }
  if (typeof mediaQuery.addEventListener === 'function') {
    mediaQuery.addEventListener('change', onChange)
  } else {
    // Safari < 14
    ;(mediaQuery as unknown as { addListener: (fn: (e: MediaQueryListEvent) => void) => void }).addListener(
      onChange
    )
  }
}

/**
 * Motion policy shared by every animated component in the OS.
 *
 * - `auto`    → honour the OS-level `prefers-reduced-motion` setting
 * - `full`    → always animate
 * - `reduced` → never animate
 *
 * `duration(ms)` collapses to `0` when motion is off so callers keep one code
 * path, and `data-ui-motion="reduced"` on `<html>` lets CSS short-circuit too.
 */
export function useMotion() {
  const theme = useTheme()

  const reduced: ComputedRef<boolean> = computed(() => {
    if (theme.settings.motion === 'reduced') return true
    if (theme.settings.motion === 'full') return false
    return systemReduced.value
  })

  function duration(ms: number): number {
    return reduced.value ? 0 : ms
  }

  function durationSec(ms: number): string {
    return `${duration(ms) / 1000}s`
  }

  const durationCss: ComputedRef<string> = computed(() => (reduced.value ? '0s' : 'var(--ui-dur)'))

  const enterTransition = computed(() =>
    reduced.value
      ? 'none'
      : 'opacity var(--ui-dur) var(--ui-ease-out), transform var(--ui-dur-slow) var(--ui-ease-out)'
  )

  watchEffect(() => {
    if (typeof document === 'undefined') return
    document.documentElement.dataset.uiMotion = reduced.value ? 'reduced' : 'full'
  })

  return { reduced, duration, durationSec, durationCss, enterTransition }
}
