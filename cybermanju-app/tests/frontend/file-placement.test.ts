// CyberManju OS — File placement.test (tests/frontend/file-placement.test.ts).
// Unified-disk placement — home choice, mirrors, moves, key holder.
import { describe, expect, it } from 'vitest'
import {
  chooseHome,
  designateKeyHolder,
  enabledProviders,
  mirrorTargets,
  planMove,
} from '../../src/utils/filePlacement'
import type { SyncConfig } from '../../src/types'
import { syncConfigDefaults } from '../../src/utils/providers'

function cfg(id: string, patch: Partial<SyncConfig> = {}): SyncConfig {
  return {
    ...syncConfigDefaults(),
    id,
    backendType: 'local',
    name: id,
    createdAt: '',
    updatedAt: '',
    ...patch,
  } as SyncConfig
}

describe('filePlacement', () => {
  it('prefers the chosen home when still enabled', () => {
    const configs = [cfg('b'), cfg('a'), cfg('c', { enabled: false })]
    expect(enabledProviders(configs).map((c) => c.id)).toEqual(['a', 'b'])
    expect(chooseHome(configs, 'b')?.id).toBe('b')
    expect(chooseHome(configs, 'c')?.id).toBe('a')
    expect(chooseHome([], 'a')).toBeNull()
  })

  it('mirror is opt-in, never default', () => {
    const configs = [cfg('a'), cfg('b', { mirror: true }), cfg('c')]
    expect(mirrorTargets(configs, 'a').map((c) => c.id)).toEqual(['b'])
    expect(mirrorTargets([cfg('a')], 'a')).toEqual([])
  })

  it('plans explicit A→B moves, noop when already home', () => {
    expect(planMove('f', 'a', 'a').noop).toBe(true)
    const m = planMove('f', 'a', 'b')
    expect(m.noop).toBe(false)
    expect(m.toConfigId).toBe('b')
  })

  it('holds the single key-holder invariant', () => {
    const out = designateKeyHolder([cfg('a', { keyHolder: true }), cfg('b')], 'b')
    expect(out.filter((c) => c.keyHolder).map((c) => c.id)).toEqual(['b'])
  })
})
