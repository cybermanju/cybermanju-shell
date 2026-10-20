// CyberManju OS — Container (crates/crypto/src/container.rs).
// Role: Rust twin of the `.cybermanju` CYBMJ01 snapshot container codec.
// Exports: encode_container, decode_container, parse_header, is_container,
//   DecodedContainer, ContainerMeta, magic/version/flag constants, image/KDF caps.
// Integrates: chacha20poly1305, hkdf, hmac, sha2, lz4_flex, rand_core.
// Contracts: byte-identical to `src/utils/container.ts` (magic `CYBMJ01`,
//   PBKDF2-HMAC-SHA512 to 64 bytes then HKDF-SHA256 to 32, ChaCha20-Poly1305,
//   size-prepended lz4) — a TS-written backup restores here and vice versa;
//   errors are plain strings, callers add `auth:/invalid:/integrity:`.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand_core::RngCore;

/// Magic prefix of every `.cybermanju` file.
pub const MAGIC: &[u8; 7] = b"CYBMJ01";
/// Container format version (matches the TS `CONTAINER_VERSION`).
pub const CONTAINER_VERSION: u8 = 1;
/// No key derivation (plaintext container).
pub const KDF_NONE: u8 = 0;
/// PBKDF2-HMAC-SHA512 key derivation.
pub const KDF_PBKDF2_SHA512: u8 = 1;
/// Body is size-prepended lz4.
pub const FLAG_COMPRESSED: u8 = 1;
/// Body is ChaCha20-Poly1305 ciphertext.
pub const FLAG_ENCRYPTED: u8 = 2;
/// Password-derivation cost (matches the TS `DEFAULT_ITERATIONS`).
pub const DEFAULT_ITERATIONS: u32 = 100_000;

const HEADER_FIXED: usize = 16;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const AEAD_TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;
/// Hard ceiling shared with the browser/WASM restore path.
pub const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;
/// Bound attacker-controlled PBKDF2 work in imported headers.
pub const MAX_KDF_ITERATIONS: u32 = 1_000_000;
const MAX_CONTAINER_BYTES: usize =
    MAX_IMAGE_BYTES + HEADER_FIXED + SALT_LEN + NONCE_LEN + AEAD_TAG_LEN;
/// PBKDF2 output length (single block, index 1 — matches the TS codec).
const PBKDF2_LEN: usize = 64;
/// HKDF info string (matches the TS codec).
const HKDF_INFO: &[u8] = b"cybermanju-container-v1";

type HmacSha512 = Hmac<sha2::Sha512>;

/// Image bytes plus the flags the container carried.
#[derive(Debug)]
pub struct DecodedContainer {
    pub image: Vec<u8>,
    pub legacy: bool,
    pub compressed: bool,
    pub encrypted: bool,
}

/// Header-only view — no key needed, so callers can report before asking.
#[derive(Debug)]
pub struct ContainerMeta {
    pub version: u8,
    pub kdf: u8,
    pub flags: u8,
    pub encrypted: bool,
    pub compressed: bool,
    pub salt_len: usize,
    pub nonce_len: usize,
    pub iterations: u32,
    pub body_len: usize,
    pub total_len: usize,
}

/// True when the bytes start with the container magic.
pub fn is_container(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() && &bytes[..MAGIC.len()] == MAGIC
}

/// Parse the header without touching the body (and without any key).
pub fn parse_header(bytes: &[u8]) -> Result<ContainerMeta, String> {
    if !is_container(bytes) {
        return Err("invalid: not a .cybermanju container (legacy raw image?)".to_string());
    }
    if bytes.len() > MAX_CONTAINER_BYTES {
        return Err("too_large: container exceeds the 256 MiB image cap".to_string());
    }
    if bytes.len() < HEADER_FIXED {
        return Err("invalid: container header truncated".to_string());
    }
    let version = bytes[7];
    if version != CONTAINER_VERSION {
        return Err(format!("invalid: container version {version}"));
    }
    let kdf = bytes[8];
    let flags = bytes[9];
    let salt_len = bytes[10] as usize;
    let nonce_len = bytes[11] as usize;
    let iterations = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if flags & !(FLAG_COMPRESSED | FLAG_ENCRYPTED) != 0 {
        return Err(format!("invalid: unknown container flags {flags}"));
    }
    let encrypted = flags & FLAG_ENCRYPTED != 0;
    let compressed = flags & FLAG_COMPRESSED != 0;
    if encrypted {
        if kdf != KDF_PBKDF2_SHA512 {
            return Err(format!("invalid: encrypted container kdf {kdf}"));
        }
        if salt_len != SALT_LEN || nonce_len != NONCE_LEN {
            return Err(
                "invalid: encrypted container has invalid salt or nonce length".to_string(),
            );
        }
        if iterations == 0 || iterations > MAX_KDF_ITERATIONS {
            return Err(format!(
                "invalid: PBKDF2 iterations must be between 1 and {MAX_KDF_ITERATIONS}"
            ));
        }
    } else if kdf != KDF_NONE || salt_len != 0 || nonce_len != 0 || iterations != 0 {
        return Err("invalid: plaintext container has encrypted metadata".to_string());
    }
    let body_at = HEADER_FIXED + salt_len + nonce_len;
    if bytes.len() < body_at {
        return Err("invalid: container header truncated".to_string());
    }
    let body_len = bytes.len() - body_at;
    let minimum_body_len =
        (if encrypted { AEAD_TAG_LEN } else { 0 }) + (if compressed { 4 } else { 0 });
    if body_len < minimum_body_len {
        return Err("invalid: container body truncated".to_string());
    }
    Ok(ContainerMeta {
        version,
        kdf,
        flags,
        encrypted,
        compressed,
        salt_len,
        nonce_len,
        iterations,
        body_len: bytes.len() - body_at,
        total_len: bytes.len(),
    })
}

/// PBKDF2-HMAC-SHA512, single block (index 1) — mirrors the TS `deriveKey`
/// before its HKDF step.
fn pbkdf2_sha512(passphrase: &[u8], salt: &[u8], iterations: u32) -> [u8; PBKDF2_LEN] {
    let mut block = Vec::with_capacity(salt.len() + 4);
    block.extend_from_slice(salt);
    block.extend_from_slice(&1u32.to_be_bytes());
    let mut mac =
        <HmacSha512 as Mac>::new_from_slice(passphrase).expect("hmac takes any key length");
    mac.update(&block);
    let mut acc = mac.finalize().into_bytes();
    let mut prev = acc;
    for _ in 1..iterations {
        let mut round =
            <HmacSha512 as Mac>::new_from_slice(passphrase).expect("hmac takes any key length");
        round.update(&prev);
        prev = round.finalize().into_bytes();
        for (a, b) in acc.iter_mut().zip(prev.iter()) {
            *a ^= *b;
        }
    }
    acc.into()
}

/// Full key schedule: PBKDF2-HMAC-SHA512 then HKDF-SHA256 (salt, info).
fn derive_key(passphrase: &str, salt: &[u8], iterations: u32) -> Result<[u8; KEY_LEN], String> {
    let stretched = pbkdf2_sha512(passphrase.as_bytes(), salt, iterations);
    let hk = Hkdf::<sha2::Sha256>::new(Some(salt), &stretched);
    let mut key = [0u8; KEY_LEN];
    hk.expand(HKDF_INFO, &mut key)
        .map_err(|_| "integrity: container key schedule failed".to_string())?;
    Ok(key)
}

fn assemble(image: &[u8], passphrase: &str, salt: &[u8], nonce: &[u8]) -> Result<Vec<u8>, String> {
    if image.len() > MAX_IMAGE_BYTES {
        return Err("too_large: backup image exceeds the 256 MiB container cap".to_string());
    }
    let encrypted = !passphrase.is_empty();
    let salt = if encrypted { salt } else { &[][..] };
    let nonce = if encrypted { nonce } else { &[][..] };
    let iterations = if encrypted { DEFAULT_ITERATIONS } else { 0 };
    let mut payload = image.to_vec();
    let mut compressed = false;
    let packed = lz4_flex::compress_prepend_size(image);
    if packed.len() < image.len() {
        payload = packed;
        compressed = true;
    }
    let mut body = payload;
    if encrypted {
        let key = derive_key(passphrase, salt, iterations)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| format!("integrity: container cipher failed: {e}"))?;
        body = cipher
            .encrypt(Nonce::from_slice(nonce), body.as_slice())
            .map_err(|e| format!("integrity: container seal failed: {e}"))?;
    }
    let flags = (compressed as u8 * FLAG_COMPRESSED) | (encrypted as u8 * FLAG_ENCRYPTED);
    let mut out = Vec::with_capacity(HEADER_FIXED + salt.len() + nonce.len() + body.len());
    out.extend_from_slice(MAGIC);
    out.push(CONTAINER_VERSION);
    out.push(if encrypted {
        KDF_PBKDF2_SHA512
    } else {
        KDF_NONE
    });
    out.push(flags);
    out.push(salt.len() as u8);
    out.push(nonce.len() as u8);
    out.extend_from_slice(&iterations.to_le_bytes());
    out.extend_from_slice(salt);
    out.extend_from_slice(nonce);
    out.extend_from_slice(&body);
    Ok(out)
}

/// Wrap a database image into a `.cybermanju` container. An empty passphrase
/// stores it compressed-but-plaintext (still a container, so a later save
/// can encrypt it without re-exporting).
pub fn encode_container(image: &[u8], passphrase: &str) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand_core::OsRng.fill_bytes(&mut salt);
    rand_core::OsRng.fill_bytes(&mut nonce);
    assemble(image, passphrase, &salt, &nonce)
}

/// Deterministic variant (fixed salt/nonce) for tests and vectors.
pub fn encode_container_with(
    image: &[u8],
    passphrase: &str,
    salt: &[u8; SALT_LEN],
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, String> {
    assemble(image, passphrase, salt, nonce)
}

/// Read a container (or a legacy raw image) back to bytes. A wrong
/// passphrase fails closed with an authentication error.
pub fn decode_container(bytes: &[u8], passphrase: &str) -> Result<DecodedContainer, String> {
    if !is_container(bytes) {
        if bytes.is_empty() {
            return Err("invalid: backup image is empty".to_string());
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err("too_large: legacy backup image exceeds the 256 MiB cap".to_string());
        }
        return Ok(DecodedContainer {
            image: bytes.to_vec(),
            legacy: true,
            compressed: false,
            encrypted: false,
        });
    }
    let meta = parse_header(bytes)?;
    if meta.kdf != KDF_NONE && meta.kdf != KDF_PBKDF2_SHA512 {
        return Err(format!("invalid: container kdf {}", meta.kdf));
    }
    let body_at = HEADER_FIXED + meta.salt_len + meta.nonce_len;
    let salt = &bytes[HEADER_FIXED..HEADER_FIXED + meta.salt_len];
    let nonce = &bytes[HEADER_FIXED + meta.salt_len..body_at];
    let mut payload = bytes[body_at..].to_vec();
    if meta.encrypted {
        if meta.kdf != KDF_PBKDF2_SHA512 {
            return Err("invalid: encrypted container without kdf".to_string());
        }
        if passphrase.is_empty() {
            return Err(
                "auth: container is passphrase-sealed (wrong or missing passphrase)".to_string(),
            );
        }
        let key = derive_key(passphrase, salt, meta.iterations)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| format!("integrity: container cipher failed: {e}"))?;
        payload = cipher
            .decrypt(Nonce::from_slice(nonce), payload.as_slice())
            .map_err(|_| {
                "auth: container refused the passphrase (wrong key or damaged body)".to_string()
            })?;
    }
    let max_payload = MAX_IMAGE_BYTES + if meta.compressed { 4 } else { 0 };
    if payload.len() > max_payload {
        return Err("too_large: container payload exceeds the 256 MiB image cap".to_string());
    }
    if meta.compressed {
        if payload.len() < 4 {
            return Err("integrity: container lz4 body truncated".to_string());
        }
        let declared_len =
            u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        if declared_len > MAX_IMAGE_BYTES {
            return Err("too_large: compressed backup declares an image over 256 MiB".to_string());
        }
        payload = lz4_flex::decompress_size_prepended(&payload)
            .map_err(|e| format!("integrity: container lz4 body failed: {e}"))?;
    }
    if payload.is_empty() {
        return Err("invalid: backup image is empty".to_string());
    }
    if payload.len() > MAX_IMAGE_BYTES {
        return Err("too_large: decoded backup image exceeds the 256 MiB cap".to_string());
    }
    Ok(DecodedContainer {
        image: payload,
        legacy: false,
        compressed: meta.compressed,
        encrypted: meta.encrypted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("test hex"))
            .collect()
    }

    /// 64-byte fixed image the TS encoder produced the vectors from.
    const VEC_IMG: &str = "0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc0126";
    /// `encodeContainer(image, 'vector-pass', deterministic-primitives)`.
    const VEC_ENC: &str = "4359424d4a3031010102100ca0860100424242424242424242424242424242422424242424242424242424243146d778b80d801a9bdd2b6a97eac7928473e841dcc92abc26e652e0f577d772d8af9c92a3a527fac0b9df98c4d6e38e3b27c451cb16998b238297ee72e399537076e896845425cdef1bf5a342ba8723";
    /// `encodeContainer(image, '', deterministic-primitives)`.
    const VEC_PLAIN: &str = "4359424d4a30310100000000000000000b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc0126";

    #[test]
    fn ts_encrypted_vector_decodes() {
        let bytes = unhex(VEC_ENC);
        let meta = parse_header(&bytes).expect("vector header parses");
        assert_eq!(meta.version, CONTAINER_VERSION);
        assert_eq!(meta.kdf, KDF_PBKDF2_SHA512);
        assert!(meta.encrypted);
        assert!(!meta.compressed);
        assert_eq!(meta.iterations, DEFAULT_ITERATIONS);
        assert_eq!((meta.salt_len, meta.nonce_len), (SALT_LEN, NONCE_LEN));
        let out = decode_container(&bytes, "vector-pass").expect("vector opens");
        assert!(!out.legacy);
        assert_eq!(hex(&out.image), VEC_IMG);
    }

    #[test]
    fn ts_plaintext_vector_decodes() {
        let bytes = unhex(VEC_PLAIN);
        let out = decode_container(&bytes, "").expect("plaintext vector opens");
        assert!(!out.encrypted);
        assert_eq!(hex(&out.image), VEC_IMG);
    }

    #[test]
    fn wrong_passphrase_fails_closed() {
        let err = decode_container(&unhex(VEC_ENC), "nope").expect_err("must refuse");
        assert!(err.starts_with("auth:"), "fails closed, got: {err}");
    }

    #[test]
    fn legacy_raw_image_passes_through() {
        let raw = b"not a container at all";
        let out = decode_container(raw, "").expect("legacy passes");
        assert!(out.legacy);
        assert_eq!(out.image, raw);
    }

    #[test]
    fn truncated_header_refuses() {
        let err = parse_header(b"CYBMJ01\x01").expect_err("must refuse");
        assert!(
            err.contains("truncated") || err.contains("invalid"),
            "got: {err}"
        );
    }

    #[test]
    fn malformed_header_metadata_refuses() {
        let mut bad_nonce = unhex(VEC_ENC);
        bad_nonce[11] = 11;
        assert!(parse_header(&bad_nonce).is_err());

        let mut excessive_kdf = unhex(VEC_ENC);
        excessive_kdf[12..16].copy_from_slice(&(MAX_KDF_ITERATIONS + 1).to_le_bytes());
        assert!(parse_header(&excessive_kdf).is_err());

        let mut unknown_flags = unhex(VEC_PLAIN);
        unknown_flags[9] = 4;
        assert!(parse_header(&unknown_flags).is_err());

        let mut plaintext_with_kdf = unhex(VEC_PLAIN);
        plaintext_with_kdf[8] = KDF_PBKDF2_SHA512;
        assert!(parse_header(&plaintext_with_kdf).is_err());
    }

    #[test]
    fn compressed_size_bomb_refuses_before_lz4_decode() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&[CONTAINER_VERSION, KDF_NONE, FLAG_COMPRESSED, 0, 0]);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let declared = u32::try_from(MAX_IMAGE_BYTES + 1).expect("cap fits in u32");
        bytes.extend_from_slice(&declared.to_le_bytes());

        let err = decode_container(&bytes, "").expect_err("oversized output must refuse");
        assert!(err.starts_with("too_large:"), "got: {err}");
    }

    #[test]
    fn round_trip_sealed_and_plain() {
        let image: Vec<u8> = (0..1024u32)
            .map(|i| (i.wrapping_mul(2654435761) % 251) as u8)
            .collect();
        let sealed = encode_container(&image, "round-trip").expect("encode");
        assert!(is_container(&sealed));
        let back = decode_container(&sealed, "round-trip").expect("decode");
        assert_eq!(back.image, image);
        assert!(back.encrypted);
        let plain = encode_container(&image, "").expect("plain encode");
        let back = decode_container(&plain, "").expect("plain decode");
        assert_eq!(back.image, image);
        assert!(!back.encrypted);
    }

    #[test]
    fn compressible_input_sets_the_flag() {
        let image = vec![7u8; 4096];
        let sealed = encode_container(&image, "z").expect("encode");
        let meta = parse_header(&sealed).expect("header");
        assert!(meta.compressed);
        assert!(sealed.len() < image.len());
        let back = decode_container(&sealed, "z").expect("decode");
        assert_eq!(back.image, image);
    }
}
