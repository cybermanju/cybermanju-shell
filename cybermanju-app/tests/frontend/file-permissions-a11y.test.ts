// CyberManju OS — File permissions accessibility (tests/frontend/file-permissions-a11y.test.ts).
// Role: regression test for screen-reader names in the file permissions panel.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/FilePermissionsPanel.vue.
// Contracts: the share URL and grant-access selectors have meaningful accessible names.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const panel = readFileSync('src/components/FilePermissionsPanel.vue', 'utf8')

describe('file permissions accessibility', () => {
  it('labels the read-only share link with its visible section heading', () => {
    expect(panel).toMatch(/<h3 id="share-link-heading"[^>]*>[\s\S]*?SHARE LINK<\/h3>/)
    expect(panel).toMatch(/<input[^>]*aria-labelledby="share-link-heading"[^>]*readonly/)
  })

  it('names the user and access-level selectors', () => {
    expect(panel).toMatch(/<select aria-label="User to grant access"[^>]*>/)
    expect(panel).toMatch(/<select aria-label="Access level"[^>]*>/)
  })
})
