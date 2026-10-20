// CyberManju OS — IndexedDB pocket for `FileSystemFileHandle` values.
//
// Handles are structured-cloneable, so they survive a page reload when kept
// in IndexedDB — that is what lets a `.cybermanju` file stay attached across
// sessions without re-picking it. The browser still gates *permission* per
// origin: after a reload the handle comes back as `prompt`, and the app must
// ask the user for readwrite again (a user gesture — see
// `useCyberManjuFile.reattach()`).

import { IS_DEVELOP_PREVIEW } from './deploymentScope'

const DB_NAME = IS_DEVELOP_PREVIEW ? 'cybermanju-develop' : 'cybermanju'
const STORE = 'handles'

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('IndexedDB unavailable'))
      return
    }
    const req = indexedDB.open(DB_NAME, 1)
    req.onupgradeneeded = () => {
      const db = req.result
      if (!db.objectStoreNames.contains(STORE)) db.createObjectStore(STORE)
    }
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error ?? new Error('IndexedDB open failed'))
  })
}

export async function idbGet<T>(key: string): Promise<T | null> {
  try {
    const db = await openDb()
    try {
      return await new Promise<T | null>((resolve, reject) => {
        const tx = db.transaction(STORE, 'readonly')
        const req = tx.objectStore(STORE).get(key)
        req.onsuccess = () => resolve((req.result as T | undefined) ?? null)
        req.onerror = () => reject(req.error ?? new Error('IndexedDB read failed'))
      })
    } finally {
      db.close()
    }
  } catch {
    return null
  }
}

export async function idbSet(key: string, value: unknown): Promise<boolean> {
  try {
    const db = await openDb()
    try {
      await new Promise<void>((resolve, reject) => {
        const tx = db.transaction(STORE, 'readwrite')
        tx.objectStore(STORE).put(value, key)
        tx.oncomplete = () => resolve()
        tx.onerror = () => reject(tx.error ?? new Error('IndexedDB write failed'))
      })
      return true
    } finally {
      db.close()
    }
  } catch {
    return false
  }
}

/** All stored keys (lets boot restore every remembered directory handle). */
export async function idbKeys(): Promise<string[]> {
  try {
    const db = await openDb()
    try {
      return await new Promise<string[]>((resolve, reject) => {
        const tx = db.transaction(STORE, 'readonly')
        const req = tx.objectStore(STORE).getAllKeys()
        req.onsuccess = () => {
          const keys = (req.result as unknown[] ?? []).filter(
            (k): k is string => typeof k === 'string',
          )
          resolve(keys)
        }
        req.onerror = () => reject(req.error ?? new Error('IndexedDB keys failed'))
      })
    } finally {
      db.close()
    }
  } catch {
    return []
  }
}

export async function idbDel(key: string): Promise<void> {  try {
    const db = await openDb()
    try {
      await new Promise<void>((resolve, reject) => {
        const tx = db.transaction(STORE, 'readwrite')
        tx.objectStore(STORE).delete(key)
        tx.oncomplete = () => resolve()
        tx.onerror = () => reject(tx.error ?? new Error('IndexedDB delete failed'))
      })
    } finally {
      db.close()
    }
  } catch {
    // Best-effort — a stale handle just means re-picking the file.
  }
}
