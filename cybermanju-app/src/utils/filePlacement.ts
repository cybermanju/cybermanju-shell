// CyberManju OS — unified-disk placement (pure, unit-tested).
//
// Single-copy home is the default; duplicate-everywhere is an opt-in
// (`SyncConfig.mirror`). A file lives on ONE home provider; moves are
// explicit relocations (download from A, upload to B, delete A, retarget the
// `sync_files` record), never silent re-uploads.

import type { SyncConfig } from '@/types'

export interface PlacementChoice {
  /** `sync_configs.id` that holds the single copy. */
  homeConfigId: string
  /** True when the user explicitly asked for copies on these providers too. */
  mirrorConfigIds: string[]
}

/** Enabled providers, stable id order (same order the Rust striped path uses). */
export function enabledProviders(configs: SyncConfig[]): SyncConfig[] {
  return configs.filter((c) => c.enabled).sort((a, b) => a.id.localeCompare(b.id))
}

/**
 * Pick the home for a file: explicit `homeConfigId` wins when it is still
 * enabled, else the first enabled provider (spanned fill), else null.
 */
export function chooseHome(
  configs: SyncConfig[],
  preferredHomeId?: string | null,
): SyncConfig | null {
  const enabled = enabledProviders(configs)
  if (!enabled.length) return null
  if (preferredHomeId) {
    const hit = enabled.find((c) => c.id === preferredHomeId)
    if (hit) return hit
  }
  return enabled[0]
}

/** Providers that opted into `mirror` (duplicate-everywhere targets). */
export function mirrorTargets(configs: SyncConfig[], homeId: string): SyncConfig[] {
  return enabledProviders(configs).filter((c) => c.id !== homeId && c.mirror === true)
}

export interface MovePlan {
  fileId: string
  fromConfigId: string
  toConfigId: string
  /** No-op when the file is already home (idempotent UI). */
  noop: boolean
}

/** Build an explicit A→B relocation plan (intelligent move, not copy+orphan). */
export function planMove(fileId: string, fromConfigId: string, toConfigId: string): MovePlan {
  return { fileId, fromConfigId, toConfigId, noop: fromConfigId === toConfigId }
}

/**
 * Designate the key holder: exactly one provider's `.cybermanju` unwraps the
 * other disks. Returns the updated id list (single-holder invariant).
 */
export function designateKeyHolder(configs: SyncConfig[], holderId: string): SyncConfig[] {
  return configs.map((c) => ({ ...c, keyHolder: c.id === holderId }))
}
