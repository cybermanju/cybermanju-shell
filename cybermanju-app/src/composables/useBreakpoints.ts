// CyberManju OS — UseBreakpoints (src/composables/useBreakpoints.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useBreakpoints.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { computed, ref, watchEffect, type ComputedRef, type Ref } from 'vue'

/**
 * Reactive breakpoint helper for an arbitrary element (or the viewport when no
 * element is given). Uses ResizeObserver, so panels inside resizable OS
 * windows learn about their own width — not the screen's.
 */
export function useBreakpoints(
  target?: Ref<HTMLElement | null> | null,
  options?: {
    /** Lower bound → breakpoint name. Evaluated largest-first. */
    points?: Record<number, string>
    /** Returned when no point matches. */
    fallback?: string
  }
) {
  const points = options?.points ?? { 1280: 'xl', 900: 'lg', 640: 'md', 0: 'sm' }
  const fallback = options?.fallback ?? 'sm'
  const width = ref(0)

  const sorted = Object.keys(points)
    .map(Number)
    .sort((a, b) => b - a)

  const name: ComputedRef<string> = computed(() => {
    for (const bound of sorted) {
      if (width.value >= bound) return points[bound]
    }
    return fallback
  })

  /** Window-oriented size bucket used by the OS components. */
  const size: ComputedRef<'sm' | 'md' | 'lg'> = computed(() => {
    if (width.value < 560) return 'sm'
    if (width.value < 880) return 'md'
    return 'lg'
  })

  const isNarrow = computed(() => width.value > 0 && width.value < 560)
  const isWide = computed(() => width.value >= 880)

  let observer: ResizeObserver | null = null
  let detachResize: (() => void) | null = null

  function attach(el: HTMLElement | null) {
    observer?.disconnect()
    observer = null
    detachResize?.()
    detachResize = null
    if (!el) return
    const measure = () => {
      width.value = el.getBoundingClientRect().width
    }
    measure()
    if (typeof ResizeObserver !== 'undefined') {
      observer = new ResizeObserver(measure)
      observer.observe(el)
    } else if (typeof window !== 'undefined') {
      window.addEventListener('resize', measure)
      detachResize = () => window.removeEventListener('resize', measure)
    }
  }

  if (target) {
    watchEffect(() => attach(target.value))
  } else if (typeof window !== 'undefined') {
    width.value = window.innerWidth
    const onResize = () => {
      width.value = window.innerWidth
    }
    window.addEventListener('resize', onResize)
    detachResize = () => window.removeEventListener('resize', onResize)
  }

  return { width, name, size, isNarrow, isWide }
}
