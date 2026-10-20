// CyberManju OS — UiModal accessibility (tests/frontend/ui-modal-a11y.test.ts).
// Role: verify dialog attributes and descriptions through the real Teleport root.
// Exports: none.
// Integrates: Vue SSR, SFC compiler, TypeScript, Vitest.
// Contracts: attrs target the dialog; subtitles get unique default descriptions.
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import * as Vue from 'vue'
import { compileScript, parse } from '@vue/compiler-sfc'
import { renderToString } from '@vue/server-renderer'
import { ModuleKind, ScriptTarget, transpileModule } from 'typescript'

const source = readFileSync('src/components/ui/UiModal.vue', 'utf8')
const { descriptor } = parse(source)
const compiled = compileScript(descriptor, { id: 'ui-modal-a11y', inlineTemplate: true })
const { outputText } = transpileModule(compiled.content, {
  compilerOptions: { module: ModuleKind.CommonJS, target: ScriptTarget.ES2022 },
})
const loaded = { exports: {} as Record<string, unknown> }
// Only leaf decoration and browser-only focus effects are stubbed. The modal's
// setup, template, defaults, attribute merging and Teleport render unchanged.
new Function('require', 'module', 'exports', outputText)((id: string) => {
  if (id === 'vue') return Vue
  if (id === '@/composables/useFocusTrap') return { useFocusTrap: () => undefined }
  if (id === '@/components/AppIcon.vue' || id === '@/components/ui/UiButton.vue') {
    return { default: { render: () => null } }
  }
  throw new Error('Unexpected UiModal dependency: ' + id)
}, loaded, loaded.exports)
const UiModal = loaded.exports.default as Vue.Component

type TeleportContext = { teleports?: Record<string, string> }
async function renderApp(app: Vue.App) {
  const warnings: string[] = []
  app.config.warnHandler = (message) => { warnings.push(message) }
  const context: TeleportContext = {}
  await renderToString(app, context)
  expect(warnings).toEqual([])
  return context.teleports?.body ?? ''
}
function renderModal(attrs: Record<string, unknown> = {}) {
  return renderApp(Vue.createSSRApp(UiModal, {
    visible: true, title: 'Dialog title', closable: false, ...attrs,
  }))
}
function dialogTag(html: string) {
  const tag = html.match(/<div\b[^>]*\brole="dialog"[^>]*>/)?.[0]
  expect(tag, 'the teleported dialog should exist').toBeDefined()
  return tag!
}

describe('UiModal dialog accessibility', () => {
  it('forwards caller attributes and layout to the dialog, not the backdrop', async () => {
    const html = await renderModal({
      id: 'import-dialog',
      'data-purpose': 'import',
      'aria-label': 'Custom dialog name',
      class: 'caller-dialog',
      style: { maxWidth: '42rem' },
    })
    const dialog = dialogTag(html)
    expect(dialog).toContain('id="import-dialog"')
    expect(dialog).toContain('data-purpose="import"')
    expect(dialog).toContain('aria-label="Custom dialog name"')
    expect(dialog).toContain('aria-modal="true"')
    expect(dialog).toContain('ui-modal--md')
    expect(dialog).toContain('caller-dialog')
    expect(dialog).toContain('max-width:42rem')
    const backdrop = html.match(/<div\b[^>]*class="ui-modal-layer"[^>]*>/)?.[0] ?? ''
    expect(backdrop).toContain('ui-modal-layer')
    expect(backdrop).not.toContain('import-dialog')
    expect(backdrop).not.toContain('caller-dialog')
    expect(backdrop).not.toContain('style=')
  })

  it('uses the title as the default name and the subtitle as its description', async () => {
    const html = await renderModal({ subtitle: 'This action cannot be undone.' })
    const subtitleId = html.match(/<p\b[^>]*id="([^"]+)"[^>]*>/)?.[1]
    expect(subtitleId).toBeTruthy()
    expect(dialogTag(html)).toContain('aria-label="Dialog title"')
    expect(dialogTag(html)).toContain(`aria-describedby="${subtitleId}"`)
    expect(html).toContain('This action cannot be undone.</p>')
  })

  it('does not reference a missing subtitle when none is provided', async () => {
    const html = await renderModal()
    expect(dialogTag(html)).not.toContain('aria-describedby=')
    expect(html).not.toContain('ui-modal__subtitle')
  })

  it('allows caller description and heading references to override defaults', async () => {
    const html = await renderModal({
      subtitle: 'Default help',
      'aria-describedby': 'custom-help',
      'aria-labelledby': 'custom-heading',
    })
    expect(dialogTag(html)).toContain('aria-describedby="custom-help"')
    expect(dialogTag(html)).toContain('aria-labelledby="custom-heading"')
    expect(html).toContain('Default help</p>')
  })

  it('assigns distinct description ids to multiple modal instances', async () => {
    const html = await renderApp(Vue.createSSRApp({
      render: () => Vue.h('div', [
        Vue.h(UiModal, { visible: true, title: 'First', subtitle: 'First help', closable: false }),
        Vue.h(UiModal, { visible: true, title: 'Second', subtitle: 'Second help', closable: false }),
      ]),
    }))
    const ids = [...html.matchAll(/<p\b[^>]*id="([^"]+)"[^>]*>/g)].map(match => match[1])
    expect(ids).toHaveLength(2)
    expect(new Set(ids).size).toBe(2)
    for (const id of ids) expect(html).toContain(`aria-describedby="${id}"`)
  })

  it('keeps the dialog absent while hidden', async () => {
    const html = await renderModal({ visible: false, subtitle: 'Hidden help', id: 'hidden-dialog' })
    expect(html).not.toContain('role="dialog"')
    expect(html).not.toContain('hidden-dialog')
    expect(html).not.toContain('Hidden help')
  })
})
