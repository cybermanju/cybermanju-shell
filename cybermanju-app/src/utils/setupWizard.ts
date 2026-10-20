// CyberManju OS — first-run setup wizard state (pure, unit-tested).
//
// The wizards live in `SetupWizard.vue` (desktop) and
// `MobileSetupWizard.vue` (phone — the merged 5-step flow): vault folder +
// cloud + disks + agent AI + appearance, every step skippable. This module
// owns the browser-free parts: the "seen" flag in localStorage and both
// step orders, so they stay honest without a browser.

export const SETUP_SEEN_KEY = 'cybermanju.setupSeen.v1'
export const SETUP_RESUME_STEP_KEY = 'cybermanju.setup.resumeStep'

export const SETUP_STEPS = ['welcome', 'vault', 'cloud', 'disks', 'agent', 'appearance', 'done'] as const

export type SetupStep = (typeof SETUP_STEPS)[number]

/** Legacy steps from older wizards (e.g. the removed standalone `sync` step). */
const LEGACY_STEP_ALIAS: Record<string, SetupStep> = {
  sync: 'vault',
}

/** Save one validated step to resume after visiting Accounts. */
export function saveSetupResumeStep(step: SetupStep): void {
  try {
    if (typeof localStorage === 'undefined') return
    localStorage.setItem(SETUP_RESUME_STEP_KEY, step)
  } catch {
    // Private mode — the wizard still opens at Welcome.
  }
}

/** Read and consume a pending continuation exactly once. */
export function takeSetupResumeStep(): SetupStep | null {
  try {
    if (typeof localStorage === 'undefined') return null
    const saved = localStorage.getItem(SETUP_RESUME_STEP_KEY)
    localStorage.removeItem(SETUP_RESUME_STEP_KEY)
    const direct = SETUP_STEPS.find(step => step === saved) ?? null
    if (direct) return direct
    // Older wizards saved `sync` (now merged into `vault`) — resume there.
    if (typeof saved === 'string' && LEGACY_STEP_ALIAS[saved]) return LEGACY_STEP_ALIAS[saved]
    return null
  } catch {
    return null
  }
}

export function clearSetupResumeStep(): void {
  try {
    if (typeof localStorage !== 'undefined') localStorage.removeItem(SETUP_RESUME_STEP_KEY)
  } catch {
    // Nothing stored — nothing to clear.
  }
}

export const SETUP_STEP_LABELS: Record<SetupStep, string> = {
  welcome: 'Welcome',
  vault: 'Vault folder',
  cloud: 'Accounts',
  disks: 'Disks',
  agent: 'Agent (optional)',
  appearance: 'Appearance',
  done: 'Done',
}

/** True once the user finished or explicitly skipped the wizard. */
export function setupSeen(): boolean {
  try {
    if (typeof localStorage === 'undefined') return false
    return localStorage.getItem(SETUP_SEEN_KEY) === '1'
  } catch {
    return false
  }
}

/** Remember the choice (finish or skip-all). Never throws. */
export function markSetupSeen(): void {
  try {
    if (typeof localStorage === 'undefined') return
    localStorage.setItem(SETUP_SEEN_KEY, '1')
  } catch {
    // Private mode — the wizard simply returns next launch.
  }
}

/** Forget the choice (used by tests and the Help-menu re-run path). */
export function clearSetupSeen(): void {
  try {
    if (typeof localStorage === 'undefined') return
    localStorage.removeItem(SETUP_SEEN_KEY)
  } catch {
    // Nothing stored — nothing to clear.
  }
}

export function nextSetupStep(step: SetupStep): SetupStep {
  const i = SETUP_STEPS.indexOf(step)
  return SETUP_STEPS[Math.min(SETUP_STEPS.length - 1, i + 1)]
}

export function prevSetupStep(step: SetupStep): SetupStep {
  const i = SETUP_STEPS.indexOf(step)
  return SETUP_STEPS[Math.max(0, i - 1)]
}

/** 1-based position for the progress dots. */
export function setupStepIndex(step: SetupStep): number {
  return SETUP_STEPS.indexOf(step) + 1
}

// ── Mobile first-run (Android) ─────────────────────────────────────
// Desktop keeps the steps above (pinned by tests). Mobile ships a merged
// 5-step flow where every concern appears exactly once — the old 8-step
// wizard asked for providers, disks and repos in three different places:
//   welcome → connect (OAuth + provider rows, one place) →
//   storage (every disk / partition / repo creation path, one list) →
//   personalize (appearance + optional agent) → done.
// Pure + unit-tested.

export const MOBILE_SETUP_STEPS = [
  'welcome',
  'connect',
  'storage',
  'personalize',
  'done',
] as const

export type MobileSetupStep = (typeof MOBILE_SETUP_STEPS)[number]

export const MOBILE_SETUP_STEP_LABELS: Record<MobileSetupStep, string> = {
  welcome: 'Welcome',
  connect: 'Accounts & providers',
  storage: 'Vaults & storage',
  personalize: 'Personalize',
  done: 'Done',
}

/**
 * Steps written by the pre-merge 8-step wizard, mapped onto the merged
 * step that now owns them — resume keys from older builds stay valid.
 */
export const MOBILE_LEGACY_STEP_ALIAS: Record<string, MobileSetupStep> = {
  account: 'connect',
  providers: 'connect',
  vaults: 'storage',
  repos: 'storage',
  agent: 'personalize',
  appearance: 'personalize',
}

/** Validate a saved step; legacy names resolve onto their merged step. */
export function resolveMobileSetupStep(saved: string | null): MobileSetupStep | null {
  if (!saved) return null
  if ((MOBILE_SETUP_STEPS as readonly string[]).includes(saved)) return saved as MobileSetupStep
  return MOBILE_LEGACY_STEP_ALIAS[saved] ?? null
}

export function mobileSetupStepIndex(step: MobileSetupStep): number {
  return MOBILE_SETUP_STEPS.indexOf(step) + 1
}

export function nextMobileSetupStep(step: MobileSetupStep): MobileSetupStep {
  const i = MOBILE_SETUP_STEPS.indexOf(step)
  return MOBILE_SETUP_STEPS[Math.min(MOBILE_SETUP_STEPS.length - 1, i + 1)]
}

export function prevMobileSetupStep(step: MobileSetupStep): MobileSetupStep {
  const i = MOBILE_SETUP_STEPS.indexOf(step)
  return MOBILE_SETUP_STEPS[Math.max(0, i - 1)]
}

export interface VaultPartitionDraft {
  name: string
  sizeMb: number
  passphrase: string
  /** sync config id, or '' = local app-private vault */
  configId: string
}

export const PARTITION_MIN_MB = 64
export const PARTITION_MAX_MB = 8192

export function blankPartitionDraft(name = ''): VaultPartitionDraft {
  return { name, sizeMb: 512, passphrase: '', configId: '' }
}

/** Clamp + sanitize a partition row (pure, so the wizard + tests agree). */
export function normalizePartitionDraft(d: VaultPartitionDraft): VaultPartitionDraft {
  const sizeMb = Math.max(
    PARTITION_MIN_MB,
    Math.min(PARTITION_MAX_MB, Math.round(Number(d.sizeMb) || 512)),
  )
  return {
    name: d.name.trim().slice(0, 64),
    sizeMb,
    passphrase: d.passphrase,
    configId: d.configId,
  }
}

export function partitionDraftValid(d: VaultPartitionDraft): boolean {
  const n = normalizePartitionDraft(d)
  return n.name.length >= 2 && n.sizeMb >= PARTITION_MIN_MB && n.sizeMb <= PARTITION_MAX_MB
}

// ── OAuth substep: folder + `.cybermanju` size per logged provider ───
// After a 200-OK sign-in the desktop wizard (`SetupWizard.vue`) shows one
// card per logged provider: a folder input (Drive folder / repo name seed)
// + a `.cybermanju` size input + a create button. Mobile reaches the same
// disk through a one-tap prefilled vault row in its merged `storage` step
// (chips per connected provider), so the shared helper below still feeds
// the desktop cards and the tests. Pure so both agree.

export type SignedProviderBackend = 'googleDrive' | 'github' | 'gitlab'

export interface OAuthAccountRef {
  provider: string
  name?: string
  email?: string
}

export interface ProviderConfigRef {
  backendType: string
}

export interface SignedProviderCard {
  backend: SignedProviderBackend
  label: string
  logo: string
  accountName: string
  hasConfig: boolean
}

export const DEFAULT_VAULT_FOLDER = 'cybermanju-vault'
export const DEFAULT_VAULT_SIZE_MB = 512

const OAUTH_BACKENDS: Array<{ slug: string; backend: SignedProviderBackend; label: string; logo: string }> = [
  { slug: 'google', backend: 'googleDrive', label: 'Google Drive', logo: 'google' },
  { slug: 'github', backend: 'github', label: 'GitHub', logo: 'github' },
  { slug: 'gitlab', backend: 'gitlab', label: 'GitLab', logo: 'gitlab' },
]

/** Clamp a `.cybermanju` size into the 64–8192 MB budget. */
export function clampVaultSizeMb(v: unknown): number {
  return Math.max(PARTITION_MIN_MB, Math.min(PARTITION_MAX_MB, Math.round(Number(v) || DEFAULT_VAULT_SIZE_MB)))
}

/**
 * One card per logged OAuth provider (deduped by backend, first account
 * wins). `accounts` is the remembered list + current identity;
 * `configs` marks which providers already have a saved connection.
 */
export function signedProviderCards(
  accounts: OAuthAccountRef[],
  configs: ProviderConfigRef[],
): SignedProviderCard[] {
  const out: SignedProviderCard[] = []
  for (const meta of OAUTH_BACKENDS) {
    const acc = accounts.find(a => a.provider === meta.slug)
    if (!acc) continue
    out.push({
      backend: meta.backend,
      label: meta.label,
      logo: meta.logo,
      accountName: (acc.name || acc.email || meta.label).trim().slice(0, 80) || meta.label,
      hasConfig: configs.some(c => c.backendType === meta.backend),
    })
  }
  return out
}
