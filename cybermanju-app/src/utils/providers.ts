// CyberManju OS — Providers (src/utils/providers.ts).
// Shared provider metadata + credential-draft helpers.
//
// AccountManager's per-provider cards ask the same questions for every
// backend — which fields does it need, what do we call the token, how do
// we explain auth — so the answers live here exactly once. (SyncPanel used
// to carry a second copy of this form; since the merge it only runs syncs.)

import { SYNC_BACKEND_INFO } from '@/types'
import type { SyncBackendType, SyncConfig } from '@/types'

export function backendLabel(b: SyncBackendType): string {
  return SYNC_BACKEND_INFO[b]?.name ?? b
}

/** GitHub (owner/repo) and GitLab (project id) share the repo+branch fields. */
export function needsRepo(b: SyncBackendType): boolean {
  return b === 'github' || b === 'gitlab'
}

/** Backends with any pasteable secret (PATs, bot tokens, short-lived OAuth). */
export function needsToken(b: SyncBackendType): boolean {
  return (
    b === 'github' ||
    b === 'gitlab' ||
    b === 'googleDrive'
  )
}

export function tokenLabel(b: SyncBackendType): string {
  if (b === 'github') return 'TOKEN — PERSONAL ACCESS TOKEN (USED AS THE PASSWORD)'
  if (b === 'gitlab') return 'TOKEN — PERSONAL ACCESS TOKEN (USED AS THE PASSWORD)'
  return 'TOKEN — OPTIONAL WHEN USING OAUTH'
}

export function authGuidance(b: SyncBackendType): string {
  switch (b) {
    case 'github':
      return 'GitHub removed account passwords: sign in with OAUTH above, or paste a personal access token (repo scope — repo creation needs it too) as the password.'
    case 'gitlab':
      return 'GitLab sign-in is OAUTH, or a personal access token (api scope — project creation needs it) pasted as the password. Self-hosted? Set the instance URL too.'
    case 'googleDrive':
      return 'Google accepts OAUTH only — there is no password login. CONNECT WITH OAUTH above.'
    default:
      return 'Local directory needs no login — just the path.'
  }
}

/** Editable credential fields for one provider card / wizard. */
export interface CredentialDraft {
  repoName: string
  branch: string
  token: string
  folderId: string
  basePath: string
  name: string
}

export function blankCredentialDraft(
  init?: Partial<CredentialDraft>,
): CredentialDraft {
  return {
    repoName: '',
    branch: 'main',
    token: '',
    folderId: '',
    basePath: '',
    name: '',
    ...init,
  }
}

const trimmed = (v: string): string | undefined => {
  const t = v.trim()
  return t ? t : undefined
};

/** Baseline `SyncConfig` body shared by every creator (wizard + manager). */
export function syncConfigDefaults(): Omit<
  SyncConfig,
  'id' | 'backendType' | 'createdAt' | 'updatedAt'
> {
  return {
    enabled: true,
    autoSync: false,
    compressBeforeUpload: true,
    createPreviews: false,
    deleteRawAfterSync: false,
    maxConcurrentUploads: 1,
    encryptBeforeUpload: true,
    conflictPolicy: 'skip',
    placement: 'whole',
    parity: 1,
    requireEncryption: false,
    obfuscateNames: false,
    mirror: false,
    keyHolder: false,
  }
}

/**
 * Draft → savable config. Empty fields become `undefined` (never empty
 * strings); the token is attached only when freshly typed, so re-saving a
 * card never wipes the stored secret with `""`.
 */
export function draftToSave(cfg: SyncConfig, d: CredentialDraft): SyncConfig {
  const updated: SyncConfig = {
    ...cfg,
    name: trimmed(d.name) ?? cfg.name,
    repoName: trimmed(d.repoName),
    branch: trimmed(d.branch),
    folderId: trimmed(d.folderId),
    basePath: trimmed(d.basePath),
  }
  const token = d.token.trim()
  if (token) updated.token = token
  else delete updated.token
  return updated
}

/**
 * Draft ⨯ stored row for *probing*: non-empty draft fields win, everything
 * else falls back to the stored row. This is what makes "paste a token, hit
 * TEST" verify the token before it is saved.
 */
export function overlayDraft(cfg: SyncConfig, d: CredentialDraft | undefined): SyncConfig {
  if (!d) return cfg
  const merged: SyncConfig = { ...cfg }
  if (d.token.trim()) merged.token = d.token.trim()
  if (d.repoName.trim()) merged.repoName = d.repoName.trim()
  if (d.branch.trim()) merged.branch = d.branch.trim()
  if (d.folderId.trim()) merged.folderId = d.folderId.trim()
  if (d.basePath.trim()) merged.basePath = d.basePath.trim()
  return merged
}

/** Refresh an in-memory draft from a freshly saved row (keeps token blank). */
export function refreshDraftFromSaved(d: CredentialDraft, saved: SyncConfig): void {
  d.repoName = saved.repoName ?? ''
  d.branch = saved.branch ?? 'main'
  d.token = ''
  d.folderId = saved.folderId ?? ''
  d.basePath = saved.basePath ?? ''
  d.name = saved.name ?? ''
}
