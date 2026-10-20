// CyberManju OS — `.cybermanju` container codec
//
// A `.cybermanju` file is one redb database image (`db_snapshot()` bytes),
// optionally compressed and optionally encrypted:
//
//   magic 'CYBMJ01' | ver u8 | kdf u8 | flags u8 | saltLen u8 | nonceLen u8
//   | iterations u32le | salt | nonce | body
//
//   kdf   0 = no key derivation (no passphrase), 1 = PBKDF2-HMAC-SHA512
//   flags bit0 = body is lz4-compressed, bit1 = body is ChaCha20-Poly1305 ciphertext
//
// Everything cryptographic is a **pkg export** (`chacha20_*`, `hmac_sha512`,
// `compress_lz4`) — no Rust change. The primitives are injectable so tests
// run against Node's `crypto` instead of the wasm module.
//
// A file that does not start with the magic is a legacy raw redb image. It
// passes through this codec, but `db_restore` validates it before replacing
// the active database.

export interface ContainerPrimitives {
  compress(data: Uint8Array): Uint8Array
  decompress(data: Uint8Array): Uint8Array
  /** Full format KDF: PBKDF2-HMAC-SHA512 (64 bytes), then HKDF-SHA256. */
  deriveKey(passphrase: string, salt: Uint8Array, length: number, iterations: number): Uint8Array
  encrypt(key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array): Uint8Array
  decrypt(key: Uint8Array, nonce: Uint8Array, ciphertext: Uint8Array): Uint8Array
  randomBytes(n: number): Uint8Array
}

export const CONTAINER_MAGIC = 'CYBMJ01'
export const CONTAINER_VERSION = 1
export const KDF_NONE = 0
export const KDF_PBKDF2_SHA512 = 1
export const FLAG_COMPRESSED = 1
export const FLAG_ENCRYPTED = 2

/** Shared hard ceiling for imported/exported redb images (matches Rust). */
export const MAX_CONTAINER_IMAGE_BYTES = 256 * 1024 * 1024
/** Refuse hostile headers that turn verification into an excessive PBKDF2 loop. */
export const MAX_CONTAINER_ITERATIONS = 1_000_000

/** Password-derivation cost. 100k HMAC-SHA512 rounds ≈ 100–300 ms in wasm. */
export const DEFAULT_ITERATIONS = 100_000

const HEADER_FIXED = 16
const SALT_LEN = 16
const NONCE_LEN = 12
const KEY_LEN = 32
const AEAD_TAG_LEN = 16
export const MAX_CONTAINER_FILE_BYTES =
  MAX_CONTAINER_IMAGE_BYTES + HEADER_FIXED + SALT_LEN + NONCE_LEN + AEAD_TAG_LEN

const MAGIC_BYTES = new TextEncoder().encode(CONTAINER_MAGIC)

export class WrongPassphraseError extends Error {
  constructor() {
    super('wrong passphrase — the container key does not decrypt this file')
    this.name = 'WrongPassphraseError'
  }
}

// ── primitives from the shipped wasm bundle ────────────────────────────────

let primitivesPromise: Promise<ContainerPrimitives> | null = null

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  return out
}

function bytesToHex(bytes: Uint8Array): string {
  let s = ''
  for (const b of bytes) s += b.toString(16).padStart(2, '0')
  return s
}

/** Lazily bind the codec to `cybermanju-os-wasm` (avoids loading it in tests). */
export function containerPrimitives(): Promise<ContainerPrimitives> {
  if (!primitivesPromise) {
    primitivesPromise = (async () => {
      const mod = (await import('cybermanju-os-wasm')) as unknown as {
        compress_lz4(d: Uint8Array): Uint8Array
        decompress_lz4(d: Uint8Array): Uint8Array
        chacha20_encrypt(k: Uint8Array, n: Uint8Array, p: Uint8Array): Uint8Array
        chacha20_decrypt(k: Uint8Array, n: Uint8Array, c: Uint8Array): Uint8Array
        hmac_sha512(k: Uint8Array, d: Uint8Array): Uint8Array
        hkdf_derive(s: Uint8Array, salt: Uint8Array, info: Uint8Array, len: number): Uint8Array
      }
      const enc = new TextEncoder()
      const xorInto = (acc: Uint8Array, u: Uint8Array) => {
        for (let i = 0; i < acc.length; i++) acc[i] ^= u[i]
      }
      return {
        compress: (d) => mod.compress_lz4(d),
        decompress: (d) => mod.decompress_lz4(d),
        deriveKey: (passphrase, salt, length, iterations) => {
          const pass = enc.encode(passphrase)
          // PBKDF2-HMAC-SHA512, chained exactly like the RFC construction.
          let u = mod.hmac_sha512(pass, concat(salt, u32le(1)))
          let t = u.slice()
          for (let i = 2; i <= iterations; i++) {
            u = mod.hmac_sha512(pass, u)
            xorInto(t, u)
          }
          return mod.hkdf_derive(t, salt, enc.encode('cybermanju-container-v1'), length)
        },
        encrypt: (k, n, p) => mod.chacha20_encrypt(k, n, p),
        decrypt: (k, n, c) => {
          try {
            return mod.chacha20_decrypt(k, n, c)
          } catch {
            throw new WrongPassphraseError()
          }
        },
        randomBytes: (n) => {
          const out = new Uint8Array(n)
          crypto.getRandomValues(out)
          return out
        },
      }
    })()
    primitivesPromise.catch(() => {
      primitivesPromise = null
    })
  }
  return primitivesPromise
}

/** Test/DI helper: replace the wasm binding (pass `null` to restore it). */
export function setContainerPrimitives(p: ContainerPrimitives | null): void {
  primitivesPromise = p ? Promise.resolve(p) : null
}

// ── byte helpers ───────────────────────────────────────────────────────────

export function concat(...parts: Uint8Array[]): Uint8Array {
  let len = 0
  for (const p of parts) len += p.length
  const out = new Uint8Array(len)
  let off = 0
  for (const p of parts) {
    out.set(p, off)
    off += p.length
  }
  return out
}

function u32le(n: number): Uint8Array {
  const b = new Uint8Array(4)
  new DataView(b.buffer).setUint32(0, n >>> 0, true)
  return b
}

function startsWithMagic(bytes: Uint8Array): boolean {
  if (bytes.length < MAGIC_BYTES.length) return false
  for (let i = 0; i < MAGIC_BYTES.length; i++) if (bytes[i] !== MAGIC_BYTES[i]) return false
  return true
}

export function isContainer(bytes: Uint8Array): boolean {
  return startsWithMagic(bytes)
}

/**
 * True when the file needs a passphrase to open (flags bit1). Header-only —
 * no key needed, so the UI can ask before touching the database.
 */
export function isEncryptedContainer(bytes: Uint8Array): boolean {
  if (bytes.length < HEADER_FIXED || !startsWithMagic(bytes)) return false
  return (bytes[9] & FLAG_ENCRYPTED) !== 0
}

export interface EncodeOptions {
  iterations?: number
  /** Force-disable compression (tests / tiny payloads). */
  compress?: boolean
}

/**
 * Wrap a redb image into a `.cybermanju` container. An empty passphrase
 * stores the image compressed-but-plaintext (the file is still a container,
 * so a later save can encrypt it without re-picking the file).
 */
export async function encodeContainer(
  image: Uint8Array,
  passphrase: string,
  primitives?: ContainerPrimitives,
  options: EncodeOptions = {},
): Promise<Uint8Array> {
  if (image.byteLength > MAX_CONTAINER_IMAGE_BYTES) {
    throw new Error('too_large: backup image exceeds the 256 MiB container cap')
  }
  const p = primitives ?? (await containerPrimitives())
  const encrypted = passphrase.length > 0
  const iterations = encrypted ? options.iterations ?? DEFAULT_ITERATIONS : 0
  if (encrypted && (iterations < 1 || iterations > MAX_CONTAINER_ITERATIONS)) {
    throw new Error(`invalid: PBKDF2 iterations must be between 1 and ${MAX_CONTAINER_ITERATIONS}`)
  }

  let payload = image
  let compressed = false
  if (options.compress !== false) {
    const packed = p.compress(image)
    if (packed.length < image.length) {
      payload = packed
      compressed = true
    }
  }

  let salt: Uint8Array = new Uint8Array(0)
  let nonce: Uint8Array = new Uint8Array(0)
  let body: Uint8Array = payload
  if (encrypted) {
    salt = p.randomBytes(SALT_LEN)
    nonce = p.randomBytes(NONCE_LEN)
    const key = p.deriveKey(passphrase, salt, KEY_LEN, iterations)
    body = p.encrypt(key, nonce, payload)
  }

  const flags = (compressed ? FLAG_COMPRESSED : 0) | (encrypted ? FLAG_ENCRYPTED : 0)
  const header = concat(
    MAGIC_BYTES,
    new Uint8Array([CONTAINER_VERSION, KDF_PBKDF2_SHA512 * (encrypted ? 1 : 0), flags, salt.length, nonce.length]),
    u32le(iterations),
    salt,
    nonce,
  )
  return concat(header, body)
}

export interface DecodedContainer {
  image: Uint8Array
  legacy: boolean
  compressed: boolean
  encrypted: boolean
}

/**
 * Read a `.cybermanju` container (or a legacy raw redb image) back to bytes.
 * Throws `WrongPassphraseError` when the key does not open it, and a plain
 * `Error` on structural damage.
 */
export async function decodeContainer(
  bytes: Uint8Array,
  passphrase: string,
  primitives?: ContainerPrimitives,
): Promise<DecodedContainer> {
  if (!startsWithMagic(bytes)) {
    if (bytes.byteLength === 0) throw new Error('invalid: backup image is empty')
    if (bytes.byteLength > MAX_CONTAINER_IMAGE_BYTES) {
      throw new Error('too_large: legacy backup image exceeds the 256 MiB container cap')
    }
    return { image: bytes, legacy: true, compressed: false, encrypted: false }
  }
  if (bytes.byteLength > MAX_CONTAINER_FILE_BYTES) {
    throw new Error('too_large: container exceeds the 256 MiB image cap')
  }
  if (bytes.length < HEADER_FIXED) throw new Error('container: truncated header')

  const version = bytes[7]
  if (version !== CONTAINER_VERSION) throw new Error(`container: unsupported version ${version}`)
  const kdf = bytes[8]
  const flags = bytes[9]
  const saltLen = bytes[10]
  const nonceLen = bytes[11]
  const iterations = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(12, true)
  const encrypted = (flags & FLAG_ENCRYPTED) !== 0
  const compressed = (flags & FLAG_COMPRESSED) !== 0
  if ((flags & ~(FLAG_COMPRESSED | FLAG_ENCRYPTED)) !== 0) {
    throw new Error(`invalid: unknown container flags ${flags}`)
  }
  if (encrypted) {
    if (kdf !== KDF_PBKDF2_SHA512) throw new Error(`container: unknown kdf ${kdf}`)
    if (saltLen !== SALT_LEN || nonceLen !== NONCE_LEN) {
      throw new Error('invalid: encrypted container has invalid salt or nonce length')
    }
    if (iterations < 1 || iterations > MAX_CONTAINER_ITERATIONS) {
      throw new Error(`invalid: PBKDF2 iterations must be between 1 and ${MAX_CONTAINER_ITERATIONS}`)
    }
  } else if (kdf !== KDF_NONE || saltLen !== 0 || nonceLen !== 0 || iterations !== 0) {
    throw new Error('invalid: plaintext container has encrypted metadata')
  }
  const bodyAt = HEADER_FIXED + saltLen + nonceLen
  if (bytes.length < bodyAt) throw new Error('container: truncated header')
  const bodyLength = bytes.length - bodyAt
  const minimumBodyLength = (encrypted ? AEAD_TAG_LEN : 0) + (compressed ? 4 : 0)
  if (bodyLength < minimumBodyLength) throw new Error('integrity: container body is truncated')

  const salt = bytes.slice(HEADER_FIXED, HEADER_FIXED + saltLen)
  const nonce = bytes.slice(HEADER_FIXED + saltLen, bodyAt)
  let payload: Uint8Array = bytes.slice(bodyAt)

  if (encrypted) {
    if (!passphrase) throw new WrongPassphraseError()
    const p = primitives ?? (await containerPrimitives())
    const key = p.deriveKey(passphrase, salt, KEY_LEN, iterations)
    try {
      payload = p.decrypt(key, nonce, payload)
    } catch {
      // AEAD failure = wrong passphrase (or a damaged body — same remedy).
      throw new WrongPassphraseError()
    }
  }
  if (payload.byteLength > MAX_CONTAINER_IMAGE_BYTES + (compressed ? 4 : 0)) {
    throw new Error('too_large: container payload exceeds the 256 MiB image cap')
  }
  if (compressed) {
    if (payload.byteLength < 4) throw new Error('integrity: compressed container body is truncated')
    const declaredLength = new DataView(
      payload.buffer,
      payload.byteOffset,
      payload.byteLength,
    ).getUint32(0, true)
    if (declaredLength > MAX_CONTAINER_IMAGE_BYTES) {
      throw new Error('too_large: compressed backup declares an image over 256 MiB')
    }
    const p = primitives ?? (await containerPrimitives())
    payload = p.decompress(payload)
  }
  if (payload.byteLength === 0) throw new Error('invalid: backup image is empty')
  if (payload.byteLength > MAX_CONTAINER_IMAGE_BYTES) {
    throw new Error('too_large: decoded backup image exceeds the 256 MiB cap')
  }
  return { image: payload, legacy: false, compressed, encrypted }
}
