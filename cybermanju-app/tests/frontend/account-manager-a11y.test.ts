// CyberManju OS — Account Manager accessibility (tests/frontend/account-manager-a11y.test.ts).
// Role: regression tests for the provider-token visibility toggle.
// Exports: none.
// Integrates: Vitest, Node fs, src/components/AccountManagerPanel.vue.
// Contracts: the token toggle has an explicit state-aware accessible name.
import { describe, expect, it } from 'vitest'
import { readFileSync } from 'node:fs'

const accountManager = readFileSync('src/components/AccountManagerPanel.vue', 'utf8')

describe('Account Manager accessibility', () => {
  it('names the token visibility control according to its current action', () => {
    expect(accountManager).toMatch(/:aria-label="showWizToken \? 'Hide token' : 'Show token'"/)
    expect(accountManager).toContain(":type=\"showWizToken ? 'text' : 'password'\"")
  })
})
