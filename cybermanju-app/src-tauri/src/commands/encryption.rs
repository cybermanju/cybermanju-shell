// CyberManju OS — Encryption (src-tauri/src/commands/encryption.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: EncryptionStatus, FrontendEncryptionStatus, FrontendKeyInfo, encrypt_file, decrypt_file, get_encryption_status, generate_keypair, list_keys.
// Integrates: chrono, redb, serde, tauri.
// Contracts: signals: AGENT-, engine.
use chrono::Utc;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// Status returned when encrypting/decrypting a file.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionStatus {
    pub file_id: String,
    pub encrypted: bool,
    pub algorithm: Option<String>,
    pub updated_at: String,
}

/// Engine status returned to the frontend — matches frontend EncryptionStatus type.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendEncryptionStatus {
    pub is_encrypted: bool,
    pub algorithm: Option<String>,
    pub nist_level: Option<u8>,
    pub key_id: Option<String>,
    pub encrypted_at: Option<String>,
}

/// Key info returned to the frontend — matches frontend EncryptionKeyInfo type.
/// NEVER exposes private key material.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendKeyInfo {
    pub id: String,
    pub algorithm: String,
    pub algorithm_display: String,
    pub nist_level: u8,
    pub color: String,
    pub public_key_preview: String,
    pub has_private_key: bool,
    pub created_at: String,
}

/// Map a stored algorithm string to display info.
fn algorithm_info(algo: &str) -> (&'static str, u8, &'static str) {
    match algo {
        "kyber1024" => ("ML-KEM-1024 — FIPS 203", 5, "#00FF41"),
        "kyber768" | "hybrid" | "kyber512" => {
            ("Hybrid ML-KEM-768 (NIST Level 3) + X25519", 3, "#FFB800")
        }
        "ml_dsa44" | "ml-dsa44" | "ml-dsa-44" | "dilithium2" => {
            ("ML-DSA-44 — FIPS 204 (NIST Level 2)", 2, "#00D4FF")
        }
        "ml_dsa65" | "ml-dsa65" | "ml-dsa-65" | "dilithium3" => {
            ("ML-DSA-65 — FIPS 204 (NIST Level 3)", 3, "#A855F7")
        }
        "ml_dsa87" | "ml-dsa87" | "ml-dsa-87" | "dilithium5" => {
            ("ML-DSA-87 — FIPS 204 (NIST Level 5)", 5, "#FF2D6F")
        }
        "classical_sign" | "hmac" | "sphincsplus" | "sphincs+" => {
            ("HMAC-SHA512 (Classical — NOT post-quantum)", 0, "#6B7280")
        }
        "aes256" | "chacha20" => ("ChaCha20Poly1305 (Classical)", 0, "#FF6B2B"),
        _ => ("Unknown", 0, "#6B7280"),
    }
}

/// Find the most recently created encryption key of any algorithm.
fn find_any_latest_key(
    db: &crate::db::Database,
) -> Result<Option<crate::db::schema::EncryptionKey>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_encryption_keys_table())
        .map_err(|e| e.to_string())?;

    let mut best: Option<crate::db::schema::EncryptionKey> = None;
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let key: crate::db::schema::EncryptionKey =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        if best.as_ref().is_none_or(|b| key.created_at > b.created_at) {
            best = Some(key);
        }
    }
    Ok(best)
}

// <<< AGENT-3 KEYS AT REST >>>

/// Marker prefix for a private key sealed with the master passphrase.
const SEALED_PREFIX: &str = "sealed:v1:";

/// Wrap private key bytes with an Argon2id-derived key (ChaCha20Poly1305).
///
/// Fails when no master passphrase can be resolved — the contract is to
/// **refuse to persist a raw private key**, not to fall back to plaintext.
/// Shared (`pub(crate)`) with the device-identity module, which seals ML-DSA
/// device seeds under the same posture.
pub(crate) fn seal_private_key(raw: &[u8]) -> Result<String, String> {
    let passphrase = cybermanju_crypto::keystore::master_passphrase().ok_or_else(|| {
        "No master passphrase available — refusing to store a raw private key".to_string()
    })?;
    let sealed = cybermanju_crypto::keystore::seal(&passphrase, raw)?;
    Ok(format!(
        "{}{}",
        SEALED_PREFIX,
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, sealed)
    ))
}

/// Decode a stored private key, opening the sealed form when present.
///
/// Rows written before keys-at-rest landed are still raw base64; they stay
/// readable and are migrated to the sealed form by [`seal_legacy_keys`].
/// Shared (`pub(crate)`) with the device-identity module for seed signing.
pub(crate) fn open_private_key(stored: &str) -> Result<Vec<u8>, String> {
    if let Some(payload) = stored.strip_prefix(SEALED_PREFIX) {
        let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload)
            .map_err(|e| format!("Failed to decode sealed private key: {}", e))?;
        let passphrase = cybermanju_crypto::keystore::master_passphrase().ok_or_else(|| {
            "Master passphrase unavailable — cannot open sealed private key".to_string()
        })?;
        return cybermanju_crypto::keystore::open_sealed(&passphrase, &blob);
    }
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, stored)
        .map_err(|e| format!("Failed to decode private key: {}", e))
}

/// Wrap every legacy plaintext private key at rest.
///
/// Best effort: a failure is logged rather than propagated so an unreadable
/// passphrase file never blocks file encryption. The caller must hold the
/// database write lock.
fn seal_legacy_keys(db: &crate::db::Database) {
    let run = || -> Result<u32, String> {
        let tx = db.begin_write().map_err(|e| e.to_string())?;
        let mut updates: Vec<(String, String)> = Vec::new();
        {
            let mut table = tx
                .open_table(crate::db::Database::get_encryption_keys_table())
                .map_err(|e| e.to_string())?;
            for entry in table.iter().map_err(|e| e.to_string())? {
                let (id, value) = entry.map_err(|e| e.to_string())?;
                let mut key: crate::db::schema::EncryptionKey =
                    match serde_json::from_str(value.value()) {
                        Ok(k) => k,
                        Err(_) => continue,
                    };
                if key.private_key.starts_with(SEALED_PREFIX) {
                    continue;
                }
                let raw = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &key.private_key,
                )
                .map_err(|e| format!("Failed to decode private key {}: {}", id.value(), e))?;
                key.private_key = seal_private_key(&raw)?;
                updates.push((
                    key.id.clone(),
                    serde_json::to_string(&key).map_err(|e| e.to_string())?,
                ));
            }
            for (id, json) in &updates {
                table
                    .insert(id.as_str(), json.as_str())
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(updates.len() as u32)
    };

    match run() {
        Ok(count) if count > 0 => {
            log::info!("Sealed {} legacy private key(s) at rest", count)
        }
        Ok(_) => {}
        Err(e) => log::warn!("Could not seal legacy private keys: {}", e),
    }
}

/// Encrypt a file using real ML-KEM key encapsulation.
///
/// Flow:
///   1. Validate algorithm, find the latest matching key from the database
///   2. Read the original file bytes
///   3. Call `pqc::encrypt_data()` — performs real ML-KEM encapsulation
///   4. Write encrypted bytes to `{path}.enc`
///   5. Write metadata to `{path}.enc.meta.json`
///   6. Update the file node in the database
#[tauri::command(async)]
pub fn encrypt_file(
    file_id: String,
    algorithm: String,
    state: State<'_, AppState>,
) -> Result<EncryptionStatus, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    // <<< AGENT-3 KEYS AT REST: migrate plaintext keys while we hold the write lock >>>
    seal_legacy_keys(&db);
    let enc_algorithm = cybermanju_web::api::batch::encrypt_one(&db, &file_id, &algorithm)?;
    let now = Utc::now().to_rfc3339();
    Ok(EncryptionStatus {
        file_id,
        encrypted: true,
        algorithm: Some(enc_algorithm),
        updated_at: now,
    })
}

/// Decrypt a file using real ML-KEM decapsulation.
///
/// Flow:
///   1. Read the file node and locate the .enc.meta.json
///   2. Load the encryption key by key_id from the metadata
///   3. Read the .enc ciphertext
///   4. Call `pqc::decrypt_data()` — performs real ML-KEM decapsulation
///   5. Write the decrypted bytes back to the original path
///   6. Update the file node in the database
#[tauri::command(async)]
pub fn decrypt_file(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<EncryptionStatus, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    // <<< AGENT-3 KEYS AT REST: migrate plaintext keys while we hold the write lock >>>
    seal_legacy_keys(&db);
    cybermanju_web::api::batch::decrypt_one(&db, &file_id)?;
    let now = Utc::now().to_rfc3339();

    Ok(EncryptionStatus {
        file_id,
        encrypted: false,
        algorithm: None,
        updated_at: now,
    })
}

/// Return the PQC encryption engine status based on actual key state.
///
/// Reads the most recently created key from the database and reports
/// its algorithm, NIST level, and creation time.
#[tauri::command(async)]
pub fn get_encryption_status(
    state: State<'_, AppState>,
) -> Result<FrontendEncryptionStatus, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;

    // Check if any keys exist at all
    let latest_key = find_any_latest_key(&db)?;

    // Also check if any files are actually encrypted
    let any_encrypted = {
        let tx = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        let mut found = false;
        for (_, value) in table.iter().map_err(|e| e.to_string())?.flatten() {
            if let Ok(node) = serde_json::from_str::<crate::db::schema::FileNode>(value.value()) {
                if node.encrypted {
                    found = true;
                    break;
                }
            }
        }
        found
    };

    if let Some(key) = latest_key {
        let (display, nist_level, _color) = algorithm_info(&key.algorithm);
        Ok(FrontendEncryptionStatus {
            is_encrypted: any_encrypted,
            algorithm: Some(display.to_string()),
            nist_level: Some(nist_level),
            key_id: Some(key.id),
            encrypted_at: Some(key.created_at),
        })
    } else {
        Ok(FrontendEncryptionStatus {
            is_encrypted: false,
            algorithm: None,
            nist_level: None,
            key_id: None,
            encrypted_at: None,
        })
    }
}

/// Generate a new PQC keypair using real ML-KEM key generation.
///
/// Uses the pqcrypto-mlkem crate for actual post-quantum key generation:
///   - kyber1024  → ML-KEM-1024 encapsulation/decapsulation keypair
///   - kyber768/kyber512/hybrid → ML-KEM-768 keypair (X25519 derived at use time)
///   - dilithium*/sphincsplus/classical_sign  → 64-byte HMAC-SHA512 key with SHA-256 fingerprint
///   - aes256      → 32-byte ChaCha20Poly1305 key
///
/// Public key bytes are base64. Private material is sealed under the local
/// master passphrase before persistence. This trusted-local IPC result contains
/// the sealed private field; the REST wrapper exposes public information only.
#[tauri::command(async)]
pub fn generate_keypair(
    algorithm: String,
    state: State<'_, AppState>,
) -> Result<crate::db::schema::EncryptionKey, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    seal_legacy_keys(&db);
    cybermanju_web::api::batch::generate_keypair(&db, &algorithm)
}

/// List all encryption keys — NEVER exposes private key material.
///
/// Returns only a 16-byte base64 preview of the public key, the algorithm
/// info, and metadata. The full private key is never included in the response.
#[tauri::command(async)]
pub fn list_keys(state: State<'_, AppState>) -> Result<Vec<FrontendKeyInfo>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_encryption_keys_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let key: crate::db::schema::EncryptionKey =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;

        let (display, nist_level, color) = algorithm_info(&key.algorithm);

        // Decode the public key to get the preview bytes
        let preview = match base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &key.public_key,
        ) {
            Ok(bytes) => {
                let truncated = &bytes[..16.min(bytes.len())];
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, truncated);
                format!("{}...", encoded)
            }
            Err(_) => {
                // Fallback: show first 16 chars of the stored string
                if key.public_key.len() > 16 {
                    format!("{}...", &key.public_key[..16])
                } else {
                    key.public_key.clone()
                }
            }
        };

        results.push(FrontendKeyInfo {
            id: key.id,
            algorithm: key.algorithm,
            algorithm_display: display.to_string(),
            nist_level,
            color: color.to_string(),
            public_key_preview: preview,
            has_private_key: !key.private_key.is_empty(),
            created_at: key.created_at,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::algorithm_info;

    #[test]
    fn hybrid_nist_level_matches_shared_crypto_engine() {
        let (_, level, _) = algorithm_info("hybrid");
        assert_eq!(level, 3);
        let (_, legacy_level, _) = algorithm_info("kyber768");
        assert_eq!(legacy_level, 3);
    }
}
