// CyberManju OS — UseFocusTrap (src/composables/useFocusTrap.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useFocusTrap.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { ref, watch, onUnmounted, type Ref } from 'vue'

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'

export function useFocusTrap(containerRef: Ref<HTMLElement | null>, isActive: Ref<boolean>) {
  const previouslyFocused = ref<HTMLElement | null>(null)

  function getFocusableElements(): HTMLElement[] {
    if (!containerRef.value) return []
    return Array.from(containerRef.value.querySelectorAll(FOCUSABLE)) as HTMLElement[]
  }

  function focusFirst() {
    const elements = getFocusableElements()
    if (elements.length > 0) {
      elements[0].focus()
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key !== 'Tab') return
    const elements = getFocusableElements()
    if (elements.length === 0) return
    const first = elements[0]
    const last = elements[elements.length - 1]
    if (e.shiftKey) {
      if (document.activeElement === first) {
        e.preventDefault()
        last.focus()
      }
    } else {
      if (document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }

  watch(isActive, (active) => {
    if (active) {
      previouslyFocused.value = document.activeElement as HTMLElement
      setTimeout(() => focusFirst(), 50)
      document.addEventListener('keydown', handleKeydown)
    } else {
      document.removeEventListener('keydown', handleKeydown)
      if (previouslyFocused.value) {
        previouslyFocused.value.focus()
        previouslyFocused.value = null
      }
    }
  })

  onUnmounted(() => {
    document.removeEventListener('keydown', handleKeydown)
  })

  return { focusFirst }
}
