// CyberManju OS — Agent matchers.test (tests/frontend/agent-matchers.test.ts).
// Agent matcher parity (mirrors Rust `config::{match_wildcard,match_glob}`
// and `decide`). If these drift from the native semantics, the same ruleset
// allows a tool in one transport and denies it in another.
import { describe, expect, it } from 'vitest'
import { compileGrep, decideLocalTool, matchGlob, matchWildcard } from '../../src/composables/useAgent'
import type { PermissionRuleset } from '../../src/types'

describe('matchWildcard', () => {
  it('matches shell-style patterns', () => {
    expect(matchWildcard('*', 'anything')).toBe(true)
    expect(matchWildcard('git *', 'git status')).toBe(true)
    expect(matchWildcard('git *', 'git')).toBe(false)
    expect(matchWildcard('rm ?', 'rm x')).toBe(true)
    expect(matchWildcard('rm ?', 'rm xy')).toBe(false)
  })
})

describe('matchGlob', () => {
  it('keeps * inside segments and lets ** cross them', () => {
    expect(matchGlob('*.rs', 'main.rs')).toBe(true)
    expect(matchGlob('*.rs', 'src/main.rs')).toBe(false)
    expect(matchGlob('src/**/*.rs', 'src/a/b/main.rs')).toBe(true)
    expect(matchGlob('**/Cargo.toml', 'Cargo.toml')).toBe(true)
    expect(matchGlob('**/Cargo.toml', 'Cargo.lock')).toBe(false)
  })
})

describe('compileGrep', () => {
  it('prefers regex, falls back to literal', () => {
    const re = compileGrep('fn\\s+\\w+')
    expect(re.regex).toBe(true)
    expect(re.test('fn alpha() {}')).toBe(true)
    const lit = compileGrep('a(b')
    expect(lit.regex).toBe(false)
    expect(lit.test('has a(b inside')).toBe(true)
  })
})

describe('decideLocalTool', () => {
  const rules: PermissionRuleset = {
    default: 'ask',
    rules: { bash: [['*', 'ask'], ['git *', 'allow']] },
  }
  it('applies last-match-wins granular rules', () => {
    expect(decideLocalTool(rules, 'build', 'bash', { command: 'git status' }).kind).toBe('allow')
    expect(decideLocalTool(rules, 'build', 'bash', { command: 'rm -rf /' }).kind).toBe('ask')
  })
  it('keeps plan read-only whatever the rules say', () => {
    const permissive: PermissionRuleset = { default: 'allow', rules: {} }
    expect(decideLocalTool(permissive, 'plan', 'write', { path: 'a' }).kind).toBe('deny')
    expect(decideLocalTool(permissive, 'plan', 'read', { path: 'a' }).kind).toBe('allow')
  })
})
