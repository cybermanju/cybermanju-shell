// CyberManju OS — UiSelect accessibility (tests/frontend/ui-select-a11y.test.ts).
// Role: regression test for accessible attributes on the native select control.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/ui/UiSelect.vue.
// Contracts: caller-provided aria-label/title attributes reach the native select.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const uiSelect = readFileSync('src/components/ui/UiSelect.vue', 'utf8')

describe('UiSelect accessibility', () => {
  it('forwards caller attributes to the native select instead of the label wrapper', () => {
    const nativeSelect = uiSelect.match(/<select\b[\s\S]*?<\/select>/)?.[0]
    expect(nativeSelect).toBeDefined()
    expect(nativeSelect).toContain('v-bind="$attrs"')
    expect(uiSelect).toMatch(/defineOptions\(\{\s*inheritAttrs:\s*false\s*\}\)/)
  })
})
