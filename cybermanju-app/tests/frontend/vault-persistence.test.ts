// CyberManju OS — vault persistence tests (tests/frontend/vault-persistence.test.ts).
// Role: protect the vault's durable-storage and explicit legacy-fallback contracts.
// Exports: none (Vitest suite).
// Integrates: useVault and the WASM database dispatcher boundary.
// Contracts: real storage failures reject; only explicit unsupported KV ops use localStorage.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const { dispatch } = vi.hoisted(() => ({ dispatch: vi.fn() }))
vi.mock('@/composables/useWasmBackend', () => ({ wasmDbDispatch: dispatch }))

type Row = { key: string; value: string }
class TestStorage {
  private rows = new Map<string, string>()
  failGet = false
  failSet = false
  failRemove = false
  failKey = false

  get length(): number { return this.rows.size }
  key(index: number): string | null {
    if (this.failKey) throw new Error('localStorage enumeration failed')
    return Array.from(this.rows.keys())[index] ?? null
  }
  getItem(key: string): string | null {
    if (this.failGet) throw new Error('localStorage read failed')
    return this.rows.get(key) ?? null
  }
  setItem(key: string, value: string): void {
    if (this.failSet) throw new Error('QuotaExceededError')
    this.rows.set(key, value)
  }
  removeItem(key: string): void {
    if (this.failRemove) throw new Error('localStorage removal failed')
    this.rows.delete(key)
  }
  seed(key: string, value: string): void { this.rows.set(key, value) }
  entries(): Array<[string, string]> { return Array.from(this.rows.entries()) }
}

let storage: TestStorage
let kv: Map<string, string>
let vault: typeof import('@/composables/useVault')

beforeEach(async () => {
  vi.resetModules()
  dispatch.mockReset()
  storage = new TestStorage()
  kv = new Map()
  vi.stubGlobal('localStorage', storage)
  dispatch.mockImplementation(async (op: string, args: Record<string, unknown>) => {
    const key = String(args.key ?? '')
    if (op === 'kv.get') return kv.has(key) ? { key, value: kv.get(key) } : null
    if (op === 'kv.set') {
      kv.set(key, String(args.value))
      return null
    }
    if (op === 'kv.delete') {
      kv.delete(key)
      return null
    }
    if (op === 'kv.list') {
      return Array.from(kv, ([rowKey, value]): Row => ({ key: rowKey, value }))
        .filter((row) => row.key.startsWith(String(args.prefix ?? '')))
        .map(({ key: rowKey, value }) => ({ key: rowKey, bytes: value.length }))
    }
    throw new Error(`unexpected test op ${op}`)
  })
  vault = await import('@/composables/useVault')
})

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetModules()
})

describe('vault persistence', () => {
  it('fails closed on localStorage quota and read failures in explicit legacy mode', async () => {
    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.set'))
    storage.failSet = true
    await expect(vault.vaultSet('prefs', 'value')).rejects.toThrow('QuotaExceededError')

    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.get'))
    storage.failGet = true
    await expect(vault.vaultGet('prefs')).rejects.toThrow('localStorage read failed')
  })

  it('does not report success or shadow a transient KV write failure in localStorage', async () => {
    dispatch.mockRejectedValue(new Error('disk_full: redb write failed'))
    await expect(vault.vaultSet('secret:token', 'value')).rejects.toThrow('disk_full: redb write failed')
    expect(storage.entries()).toEqual([])
    expect(vault.vaultBackend()).toBe('unknown')
  })

  it('does not convert a KV read failure into an absent value or localStorage value', async () => {
    storage.seed('cybermanju.vault.secret:token', 'shadow')
    dispatch.mockRejectedValue(new Error('database worker error'))
    await expect(vault.vaultGet('secret:token')).rejects.toThrow('database worker error')
  })

  it('uses localStorage only for an explicit unsupported legacy KV response', async () => {
    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.set'))
    await vault.vaultSet('config:legacy', 'stored')
    expect(storage.getItem('cybermanju.vault.config:legacy')).toBe('stored')
    expect(vault.vaultBackend()).toBe('localStorage')

    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.get'))
    await expect(vault.vaultGet('config:legacy')).resolves.toBe('stored')
  })

  it('preserves empty-string values in the localStorage fallback', async () => {
    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.set'))
    await vault.vaultSet('content:empty', '')
    expect(storage.getItem('cybermanju.vault.content:empty')).toBe('')

    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.get'))
    await expect(vault.vaultGet('content:empty')).resolves.toBe('')
    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.list'))
    await expect(vault.vaultList('content:')).resolves.toEqual([{ key: 'content:empty', bytes: 0 }])
  })

  it('does not fail a durable KV write solely because stale-shadow cleanup fails', async () => {
    storage.seed('cybermanju.vault.config:theme', 'old')
    storage.failRemove = true
    await expect(vault.vaultSet('config:theme', 'new')).resolves.toBeUndefined()
    expect(kv.get('config:theme')).toBe('new')
    await expect(vault.vaultGet('config:theme')).resolves.toBe('new')
  })

  it('rejects a failed migration write and preserves localStorage sources without a marker', async () => {
    storage.seed('cybermanju.vault.secret:legacy', 'original')
    dispatch.mockImplementation(async (op: string, args: Record<string, unknown>) => {
      if (op === 'kv.get') return null
      if (op === 'kv.set') throw new Error('disk_full: migration write failed')
      throw new Error(`unexpected test op ${op}`)
    })
    await expect(vault.migrateVaultFromLocalStorage()).rejects.toThrow('disk_full: migration write failed')
    expect(storage.getItem('cybermanju.vault.secret:legacy')).toBe('original')
    expect(kv.has('config:vault.migrated')).toBe(false)
  })

  it('clears legacy shadows only after migration sees existing KV values and persists its marker', async () => {
    kv.set('secret:present', 'authoritative')
    storage.seed('cybermanju.vault.secret:present', 'stale shadow')
    storage.seed('cybermanju.vault.config:missing', 'copied value')

    await expect(vault.migrateVaultFromLocalStorage()).resolves.toBe(1)

    expect(kv.get('secret:present')).toBe('authoritative')
    expect(kv.get('config:missing')).toBe('copied value')
    expect(storage.getItem('cybermanju.vault.secret:present')).toBeNull()
    expect(storage.getItem('cybermanju.vault.config:missing')).toBeNull()
    expect(kv.has('config:vault.migrated')).toBe(true)
  })

  it('removes stale shadows left by an older migration after its marker already exists', async () => {
    kv.set('config:vault.migrated', 'old marker')
    kv.set('secret:token', 'durable value')
    storage.seed('cybermanju.vault.secret:token', 'stale shadow')
    storage.seed('cybermanju.vault.secret:orphan', 'no durable KV value')

    await expect(vault.migrateVaultFromLocalStorage()).resolves.toBe(0)

    expect(storage.getItem('cybermanju.vault.secret:token')).toBeNull()
    expect(storage.getItem('cybermanju.vault.secret:orphan')).toBe('no durable KV value')
  })

  it('leaves legacy localStorage untouched when KV is truly unsupported during migration', async () => {
    storage.seed('cybermanju.vault.config:legacy', '')
    dispatch.mockRejectedValue(new Error('unsupported: unknown demo-db op kv.get'))
    await expect(vault.migrateVaultFromLocalStorage()).resolves.toBe(0)
    expect(storage.getItem('cybermanju.vault.config:legacy')).toBe('')
  })
})
