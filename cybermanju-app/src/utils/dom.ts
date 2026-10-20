// CyberManju OS — Dom (src/utils/dom.ts).
// Role: pure TS helpers (unit-tested, transport-agnostic).
// Exports: isEditableTarget.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
type EditableLike = {
  tagName?: string
  isContentEditable?: boolean
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (!target || typeof (target as EditableLike).tagName !== 'string') return false
  const el = target as EditableLike
  if (el.isContentEditable) return true
  const tag = (el.tagName || '').toUpperCase()
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
}
