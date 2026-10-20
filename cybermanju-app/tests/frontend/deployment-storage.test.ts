// CyberManju OS — Deployment storage.test (tests/frontend/deployment-storage.test.ts).
// Role: Vitest frontend suite.
// Exports: module root (see items below).
// Integrates: vitest.
// Contracts: none special (pure logic; errors bubble to caller).
import { describe, expect, it } from 'vitest'
import {
  createScopedStorage,
  installScopedStorage,
} from '../../src/utils/deploymentStorage'
import {
  DEVELOP_STORAGE_PREFIX,
  deploymentStorageEventKey,
} from '../../src/utils/deploymentScope'

function memoryStorage(): Storage {
  const values = new Map<string, string>()
  return {
    get length() { return values.size },
    clear() { values.clear() },
    getItem(key: string) { return values.get(String(key)) ?? null },
    key(index: number) { return [...values.keys()][index] ?? null },
    removeItem(key: string) { values.delete(String(key)) },
    setItem(key: string, value: string) { values.set(String(key), String(value)) },
  } as Storage
}

describe('Pages deployment storage isolation', () => {
  it('isolates keys, iteration, and clear while preserving production values', () => {
    const native = memoryStorage()
    native.setItem('cybermanju.theme', 'production')
    const develop = createScopedStorage(native, DEVELOP_STORAGE_PREFIX)

    develop.setItem('cybermanju.theme', 'develop')
    develop.setItem('agent.config', 'preview')

    expect(develop.getItem('cybermanju.theme')).toBe('develop')
    expect(native.getItem('cybermanju.theme')).toBe('production')
    expect(develop.length).toBe(2)
    expect(new Set([develop.key(0), develop.key(1)])).toEqual(
      new Set(['cybermanju.theme', 'agent.config']),
    )

    develop.clear()
    expect(develop.length).toBe(0)
    expect(native.getItem('cybermanju.theme')).toBe('production')
  })

  it('installs a scoped localStorage view without exposing unscoped keys', () => {
    const native = memoryStorage()
    native.setItem('main-only', 'production')
    const context = { localStorage: native }

    installScopedStorage(context, DEVELOP_STORAGE_PREFIX)
    context.localStorage.setItem('preview-only', 'develop')

    expect(context.localStorage.getItem('main-only')).toBeNull()
    expect(context.localStorage.getItem('preview-only')).toBe('develop')
    expect(native.getItem('main-only')).toBe('production')
    expect(native.getItem(`${DEVELOP_STORAGE_PREFIX}preview-only`)).toBe('develop')
  })

  it('normalizes only storage events belonging to the active deployment', () => {
    expect(
      deploymentStorageEventKey(`${DEVELOP_STORAGE_PREFIX}agent.config`, true),
    ).toBe('agent.config')
    expect(deploymentStorageEventKey('agent.config', true)).toBeNull()
    expect(
      deploymentStorageEventKey(`${DEVELOP_STORAGE_PREFIX}agent.config`, false),
    ).toBeNull()
    expect(deploymentStorageEventKey('agent.config', false)).toBe('agent.config')
  })
})
