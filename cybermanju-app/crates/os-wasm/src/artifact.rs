// CyberManju OS — WASM artifact codec (the read half of canal B).
//
// Undo exactly what `crates/sync/src/pipeline.rs::transform_payload` did on
// the way out, byte for byte, so a browser can open artifacts a desktop
// produced. The layouts are copied from the native sources rather than
// re-invented:
//
//   artifact   = "CYBE1" || salt(16) || nonce(12) || ChaCha20-Poly1305(ct||tag16)
//   plaintext  = Brotli(ZSTD(LZ4(plaintext)))   when compression was on
//   key        = Argon2id(m=19456, t=2, p=1, 32B)(passphrase, salt)
//
// `CYBMJ01` (the browser vault, `src/utils/container.ts`) and `CYBMJU1`
// (the disk container, `crates/disk/src/superblock.rs`) are NOT opened
// here — both magics are reported by `artifact_magic` and handed back to
// the caller untouched, because their codecs live elsewhere.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use wasm_bindgen::prelude::*;

/// Sync/volume artifact magic — `sync::pipeline::CYBE_MAGIC`.
pub const CYBE_MAGIC: &[u8; 5] = b"CYBE1";
/// Browser vault container magic — `src/utils/container.ts`.
pub const CONTAINER_MAGIC: &[u8; 7] = b"CYBMJ01";
/// Disk container magic — `cybermanju_disk::DISK_MAGIC`.
pub const DISK_MAGIC: &[u8; 7] = b"CYBMJU1";

/// AEAD layout lengths (see `cybermanju_crypto::keystore`).
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;

/// Argon2id parameters — pinned to `keystore::derive_key`, never tuned here:
/// a key derived by the desktop must open in the browser and vice versa.
const ARGON2_M_COST: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;
const ARGON2_OUTPUT_LEN: usize = 32;

// ─── sniffing ────────────────────────────────────────────────────────

/// Which of the three magics (if any) fronts these bytes.
/// `"raw"` means "no known header — treat the payload as-is".
pub fn magic_of(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(CYBE_MAGIC) {
        "CYBE1"
    } else if bytes.starts_with(CONTAINER_MAGIC) {
        "CYBMJ01"
    } else if bytes.starts_with(DISK_MAGIC) {
        "CYBMJU1"
    } else {
        "raw"
    }
}

/// JS-facing header probe: cheap, allocation-free, no key material needed.
/// The TypeScript side branches on this before asking for a passphrase.
#[wasm_bindgen]
pub fn artifact_magic(bytes: &[u8]) -> String {
    magic_of(bytes).to_string()
}

// ─── AEAD (mirror of `keystore::seal` / `keystore::open_sealed`) ─────

/// Argon2id derivation with the pinned native parameters.
pub fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], String> {
    let params = argon2::Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| format!("unavailable: argon2 params rejected ({e})"))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; ARGON2_OUTPUT_LEN];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| format!("unavailable: argon2 derivation failed ({e})"))?;
    Ok(out)
}

/// Seal a payload exactly like the native keystore does:
/// `salt(16) || nonce(12) || ct || tag(16)`.
///
/// Rust-visible only (no `#[wasm_bindgen]`): the browser canal reads, it
/// does not mint artifacts — the tests use it as the encode side of the
/// round trip, because the native crate cannot build for wasm32.
pub fn seal(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use chacha20poly1305::aead::OsRng;
    use rand_core::RngCore;

    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let k = derive_key(passphrase, &salt)?;
    let ciphertext = encrypt_with_key(&k, &nonce, plaintext)?;
    let mut out = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Open a blob produced by [`seal`] (or by the native `keystore::seal`).
pub fn open_sealed(passphrase: &str, blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < SALT_LEN + NONCE_LEN + TAG_LEN {
        return Err("integrity: sealed blob is too short".to_string());
    }
    let (salt, rest) = blob.split_at(SALT_LEN);
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let k = derive_key(passphrase, salt)?;
    decrypt_with_key(&k, nonce, ciphertext)
}

fn encrypt_with_key(key: &[u8], nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != NONCE_LEN {
        return Err("integrity: bad nonce length".to_string());
    }
    let cipher = cipher(key)?;
    cipher
        .encrypt(Nonce::from_slice(nonce), plaintext)
        .map_err(|_| "integrity: encryption failed".to_string())
}

fn decrypt_with_key(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != NONCE_LEN {
        return Err("integrity: bad nonce length".to_string());
    }
    let cipher = cipher(key)?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| DECRYPT_ERROR.to_string())
}

fn cipher(key: &[u8]) -> Result<ChaCha20Poly1305, String> {
    ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("integrity: bad ChaCha20-Poly1305 key ({e})"))
}

/// The one error string the UI shows for a failed AEAD open — wrong
/// passphrase and corruption are indistinguishable by design.
const DECRYPT_ERROR: &str =
    "integrity: could not decrypt the artifact (wrong passphrase, or the data is corrupt)";

// ─── triple decompression (mirror of `TripleCompressor::decompress_triple`) ─

/// Brotli → ZSTD → LZ4, the exact inverse of the native encoder chain.
///
/// The native `decode_artifact` treats a failed chain as "the payload was
/// never compressed" and hands the bytes back; call sites here do the same,
/// so this function reports the failure and lets the caller decide.
pub fn decompress_triple(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let brotli_out = brotli_decompress(data)?;
    let zstd_out = zstd_decompress(&brotli_out)?;
    lz4_decompress(&zstd_out)
}

fn brotli_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Write;

    let mut decompressor = brotli::DecompressorWriter::new(Vec::new(), 4096);
    decompressor
        .write_all(data)
        .map_err(|e| format!("integrity: brotli rejected ({e})"))?;
    // `into_inner` hands back the partial output on `Err` — an incomplete
    // stream is exactly how "this was not brotli" shows up.
    decompressor
        .into_inner()
        .map_err(|partial| format!("integrity: brotli stream truncated at {}B", partial.len()))
}

fn zstd_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let mut decoder = ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(data))
        .map_err(|_| "integrity: not a valid zstd frame".to_string())?;
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|_| "integrity: zstd frame truncated")?;
    Ok(out)
}

fn lz4_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    lz4_flex::decompress_size_prepended(data)
        .map_err(|e| format!("integrity: lz4 block rejected ({e})"))
}

// ─── the whole unwrap ────────────────────────────────────────────────

/// Strip one sync layer: `CYBE1` → open → triple-decompress, with the same
/// "a failed chain means it was never compressed" fallback the native
/// `manifest::decode_artifact` uses.
///
/// `passphrase` is the master passphrase (`secrets:masterPassphrase` in the
/// container's `kv`); an unencrypted artifact ignores it entirely.
pub fn decode_artifact(passphrase: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut payload = bytes.to_vec();
    if payload.starts_with(CYBE_MAGIC) {
        if passphrase.is_empty() {
            return Err(
                "integrity: the artifact is encrypted but no master passphrase was provided"
                    .to_string(),
            );
        }
        payload = open_sealed(passphrase, &payload[CYBE_MAGIC.len()..])?;
    }
    match decompress_triple(&payload) {
        Ok(plain) => Ok(plain),
        // Not compressed (or not a layer we recognise): the bytes are the
        // payload — never fabricate data, never fail a readable file.
        Err(_) => Ok(payload),
    }
}

/// JS-facing open: one `Uint8Array` in, plaintext `Uint8Array` out.
/// Errors carry the house prefixes so the UI can show them verbatim.
#[wasm_bindgen]
pub fn artifact_open(bytes: &[u8], passphrase: &str) -> Result<Vec<u8>, JsValue> {
    decode_artifact(passphrase, bytes).map_err(|e| JsValue::from_str(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single-block, no-checksum zstd frame built by hand: raw blocks are
    /// the one frame flavour small enough to synthesise without linking a
    /// (C) zstd encoder, and every decoder must accept them.
    /// Test-only — the frame content size field is a single byte, so the
    /// payload must stay ≤ 255 bytes.
    fn zstd_raw_frame(data: &[u8]) -> Vec<u8> {
        assert!(data.len() <= 255, "test frame helper supports 255 bytes");
        let mut out = Vec::new();
        out.extend_from_slice(&[0x28, 0xB5, 0x2F, 0xFD]); // zstd magic
        out.push(0x20); // single segment, no checksum, no dictionary
        out.push(data.len() as u8); // frame content size (1 byte)
        let block_header = 1u32 | ((data.len() as u32) << 3); // last + raw
        out.extend_from_slice(&block_header.to_le_bytes()[..3]);
        out.extend_from_slice(data);
        out
    }

    fn brotli_compress(data: &[u8]) -> Vec<u8> {
        use std::io::Write;

        let mut compressor = brotli::CompressorWriter::new(Vec::new(), 4096, 5, 22);
        compressor.write_all(data).expect("brotli write");
        compressor.into_inner()
    }

    #[test]
    fn sniffing_reports_each_magic() {
        assert_eq!(magic_of(b"CYBE1payload"), "CYBE1");
        assert_eq!(magic_of(b"CYBMJ01...."), "CYBMJ01");
        assert_eq!(magic_of(b"CYBMJU1...."), "CYBMJU1");
        assert_eq!(magic_of(b"plain bytes"), "raw");
        assert_eq!(magic_of(b""), "raw");
    }

    #[test]
    fn sealed_blob_round_trips() {
        let plaintext = b"the quick brown fox jumps over the lazy dog";
        let blob = seal("correct horse battery staple", plaintext).expect("seal");
        assert_eq!(blob.len(), SALT_LEN + NONCE_LEN + plaintext.len() + TAG_LEN);
        let opened = open_sealed("correct horse battery staple", &blob).expect("open");
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let blob = seal("right", b"secret payload").expect("seal");
        let err = open_sealed("wrong", &blob).expect_err("must not open");
        assert!(err.starts_with("integrity:"), "house prefix, got {err}");
    }

    #[test]
    fn triple_chain_round_trips() {
        let payload: Vec<u8> = (0u8..60)
            .map(|i| i.wrapping_mul(7).wrapping_add(3))
            .collect();
        let lz4 = lz4_flex::compress_prepend_size(&payload);
        let framed = zstd_raw_frame(&lz4);
        let wrapped = brotli_compress(&framed);
        let opened = decompress_triple(&wrapped).expect("triple decode");
        assert_eq!(opened, payload);
    }

    #[test]
    fn unrecognised_bytes_are_not_brotli() {
        assert!(decompress_triple(b"CYBMJ01 not a compressed stream").is_err());
        assert_eq!(decompress_triple(b"").expect("empty"), Vec::<u8>::new());
    }

    #[test]
    fn decode_opens_a_sealed_and_compressed_artifact() {
        let payload = b"file contents that were synced from the desktop";
        let lz4 = lz4_flex::compress_prepend_size(payload.as_slice());
        let wrapped = brotli_compress(&zstd_raw_frame(&lz4));
        let sealed = seal("master-pass", &wrapped).expect("seal");
        let mut artifact = CYBE_MAGIC.to_vec();
        artifact.extend_from_slice(&sealed);
        let opened = decode_artifact("master-pass", &artifact).expect("decode");
        assert_eq!(opened, payload);
    }

    #[test]
    fn decode_hands_containers_through_untouched() {
        let mut container = CONTAINER_MAGIC.to_vec();
        container.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0, 0]);
        let opened = decode_artifact("", &container).expect("pass through");
        assert_eq!(opened, container);
    }

    #[test]
    fn decode_refuses_encrypted_artifacts_without_a_passphrase() {
        let sealed = seal("master-pass", b"payload").expect("seal");
        let mut artifact = CYBE_MAGIC.to_vec();
        artifact.extend_from_slice(&sealed);
        let err = decode_artifact("", &artifact).expect_err("must refuse");
        assert!(err.starts_with("integrity:"), "house prefix, got {err}");
    }
}
