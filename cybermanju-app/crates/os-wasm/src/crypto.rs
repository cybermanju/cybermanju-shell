// CyberManju OS — Crypto (crates/os-wasm/src/crypto.rs).
// Role: browser volume backend (OPFS/localStorage) for Pages.
// Exports: blake3_hash, chacha20_generate_key, chacha20_generate_nonce, chacha20_encrypt, chacha20_decrypt, hkdf_derive, hmac_sha512, x25519_generate_keypair.
// Integrates: hkdf, hmac, ml_dsa, sha2, signature, wasm_bindgen.
// Contracts: signals: engine.
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use ml_dsa::{Generate, Keypair, MlDsa65, SignatureEncoding, SigningKey, VerifyingKey};
use sha2::Sha512;
use signature::Signer;
use wasm_bindgen::prelude::*;
use x25519_dalek::{PublicKey, StaticSecret};

type HmacSha512 = Hmac<Sha512>;

#[wasm_bindgen]
pub fn blake3_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

#[wasm_bindgen]
pub fn chacha20_generate_key() -> Vec<u8> {
    use chacha20poly1305::aead::OsRng as AeadOsRng;
    use rand_core::RngCore;
    let mut key = [0u8; 32];
    AeadOsRng.fill_bytes(&mut key);
    key.to_vec()
}

#[wasm_bindgen]
pub fn chacha20_generate_nonce() -> Vec<u8> {
    use rand_core::RngCore;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    nonce.to_vec()
}

#[wasm_bindgen]
pub fn chacha20_encrypt(key: &[u8], nonce: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, JsValue> {
    let key_arr: [u8; 32] = key
        .try_into()
        .map_err(|_| JsValue::from_str("Key must be 32 bytes"))?;
    let nonce_arr: [u8; 12] = nonce
        .try_into()
        .map_err(|_| JsValue::from_str("Nonce must be 12 bytes"))?;

    let cipher = ChaCha20Poly1305::new_from_slice(&key_arr)
        .map_err(|e| JsValue::from_str(&format!("Failed to create cipher: {}", e)))?;
    let nonce = Nonce::from(nonce_arr);

    cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| JsValue::from_str(&format!("Encryption failed: {}", e)))
}

#[wasm_bindgen]
pub fn chacha20_decrypt(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, JsValue> {
    let key_arr: [u8; 32] = key
        .try_into()
        .map_err(|_| JsValue::from_str("Key must be 32 bytes"))?;
    let nonce_arr: [u8; 12] = nonce
        .try_into()
        .map_err(|_| JsValue::from_str("Nonce must be 12 bytes"))?;

    let cipher = ChaCha20Poly1305::new_from_slice(&key_arr)
        .map_err(|e| JsValue::from_str(&format!("Failed to create cipher: {}", e)))?;
    let nonce = Nonce::from(nonce_arr);

    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| JsValue::from_str(&format!("Decryption failed: {}", e)))
}

#[wasm_bindgen]
pub fn hkdf_derive(secret: &[u8], salt: &[u8], info: &[u8], length: usize) -> Vec<u8> {
    let hk = Hkdf::<sha2::Sha256>::new(Some(salt), secret);
    let mut okm = vec![0u8; length];
    hk.expand(info, &mut okm).expect("HKDF expand failed");
    okm
}

#[wasm_bindgen]
pub fn hmac_sha512(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = <HmacSha512 as Mac>::new_from_slice(key).expect("HMAC key should be valid");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[wasm_bindgen]
pub fn x25519_generate_keypair() -> Result<js_sys::Object, JsValue> {
    use rand_core::OsRng;

    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);

    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("privateKey"),
        &js_sys::Uint8Array::from(secret.to_bytes().as_slice()),
    )?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("publicKey"),
        &js_sys::Uint8Array::from(public.as_bytes().as_slice()),
    )?;
    Ok(obj)
}

#[wasm_bindgen]
pub fn x25519_shared_secret(private_key: &[u8], peer_public: &[u8]) -> Result<Vec<u8>, JsValue> {
    let private_arr: [u8; 32] = private_key
        .try_into()
        .map_err(|_| JsValue::from_str("Private key must be 32 bytes"))?;
    let public_arr: [u8; 32] = peer_public
        .try_into()
        .map_err(|_| JsValue::from_str("Public key must be 32 bytes"))?;

    let secret = StaticSecret::from(private_arr);
    let public = PublicKey::from(public_arr);
    let shared = secret.diffie_hellman(&public);
    Ok(shared.as_bytes().to_vec())
}

#[wasm_bindgen]
pub fn ml_dsa65_generate_keypair() -> Result<js_sys::Object, JsValue> {
    let sk = SigningKey::<MlDsa65>::generate();
    let seed = sk.to_seed();
    let verifying_key = sk.verifying_key().encode();

    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("privateKey"),
        &js_sys::Uint8Array::from(seed.as_slice()),
    )?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("publicKey"),
        &js_sys::Uint8Array::from(verifying_key.as_slice()),
    )?;
    Ok(obj)
}

#[wasm_bindgen]
pub fn ml_dsa65_sign(message: &[u8], private_key: &[u8]) -> Result<Vec<u8>, JsValue> {
    let signing_key = <SigningKey<MlDsa65> as ml_dsa::KeyInit>::new_from_slice(private_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid private key: {}", e)))?;
    let signature = signing_key.sign(message);
    Ok(signature.to_bytes().to_vec())
}

#[wasm_bindgen]
pub fn ml_dsa65_verify(
    message: &[u8],
    signature: &[u8],
    public_key: &[u8],
) -> Result<bool, JsValue> {
    use signature::Verifier;

    let verifying_key = <VerifyingKey<MlDsa65> as ml_dsa::KeyInit>::new_from_slice(public_key)
        .map_err(|e| JsValue::from_str(&format!("Invalid public key: {}", e)))?;
    let sig = ml_dsa::Signature::<MlDsa65>::try_from(signature)
        .map_err(|e| JsValue::from_str(&format!("Invalid signature: {:?}", e)))?;

    Ok(verifying_key.verify(message, &sig).is_ok())
}

// ─── seal:v1 blob helpers (Phase 3 — secrets keystore) ────────────────────
//
// Byte-identical to the native `keystore::seal` / `open_sealed`: the WASM
// build seals the session vault passphrase (held after unlock) and stores
// the base64 blob in the `secrets` table. The encode side is
// `artifact::seal` (same Argon2id m=19456,t=2,p=1 + ChaCha20Poly1305).

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding — same alphabet/-padding as the native
/// `base64 0.22` STANDARD engine, so the on-disk strings are identical.
fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn b64_decode(text: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Result<u32, String> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as u32),
            b'a'..=b'z' => Ok((c - b'a') as u32 + 26),
            b'0'..=b'9' => Ok((c - b'0') as u32 + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(format!("invalid: bad base64 byte {c}")),
        }
    }
    let bytes = text.trim().as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("invalid: base64 length is not a multiple of 4".to_string());
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && chunk[3] != b'=' && chunk[2] != b'=') {
            return Err("invalid: misplaced base64 padding".to_string());
        }
        let n = (val(chunk[0])? << 18)
            | (val(chunk[1])? << 12)
            | (val(chunk[2]).unwrap_or(0) << 6)
            | val(chunk[3]).unwrap_or(0);
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

/// Seal `plaintext` under `passphrase` in the `seal:v1` layout
/// (`salt(16) || nonce(12) || ct+tag`) and return the base64 string the
/// `secrets` table stores. The session passphrase is held after unlock and
/// never persisted — reveal/open take it as an argument.
#[wasm_bindgen]
pub fn seal_blob(passphrase: &str, plaintext: &[u8]) -> Result<String, JsValue> {
    crate::artifact::seal(passphrase, plaintext)
        .map(|bytes| b64_encode(&bytes))
        .map_err(|e| JsValue::from_str(&e))
}

/// Open a base64 `seal:v1` blob produced by [`seal_blob`] (or by the native
/// `keystore::seal_str`) and return the UTF-8 plaintext.
#[wasm_bindgen]
pub fn open_blob(passphrase: &str, encoded: &str) -> Result<String, JsValue> {
    open_blob_str(passphrase, encoded).map_err(|e| JsValue::from_str(&e))
}

/// Rust-visible [`seal_blob`] — used by the `secrets.*` db ops.
pub(crate) fn seal_blob_str(passphrase: &str, plaintext: &str) -> Result<String, String> {
    crate::artifact::seal(passphrase, plaintext.as_bytes()).map(|bytes| b64_encode(&bytes))
}

/// Rust-visible [`open_blob`] — used by the `secrets.*` db ops.
pub(crate) fn open_blob_str(passphrase: &str, encoded: &str) -> Result<String, String> {
    let bytes = b64_decode(encoded)?;
    let plain = crate::artifact::open_sealed(passphrase, &bytes)?;
    String::from_utf8(plain).map_err(|_| "sealed blob is not UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64_round_trips_edge_lengths() {
        for len in 0..13usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let encoded = b64_encode(&data);
            assert_eq!(b64_decode(&encoded).expect("decode"), data, "len {len}");
        }
    }

    #[test]
    fn b64_matches_standard_vectors() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64_decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn seal_open_round_trips() {
        let sealed = crate::artifact::seal("vault-pass", b"hunter2").expect("seal");
        let opened = crate::artifact::open_sealed("vault-pass", &sealed).expect("open");
        assert_eq!(opened, b"hunter2");
        assert!(crate::artifact::open_sealed("wrong", &sealed).is_err());
    }
}
