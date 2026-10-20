// CyberManju OS — File Manager dialog accessibility (tests/frontend/file-manager-dialog-a11y.test.ts).
// Role: regression tests for screen-reader names in file-management dialogs.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/FileManager.vue.
// Contracts: folder creation and rename inputs have persistent accessible names.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const fileManager = readFileSync('src/components/FileManager.vue', 'utf8')

describe('File Manager dialog accessibility', () => {
  it('names the new-folder field independently of its placeholder', () => {
    expect(fileManager).toMatch(/<input ref="folderRef"[^>]*aria-label="Folder name"[^>]*>/)
  })

  it('names the rename field', () => {
    expect(fileManager).toMatch(/<input ref="renameRef"[^>]*aria-label="New name"[^>]*>/)
  })
})
