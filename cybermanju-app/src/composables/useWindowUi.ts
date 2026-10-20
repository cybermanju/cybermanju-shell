// CyberManju OS — UseWindowUi (src/composables/useWindowUi.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: WindowUiContext, WindowUiKey, createWindowUi, provideWindowUi, useWindowUi, peekWindowUi.
// Integrates: @/….
// Contracts: none special (pure logic; errors bubble to caller).
import {
  computed,
  inject,
  provide,
  ref,
  type ComputedRef,
  type InjectionKey,
  type Ref,
} from 'vue'
import type { PanelType } from '@/types'
import type { Density } from '@/ui/tokens'
import { useMotion } from '@/composables/useMotion'
import { useTheme } from '@/composables/useTheme'

/**
 * Everything a component needs to know about *where* it currently lives inside
 * the virtual OS: which window, how wide that window is, whether it has focus
 * and which density/motion policy applies. `AppWindow` provides it, so every
 * widget rendered inside a window can adapt to that window's geometry instead
 * of the viewport's.
 */
export interface WindowUiContext {
  /** Stable id of the hosting window (`win-1`, …) or `'root'` for overlays. */
  id: string
  panelType: PanelType | null
  title: string
  icon: string
  /** Live geometry of the hosting surface (CSS pixels). */
  width: Ref<number>
  height: Ref<number>
  focused: Ref<boolean>
  /** `'sm'` < 560px · `'md'` < 880px · `'lg'` ≥ 880px. */
  breakpoint: ComputedRef<'sm' | 'md' | 'lg'>
  isNarrow: ComputedRef<boolean>
  isWide: ComputedRef<boolean>
  /** Effective density — compact in narrow windows or compact theme mode. */
  density: ComputedRef<Density>
  /** True when layout should collapse to its compact variant. */
  compact: ComputedRef<boolean>
  /** True when motion should be suppressed. */
  reducedMotion: ComputedRef<boolean>
  /** Register a live element to measure (the window root, in practice). */
  observe: (el: HTMLElement | null) => void
}

export const WindowUiKey: InjectionKey<WindowUiContext> = Symbol('cybermanju:window-ui')

export function createWindowUi(init: {
  id: string
  panelType?: PanelType | null
  title?: string
  icon?: string
  width?: Ref<number>
  height?: Ref<number>
  focused?: Ref<boolean>
}): WindowUiContext {
  const theme = useTheme()
  const motion = useMotion()
  const el = ref<HTMLElement | null>(null)
  const localWidth = ref(init.width?.value ?? 0)
  const localHeight = ref(init.height?.value ?? 0)

  // Fall back to the supplied width until a real element is measured, so
  // children never observe a zero-sized window on first paint.
  const width = computed(
    () => localWidth.value || init.width?.value || (typeof window !== 'undefined' ? window.innerWidth : 1200)
  )
  const height = computed(
    () => localHeight.value || init.height?.value || (typeof window !== 'undefined' ? window.innerHeight : 800)
  )

  let observer: ResizeObserver | null = null
  function observe(target: HTMLElement | null) {
    el.value = target
    observer?.disconnect()
    observer = null
    if (!target) return
    const measure = () => {
      const rect = target.getBoundingClientRect()
      localWidth.value = rect.width
      localHeight.value = rect.height
    }
    measure()
    if (typeof ResizeObserver !== 'undefined') {
      observer = new ResizeObserver(measure)
      observer.observe(target)
    }
  }

  const focused = init.focused ?? ref(true)

  const breakpoint = computed<'sm' | 'md' | 'lg'>(() => {
    const w = width.value || (typeof window !== 'undefined' ? window.innerWidth : 1200)
    if (w < 560) return 'sm'
    if (w < 880) return 'md'
    return 'lg'
  })

  const density = computed<Density>(() => {
    if (theme.settings.density === 'compact') return 'compact'
    // Windows squeeze themselves before the whole OS does.
    return breakpoint.value === 'sm' ? 'compact' : 'comfortable'
  })

  const isNarrow = computed(() => breakpoint.value === 'sm')
  const isWide = computed(() => breakpoint.value === 'lg')
  const compact = computed(() => density.value === 'compact' || breakpoint.value === 'sm')

  return {
    id: init.id,
    panelType: init.panelType ?? null,
    title: init.title ?? '',
    icon: init.icon ?? 'solar:grid-2x2-bold',
    width,
    height,
    focused,
    breakpoint,
    isNarrow,
    isWide,
    density,
    compact,
    reducedMotion: motion.reduced,
    observe,
  }
}

let viewportContext: WindowUiContext | null = null

/** Context used by components rendered outside any window (dialogs, menus). */
function getViewportContext(): WindowUiContext {
  if (!viewportContext) {
    const width = ref(typeof window !== 'undefined' ? window.innerWidth : 1200)
    const height = ref(typeof window !== 'undefined' ? window.innerHeight : 800)
    if (typeof window !== 'undefined') {
      window.addEventListener('resize', () => {
        width.value = window.innerWidth
        height.value = window.innerHeight
      })
    }
    viewportContext = createWindowUi({ id: 'root', width, height, focused: ref(true) })
  }
  return viewportContext
}

export function provideWindowUi(ctx: WindowUiContext) {
  provide(WindowUiKey, ctx)
  return ctx
}

/**
 * Window-aware context with a viewport fallback, so a component can be used
 * either inside an OS window or as a free-floating overlay without changes.
 */
export function useWindowUi(): WindowUiContext {
  return inject(WindowUiKey, getViewportContext())
}

/** Non-injecting probe — `null` when the component is outside any window. */
export function peekWindowUi(): WindowUiContext | null {
  return inject(WindowUiKey, null)
}
