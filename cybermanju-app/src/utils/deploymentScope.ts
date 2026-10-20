// CyberManju OS — DeploymentScope (src/utils/deploymentScope.ts).
// Role: pure TS helpers (unit-tested, transport-agnostic).
// Exports: DEVELOP_STORAGE_PREFIX, IS_DEVELOP_PREVIEW, DEPLOYMENT_STORAGE_PREFIX, deploymentStorageEventKey.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
export const DEVELOP_STORAGE_PREFIX = 'cybermanju:develop:'

export const IS_DEVELOP_PREVIEW = import.meta.env.BASE_URL === '/develop/'

export const DEPLOYMENT_STORAGE_PREFIX = IS_DEVELOP_PREVIEW
  ? DEVELOP_STORAGE_PREFIX
  : ''

/**
 * Translate storage-event keys into the logical key space for this deployment.
 * The production root ignores preview events; the preview ignores production
 * events and strips its physical namespace before app listeners see a key.
 */
export function deploymentStorageEventKey(
  key: string | null,
  isDevelopPreview = IS_DEVELOP_PREVIEW,
): string | null {
  if (key === null) return null

  if (isDevelopPreview) {
    return key.startsWith(DEVELOP_STORAGE_PREFIX)
      ? key.slice(DEVELOP_STORAGE_PREFIX.length)
      : null
  }

  return key.startsWith(DEVELOP_STORAGE_PREFIX) ? null : key
}
