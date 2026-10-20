// CyberManju OS — Mobile setup.test (tests/frontend/mobile-setup.test.ts).
// Mobile first-run flow stays honest: the merged 5-step order, the legacy
// resume-step mapping (old 8-step builds), partition clamping/validity.
import { describe, expect, it } from 'vitest'
import {
  MOBILE_LEGACY_STEP_ALIAS,
  MOBILE_SETUP_STEPS,
  MOBILE_SETUP_STEP_LABELS,
  blankPartitionDraft,
  clampVaultSizeMb,
  mobileSetupStepIndex,
  nextMobileSetupStep,
  normalizePartitionDraft,
  partitionDraftValid,
  prevMobileSetupStep,
  resolveMobileSetupStep,
  signedProviderCards,
  type MobileSetupStep,
} from '@/utils/setupWizard'

describe('mobile setup flow', () => {
  it('walks welcome → connect → storage → personalize → done', () => {
    expect([...MOBILE_SETUP_STEPS]).toEqual([
      'welcome',
      'connect',
      'storage',
      'personalize',
      'done',
    ])
    let s: MobileSetupStep = 'welcome'
    for (const expected of MOBILE_SETUP_STEPS.slice(1)) {
      s = nextMobileSetupStep(s)
      expect(s).toBe(expected)
    }
    expect(nextMobileSetupStep('done')).toBe('done')
    expect(prevMobileSetupStep('welcome')).toBe('welcome')
    expect(mobileSetupStepIndex('storage')).toBe(3)
    expect(mobileSetupStepIndex('personalize')).toBe(4)
    expect(nextMobileSetupStep('personalize')).toBe('done')
    expect(MOBILE_SETUP_STEP_LABELS.storage).toBe('Vaults & storage')
    expect(MOBILE_SETUP_STEP_LABELS.personalize).toBe('Personalize')
  })

  it('folds every pre-merge 8-step step onto its single owner', () => {
    expect(MOBILE_LEGACY_STEP_ALIAS).toEqual({
      account: 'connect',
      providers: 'connect',
      vaults: 'storage',
      repos: 'storage',
      agent: 'personalize',
      appearance: 'personalize',
    })
    // A resume key from an older build resolves onto the merged step…
    expect(resolveMobileSetupStep('repos')).toBe('storage')
    expect(resolveMobileSetupStep('appearance')).toBe('personalize')
    expect(resolveMobileSetupStep('account')).toBe('connect')
    // …a live step passes through, junk and absent keys fall back to null.
    expect(resolveMobileSetupStep('storage')).toBe('storage')
    expect(resolveMobileSetupStep('bogus')).toBeNull()
    expect(resolveMobileSetupStep(null)).toBeNull()
    // Live steps never collide with the legacy alias map.
    for (const step of MOBILE_SETUP_STEPS) {
      expect(MOBILE_LEGACY_STEP_ALIAS[step]).toBeUndefined()
    }
  })

  it('keeps provider + disk creation on exactly one merged step each', () => {
    // Accounts (OAuth + provider rows) live in `connect`; every disk,
    // partition and repo path lives in `storage` — no step owns a concern
    // that another step also asks for.
    expect(MOBILE_SETUP_STEP_LABELS.connect).toContain('providers')
    expect(MOBILE_SETUP_STEP_LABELS.storage).toContain('Vaults')
    expect(MOBILE_SETUP_STEPS.filter(s => MOBILE_SETUP_STEP_LABELS[s].includes('Repos'))).toHaveLength(0)
  })

  it('clamps partition sizes into 64–8192 MB and trims names', () => {
    expect(normalizePartitionDraft(blankPartitionDraft('vault-1')).sizeMb).toBe(512)
    expect(normalizePartitionDraft({ ...blankPartitionDraft(), sizeMb: 4 }).sizeMb).toBe(64)
    expect(normalizePartitionDraft({ ...blankPartitionDraft(), sizeMb: 99999 }).sizeMb).toBe(8192)
    expect(partitionDraftValid(blankPartitionDraft('ab'))).toBe(true)
    expect(partitionDraftValid(blankPartitionDraft('x'))).toBe(false)
  })

  it('keeps the shared OAuth substep helper honest (desktop wizard cards)', () => {
    const cards = signedProviderCards(
      [{ provider: 'gitlab', name: 'Mobile User' }],
      [],
    )
    expect(cards).toHaveLength(1)
    expect(cards[0].backend).toBe('gitlab')
    expect(cards[0].accountName).toBe('Mobile User')
    expect(cards[0].hasConfig).toBe(false)
    expect(clampVaultSizeMb(99999)).toBe(8192)
  })
})
