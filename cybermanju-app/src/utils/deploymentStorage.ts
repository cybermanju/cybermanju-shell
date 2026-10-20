// CyberManju OS — DeploymentStorage (src/utils/deploymentStorage.ts).
// Role: pure TS helpers (unit-tested, transport-agnostic).
// Exports: createScopedStorage, installScopedStorage.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
import {
  DEPLOYMENT_STORAGE_PREFIX,
  IS_DEVELOP_PREVIEW,
} from './deploymentScope'

function scopedPhysicalKeys(storage: Storage, prefix: string): string[] {
  const keys: string[] = []
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index)
    if (key?.startsWith(prefix)) keys.push(key)
  }
  return keys
}

/** Expose only one deployment's keys while preserving the Storage contract. */
export function createScopedStorage(storage: Storage, prefix: string): Storage {
  const logicalKeys = () =>
    scopedPhysicalKeys(storage, prefix).map(key => key.slice(prefix.length))

  return new Proxy(storage, {
    get(target, property) {
      if (property === 'length') return logicalKeys().length
      if (property === 'key') {
        return (index: number) => logicalKeys()[index] ?? null
      }
      if (property === 'getItem') {
        return (key: string) => target.getItem(prefix + key)
      }
      if (property === 'setItem') {
        return (key: string, value: string) => target.setItem(prefix + key, value)
      }
      if (property === 'removeItem') {
        return (key: string) => target.removeItem(prefix + key)
      }
      if (property === 'clear') {
        return () => {
          for (const key of scopedPhysicalKeys(target, prefix)) {
            target.removeItem(key)
          }
        }
      }

      const value = Reflect.get(target, property, target)
      return typeof value === 'function' ? value.bind(target) : value
    },
  })
}

/** Replace a window-like object's Storage property with the scoped view. */
export function installScopedStorage(
  target: { localStorage: Storage },
  prefix: string,
): void {
  const scoped = createScopedStorage(target.localStorage, prefix)
  const descriptor = Object.getOwnPropertyDescriptor(target, 'localStorage')
  Object.defineProperty(target, 'localStorage', {
    configurable: true,
    enumerable: descriptor?.enumerable ?? true,
    get: () => scoped,
  })
}

// This module is imported before App.vue so every module sees the scoped view
// from its first localStorage access. Production and non-Pages builds remain
// byte-for-byte on their existing storage namespace.
if (IS_DEVELOP_PREVIEW && typeof window !== 'undefined') {
  installScopedStorage(window, DEPLOYMENT_STORAGE_PREFIX)
}
