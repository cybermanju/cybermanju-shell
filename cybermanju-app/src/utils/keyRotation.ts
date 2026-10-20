// CyberManju OS — Key rotation (src/utils/keyRotation.ts).
// Role: server-side keystore rotation checklist (ANALYSIS.md §3 recovery
// follow-up) — the Argon2id keystore stays the at-rest holder, Shamir is the
// escrow layer; rotation re-wraps the master key and re-cuts escrow shares.
// Exports: RotationStep, rotationChecklist, rotationVerifySummary.
// Integrates: ShieldPanel RECOVERY tab renders the checklist; no new crypto
// here (rotation itself runs server-side via `set_disk_key_holder`).
// Contracts: pure, never throws; steps are ordered and idempotent — re-running
// the checklist never invalidates a completed rotation.
export interface RotationStep {
  id: string
  title: string
  detail: string
}

/** Ordered, idempotent rotation procedure (verify each step before the next). */
export function rotationChecklist(): RotationStep[] {
  return [
    {
      id: 'backup',
      title: 'Export a sealed backup',
      detail: 'Disks → BACKUP → Export. A rotation without a restorable backup is a data-loss drill.',
    },
    {
      id: 'escrow',
      title: 'Confirm recovery shares verify',
      detail: 'Shield → RECOVERY → combine your Shamir shares once. If they do not verify, re-split before rotating.',
    },
    {
      id: 'rewrap',
      title: 'Re-wrap the master key',
      detail: 'Set a new disk passphrase (Shield → keys). The keystore re-wraps; the old wrap stays until the new one verifies.',
    },
    {
      id: 'resplit',
      title: 'Re-cut escrow shares',
      detail: 'Split the new secret again (same N/M or better) and redistribute. Old shares die with the old secret.',
    },
    {
      id: 'verify',
      title: 'Verify + restart',
      detail: 'Unlock once with the new passphrase, restart the daemon, confirm scheduled backups still run.',
    },
  ]
}

/** Short verify summary for toasts and the cybsh verb. */
export function rotationVerifySummary(doneIds: string[]): string {
  const total = rotationChecklist().length
  const done = new Set(doneIds).size
  if (done >= total) return `rotation checklist complete (${total}/${total})`
  return `rotation checklist ${done}/${total} — next: ${rotationChecklist()[done]?.title ?? 'done'}`
}
