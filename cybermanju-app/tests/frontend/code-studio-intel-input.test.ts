// CyberManju OS — Code Intel inputs (tests/frontend/code-studio-intel-input.test.ts).
// Role: verify accessible names and literal technical input on mobile keyboards.
// Exports: none.
// Integrates: production CodeStudio markup, Vue compiler, Vue SSR, Vitest.
// Contracts: snippet/file/path fields have names and disable automatic text changes.
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import * as Vue from 'vue'
import { compile } from '@vue/compiler-dom'
import { renderToString } from '@vue/server-renderer'

const source = readFileSync('src/components/CodeStudio.vue', 'utf8')

// Render the real field markup without booting the entire editor/agent backend.
async function renderField(model: string, value: string) {
  const tag = source.match(new RegExp(`<(?:input|textarea)\\b[^>]*\\bv-model="${model}"[^>]*\\/>`))?.[0]
  if (!tag) throw new Error('Missing Code Intel input: ' + model)
  const { code } = compile(tag, { mode: 'function', prefixIdentifiers: true })
  const render = new Function('Vue', code)(Vue) as Vue.RenderFunction
  return renderToString(Vue.createSSRApp({
    render,
    setup: () => ({ [model]: value, runIntelPath: () => undefined }),
  }))
}

describe('Code Intel technical input', () => {
  it.each([
    ['intelFileName', 'Snippet file name', 'snippet.rs'],
    ['intelPaste', 'Code snippet', 'fn main() {}'],
    ['intelPath', 'Local file path', '/home/user/main.rs'],
  ])('names %s and opts out of mobile text changes', async (model, label, value) => {
    const html = await renderField(model, value)
    expect(html).toContain(`aria-label="${label}"`)
    expect(html).toContain('autocapitalize="off"')
    expect(html).toContain('autocorrect="off"')
    expect(html).toContain('autocomplete="off"')
    expect(html).toContain('spellcheck="false"')
    expect(html).toContain(value)
  })
})
