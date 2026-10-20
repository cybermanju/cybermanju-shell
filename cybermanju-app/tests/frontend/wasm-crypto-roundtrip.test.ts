// CyberManju OS — WASM crypto round-trip tests (tests/frontend/wasm-crypto-roundtrip.test.ts).
// Role: exercise the browser file pipeline's authenticated encryption and persistence contracts.
// Exports: Vitest coverage for nonce framing, byte fidelity, tamper/key rejection, and failed writes.
// Integrates: the composable against in-memory vault/WASM boundaries and Node's real ChaCha20-Poly1305.
// Contracts: crypto is real AEAD; only Rust/WASM loading and vault/database storage are shimmed.
import { createCipheriv, createDecipheriv, hkdfSync, randomBytes, createHash } from 'node:crypto'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const mockState = vi.hoisted(() => ({
  rows: new Map<string, string>(),
  node: { id: 'file-1', sizeBytes: 0, hashBlake3: '', encrypted: false, encryptionAlgorithm: null as string | null, compressionLayers: [] as string[] },
  wasm: {} as Record<string, unknown>,
  failBodyWrite: false,
}))

vi.mock('@/composables/useVault', () => ({
  vaultGet: async (key: string) => mockState.rows.get(key) ?? null,
  vaultSet: async (key: string, value: string) => {
    if (key === 'content:file-1' && mockState.failBodyWrite) {
      mockState.failBodyWrite = false
      throw new Error('injected body persistence failure')
    }
    mockState.rows.set(key, value)
  },
  vaultDelete: async (key: string) => { mockState.rows.delete(key) },
  vaultList: async (prefix = '') => Array.from(mockState.rows.keys())
    .filter((key) => key.startsWith(prefix)).map((key) => ({ key, bytes: (mockState.rows.get(key) ?? '').length })),
  vaultGetJson: async (key: string) => {
    const value = mockState.rows.get(key)
    if (!value) return null
    try { return JSON.parse(value) } catch { return null }
  },
  vaultSetJson: async (key: string, value: unknown) => { mockState.rows.set(key, JSON.stringify(value)) },
}))

vi.mock('@/composables/useWasmBackend', () => ({
  wasmModuleExports: async () => mockState.wasm,
  wasmDbDispatch: async (op: string, args: Record<string, unknown>) => {
    if (op === 'files.get') return { ...mockState.node }
    if (op === 'files.patch') {
      Object.assign(mockState.node, args.patch)
      return null
    }
    throw new Error(`unexpected mock DB operation: ${op}`)
  },
}))

import { decryptFile, encryptFile, getEncryptionStatus, readOriginalBytes } from '@/composables/useWasmCrypto'

const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64')
const fromB64 = (text: string) => new Uint8Array(Buffer.from(text, 'base64'))

function decryptAead(key: Uint8Array, nonce: Uint8Array, input: Uint8Array): Uint8Array {
  const bytes = Buffer.from(input)
  const decipher = createDecipheriv('chacha20-poly1305', Buffer.from(key), Buffer.from(nonce), { authTagLength: 16 })
  decipher.setAuthTag(bytes.subarray(bytes.length - 16))
  return new Uint8Array(Buffer.concat([decipher.update(bytes.subarray(0, -16)), decipher.final()]))
}

const nodeCrypto = {
  x25519_generate_keypair: () => ({ privateKey: new Uint8Array(randomBytes(32)), publicKey: new Uint8Array(randomBytes(32)) }),
  ml_dsa65_generate_keypair: () => ({ privateKey: new Uint8Array(randomBytes(32)), publicKey: new Uint8Array(randomBytes(32)) }),
  ml_dsa65_sign: () => new Uint8Array(),
  ml_dsa65_verify: () => true,
  chacha20_generate_key: () => new Uint8Array(randomBytes(32)),
  chacha20_generate_nonce: () => new Uint8Array(randomBytes(12)),
  chacha20_encrypt: (key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array) => {
    const cipher = createCipheriv('chacha20-poly1305', Buffer.from(key), Buffer.from(nonce), { authTagLength: 16 })
    const body = Buffer.concat([cipher.update(Buffer.from(plaintext)), cipher.final()])
    return new Uint8Array(Buffer.concat([body, cipher.getAuthTag()]))
  },
  chacha20_decrypt: decryptAead,
  hkdf_derive: (secret: Uint8Array, salt: Uint8Array, info: Uint8Array, length: number) =>
    new Uint8Array(hkdfSync('sha256', Buffer.from(secret), Buffer.from(salt), Buffer.from(info), length)),
  // The file-flow tests do not validate BLAKE3; this stable digest supplies its 32-byte/hex interface.
  blake3_hash: (data: Uint8Array) => createHash('sha256').update(Buffer.from(data)).digest('hex'),
  compress_lz4: (data: Uint8Array) => data,
  decompress_lz4: (data: Uint8Array) => data,
  compress_brotli: (data: Uint8Array) => data,
  decompress_brotli: (data: Uint8Array) => data,
}

const original = new Uint8Array([
  0, 255, 17, 128, 42, 0, 99,
  ...new TextEncoder().encode('Hello, 世界 — café 🧪\n'),
])

function seedPlaintext(): void {
  mockState.rows.clear()
  mockState.rows.set('content:file-1', b64(original))
  mockState.rows.set('encoding:file-1', 'base64')
  mockState.node = { id: 'file-1', sizeBytes: original.length, hashBlake3: '', encrypted: false, encryptionAlgorithm: null, compressionLayers: [] }
  mockState.failBodyWrite = false
}

beforeEach(() => {
  seedPlaintext()
  mockState.wasm = nodeCrypto
})

describe('WASM file encryption pipeline', () => {
  it('round-trips binary and Unicode bytes through nonce-prefixed authenticated ciphertext', async () => {
    await encryptFile('file-1', 'aes256')
    const encoded = mockState.rows.get('content:file-1')!
    const frame = fromB64(encoded)
    expect(frame.length).toBeGreaterThan(12 + 16)
    const meta = JSON.parse(mockState.rows.get('meta:file-1')!).enc
    expect(Array.from(frame.subarray(0, 12))).toEqual(Array.from(Buffer.from(meta.nonceHex, 'hex')))
    expect(Array.from(await readOriginalBytes('file-1'))).toEqual(Array.from(original))
    await decryptFile('file-1')
    expect(Array.from(await readOriginalBytes('file-1'))).toEqual(Array.from(original))
    expect(await getEncryptionStatus('file-1')).toEqual({ isEncrypted: false })
  })

  it('rejects a ciphertext modified after persistence', async () => {
    await encryptFile('file-1', 'aes256')
    const frame = fromB64(mockState.rows.get('content:file-1')!)
    frame[frame.length - 1] ^= 1
    mockState.rows.set('content:file-1', b64(frame))
    await expect(readOriginalBytes('file-1')).rejects.toThrow()
  })

  it('rejects decryption with the wrong stored key', async () => {
    await encryptFile('file-1', 'aes256')
    const configKey = Array.from(mockState.rows.keys()).find((key) => key.startsWith('config:key:'))!
    const keyId = configKey.slice('config:key:'.length)
    const secretKey = `secret:key:${keyId}`
    const secret = JSON.parse(mockState.rows.get(secretKey)!)
    secret.privateHex = Buffer.from(randomBytes(32)).toString('hex')
    mockState.rows.set(secretKey, JSON.stringify(secret))
    await expect(readOriginalBytes('file-1')).rejects.toThrow()
  })

  it('does not report success or leave encryption metadata when ciphertext persistence fails', async () => {
    mockState.failBodyWrite = true
    await expect(encryptFile('file-1', 'aes256')).rejects.toThrow(/vault persistence failed/)
    expect(mockState.rows.get('content:file-1')).toBe(b64(original))
    expect(mockState.rows.has('meta:file-1')).toBe(false)
    expect(await getEncryptionStatus('file-1')).toEqual({ isEncrypted: false })
  })

  it('does not infer that a specific file is encrypted just because a key exists', async () => {
    await encryptFile('file-1', 'aes256')
    await decryptFile('file-1')
    expect((await getEncryptionStatus()).isEncrypted).toBe(true)
    expect(await getEncryptionStatus('unrelated-file')).toEqual({ isEncrypted: false })
  })
})
