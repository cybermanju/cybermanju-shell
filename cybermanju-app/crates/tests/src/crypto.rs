// CyberManju OS — Crypto (crates/tests/src/crypto.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: cybermanju_crypto.
// Contracts: none special (pure logic; errors bubble to caller).
use cybermanju_crypto::pqc::*;

#[test]
fn test_encryption_algo_nist_levels() {
    assert_eq!(EncryptionAlgo::Kyber1024.nist_level(), 5);
    // P0-15: ML-KEM-768 is Level 3 — the hybrid label must never claim 5.
    assert_eq!(EncryptionAlgo::Hybrid.nist_level(), 3);
    assert_eq!(EncryptionAlgo::MlDsa44.nist_level(), 2);
    assert_eq!(EncryptionAlgo::MlDsa65.nist_level(), 3);
    assert_eq!(EncryptionAlgo::MlDsa87.nist_level(), 5);
    assert_eq!(EncryptionAlgo::ClassicalSign.nist_level(), 0);
    assert_eq!(EncryptionAlgo::Aes256.nist_level(), 0);
}

#[test]
fn test_encryption_algo_display_names() {
    assert!(EncryptionAlgo::Kyber1024
        .display_name()
        .contains("ML-KEM-1024"));
    assert!(EncryptionAlgo::Hybrid.display_name().contains("Hybrid"));
    assert!(EncryptionAlgo::MlDsa44.display_name().contains("ML-DSA-44"));
    assert!(EncryptionAlgo::MlDsa65.display_name().contains("ML-DSA-65"));
    assert!(EncryptionAlgo::MlDsa87.display_name().contains("ML-DSA-87"));
    assert!(EncryptionAlgo::ClassicalSign
        .display_name()
        .contains("HMAC"));
    assert!(EncryptionAlgo::Aes256
        .display_name()
        .contains("ChaCha20Poly1305"));
}

#[test]
fn test_encryption_algo_colors() {
    assert!(!EncryptionAlgo::Kyber1024.color().is_empty());
    assert!(!EncryptionAlgo::Hybrid.color().is_empty());
    assert!(!EncryptionAlgo::MlDsa44.color().is_empty());
    assert!(!EncryptionAlgo::MlDsa65.color().is_empty());
    assert!(!EncryptionAlgo::MlDsa87.color().is_empty());
    assert!(!EncryptionAlgo::ClassicalSign.color().is_empty());
    assert!(!EncryptionAlgo::Aes256.color().is_empty());
}

#[test]
fn test_encryption_algo_signature_only() {
    assert!(!EncryptionAlgo::Kyber1024.is_signature_only());
    assert!(!EncryptionAlgo::Hybrid.is_signature_only());
    assert!(EncryptionAlgo::MlDsa44.is_signature_only());
    assert!(EncryptionAlgo::MlDsa65.is_signature_only());
    assert!(EncryptionAlgo::MlDsa87.is_signature_only());
    assert!(EncryptionAlgo::ClassicalSign.is_signature_only());
    assert!(!EncryptionAlgo::Aes256.is_signature_only());
}

#[test]
fn test_algorithm_from_str() {
    assert_eq!(
        algorithm_from_str("kyber1024"),
        Some(EncryptionAlgo::Kyber1024)
    );
    assert_eq!(algorithm_from_str("hybrid"), Some(EncryptionAlgo::Hybrid));
    assert_eq!(algorithm_from_str("kyber768"), Some(EncryptionAlgo::Hybrid));
    assert_eq!(
        algorithm_from_str("ml-dsa-44"),
        Some(EncryptionAlgo::MlDsa44)
    );
    assert_eq!(
        algorithm_from_str("ml-dsa-65"),
        Some(EncryptionAlgo::MlDsa65)
    );
    assert_eq!(
        algorithm_from_str("ml-dsa-87"),
        Some(EncryptionAlgo::MlDsa87)
    );
    assert_eq!(
        algorithm_from_str("dilithium2"),
        Some(EncryptionAlgo::MlDsa44)
    );
    assert_eq!(
        algorithm_from_str("hmac"),
        Some(EncryptionAlgo::ClassicalSign)
    );
    assert_eq!(algorithm_from_str("chacha20"), Some(EncryptionAlgo::Aes256));
    assert_eq!(algorithm_from_str("unknown"), None);
}

#[test]
fn test_generate_keypair_aes256() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::Aes256);
    assert_eq!(kp.public_key.len(), 32);
    assert_eq!(kp.private_key.len(), 32);
    assert!(!kp.id.is_empty());
}

#[test]
fn test_generate_keypair_classical_sign() {
    let mut engine = PqcEngine::new();
    let kp = engine
        .generate_keypair(EncryptionAlgo::ClassicalSign)
        .unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::ClassicalSign);
    assert_eq!(kp.private_key.len(), 64);
    assert_eq!(kp.public_key.len(), 32);
}

#[test]
fn test_generate_keypair_kyber1024() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Kyber1024).unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::Kyber1024);
    assert!(!kp.public_key.is_empty());
    assert!(!kp.private_key.is_empty());
}

#[test]
fn test_generate_keypair_hybrid() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Hybrid).unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::Hybrid);
    assert!(!kp.public_key.is_empty());
    assert!(!kp.private_key.is_empty());
    // P0-13: an independent classical pair is stored alongside the KEM keys.
    assert_eq!(kp.x25519_public_key.as_ref().map(|k| k.len()), Some(32));
    assert_eq!(kp.x25519_private_key.as_ref().map(|k| k.len()), Some(32));
}

#[test]
fn test_generate_keypair_mldsdsa44() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa44).unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::MlDsa44);
    assert!(!kp.public_key.is_empty());
    assert!(!kp.private_key.is_empty());
}

#[test]
fn test_generate_keypair_mldsdsa65() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa65).unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::MlDsa65);
    assert!(!kp.public_key.is_empty());
    assert!(!kp.private_key.is_empty());
}

#[test]
fn test_generate_keypair_mldsdsa87() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa87).unwrap();
    assert_eq!(kp.algorithm, EncryptionAlgo::MlDsa87);
    assert!(!kp.public_key.is_empty());
    assert!(!kp.private_key.is_empty());
}

#[test]
fn test_list_keys_hides_private_keys() {
    let mut engine = PqcEngine::new();
    engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    engine.generate_keypair(EncryptionAlgo::Hybrid).unwrap();
    let keys = engine.list_keys();
    assert_eq!(keys.len(), 2);
    for k in &keys {
        assert!(k.get("has_private_key").is_some());
        assert!(k.get("algorithm").is_some());
        assert!(k.get("nist_level").is_some());
    }
}

#[test]
fn test_active_keypair() {
    let mut engine = PqcEngine::new();
    assert!(engine.get_active_keypair().is_none());
    let kp = engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    let active = engine.get_active_keypair().unwrap();
    assert_eq!(active.id, kp.id);
}

#[test]
fn test_encryption_status() {
    let mut engine = PqcEngine::new();
    let status = engine.get_status();
    assert!(!status.is_encrypted);

    engine.generate_keypair(EncryptionAlgo::Hybrid).unwrap();
    let status = engine.get_status();
    assert!(status.is_encrypted);
    assert!(status.algorithm.is_some());
    assert!(status.nist_level.is_some());
    assert!(status.key_id.is_some());
}

#[test]
fn test_encrypt_decrypt_aes256() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    let plaintext = b"Hello, quantum-resistant world!";
    let encrypted = encrypt_data(plaintext, &kp).unwrap();
    let decrypted = decrypt_data(&encrypted, &kp).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());
}

#[test]
fn test_encrypt_decrypt_kyber1024() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Kyber1024).unwrap();
    let plaintext = b"Post-quantum encryption test";
    let encrypted = encrypt_data(plaintext, &kp).unwrap();
    let decrypted = decrypt_data(&encrypted, &kp).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());
}

#[test]
fn test_encrypt_decrypt_hybrid() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Hybrid).unwrap();
    let plaintext = b"Hybrid encryption test with ML-KEM + X25519";
    let encrypted = encrypt_data(plaintext, &kp).unwrap();
    let decrypted = decrypt_data(&encrypted, &kp).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());
}

#[test]
fn test_encrypt_decrypt_large_data() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    let plaintext = vec![0xABu8; 100000];
    let encrypted = encrypt_data(&plaintext, &kp).unwrap();
    let decrypted = decrypt_data(&encrypted, &kp).unwrap();
    assert_eq!(plaintext, decrypted);
}

#[test]
fn test_encrypted_data_has_metadata() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Hybrid).unwrap();
    let plaintext = b"metadata test";
    let encrypted = encrypt_data(plaintext, &kp).unwrap();
    assert!(!encrypted.ciphertext.is_empty());
    assert!(!encrypted.nonce.is_empty());
    assert!(!encrypted.algorithm.is_empty());
    assert!(!encrypted.blake3_original.is_empty());
    assert!(!encrypted.encrypted_at.is_empty());
    assert!(!encrypted.kem_ciphertext.is_empty());
}

#[test]
fn test_encrypted_file_meta_serde() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    let plaintext = b"serde test";
    let encrypted = encrypt_data(plaintext, &kp).unwrap();
    let meta: EncryptedFileMeta = (&encrypted).into();
    let json = serde_json::to_string(&meta).unwrap();
    let back_meta: EncryptedFileMeta = serde_json::from_str(&json).unwrap();
    assert_eq!(meta.algorithm, back_meta.algorithm);
    assert_eq!(meta.key_id, back_meta.key_id);
    let restored = back_meta
        .to_encrypted_data(encrypted.ciphertext.clone())
        .unwrap();
    assert_eq!(restored.ciphertext, encrypted.ciphertext);
}

#[test]
fn test_sign_verify_hmac() {
    let mut engine = PqcEngine::new();
    let kp = engine
        .generate_keypair(EncryptionAlgo::ClassicalSign)
        .unwrap();
    let message = b"sign this message";
    let sig = sign_message(message, &kp).unwrap();
    assert!(!sig.is_empty());
    let valid = verify_signature(message, &sig, &kp).unwrap();
    assert!(valid);
}

#[test]
fn test_verify_wrong_message() {
    let mut engine = PqcEngine::new();
    let kp = engine
        .generate_keypair(EncryptionAlgo::ClassicalSign)
        .unwrap();
    let sig = sign_message(b"original", &kp).unwrap();
    let valid = verify_signature(b"tampered", &sig, &kp).unwrap();
    assert!(!valid);
}

#[test]
fn test_sign_verify_ml_dsa44() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa44).unwrap();
    let message = b"PQC digital signature test";
    let sig = sign_message(message, &kp).unwrap();
    let valid = verify_signature(message, &sig, &kp).unwrap();
    assert!(valid);
}

#[test]
fn test_sign_verify_ml_dsa65() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa65).unwrap();
    let message = b"PQC ML-DSA-65 signature test";
    let sig = sign_message(message, &kp).unwrap();
    let valid = verify_signature(message, &sig, &kp).unwrap();
    assert!(valid);
}

#[test]
fn test_sign_verify_ml_dsa87() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa87).unwrap();
    let message = b"PQC ML-DSA-87 signature test";
    let sig = sign_message(message, &kp).unwrap();
    let valid = verify_signature(message, &sig, &kp).unwrap();
    assert!(valid);
}

#[test]
fn test_sign_verify_tampered_sig() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa44).unwrap();
    let sig = sign_message(b"test", &kp).unwrap();
    let mut tampered = sig.clone();
    if let Some(b) = tampered.first_mut() {
        *b = b.wrapping_add(1);
    }
    let valid = verify_signature(b"test", &tampered, &kp).unwrap();
    assert!(!valid);
}

#[test]
fn test_generate_random_nonce() {
    let n1 = generate_random_nonce();
    let n2 = generate_random_nonce();
    assert_eq!(n1.len(), 12);
    assert_eq!(n2.len(), 12);
    assert_ne!(n1, n2);
}

#[test]
fn test_keypair_timestamp_format() {
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Aes256).unwrap();
    assert!(kp.created_at.contains("T"));
    assert!(kp.created_at.contains("Z"));
}

#[test]
fn test_hybrid_encrypt_needs_only_the_public_key() {
    // P0-13: encryption with a public-only Hybrid keypair (no private half,
    // no stored X25519 secret — only the two public halves) must succeed.
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::Hybrid).unwrap();
    let public_only = KeyPair {
        id: kp.id.clone(),
        algorithm: kp.algorithm.clone(),
        public_key: kp.public_key.clone(),
        private_key: Vec::new(),
        x25519_public_key: kp.x25519_public_key.clone(),
        x25519_private_key: None,
        created_at: kp.created_at.clone(),
    };
    let plaintext = b"public-key-only hybrid encryption";
    let encrypted = encrypt_data(plaintext, &public_only).unwrap();
    let decrypted = decrypt_data(&encrypted, &kp).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());
}

#[test]
fn test_classical_data_key_is_kdf_separated_with_legacy_fallback() {
    // P0-14: fresh ML-DSA-65 artifacts use the KDF key (never the raw seed),
    // while pre-fix artifacts (raw-slice key) still decrypt.
    let mut engine = PqcEngine::new();
    let kp = engine.generate_keypair(EncryptionAlgo::MlDsa65).unwrap();
    let plaintext = b"classical separation round-trip";
    let encrypted = encrypt_data(plaintext, &kp).unwrap();
    let decrypted = decrypt_data(&encrypted, &kp).unwrap();
    assert_eq!(plaintext.as_slice(), decrypted.as_slice());

    // The KDF key must differ from the legacy raw slice (else no separation).
    let kdf = derive_classical_data_key(&kp.private_key, &kp.algorithm);
    assert_ne!(&kdf[..], &kp.private_key[..32]);

    // Legacy artifact: encrypt under the raw slice by hand, decrypt via the
    // current path (KDF attempt fails closed, fallback opens it).
    use chacha20poly1305::aead::{Aead, KeyInit};
    let legacy_key: [u8; 32] = kp.private_key[..32].try_into().unwrap();
    let cipher = chacha20poly1305::ChaCha20Poly1305::new_from_slice(&legacy_key).unwrap();
    let nonce_bytes = generate_random_nonce();
    let nonce = chacha20poly1305::Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).unwrap();
    let legacy = FileEncryptedData {
        ciphertext,
        nonce: nonce_bytes,
        algorithm: "legacy".to_string(),
        key_id: kp.id.clone(),
        kem_ciphertext: Vec::new(),
        x25519_ephemeral_pk: None,
        blake3_original: blake3::hash(plaintext).to_hex().to_string(),
        encrypted_at: "2026-10-11T00:00:00Z".to_string(),
    };
    let reopened = decrypt_data(&legacy, &kp).unwrap();
    assert_eq!(plaintext.as_slice(), reopened.as_slice());
}
