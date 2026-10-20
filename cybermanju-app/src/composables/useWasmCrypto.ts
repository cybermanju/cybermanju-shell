// CyberManju OS — encryption + compression in the browser build
//
// Everything here runs on the wasm-pack exports (`crates/os-wasm/src/
// crypto.rs`, `compression.rs`) and stores its state in the `.cybermanju`
// vault through `useVault` — keys, per-file metadata and the transformed
// bytes all live in redb's `kv` table, so a file that is encrypted here
// stays encrypted when the same container opens on the desktop app's table
// layout (same `files` row, body in `content:<id>`).
//
// Honest algorithm mapping (the wasm pack ships X25519 + ML-DSA-65 +
// ChaCha20-Poly1305, and NO ML-KEM/Frodo/AES-GCM):
//
//   kyber1024 / frodokem1344 / hybrid → X25519 keypair, content key = HKDF
//   dilithium5                        → ML-DSA-65 keypair + signature
//   aes256                            → random 32-byte key
//   every algorithm                   → body encrypted with ChaCha20-Poly1305
//
// The `algorithmDisplay` strings say exactly that, so nothing in the UI
// claims a cipher the browser never ran.

import type {
  CompressionStats,
  EncryptionAlgo,
  EncryptionKeyInfo,
  EncryptionStatus,
  LayerDetail,
} from '@/types'
import { ENCRYPTION_INFO } from '@/types'
import { vaultDelete, vaultGet, vaultList, vaultSet, vaultGetJson, vaultSetJson } from './useVault'
import { wasmModuleExports } from './useWasmBackend'

interface WasmCrypto {
  x25519_generate_keypair(): { privateKey: Uint8Array; publicKey: Uint8Array }
  ml_dsa65_generate_keypair(): { privateKey: Uint8Array; publicKey: Uint8Array }
  ml_dsa65_sign(message: Uint8Array, privateKey: Uint8Array): Uint8Array
  ml_dsa65_verify(message: Uint8Array, signature: Uint8Array, publicKey: Uint8Array): boolean
  chacha20_generate_key(): Uint8Array
  chacha20_generate_nonce(): Uint8Array
  chacha20_encrypt(key: Uint8Array, nonce: Uint8Array, plaintext: Uint8Array): Uint8Array
  chacha20_decrypt(key: Uint8Array, nonce: Uint8Array, ciphertext: Uint8Array): Uint8Array
  hkdf_derive(secret: Uint8Array, salt: Uint8Array, info: Uint8Array, length: number): Uint8Array
  blake3_hash(data: Uint8Array): string
  compress_lz4(data: Uint8Array): Uint8Array
  decompress_lz4(data: Uint8Array): Uint8Array
  compress_brotli(data: Uint8Array, quality: number): Uint8Array
  decompress_brotli(data: Uint8Array): Uint8Array
  /** Present once the deployed pkg includes the ruzstd decode export. */
  decompress_zstd?(data: Uint8Array): Uint8Array
}

let wasmCrypto: WasmCrypto | null = null

async function wasm(): Promise<WasmCrypto> {
  if (!wasmCrypto) {
    wasmCrypto = await wasmModuleExports<WasmCrypto>()
  }
  return wasmCrypto
}

// ── codecs ────────────────────────────────────────────────────────────

function toHex(bytes: Uint8Array): string {
  let out = ''
  for (const b of bytes) out += b.toString(16).padStart(2, '0')
  return out
}

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  return out
}

function bytesToB64(bytes: Uint8Array): string {
  let bin = ''
  const CHUNK = 0x8000
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK))
  }
  return btoa(bin)
}

function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

function utf8(text: string): Uint8Array {
  return new TextEncoder().encode(text)
}

function fromUtf8(bytes: Uint8Array): string {
  return new TextDecoder().decode(bytes)
}

function isValidUtf8(bytes: Uint8Array): boolean {
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    return true
  } catch {
    return false
  }
}

function concatBytes(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length)
  out.set(a, 0)
  out.set(b, a.length)
  return out
}

/** AES-256 slot gets ChaCha20; the ML-KEM/Frodo slots get X25519. */
function honestDisplay(algorithm: EncryptionAlgo): string {
  switch (algorithm) {
    case 'dilithium5':
      return 'ML-DSA-65 + ChaCha20-Poly1305'
    case 'aes256':
      return 'ChaCha20-Poly1305 (AES-256 slot)'
    case 'frodokem1344':
      return 'X25519 + ChaCha20-Poly1305 (Frodo slot)'
    case 'hybrid':
      return 'X25519 + ChaCha20-Poly1305 (hybrid slot)'
    case 'kyber1024':
    default:
      return 'X25519 + ChaCha20-Poly1305 (ML-KEM slot)'
  }
}

// ── key storage (`config:key:*` + `secret:key:*` inside the vault) ────

interface KeySecret {
  privateHex: string
  publicHex: string
  algorithm: EncryptionAlgo
}

function newKeyId(): string {
  return `key-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

export async function generateKeyPair(algorithm: EncryptionAlgo): Promise<EncryptionKeyInfo> {
  const w = await wasm()
  const id = newKeyId()
  let secret: KeySecret

  if (algorithm === 'dilithium5') {
    const kp = w.ml_dsa65_generate_keypair()
    secret = { privateHex: toHex(kp.privateKey), publicHex: toHex(kp.publicKey), algorithm }
  } else if (algorithm === 'aes256') {
    const key = w.chacha20_generate_key()
    // Symmetric slot: the "public" half is a fingerprint, not a key.
    secret = {
      privateHex: toHex(key),
      publicHex: w.blake3_hash(key),
      algorithm,
    }
  } else {
    const kp = w.x25519_generate_keypair()
    secret = { privateHex: toHex(kp.privateKey), publicHex: toHex(kp.publicKey), algorithm }
  }

  const info: EncryptionKeyInfo = {
    id,
    algorithm,
    algorithmDisplay: honestDisplay(algorithm),
    nistLevel: ENCRYPTION_INFO[algorithm]?.nistLevel ?? 0,
    color: ENCRYPTION_INFO[algorithm]?.color ?? '#FFFFFF',
    publicKeyPreview: secret.publicHex.slice(0, 16),
    hasPrivateKey: true,
    createdAt: new Date().toISOString(),
  }
  await vaultSetJson(`config:key:${id}`, info)
  await vaultSetJson(`secret:key:${id}`, secret)
  return info
}

export async function listKeys(): Promise<EncryptionKeyInfo[]> {
  const rows = await vaultList('config:key:')
  const keys: EncryptionKeyInfo[] = []
  for (const row of rows) {
    const info = await vaultGetJson<EncryptionKeyInfo>(row.key)
    if (info?.id) keys.push(info)
  }
  return keys.sort((a, b) => (a.createdAt < b.createdAt ? 1 : -1))
}

async function loadSecret(keyId: string): Promise<KeySecret | null> {
  return vaultGetJson<KeySecret>(`secret:key:${keyId}`)
}

// ── per-file meta (`meta:<fileId>`) ───────────────────────────────────

interface FileMeta {
  enc?: { keyId: string; algorithm: EncryptionAlgo; display: string; nonceHex: string; sigHex?: string; at: string }
  comp?: { layers: string[]; originalSize: number; at: string }
}

const contentKey = (fileId: string) => `content:${fileId}`
const encodingKey = (fileId: string) => `encoding:${fileId}`
const metaKey = (fileId: string) => `meta:${fileId}`

async function readMeta(fileId: string): Promise<FileMeta> {
  return (await vaultGetJson<FileMeta>(metaKey(fileId))) ?? {}
}

async function writeMeta(fileId: string, meta: FileMeta): Promise<void> {
  const empty = !meta.enc && !meta.comp
  if (empty) await vaultDelete(metaKey(fileId))
  else await vaultSetJson(metaKey(fileId), meta)
}

async function contentNode(fileId: string): Promise<Record<string, unknown> | null> {
  try {
    const { wasmDbDispatch } = await import('./useWasmBackend')
    const node = (await wasmDbDispatch('files.get', { fileId })) as Record<string, unknown> | null
    return node && typeof node === 'object' ? node : null
  } catch {
    return null
  }
}

async function patchNode(fileId: string, patch: Record<string, unknown>): Promise<void> {
  const { wasmDbDispatch } = await import('./useWasmBackend')
  await wasmDbDispatch('files.patch', { fileId, patch })
}

// ── content pipeline: decode (read) / encode (write) ──────────────────

async function keyFor(secret: KeySecret): Promise<Uint8Array> {
  const w = await wasm()
  const priv = fromHex(secret.privateHex)
  const saltSource = fromHex(secret.publicHex)
  const salt = saltSource.subarray(0, 32)
  const info = utf8(`cybermanju-file-v1:${secret.algorithm}`)
  return w.hkdf_derive(priv, salt, info, 32)
}

/** Vault bytes → plaintext bytes (decrypt → decompress). */
async function decodeStored(
  fileId: string,
  raw: string,
): Promise<{ bytes: Uint8Array; meta: FileMeta }> {
  const w = await wasm()
  const meta = await readMeta(fileId)
  const encoding = await vaultGet(encodingKey(fileId))
  let bytes = encoding === 'base64' ? b64ToBytes(raw) : utf8(raw)

  if (meta.enc) {
    const secret = await loadSecret(meta.enc.keyId)
    if (!secret) {
      throw new Error(
        `encryption key ${meta.enc.keyId} is missing from this vault — the file cannot be decrypted here`,
      )
    }
    const key = await keyFor(secret)
    const nonce = fromHex(meta.enc.nonceHex)
    // Older browser builds persisted nonce || ciphertext but then handed the
    // whole frame to Rust's AEAD decrypt export, which accepts ciphertext only.
    // Recognize that frame by its exact metadata nonce; otherwise treat bytes
    // as ciphertext-only. Never guess/strip arbitrary bytes from a damaged file.
    const framed = nonce.length > 0 && bytes.length >= nonce.length &&
      nonce.every((byte, index) => bytes[index] === byte)
    const cipher = framed ? bytes.subarray(nonce.length) : bytes
    const plain = w.chacha20_decrypt(key, nonce, cipher)
    if (meta.enc.sigHex) {
      const hash = fromHex(w.blake3_hash(plain))
      const ok = w.ml_dsa65_verify(hash, fromHex(meta.enc.sigHex), fromHex(secret.publicHex))
      if (!ok) throw new Error('ML-DSA-65 signature check failed — this content was modified')
    }
    bytes = plain
  }

  if (meta.comp?.layers.length) {
    bytes = await decompressBytes(w, bytes, meta.comp.layers)
  }
  return { bytes, meta }
}

/** Plaintext bytes → vault bytes (compress → encrypt), writing keys + node. */
async function encodeStored(
  fileId: string,
  bytes: Uint8Array,
  meta: FileMeta,
): Promise<{ stored: string; byteLength: number; final: Uint8Array; compDetails: LayerDetail[] }> {
  const w = await wasm()
  let out = bytes
  const compDetails: LayerDetail[] = []

  if (meta.comp?.layers.length) {
    for (const layer of meta.comp.layers) {
      const step = await compressBytes(w, out, layer)
      out = step.bytes
      compDetails.push(...step.details)
    }
    // The pipeline always re-runs from plaintext, so this is the size the
    // stats compare against after an edit.
    meta.comp.originalSize = bytes.length
  }

  if (meta.enc) {
    const secret = await loadSecret(meta.enc.keyId)
    if (!secret) {
      throw new Error(`encryption key ${meta.enc.keyId} is missing from this vault`)
    }
    // Sign the plaintext first, then seal it: the signature proves the
    // content, the AEAD protects the transport.
    if (meta.enc.algorithm === 'dilithium5') {
      const hash = fromHex(w.blake3_hash(out))
      meta.enc.sigHex = toHex(w.ml_dsa65_sign(hash, fromHex(secret.privateHex)))
    }
    const nonce = w.chacha20_generate_nonce()
    const cipher = w.chacha20_encrypt(await keyFor(secret), nonce, out)
    meta.enc.nonceHex = toHex(nonce)
    meta.enc.at = new Date().toISOString()
    out = concatBytes(nonce, cipher)
  }

  const transformed = !!meta.enc || !!meta.comp?.layers.length || out.length !== bytes.length
  // Untransformed binary payloads must remain bytes; UTF-8 replacement
  // decoding would irreversibly alter invalid byte sequences on decrypt.
  const base64Stored = transformed || !isValidUtf8(out)
  const stored = base64Stored ? bytesToB64(out) : fromUtf8(out)
  const bodyKey = contentKey(fileId)
  const encKey = encodingKey(fileId)
  const metadataKey = metaKey(fileId)
  const before = {
    body: await vaultGet(bodyKey),
    encoding: await vaultGet(encKey),
    meta: await vaultGet(metadataKey),
    node: await contentNode(fileId),
  }
  const nodePatch = {
    sizeBytes: out.length,
    hashBlake3: w.blake3_hash(out),
    encrypted: !!meta.enc,
    encryptionAlgorithm: meta.enc?.display ?? null,
    compressionLayers: meta.comp?.layers ?? [],
  }
  try {
    await vaultSet(bodyKey, stored)
    if ((await vaultGet(bodyKey)) !== stored) throw new Error('body write did not persist')
    if (base64Stored) {
      await vaultSet(encKey, 'base64')
      if ((await vaultGet(encKey)) !== 'base64') throw new Error('encoding write did not persist')
    } else {
      await vaultDelete(encKey)
      if ((await vaultGet(encKey)) !== null) throw new Error('encoding deletion did not persist')
    }
    await writeMeta(fileId, meta)
    const expectedMeta = meta.enc || meta.comp?.layers.length ? JSON.stringify(meta) : null
    if ((await vaultGet(metadataKey)) !== expectedMeta) throw new Error('metadata write did not persist')
    await patchNode(fileId, nodePatch)
  } catch (error) {
    // redb kv has no multi-key transaction API. Preserve the prior version and
    // restore all three vault records if any leg (including node bookkeeping)
    // fails, rather than returning success with mismatched ciphertext/metadata.
    const rollbackErrors: string[] = []
    for (const [key, value] of [[bodyKey, before.body], [encKey, before.encoding], [metadataKey, before.meta]] as const) {
      try {
        if (value === null) await vaultDelete(key)
        else await vaultSet(key, value)
        if ((await vaultGet(key)) !== value) throw new Error(`restore verification failed for ${key}`)
      } catch (restoreError) {
        rollbackErrors.push(restoreError instanceof Error ? restoreError.message : String(restoreError))
      }
    }
    if (before.node) {
      try {
        await patchNode(fileId, {
          sizeBytes: before.node.sizeBytes ?? null,
          hashBlake3: before.node.hashBlake3 ?? null,
          encrypted: before.node.encrypted ?? false,
          encryptionAlgorithm: before.node.encryptionAlgorithm ?? null,
          compressionLayers: before.node.compressionLayers ?? [],
        })
      } catch (restoreError) {
        rollbackErrors.push(restoreError instanceof Error ? restoreError.message : String(restoreError))
      }
    }
    const detail = error instanceof Error ? error.message : String(error)
    if (rollbackErrors.length) {
      throw new Error(`vault persistence failed (${detail}); rollback incomplete: ${rollbackErrors.join('; ')}`)
    }
    throw new Error(`vault persistence failed (${detail}); previous content restored`)
  }
  return { stored, byteLength: out.length, final: out, compDetails }
}

// ── compression ───────────────────────────────────────────────────────

// zstd + triple run in the browser through `@dweb-browser/zstd-wasm`
// (`src/utils/zstd.ts`, same standard frames as the desktop `zstd` crate) —
// composed with the wasm crate's lz4/brotli in the desktop order
// (LZ4 → ZSTD → Brotli), so a `.cyb3` written here opens on the desktop.

export function compressionCapable(layer: string): boolean {
  return (
    layer === 'none' ||
    layer === 'all' ||
    layer === 'lz4' ||
    layer === 'zstd' ||
    layer === 'brotli' ||
    layer === 'triple'
  )
}

function tripleCodecs(w: WasmCrypto): {
  compressLz4(data: Uint8Array): Uint8Array
  decompressLz4(data: Uint8Array): Uint8Array
  compressBrotli(data: Uint8Array): Uint8Array
  decompressBrotli(data: Uint8Array): Uint8Array
} {
  return {
    compressLz4: (d) => w.compress_lz4(d),
    decompressLz4: (d) => w.decompress_lz4(d),
    compressBrotli: (d) => w.compress_brotli(d, 11),
    decompressBrotli: (d) => w.decompress_brotli(d),
  }
}

/** Per-layer stat rows in the desktop palette (`triple.rs` colors kept). */
function zstdDetail(inputSize: number, outputSize: number): LayerDetail {
  return {
    name: 'Layer 2: Zstandard',
    algorithm: 'zstd level 15',
    inputSize,
    outputSize,
    ratio: inputSize ? outputSize / inputSize : 1,
    color: '#00FF41',
  }
}

async function compressBytes(w: WasmCrypto, bytes: Uint8Array, layer: string): Promise<{
  bytes: Uint8Array
  details: LayerDetail[]
}> {
  if (layer === 'none') return { bytes, details: [] }
  if (layer === 'lz4' || layer === 'brotli') {
    const out = layer === 'lz4' ? w.compress_lz4(bytes) : w.compress_brotli(bytes, 11)
    return {
      bytes: out,
      details: [
        {
          name: layer === 'lz4' ? 'LZ4 (lz4_flex)' : 'Brotli-11',
          algorithm: layer,
          inputSize: bytes.length,
          outputSize: out.length,
          ratio: bytes.length ? out.length / bytes.length : 1,
          color: '#FFFFFF',
        },
      ],
    }
  }
  if (layer === 'zstd') {
    try {
      const { compressZstd } = await import('@/utils/zstd')
      const out = await compressZstd(bytes)
      return {
        bytes: out,
        details: [zstdDetail(bytes.length, out.length)],
      }
    } catch (e) {
      throw withIntegrityPrefix(e, 'zstd compression failed')
    }
  }
  if (layer === 'triple') {
    try {
      const { compressTriple } = await import('@/utils/zstd')
      const { bytes: out, sizes } = await compressTriple(tripleCodecs(w), bytes)
      return {
        bytes: out,
        details: [
          {
            name: 'Layer 1: LZ4',
            algorithm: 'lz4_flex',
            inputSize: bytes.length,
            outputSize: sizes.lz4,
            ratio: bytes.length ? sizes.lz4 / bytes.length : 1,
            color: '#00D4FF',
          },
          zstdDetail(sizes.lz4, sizes.zstd),
          {
            name: 'Layer 3: Brotli',
            algorithm: 'brotli level 11',
            inputSize: sizes.zstd,
            outputSize: sizes.brotli,
            ratio: sizes.zstd ? sizes.brotli / sizes.zstd : 1,
            color: '#FFB800',
          },
        ],
      }
    } catch (e) {
      throw withIntegrityPrefix(e, 'triple compression failed')
    }
  }
  throw new Error(`unsupported: compress layer '${layer}' (none|lz4|zstd|brotli|triple)`)
}

async function decompressBytes(w: WasmCrypto, bytes: Uint8Array, layers: string[]): Promise<Uint8Array> {
  // Applied left→right, so unwind right→left. A stored `triple` marker is
  // one Brotli → ZSTD → LZ4 chain; anything else unwinds layer by layer.
  let out = bytes
  for (let i = layers.length - 1; i >= 0; i--) {
    const layer = layers[i]
    try {
      if (layer === 'lz4') out = w.decompress_lz4(out)
      else if (layer === 'brotli') out = w.decompress_brotli(out)
      else if (layer === 'zstd') out = await decodeZstd(w, out)
      else if (layer === 'triple') {
        const { decompressTriple } = await import('@/utils/zstd')
        out = await decompressTriple(tripleCodecs(w), out)
      } else {
        throw new Error(`unsupported: decompress layer '${layer}' (none|lz4|zstd|brotli|triple)`)
      }
    } catch (e) {
      throw withIntegrityPrefix(e, `${layer} decompression failed`)
    }
  }
  return out
}

/**
 * Raw codec failures carry no house prefix — stamp them `integrity:` so the
 * UI hints match. An `unsupported:` (missing zstd module) passes through
 * untouched: a stale bundle is not a tampered file.
 */
function withIntegrityPrefix(e: unknown, what: string): Error {
  const detail = e instanceof Error ? e.message : String(e)
  if (/^unsupported:/.test(detail)) return e instanceof Error ? e : new Error(detail)
  return new Error(`integrity: ${what} (${detail})`)
}

/** Prefer the crate's `ruzstd` export when deployed; else the JS module. */
async function decodeZstd(w: WasmCrypto, data: Uint8Array): Promise<Uint8Array> {
  if (typeof w.decompress_zstd === 'function') return w.decompress_zstd(data)
  const { decompressZstd } = await import('@/utils/zstd')
  return decompressZstd(data)
}

// ── public file operations (wired to the panel commands) ──────────────

export async function getEncryptionStatus(fileId?: string): Promise<EncryptionStatus> {
  if (fileId) {
    const meta = await readMeta(fileId)
    if (meta.enc) {
      const keys = await listKeys()
      const key = keys.find((k) => k.id === meta.enc?.keyId)
      return {
        isEncrypted: true,
        algorithm: meta.enc.display,
        nistLevel: key?.nistLevel ?? 0,
        keyId: meta.enc.keyId,
        encryptedAt: meta.enc.at,
      }
    }
    return { isEncrypted: false }
  }
  const newest = await listKeys()
  const top = newest[0]
  if (!top) return { isEncrypted: false }
  return {
    isEncrypted: true,
    algorithm: top.algorithmDisplay,
    nistLevel: top.nistLevel,
    keyId: top.id,
    encryptedAt: top.createdAt,
  }
}

export async function encryptFile(fileId: string, algorithm: EncryptionAlgo): Promise<void> {
  const existing = await listKeys()
  let key = existing.find((k) => k.algorithm === algorithm)
  if (!key) key = await generateKeyPair(algorithm)
  const secret = await loadSecret(key.id)
  if (!secret) throw new Error('key generation produced no private material')

  const raw = (await vaultGet(contentKey(fileId))) ?? ''
  const { bytes, meta } = await decodeStored(fileId, raw)
  if (meta.enc) throw new Error('this file is already encrypted — decrypt it first')

  // `encodeStored` generates the nonce, seals with ChaCha20-Poly1305 and
  // (for the ML-DSA slot) signs the plaintext — one pipeline, one place.
  meta.enc = {
    keyId: key.id,
    algorithm,
    display: key.algorithmDisplay,
    nonceHex: '',
    at: new Date().toISOString(),
  }
  await encodeStored(fileId, bytes, meta)
}

export async function decryptFile(fileId: string): Promise<void> {
  const raw = (await vaultGet(contentKey(fileId))) ?? ''
  const { bytes, meta } = await decodeStored(fileId, raw)
  if (!meta.enc) throw new Error('this file is not encrypted')
  delete meta.enc
  await encodeStored(fileId, bytes, meta)
}

export async function compressFile(fileId: string, layer: string): Promise<CompressionStats> {
  const w = await wasm()
  const started = performance.now()
  const raw = (await vaultGet(contentKey(fileId))) ?? ''
  const { bytes, meta } = await decodeStored(fileId, raw)
  if (meta.comp) throw new Error('this file is already compressed — decompress it first')

  const effective = layer === 'all' ? 'brotli' : layer
  if (!compressionCapable(effective)) {
    throw new Error(`unsupported: compress layer '${layer}' (none|lz4|zstd|brotli|triple)`)
  }

  if (effective === 'none') {
    const { final } = await encodeStored(fileId, bytes, meta)
    return {
      originalSize: bytes.length,
      compressedSize: final.length,
      ratio: 1,
      layer,
      layerDetails: [],
      blake3Hash: w.blake3_hash(final),
      durationMs: Math.max(1, Math.round(performance.now() - started)),
    }
  }

  meta.comp = { layers: [effective], originalSize: bytes.length, at: new Date().toISOString() }
  // `encodeStored` runs the pipeline (compress → encrypt) from plaintext, so
  // the input stays untouched — and its per-layer details describe the exact
  // bytes that landed (one run, no re-compress for stats).
  const { final, compDetails: layerDetails } = await encodeStored(fileId, bytes, meta)
  return {
    originalSize: bytes.length,
    compressedSize: final.length,
    ratio: bytes.length ? final.length / bytes.length : 1,
    layer,
    layerDetails,
    blake3Hash: w.blake3_hash(final),
    durationMs: Math.max(1, Math.round(performance.now() - started)),
  }
}

export async function decompressFile(fileId: string): Promise<CompressionStats> {
  const w = await wasm()
  const started = performance.now()
  const raw = (await vaultGet(contentKey(fileId))) ?? ''
  const { bytes, meta } = await decodeStored(fileId, raw)
  if (!meta.comp) throw new Error('this file is not compressed')
  const originalSize = meta.comp.originalSize || bytes.length
  delete meta.comp
  await encodeStored(fileId, bytes, meta)
  return {
    originalSize,
    compressedSize: bytes.length,
    ratio: originalSize ? bytes.length / originalSize : 1,
    layer: 'none',
    layerDetails: [],
    blake3Hash: w.blake3_hash(bytes),
    durationMs: Math.max(1, Math.round(performance.now() - started)),
  }
}

/** Editor read: whatever is stored → displayable text. */
export async function readContentText(fileId: string, raw: string): Promise<string> {
  try {
    const { bytes } = await decodeStored(fileId, raw)
    return fromUtf8(bytes)
  } catch {
    // Missing key / damaged body — show the raw stored text rather than
    // an empty editor, so the user can see something happened.
    return raw
  }
}

/** Editor write: text → whatever this file's pipeline stores. */
export async function writeContentText(fileId: string, text: string): Promise<string> {
  const meta = await readMeta(fileId)
  const { stored } = await encodeStored(fileId, utf8(text), meta)
  return stored
}

/**
 * Plaintext bytes for non-text consumers (face detection, hashing).
 * Runs the full decrypt → decompress pipeline; throws the same honest
 * errors the editor paths surface (missing key, damaged body).
 */
export async function readOriginalBytes(fileId: string): Promise<Uint8Array> {
  const raw = (await vaultGet(contentKey(fileId))) ?? ''
  const { bytes } = await decodeStored(fileId, raw)
  return bytes
}
