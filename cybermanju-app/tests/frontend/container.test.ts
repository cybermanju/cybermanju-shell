// CyberManju OS — Container.test (tests/frontend/container.test.ts).
// `.cybermanju` container codec — round-trip and failure paths.
//
// The codec takes injectable primitives so this suite runs on Node's own
// `crypto` (PBKDF2-HMAC-SHA512 + ChaCha20-Poly1305) instead of the wasm
// bundle: the *format* (header, flags, legacy pass-through) is what is under
// test, and the wasm binding is exercised in the browser build.
import { createCipheriv, createDecipheriv, hkdfSync, pbkdf2Sync, randomBytes } from 'node:crypto'
import { deflateRawSync, inflateRawSync } from 'node:zlib'
import { beforeAll, describe, expect, it, vi } from 'vitest'
import {
  CONTAINER_MAGIC,
  DEFAULT_ITERATIONS,
  FLAG_COMPRESSED,
  FLAG_ENCRYPTED,
  MAX_CONTAINER_IMAGE_BYTES,
  MAX_CONTAINER_ITERATIONS,
  WrongPassphraseError,
  concat,
  decodeContainer,
  encodeContainer,
  isContainer,
  setContainerPrimitives,
  type ContainerPrimitives,
} from '../../src/utils/container'

const text = new TextEncoder()
const utf8 = (b: Uint8Array) => new TextDecoder().decode(b)

/** Node-backed twin of the wasm primitives. */
const nodePrimitives: ContainerPrimitives = {
  compress: (d) => {
    const size = new Uint8Array(4)
    new DataView(size.buffer).setUint32(0, d.length, true)
    return concat(size, new Uint8Array(deflateRawSync(Buffer.from(d))))
  },
  decompress: (d) => {
    const size = new DataView(d.buffer, d.byteOffset, d.byteLength).getUint32(0, true)
    const inflated = new Uint8Array(inflateRawSync(Buffer.from(d.subarray(4))))
    if (inflated.length !== size) throw new Error('declared image length mismatch')
    return inflated
  },
  deriveKey: (passphrase, salt, length, iterations) => {
    const stretched = pbkdf2Sync(passphrase, Buffer.from(salt), iterations, 64, 'sha512')
    return new Uint8Array(
      hkdfSync(
        'sha256',
        stretched,
        Buffer.from(salt),
        Buffer.from('cybermanju-container-v1'),
        length,
      ),
    )
  },
  encrypt: (key, nonce, plaintext) => {
    const cipher = createCipheriv('chacha20-poly1305', Buffer.from(key), Buffer.from(nonce), {
      authTagLength: 16,
    })
    const body = Buffer.concat([cipher.update(Buffer.from(plaintext)), cipher.final()])
    return new Uint8Array(Buffer.concat([body, cipher.getAuthTag()]))
  },
  decrypt: (key, nonce, ciphertext) => {
    const buf = Buffer.from(ciphertext)
    const tag = buf.subarray(buf.length - 16)
    const body = buf.subarray(0, buf.length - 16)
    const decipher = createDecipheriv('chacha20-poly1305', Buffer.from(key), Buffer.from(nonce), {
      authTagLength: 16,
    })
    decipher.setAuthTag(tag)
    return new Uint8Array(Buffer.concat([decipher.update(body), decipher.final()]))
  },
  randomBytes: (n) => new Uint8Array(randomBytes(n)),
}

/** A compressible stand-in for a redb image. */
function fakeImage(bytes = 4096): Uint8Array {
  const unit = text.encode('CYBERMANJU REDB IMAGE PAGE '.repeat(16))
  const out = new Uint8Array(bytes)
  for (let off = 0; off < bytes; off += unit.length) out.set(unit.subarray(0, bytes - off), off)
  return out
}

beforeAll(() => setContainerPrimitives(nodePrimitives))

describe('container codec', () => {
  it('round-trips an encrypted + compressed container', async () => {
    const image = fakeImage()
    const bytes = await encodeContainer(image, 'correct horse', nodePrimitives)
    expect(isContainer(bytes)).toBe(true)
    expect(utf8(bytes.subarray(0, 7))).toBe(CONTAINER_MAGIC)

    const flags = bytes[9]
    expect(flags & FLAG_ENCRYPTED).toBe(FLAG_ENCRYPTED)
    expect(flags & FLAG_COMPRESSED).toBe(FLAG_COMPRESSED)
    // ciphertext must not leak the plaintext
    expect(utf8(bytes)).not.toContain('CYBERMANJU REDB IMAGE')

    const decoded = await decodeContainer(bytes, 'correct horse', nodePrimitives)
    expect(decoded.legacy).toBe(false)
    expect(decoded.encrypted).toBe(true)
    expect(decoded.compressed).toBe(true)
    expect(Array.from(decoded.image)).toEqual(Array.from(image))
  })

  it('actually shrinks the payload', async () => {
    const image = fakeImage(64 * 1024)
    const bytes = await encodeContainer(image, '', nodePrimitives)
    expect(bytes.length).toBeLessThan(image.length)
    const decoded = await decodeContainer(bytes, '', nodePrimitives)
    expect(decoded.compressed).toBe(true)
    expect(decoded.encrypted).toBe(false)
    expect(Array.from(decoded.image)).toEqual(Array.from(image))
  })

  it('skips compression when it does not pay off', async () => {
    const random = nodePrimitives.randomBytes(4096)
    const bytes = await encodeContainer(random, '', nodePrimitives, { compress: true })
    const decoded = await decodeContainer(bytes, '', nodePrimitives)
    expect(decoded.compressed).toBe(false)
    expect(decoded.encrypted).toBe(false)
    expect(Array.from(decoded.image)).toEqual(Array.from(random))
  })

  it('rejects a wrong passphrase and accepts the right one', async () => {
    const bytes = await encodeContainer(fakeImage(1024), 'right', nodePrimitives, {
      iterations: 1000,
    })
    await expect(decodeContainer(bytes, 'wrong', nodePrimitives)).rejects.toBeInstanceOf(
      WrongPassphraseError,
    )
    const decoded = await decodeContainer(bytes, 'right', nodePrimitives)
    expect(decoded.image.length).toBe(1024)
  })

  it('requires a passphrase for an encrypted container', async () => {
    const bytes = await encodeContainer(fakeImage(512), 'secret', nodePrimitives, {
      iterations: 1000,
    })
    await expect(decodeContainer(bytes, '', nodePrimitives)).rejects.toBeInstanceOf(
      WrongPassphraseError,
    )
  })

  it('passes a legacy raw redb image straight through', async () => {
    // Legacy images have no magic prefix — `db_restore` takes them as-is.
    const raw = new Uint8Array([0x00, 0x01, 0x02, 0xff, 0xfe, 0x42])
    const decoded = await decodeContainer(raw, 'ignored', nodePrimitives)
    expect(decoded.legacy).toBe(true)
    expect(Array.from(decoded.image)).toEqual(Array.from(raw))
  })

  it('flags a truncated header instead of decoding nonsense', async () => {
    const bytes = await encodeContainer(fakeImage(512), 'secret', nodePrimitives, {
      iterations: 1000,
    })
    const truncated = bytes.slice(0, 8)
    await expect(decodeContainer(truncated, 'secret', nodePrimitives)).rejects.toThrow(
      /truncated/,
    )
  })

  it('stores the KDF iteration count in the header', async () => {
    const bytes = await encodeContainer(fakeImage(256), 'pw', nodePrimitives, {
      iterations: 4321,
      compress: false,
    })
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    expect(view.getUint32(12, true)).toBe(4321)
    expect(bytes[8]).toBe(1) // PBKDF2-HMAC-SHA512
    expect(bytes[10]).toBe(16) // salt
    expect(bytes[11]).toBe(12) // nonce
    const decoded = await decodeContainer(bytes, 'pw', nodePrimitives)
    expect(decoded.compressed).toBe(false)
    expect(Array.from(decoded.image)).toEqual(Array.from(fakeImage(256)))
  })

  it('rejects a compressed-size bomb before invoking the decompressor', async () => {
    const bytes = new Uint8Array(20)
    bytes.set(text.encode(CONTAINER_MAGIC))
    bytes[7] = 1
    bytes[8] = 0
    bytes[9] = FLAG_COMPRESSED
    new DataView(bytes.buffer).setUint32(16, MAX_CONTAINER_IMAGE_BYTES + 1, true)
    const decompress = vi.fn(nodePrimitives.decompress)
    await expect(
      decodeContainer(bytes, '', { ...nodePrimitives, decompress }),
    ).rejects.toThrow(/too_large/)
    expect(decompress).not.toHaveBeenCalled()
  })

  it('rejects excessive KDF work before deriving a key', async () => {
    const bytes = await encodeContainer(fakeImage(64), 'pw', nodePrimitives, {
      iterations: 1000,
      compress: false,
    })
    new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).setUint32(
      12,
      MAX_CONTAINER_ITERATIONS + 1,
      true,
    )
    const deriveKey = vi.fn(nodePrimitives.deriveKey)
    await expect(
      decodeContainer(bytes, 'pw', { ...nodePrimitives, deriveKey }),
    ).rejects.toThrow(/iterations/)
    expect(deriveKey).not.toHaveBeenCalled()
  })

  it('default passphrase cost is a real KDF, not a hash of the password', () => {
    expect(DEFAULT_ITERATIONS).toBeGreaterThanOrEqual(50_000)
    const a = nodePrimitives.deriveKey('pw', new Uint8Array(16).fill(7), 32, 1000)
    const b = nodePrimitives.deriveKey('pw', new Uint8Array(16).fill(8), 32, 1000)
    expect(Array.from(a)).not.toEqual(Array.from(b))
  })
})

describe('concat helper', () => {
  it('joins parts in order', () => {
    const out = concat(new Uint8Array([1, 2]), new Uint8Array([]), new Uint8Array([3]))
    expect(Array.from(out)).toEqual([1, 2, 3])
  })
})
