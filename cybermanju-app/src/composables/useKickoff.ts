// CyberManju OS — UseKickoff (src/composables/useKickoff.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: useKickoff.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { ref } from 'vue'

/**
 * Kickoff launcher open state (plasma shell). Module-level singleton so the
 * panel launcher button, the Alt+F1 shortcut and the menu itself share one
 * source of truth without prop-drilling through DesktopShell.
 */
const kickoffOpen = ref(false)

export function useKickoff() {
  function open() {
    kickoffOpen.value = true
  }
  function close() {
    kickoffOpen.value = false
  }
  function toggle() {
    kickoffOpen.value = !kickoffOpen.value
  }
  return { kickoffOpen, open, close, toggle }
}
