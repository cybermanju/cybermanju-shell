// CyberManju OS — Batch (crates/web/src/api/batch.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: delete, encrypt, encrypt_one, decrypt, decrypt_one, generate_keypair, compress.
// Integrates: cybermanju_crypto, cybermanju_db, cybermanju_types.
// Contracts: encryption always handles bytes; private keys stay sealed at rest.
// CyberManju OS — Batch operations (shared by Tauri IPC and REST)

use cybermanju_crypto::pqc::{self, EncryptedFileMeta, EncryptionAlgo, KeyPair};
use cybermanju_db::Database;
use cybermanju_types::schema::{EncryptionKey, FileNode};
use redb::ReadableTable;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Explicitly generate and persist a keypair for REST or desktop callers.
/// Private material is sealed before it is written, and missing master
/// passphrases fail closed rather than storing plaintext secrets.
pub fn generate_keypair(db: &Database, algorithm: &str) -> Result<EncryptionKey, String> {
    let parsed = pqc::algorithm_from_str(algorithm)
        .ok_or_else(|| format!("unsupported: unknown encryption algorithm '{algorithm}'"))?;
    let mut engine = pqc::PqcEngine::new();
    let pair = engine
        .generate_keypair(parsed)
        .map_err(|e| format!("Key generation failed: {e}"))?;
    let passphrase = cybermanju_crypto::keystore::master_passphrase().ok_or_else(|| {
        "No master passphrase available — refusing to store a raw private key".to_string()
    })?;
    let seal = |raw: &[u8]| -> Result<String, String> {
        let blob = cybermanju_crypto::keystore::seal(&passphrase, raw)?;
        Ok(format!(
            "sealed:v1:{}",
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, blob)
        ))
    };
    let encode =
        |raw: &[u8]| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw);
    let key = EncryptionKey {
        id: pair.id.clone(),
        algorithm: algorithm.to_string(),
        public_key: encode(&pair.public_key),
        private_key: seal(&pair.private_key)?,
        label: Some(format!("{} keypair", pair.algorithm.display_name())),
        created_at: pair.created_at,
        x25519_public_key: pair.x25519_public_key.as_deref().map(encode),
        x25519_private_key: pair.x25519_private_key.as_deref().map(seal).transpose()?,
    };
    let serialized = serde_json::to_string(&key).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_encryption_keys_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(key.id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(key)
}

/// Batch delete: move multiple files to trash in a single operation.
pub fn delete(db: &Database, file_ids: &[String]) -> Result<u32, String> {
    let mut count = 0u32;

    for file_id in file_ids {
        let tx_read = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx_read
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        let file_node: Option<FileNode> = table
            .get(file_id.as_str())
            .map_err(|e| e.to_string())?
            .and_then(|v| serde_json::from_str(v.value()).ok());
        drop(tx_read);

        if let Some(node) = file_node {
            db.log_audit("batch_delete", "file", file_id, None, None)
                .map_err(|e| e.to_string())?;
            db.trash_file(file_id, &node, None)
                .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok(count)
}

/// Batch encrypt files with the real shared AEAD pipeline and persist the same
/// ciphertext + metadata sidecars consumed by the Tauri decrypt command.
/// Missing source bytes or required key material are errors, never metadata-only success.
pub fn encrypt(db: &Database, file_ids: &[String], algorithm: &str) -> Result<u32, String> {
    pqc::algorithm_from_str(algorithm)
        .ok_or_else(|| format!("unsupported: unknown encryption algorithm '{algorithm}'"))?;
    let mut count = 0u32;
    for file_id in file_ids {
        let file_node = read_file(db, file_id)?;
        encrypt_one(db, file_id, algorithm)?;
        if !file_node.encrypted {
            count += 1;
        }
    }

    Ok(count)
}

/// Encrypt one file and return the algorithm label actually stored in its row.
/// The original plaintext path is removed only after encrypted bytes and
/// metadata are durably installed. Conflicts retain source/recovery bytes.
/// Callers must quiesce external in-place writers: portable filesystem APIs do
/// not provide an atomic compare-bytes-and-unlink against already-open handles.
pub fn encrypt_one(db: &Database, file_id: &str, algorithm: &str) -> Result<String, String> {
    let algorithm = pqc::algorithm_from_str(algorithm)
        .ok_or_else(|| format!("unsupported: unknown encryption algorithm '{algorithm}'"))?;
    let existing = read_file(db, file_id)?;
    if existing.encrypted {
        let path = original_path(&existing, file_id)?;
        let (cipher_path, metadata_path) = sidecar_paths(&path);
        if !cipher_path.exists() || !metadata_path.exists() {
            return Err(
                "integrity: encrypted row is missing its ciphertext or metadata sidecar"
                    .to_string(),
            );
        }
        let verified_plaintext = authenticate_sidecars(db, &path)?;
        if path.exists() {
            let (current, metadata) = read_source_snapshot(&path)?;
            if current != verified_plaintext {
                return Err("conflict: refusing to remove source bytes that do not match the encrypted artifact".to_string());
            }
            let claim = SourceClaim::capture(&path, &current, &metadata)?;
            claim.finish(&current)?;
        }
        return existing
            .encryption_algorithm
            .ok_or_else(|| "conflict: encrypted file has no recorded algorithm".to_string());
    }
    let keypair = latest_public_key(db, &algorithm)?.ok_or_else(|| {
        format!(
            "not_found: no {} public keypair found",
            algorithm.display_name()
        )
    })?;
    encrypt_with_key(db, file_id, &keypair)
}

fn encrypt_with_key(db: &Database, file_id: &str, keypair: &KeyPair) -> Result<String, String> {
    encrypt_with_key_before_capture(db, file_id, keypair, |_| {})
}

// The hook permits deterministic regression tests at the original data-loss
// window, without global state or timing-dependent racing threads.
fn encrypt_with_key_before_capture(
    db: &Database,
    file_id: &str,
    keypair: &KeyPair,
    before_capture: impl FnOnce(&Path),
) -> Result<String, String> {
    let mut node = read_file(db, file_id)?;
    if node.file_type != "file" {
        return Err(format!("unsupported: '{}' is not a file", file_id));
    }
    let path = original_path(&node, file_id)?;
    let (cipher_path, metadata_path) = sidecar_paths(&path);
    if node.encrypted {
        let verified_plaintext = authenticate_sidecars(db, &path)?;
        if path.exists() {
            let (current, metadata) = read_source_snapshot(&path)?;
            if current != verified_plaintext {
                return Err(format!(
                    "conflict: marked-encrypted file '{}' does not match its encryption metadata",
                    path.display()
                ));
            }
            let claim = SourceClaim::capture(&path, &current, &metadata)?;
            claim.finish(&current)?;
        }
        return node
            .encryption_algorithm
            .ok_or_else(|| "conflict: encrypted file has no recorded algorithm".to_string());
    }
    if !path.exists() {
        // Crash window recovery: sidecars were fsynced and plaintext unlinked,
        // but the DB flag had not committed yet. Finalize only with a complete
        // package matching the selected key, preserving the sole recovery copy.
        if cipher_path.exists() && metadata_path.exists() {
            let metadata = read_metadata(&metadata_path)?;
            if metadata.key_id != keypair.id {
                return Err("conflict: pending ciphertext belongs to another key; use decrypt_one to recover it".to_string());
            }
            authenticate_sidecars(db, &path)?;
            let previous = node.clone();
            node.encrypted = true;
            node.encryption_algorithm = Some(metadata.algorithm.clone());
            node.modified_at = chrono::Utc::now().to_rfc3339();
            write_file(db, file_id, &node).map_err(|e| {
                format!("integrity: could not finalize recovery ({e}); sidecars retained")
            })?;
            if let Err(error) = db.log_audit("batch_encrypt_recovered", "file", file_id, None, None)
            {
                let rollback = write_file(db, file_id, &previous);
                return Err(format!("integrity: recovery audit failed ({error}); rollback: {rollback:?}; sidecars retained"));
            }
            return Ok(metadata.algorithm);
        }
        return Err(format!(
            "not_found: plaintext '{}' is missing and no complete encryption sidecars exist",
            path.display()
        ));
    }
    if cipher_path.exists() || metadata_path.exists() {
        discard_stale_sidecars(&path)?;
    }
    let (plaintext, source_metadata) = read_source_snapshot(&path)?;
    let encrypted = pqc::encrypt_data(&plaintext, keypair)
        .map_err(|e| format!("integrity: encryption failed for '{}': {e}", file_id))?;
    persist_encrypted_sidecars(&path, &encrypted, &source_metadata)?;
    let verification_error = match authenticate_sidecars(db, &path) {
        Ok(restored) if restored == plaintext => None,
        Ok(_) => Some("restored bytes did not match the source".to_string()),
        Err(e) => Some(e),
    };
    if let Some(error) = verification_error {
        let cleanup = remove_sidecars(&path);
        return Err(format!("integrity: installed ciphertext did not round-trip ({error}); plaintext retained; cleanup: {cleanup:?}"));
    }
    // Rename into a private, unique directory first. A replacement installed at
    // the original pathname after this point is never removed by this operation.
    // On mismatch the captured file is retained, not unlinked or overwritten.
    before_capture(&path);
    let claim = SourceClaim::capture(&path, &plaintext, &source_metadata)?;

    // Finish before touching the DB: a late source mismatch must not leave the
    // row marked encrypted. Complete sidecars permit recovery if the process
    // stops between source retirement and the DB commit.
    claim.finish(&plaintext)?;
    if path.exists() {
        return Err("conflict: another writer recreated the original pathname; new source and encrypted snapshot retained".to_string());
    }
    let previous = node.clone();
    node.encrypted = true;
    node.encryption_algorithm = Some(encrypted.algorithm.clone());
    node.modified_at = chrono::Utc::now().to_rfc3339();
    if let Err(error) = write_file(db, file_id, &node) {
        return Err(restore_after_encrypt_failure(
            &path,
            &plaintext,
            &source_metadata,
            error,
        ));
    }
    if let Err(error) = db.log_audit("batch_encrypt", "file", file_id, None, None) {
        let rollback = write_file(db, file_id, &previous);
        return Err(restore_after_encrypt_failure(
            &path,
            &plaintext,
            &source_metadata,
            format!("integrity: audit failed ({error}); DB rollback: {rollback:?}"),
        ));
    }
    Ok(encrypted.algorithm)
}

/// Decrypt one file from the sidecar package and restore its original path.
/// The same metadata format is consumed by the existing Tauri decrypt flow.
pub fn decrypt_one(db: &Database, file_id: &str) -> Result<(), String> {
    let mut node = read_file(db, file_id)?;
    let path = original_path(&node, file_id)?;
    let (cipher_path, metadata_path) = sidecar_paths(&path);
    if !node.encrypted && path.exists() {
        if !metadata_path.exists() {
            if cipher_path.exists() {
                return Err(preserve_orphan_ciphertext(&cipher_path)?);
            }
            return Ok(());
        }
        let stale_metadata = match read_metadata(&metadata_path) {
            Ok(metadata) => metadata,
            Err(error) => return Err(preserve_incomplete_sidecars(&path, &error)?),
        };
        let current = std::fs::read(&path).map_err(|e| e.to_string())?;
        if blake3::hash(&current).to_hex().to_string() != stale_metadata.blake3_original {
            return Err("conflict: file is not marked encrypted and plaintext does not match leftover metadata".to_string());
        }
        // Ciphertext is removed and the directory synced BEFORE metadata. A
        // retry can therefore always validate the plaintext after interruption.
        remove_sidecars(&path)?;
        return Ok(());
    }
    if !node.encrypted && !(metadata_path.exists() && cipher_path.exists()) {
        return Err(
            "conflict: unencrypted row has no plaintext or complete encryption recovery sidecars"
                .to_string(),
        );
    }
    let metadata: EncryptedFileMeta = serde_json::from_slice(
        &std::fs::read(&metadata_path)
            .map_err(|e| format!("not_found: cannot read encryption metadata: {e}"))?,
    )
    .map_err(|e| format!("integrity: invalid encryption metadata: {e}"))?;
    let ciphertext = std::fs::read(&cipher_path)
        .map_err(|e| format!("not_found: cannot read encrypted bytes: {e}"))?;
    let encrypted = metadata
        .to_encrypted_data(ciphertext)
        .map_err(|e| format!("integrity: cannot reconstruct encrypted file: {e}"))?;
    let keypair = key_by_id(db, &encrypted.key_id)?
        .ok_or_else(|| format!("not_found: encryption key '{}' not found", encrypted.key_id))?;
    let plaintext = pqc::decrypt_data(&encrypted, &keypair)
        .map_err(|e| format!("integrity: decryption failed: {e}"))?;
    let created_plaintext = if path.exists() {
        let current = std::fs::read(&path)
            .map_err(|e| format!("network: cannot inspect existing plaintext path: {e}"))?;
        if blake3::hash(&current).to_hex().to_string() != encrypted.blake3_original {
            return Err(format!(
                "conflict: refusing to overwrite non-matching existing path '{}'",
                path.display()
            ));
        }
        false
    } else {
        write_restored(
            &path,
            &plaintext,
            &read_restore_permissions(&metadata_path)?,
        )?;
        sync_parent_dir(&path)?;
        true
    };
    let previous = node.clone();
    node.encrypted = false;
    node.encryption_algorithm = None;
    node.modified_at = chrono::Utc::now().to_rfc3339();
    if let Err(error) = write_file(db, file_id, &node) {
        return Err(format!("integrity: DB update failed ({error}); plaintext (created: {created_plaintext}) and encrypted recovery sidecars retained"));
    }
    if let Err(error) = db.log_audit("batch_decrypt", "file", file_id, None, None) {
        let db_rollback = write_file(db, file_id, &previous);
        if let Err(rollback_error) = db_rollback {
            return Err(format!("integrity: audit failed ({error}); DB rollback failed ({rollback_error}); ciphertext and metadata retained"));
        }
        return Err(error.to_string());
    }
    // The plaintext and DB state are now safely committed. If cleanup fails,
    // return an explicit warning while keeping the recovered plaintext intact.
    remove_sidecars(&path)?;
    Ok(())
}

pub fn decrypt(db: &Database, file_ids: &[String]) -> Result<u32, String> {
    let mut count = 0;
    for file_id in file_ids {
        let node = read_file(db, file_id)?;
        let path = original_path(&node, file_id)?;
        let (cipher_path, metadata_path) = sidecar_paths(&path);
        let needs_recovery = !path.exists() && cipher_path.exists() && metadata_path.exists();
        if node.encrypted
            || needs_recovery
            || (path.exists() && (cipher_path.exists() || metadata_path.exists()))
        {
            decrypt_one(db, file_id)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Recovery may delete plaintext only after the retained package authenticates.
fn authenticate_sidecars(db: &Database, path: &Path) -> Result<Vec<u8>, String> {
    let (cipher_path, metadata_path) = sidecar_paths(path);
    let metadata = read_metadata(&metadata_path)?;
    let ciphertext = std::fs::read(&cipher_path)
        .map_err(|e| format!("integrity: cannot read recovery ciphertext: {e}"))?;
    let encrypted = metadata
        .to_encrypted_data(ciphertext)
        .map_err(|e| format!("integrity: invalid recovery package: {e}"))?;
    let key = key_by_id(db, &encrypted.key_id)?
        .ok_or_else(|| "not_found: recovery encryption key not found".to_string())?;
    pqc::decrypt_data(&encrypted, &key)
        .map_err(|e| format!("integrity: recovery package did not authenticate: {e}"))
}

fn latest_public_key(db: &Database, algorithm: &EncryptionAlgo) -> Result<Option<KeyPair>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_encryption_keys_table())
        .map_err(|e| e.to_string())?;
    let mut best: Option<(EncryptionKey, EncryptionAlgo)> = None;
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let stored: EncryptionKey =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        let Some(stored_algorithm) = pqc::algorithm_from_str(&stored.algorithm) else {
            continue;
        };
        if &stored_algorithm == algorithm
            && best
                .as_ref()
                .is_none_or(|(current, _)| stored.created_at > current.created_at)
        {
            best = Some((stored, stored_algorithm));
        }
    }
    let Some((stored, key_algorithm)) = best else {
        return Ok(None);
    };
    // KEM encryption needs only the public half. Symmetric/signing-seed
    // modes use the core's domain-separated HKDF + ChaCha20-Poly1305 path;
    // open their sealed secret only inside the backend, never in the response.
    if !matches!(
        key_algorithm,
        EncryptionAlgo::Kyber1024 | EncryptionAlgo::Hybrid
    ) || (matches!(key_algorithm, EncryptionAlgo::Hybrid) && stored.x25519_public_key.is_none())
    {
        return key_by_id(db, &stored.id);
    }
    let decode = |value: &str, name: &str| {
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, value)
            .map_err(|e| format!("Failed to decode {name}: {e}"))
    };
    let public_key = decode(&stored.public_key, "public key")?;
    let x25519_public_key = stored
        .x25519_public_key
        .as_deref()
        .map(|value| decode(value, "X25519 public key"))
        .transpose()?;
    Ok(Some(KeyPair {
        id: stored.id,
        algorithm: key_algorithm,
        public_key,
        // Encryption uses only public key material; never open the sealed
        // private half from the database in the web API.
        private_key: Vec::new(),
        x25519_public_key,
        x25519_private_key: None,
        created_at: stored.created_at,
    }))
}

fn key_by_id(db: &Database, key_id: &str) -> Result<Option<KeyPair>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_encryption_keys_table())
        .map_err(|e| e.to_string())?;
    let Some(value) = table.get(key_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let stored: EncryptionKey = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    let algorithm = pqc::algorithm_from_str(&stored.algorithm).ok_or_else(|| {
        format!(
            "unsupported: unknown stored encryption algorithm '{}'",
            stored.algorithm
        )
    })?;
    let decode = |value: &str, name: &str| {
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, value)
            .map_err(|e| format!("Failed to decode {name}: {e}"))
    };
    let open_private = |value: &str| -> Result<Vec<u8>, String> {
        if let Some(sealed) = value.strip_prefix("sealed:v1:") {
            let blob = decode(sealed, "sealed private key")?;
            let passphrase = cybermanju_crypto::keystore::master_passphrase().ok_or_else(|| {
                "Master passphrase unavailable — cannot open encryption key".to_string()
            })?;
            cybermanju_crypto::keystore::open_sealed(&passphrase, &blob)
        } else {
            decode(value, "legacy private key")
        }
    };
    Ok(Some(KeyPair {
        id: stored.id,
        algorithm,
        public_key: decode(&stored.public_key, "public key")?,
        private_key: open_private(&stored.private_key)?,
        x25519_public_key: stored
            .x25519_public_key
            .as_deref()
            .map(|value| decode(value, "X25519 public key"))
            .transpose()?,
        x25519_private_key: stored
            .x25519_private_key
            .as_deref()
            .map(open_private)
            .transpose()?,
        created_at: stored.created_at,
    }))
}

fn original_path(node: &FileNode, file_id: &str) -> Result<PathBuf, String> {
    node.context_data
        .as_ref()
        .and_then(|context| context.get("original_path"))
        .and_then(serde_json::Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("unsupported: file '{}' has no local original_path", file_id))
}

fn sidecar_paths(original_path: &Path) -> (PathBuf, PathBuf) {
    (
        PathBuf::from(format!("{}.enc", original_path.display())),
        PathBuf::from(format!("{}.enc.meta.json", original_path.display())),
    )
}

fn read_metadata(path: &Path) -> Result<EncryptedFileMeta, String> {
    let bytes = std::fs::read(path).map_err(|e| {
        format!(
            "not_found: cannot read encryption metadata '{}': {e}",
            path.display()
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|e| {
        format!(
            "integrity: invalid encryption metadata '{}': {e}",
            path.display()
        )
    })
}

/// Fsync a directory after creating/removing sibling files so a power loss
/// cannot persist a plaintext unlink while losing the encrypted sidecars.
#[cfg(unix)]
fn sync_parent_dir(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("network: cannot sync directory '{}': {e}", parent.display()))
}

#[cfg(not(unix))]
fn sync_parent_dir(_path: &Path) -> Result<(), String> {
    // Directory handles are not uniformly syncable on the supported Windows
    // toolchain; each staged file itself is still fsynced before installation.
    Ok(())
}

/// Remove artifacts left by a crash before source unlink. Only discard them
/// when the still-present plaintext matches the recorded original hash.
fn discard_stale_sidecars(original_path: &Path) -> Result<(), String> {
    let (cipher_path, metadata_path) = sidecar_paths(original_path);
    if cipher_path.exists() && !metadata_path.exists() {
        return Err(preserve_orphan_ciphertext(&cipher_path)?);
    }
    if metadata_path.exists() {
        let metadata = match read_metadata(&metadata_path) {
            Ok(metadata) => metadata,
            Err(error) => return Err(preserve_incomplete_sidecars(original_path, &error)?),
        };
        let plaintext = std::fs::read(original_path).map_err(|e| e.to_string())?;
        if blake3::hash(&plaintext).to_hex().to_string() != metadata.blake3_original {
            return Err(format!(
                "conflict: existing encryption sidecars do not match plaintext '{}'",
                original_path.display()
            ));
        }
    }
    remove_sidecars(original_path)
}

/// Stage and fsync both Tauri-compatible sidecars, then install them without
/// overwriting pre-existing artifacts. Partial installs retain recovery stages.
fn persist_encrypted_sidecars(
    original_path: &Path,
    encrypted: &pqc::FileEncryptedData,
    source_metadata: &std::fs::Metadata,
) -> Result<(), String> {
    let cipher_path = PathBuf::from(format!("{}.enc", original_path.display()));
    let metadata_path = PathBuf::from(format!("{}.enc.meta.json", original_path.display()));
    if cipher_path.exists() || metadata_path.exists() {
        return Err(format!(
            "conflict: encrypted sidecar already exists for '{}'",
            original_path.display()
        ));
    }
    let mut metadata = serde_json::to_value(EncryptedFileMeta::from(encrypted))
        .map_err(|e| format!("Could not serialize encryption metadata: {e}"))?;
    let permissions = RestorePermissions::from_metadata(source_metadata);
    metadata["original_permissions"] =
        serde_json::to_value(&permissions).map_err(|e| e.to_string())?;
    let metadata = serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?;
    let cipher_stage = write_staged(original_path, &encrypted.ciphertext)?;
    let metadata_stage = match write_staged(original_path, &metadata) {
        Ok(path) => path,
        Err(error) => {
            let _ = std::fs::remove_file(&cipher_stage);
            return Err(error);
        }
    };
    if let Err(error) = install_staged(&cipher_stage, &cipher_path) {
        return Err(format!(
            "network: cannot install encrypted bytes ({error}); stages retained at '{}' and '{}'",
            cipher_stage.display(),
            metadata_stage.display()
        ));
    }
    let _ = std::fs::remove_file(&cipher_stage);
    if let Err(error) = install_staged(&metadata_stage, &metadata_path) {
        return Err(format!("network: cannot install encryption metadata ({error}); metadata stage retained at '{}'", metadata_stage.display()));
    }
    let _ = std::fs::remove_file(&metadata_stage);
    if let Err(error) = sync_parent_dir(original_path) {
        let _ = remove_sidecars(original_path);
        return Err(error);
    }
    Ok(())
}

fn write_staged(original_path: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let parent = original_path.parent().unwrap_or_else(|| Path::new("."));
    let name = original_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let staged = parent.join(format!(".{name}.encrypt-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&staged)
            .map_err(|e| format!("network: cannot stage encrypted file: {e}"))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("network: cannot flush staged encrypted file: {e}"))?;
        Ok(staged.clone())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct RestorePermissions {
    #[serde(default)]
    unix_mode: Option<u32>,
    #[serde(default)]
    readonly: bool,
}

impl RestorePermissions {
    fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Never resurrect setuid/setgid/sticky bits from mutable metadata.
            Self {
                unix_mode: Some(metadata.permissions().mode() & 0o777),
                readonly: metadata.permissions().readonly(),
            }
        }
        #[cfg(not(unix))]
        Self {
            unix_mode: None,
            readonly: metadata.permissions().readonly(),
        }
    }

    fn apply(&self, file: &std::fs::File) -> Result<(), String> {
        let mut permissions = file.metadata().map_err(|e| e.to_string())?.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(self.unix_mode.unwrap_or(0o600) & 0o777);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(self.readonly);
        file.set_permissions(permissions)
            .map_err(|e| format!("network: cannot restore permissions: {e}"))
    }
}

fn read_restore_permissions(metadata_path: &Path) -> Result<RestorePermissions, String> {
    let bytes = std::fs::read(metadata_path).map_err(|e| e.to_string())?;
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    metadata
        .get("original_permissions")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| format!("integrity: invalid original permissions: {e}"))
        .map(Option::unwrap_or_default)
}

/// Portable no-clobber installation. Unlike rename(), create_new never replaces
/// another writer's destination; unlike hard_link(), this works on FAT/network
/// filesystems. Keep the fsynced stage on failure so incomplete copies are not
/// mistaken for the only recovery artifact.
fn install_staged(staged: &Path, destination: &Path) -> Result<(), String> {
    let mut source = std::fs::File::open(staged).map_err(|e| e.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut target = options.open(destination).map_err(|e| {
        format!(
            "network: cannot exclusively create '{}': {e}",
            destination.display()
        )
    })?;
    std::io::copy(&mut source, &mut target).map_err(|e| {
        format!(
            "network: incomplete install at '{}'; stage retained at '{}': {e}",
            destination.display(),
            staged.display()
        )
    })?;
    target
        .set_permissions(source.metadata().map_err(|e| e.to_string())?.permissions())
        .map_err(|e| e.to_string())?;
    target
        .sync_all()
        .map_err(|e| format!("network: cannot sync installed file: {e}"))
}

fn write_restored(
    path: &Path,
    bytes: &[u8],
    permissions: &RestorePermissions,
) -> Result<(), String> {
    let staged = write_staged(path, bytes)?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&staged)
        .map_err(|e| e.to_string())?;
    permissions.apply(&file)?;
    file.sync_all().map_err(|e| e.to_string())?;
    install_staged(&staged, path)?;
    std::fs::remove_file(&staged).map_err(|e| e.to_string())?;
    sync_parent_dir(path)
}

/// Keep the verifier until ciphertext deletion is durable. Missing files are
/// already-cleaned successes; other failures retain metadata for a safe retry.
fn remove_sidecars(original_path: &Path) -> Result<(), String> {
    let (cipher_path, metadata_path) = sidecar_paths(original_path);
    remove_if_present(&cipher_path)?;
    sync_parent_dir(original_path)?;
    remove_if_present(&metadata_path)?;
    sync_parent_dir(original_path)
}

fn remove_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("network: cannot remove '{}': {e}", path.display())),
    }
}

fn private_recovery_dir(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = parent.join(format!(".cybermanju-recovery-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&directory)
        .map_err(|e| format!("network: cannot create private recovery directory: {e}"))?;
    Ok(directory)
}

fn preserve_orphan_ciphertext(cipher_path: &Path) -> Result<String, String> {
    let directory = private_recovery_dir(cipher_path)?;
    let recovery = directory.join("orphan.enc");
    std::fs::rename(cipher_path, &recovery)
        .map_err(|e| format!("network: cannot preserve orphan ciphertext: {e}"))?;
    sync_parent_dir(&recovery)?;
    sync_parent_dir(cipher_path)?;
    Ok(format!("conflict: ciphertext without metadata preserved at '{}'; retry to continue with the existing plaintext", recovery.display()))
}

/// An interrupted portable copy may leave malformed final metadata. Relocate,
/// never delete, the entire unverified package so a retry is not permanently
/// blocked and every artifact remains available for manual recovery.
fn preserve_incomplete_sidecars(original: &Path, reason: &str) -> Result<String, String> {
    let (cipher, metadata) = sidecar_paths(original);
    let directory = private_recovery_dir(original)?;
    for (source, name) in [(&cipher, "ciphertext.enc"), (&metadata, "metadata.json")] {
        if source.exists() {
            let destination = directory.join(name);
            std::fs::rename(source, &destination).map_err(|e| {
                format!(
                    "network: partial package recovery at '{}': {e}",
                    directory.display()
                )
            })?;
            sync_parent_dir(&destination)?;
            sync_parent_dir(source)?;
        }
    }
    Ok(format!("conflict: incomplete encryption package ({reason}) preserved at '{}'; plaintext retained; retry to continue", directory.display()))
}

fn read_source_snapshot(path: &Path) -> Result<(Vec<u8>, std::fs::Metadata), String> {
    let entry = std::fs::symlink_metadata(path)
        .map_err(|e| format!("not_found: cannot inspect source: {e}"))?;
    if !entry.is_file() || entry.file_type().is_symlink() {
        return Err(
            "unsupported: encryption source must be a regular, non-symlink file".to_string(),
        );
    }
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("not_found: cannot read source: {e}"))?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if !same_source(&entry, &metadata)
        || !same_source(&metadata, &file.metadata().map_err(|e| e.to_string())?)
    {
        return Err(
            "conflict: source changed while taking encryption snapshot; source retained"
                .to_string(),
        );
    }
    Ok((bytes, metadata))
}

fn same_source(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    if a.len() != b.len()
        || a.modified().ok() != b.modified().ok()
        || a.is_file() != b.is_file()
        || a.permissions().readonly() != b.permissions().readonly()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino() && a.mode() == b.mode()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// This is intentionally NOT an auto-deleting guard. Any mismatch/error/crash
/// leaves the captured inode in its private recovery directory for inspection.
struct SourceClaim {
    path: PathBuf,
    directory: PathBuf,
}

impl SourceClaim {
    fn capture(
        original: &Path,
        expected: &[u8],
        metadata: &std::fs::Metadata,
    ) -> Result<Self, String> {
        Self::capture_before_rename(original, expected, metadata, || {})
    }

    fn capture_before_rename(
        original: &Path,
        expected: &[u8],
        metadata: &std::fs::Metadata,
        before_rename: impl FnOnce(),
    ) -> Result<Self, String> {
        let (current, current_metadata) = read_source_snapshot(original)?;
        if current != expected || !same_source(metadata, &current_metadata) {
            return Err(format!(
                "conflict: source changed before capture; source retained at '{}'",
                original.display()
            ));
        }
        before_rename();
        let directory = private_recovery_dir(original)?;
        let claim = Self {
            path: directory.join("source"),
            directory,
        };
        std::fs::rename(original, &claim.path)
            .map_err(|e| format!("conflict: cannot capture source: {e}"))?;
        sync_parent_dir(&claim.path)
            .and_then(|()| sync_parent_dir(original))
            .map_err(|e| format!("{e}; source retained at '{}'", claim.path.display()))?;
        let captured_metadata =
            std::fs::symlink_metadata(&claim.path).map_err(|e| e.to_string())?;
        if !same_source(metadata, &captured_metadata) || captured_metadata.file_type().is_symlink()
        {
            return Err(format!(
                "conflict: source was replaced; all captured bytes retained at '{}'",
                claim.path.display()
            ));
        }
        claim.verify(expected)?;
        Ok(claim)
    }

    fn verify(&self, expected: &[u8]) -> Result<(), String> {
        let bytes = std::fs::read(&self.path).map_err(|e| {
            format!(
                "integrity: cannot verify captured source '{}': {e}",
                self.path.display()
            )
        })?;
        if bytes != expected {
            return Err(format!(
                "conflict: source changed; current bytes retained at '{}'",
                self.path.display()
            ));
        }
        Ok(())
    }

    fn finish(self, expected: &[u8]) -> Result<(), String> {
        self.verify(expected)?;
        std::fs::remove_file(&self.path)
            .map_err(|e| format!("network: source retained at '{}': {e}", self.path.display()))?;
        sync_parent_dir(&self.path)?;
        std::fs::remove_dir(&self.directory).map_err(|e| e.to_string())?;
        sync_parent_dir(&self.directory)
    }
}

fn restore_after_encrypt_failure(
    original: &Path,
    bytes: &[u8],
    metadata: &std::fs::Metadata,
    error: String,
) -> String {
    // create_new prevents overwriting a concurrently-created source pathname.
    let restore = write_restored(
        original,
        bytes,
        &RestorePermissions::from_metadata(metadata),
    );
    format!("{error}; plaintext restore: {restore:?}; encrypted recovery sidecars retained")
}

/// Batch compress: record a compression layer on multiple files.
pub fn compress(db: &Database, file_ids: &[String], layer: &str) -> Result<u32, String> {
    let mut count = 0u32;

    for file_id in file_ids {
        let mut file_node = read_file(db, file_id)?;
        if !file_node.compression_layers.contains(&layer.to_string()) {
            file_node.compression_layers.push(layer.to_string());
            file_node.modified_at = chrono::Utc::now().to_rfc3339();
            write_file(db, file_id, &file_node)?;
            db.log_audit("batch_compress", "file", file_id, None, None)
                .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok(count)
}

fn read_file(db: &Database, file_id: &str) -> Result<FileNode, String> {
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx_read
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    table
        .get(file_id)
        .map_err(|e| e.to_string())?
        .and_then(|v| serde_json::from_str(v.value()).ok())
        .ok_or_else(|| format!("File not found: {}", file_id))
}

fn write_file(db: &Database, file_id: &str, node: &FileNode) -> Result<(), String> {
    let serialized = serde_json::to_string(node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(file_id, serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cybermanju_crypto::pqc::PqcEngine;

    fn test_node(id: &str, path: Option<&Path>) -> FileNode {
        FileNode {
            id: id.to_string(),
            name: "roundtrip.bin".to_string(),
            file_type: "file".to_string(),
            parent_id: None,
            size_bytes: 0,
            mime_type: Some("application/octet-stream".to_string()),
            hash_blake3: None,
            encrypted: false,
            encryption_algorithm: None,
            compression_layers: Vec::new(),
            thumbnail_path: None,
            context_data: path
                .map(|p| serde_json::json!({"original_path": p.to_string_lossy().to_string()})),
            tags: Vec::new(),
            collection_ids: Vec::new(),
            face_group_ids: Vec::new(),
            loose_group_ids: Vec::new(),
            gps_lat: None,
            gps_lon: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn b64(bytes: &[u8]) -> String {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    }

    fn seed_key(db: &Database) -> KeyPair {
        seed_key_for(db, "hybrid")
    }

    fn seed_key_for(db: &Database, algorithm: &str) -> KeyPair {
        let mut engine = PqcEngine::new();
        let pair = engine
            .generate_keypair(pqc::algorithm_from_str(algorithm).unwrap())
            .unwrap();
        let stored = EncryptionKey {
            id: pair.id.clone(),
            algorithm: algorithm.to_string(),
            public_key: b64(&pair.public_key),
            private_key: b64(&pair.private_key),
            label: Some("test key".to_string()),
            created_at: pair.created_at.clone(),
            x25519_public_key: pair.x25519_public_key.as_deref().map(b64),
            x25519_private_key: pair.x25519_private_key.as_deref().map(b64),
        };
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx
                .open_table(Database::get_encryption_keys_table())
                .unwrap();
            let serialized = serde_json::to_string(&stored).unwrap();
            table
                .insert(stored.id.as_str(), serialized.as_str())
                .unwrap();
        }
        tx.commit().unwrap();
        pair
    }

    fn seed_file(db: &Database, node: &FileNode) {
        let json = serde_json::to_string(node).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_files_table()).unwrap();
            table.insert(node.id.as_str(), json.as_str()).unwrap();
        }
        tx.commit().unwrap();
    }

    #[test]
    fn batch_crypto_persists_real_ciphertext_and_round_trips_without_plaintext_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        let pair = seed_key(&db);
        let file_path = dir.path().join("source.bin");
        let plaintext = b"batch encryption must protect actual bytes\0\xff";
        std::fs::write(&file_path, plaintext).unwrap();
        seed_file(&db, &test_node("file-1", Some(&file_path)));

        let count = encrypt(&db, &["file-1".to_string()], "hybrid").unwrap();
        assert_eq!(count, 1);
        assert!(
            !file_path.exists(),
            "plaintext source must not remain after success"
        );
        let cipher_path = PathBuf::from(format!("{}.enc", file_path.display()));
        let metadata_path = PathBuf::from(format!("{}.enc.meta.json", file_path.display()));
        let ciphertext = std::fs::read(&cipher_path).unwrap();
        let metadata_bytes = std::fs::read(&metadata_path).unwrap();
        assert_ne!(ciphertext, plaintext);
        let metadata: EncryptedFileMeta = serde_json::from_slice(&metadata_bytes).unwrap();
        let artifact = metadata.to_encrypted_data(ciphertext.clone()).unwrap();
        assert_eq!(pqc::decrypt_data(&artifact, &pair).unwrap(), plaintext);

        // Model a crash after durable sidecar creation + plaintext unlink but
        // before the DB's encrypted flag commits: batch decryption must still
        // discover and recover the only remaining bytes.
        let mut interrupted = read_file(&db, "file-1").unwrap();
        interrupted.encrypted = false;
        interrupted.encryption_algorithm = None;
        write_file(&db, "file-1", &interrupted).unwrap();
        decrypt(&db, &["file-1".to_string()]).unwrap();
        assert_eq!(std::fs::read(&file_path).unwrap(), plaintext);
        assert!(!cipher_path.exists());
        assert!(!metadata_path.exists());
        assert!(!read_file(&db, "file-1").unwrap().encrypted);

        // Model a crash after the plaintext + decrypted DB commit but before
        // sidecar cleanup. Repeating decrypt removes only hash-validated stale
        // artifacts and leaves the restored plaintext intact.
        std::fs::write(&cipher_path, &artifact.ciphertext).unwrap();
        std::fs::write(&metadata_path, &metadata_bytes).unwrap();
        decrypt_one(&db, "file-1").unwrap();
        assert!(!cipher_path.exists());
        assert!(!metadata_path.exists());
        assert_eq!(std::fs::read(&file_path).unwrap(), plaintext);
    }

    #[test]
    fn symmetric_and_signing_seed_modes_use_real_aead_and_round_trip() {
        for algorithm in [
            "kyber1024",
            "aes256",
            "ml_dsa44",
            "ml_dsa65",
            "ml_dsa87",
            "classical_sign",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
            seed_key_for(&db, algorithm);
            let path = dir.path().join("source.bin");
            let plaintext = b"real byte protection, not a metadata flag\0\xff";
            std::fs::write(&path, plaintext).unwrap();
            seed_file(&db, &test_node("file", Some(&path)));
            assert_eq!(encrypt(&db, &["file".to_string()], algorithm).unwrap(), 1);
            let (cipher, _) = sidecar_paths(&path);
            assert!(!path.exists());
            assert_ne!(std::fs::read(&cipher).unwrap(), plaintext);
            decrypt_one(&db, "file").unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), plaintext);
            assert!(!read_file(&db, "file").unwrap().encrypted);
        }
    }

    #[test]
    fn tampered_recovery_package_never_removes_the_last_good_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        seed_key(&db);
        let path = dir.path().join("source.bin");
        let plaintext = b"retain this good copy";
        std::fs::write(&path, plaintext).unwrap();
        seed_file(&db, &test_node("file", Some(&path)));
        encrypt_one(&db, "file", "hybrid").unwrap();
        let (cipher, _) = sidecar_paths(&path);
        let mut bad = std::fs::read(&cipher).unwrap();
        bad[0] ^= 1;
        std::fs::write(&cipher, bad).unwrap();
        assert!(encrypt_one(&db, "file", "hybrid")
            .unwrap_err()
            .starts_with("integrity:"));
        assert!(!path.exists());
        std::fs::write(&path, plaintext).unwrap();
        assert!(encrypt_one(&db, "file", "hybrid")
            .unwrap_err()
            .starts_with("integrity:"));
        assert_eq!(std::fs::read(&path).unwrap(), plaintext);
        assert!(decrypt_one(&db, "file")
            .unwrap_err()
            .starts_with("integrity:"));
        assert_eq!(std::fs::read(&path).unwrap(), plaintext);
    }

    fn recovery_sources(directory: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(directory)
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.unwrap();
                let source = entry.path().join("source");
                source.exists().then_some(source)
            })
            .collect()
    }

    #[test]
    fn external_edit_after_encryption_verification_is_retained_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        let key = seed_key(&db);
        let path = dir.path().join("source.bin");
        std::fs::write(&path, b"snapshot").unwrap();
        seed_file(&db, &test_node("file", Some(&path)));
        let error = encrypt_with_key_before_capture(&db, "file", &key, |source| {
            std::fs::write(source, b"external edit must survive").unwrap();
        })
        .unwrap_err();
        assert!(error.starts_with("conflict:"), "{error}");
        assert!(recovery_sources(dir.path()).is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"external edit must survive");
        assert_eq!(authenticate_sidecars(&db, &path).unwrap(), b"snapshot");
        assert!(!read_file(&db, "file").unwrap().encrypted);
    }

    #[test]
    fn external_replacement_after_snapshot_is_retained_with_original_ciphertext() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        let key = seed_key(&db);
        let path = dir.path().join("source.bin");
        std::fs::write(&path, b"original").unwrap();
        seed_file(&db, &test_node("file", Some(&path)));
        let error = encrypt_with_key_before_capture(&db, "file", &key, |source| {
            let replacement = dir.path().join("replacement");
            std::fs::write(&replacement, b"replacement bytes").unwrap();
            std::fs::remove_file(source).unwrap();
            std::fs::rename(&replacement, source).unwrap();
        })
        .unwrap_err();
        assert!(error.starts_with("conflict:"), "{error}");
        assert!(recovery_sources(dir.path()).is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement bytes");
        assert_eq!(authenticate_sidecars(&db, &path).unwrap(), b"original");
        assert!(!read_file(&db, "file").unwrap().encrypted);
    }

    #[test]
    fn replacement_between_preflight_and_rename_is_preserved_in_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.bin");
        std::fs::write(&path, b"old snapshot").unwrap();
        let (bytes, metadata) = read_source_snapshot(&path).unwrap();
        let result = SourceClaim::capture_before_rename(&path, &bytes, &metadata, || {
            let replacement = dir.path().join("replacement");
            std::fs::write(&replacement, b"latest replacement bytes").unwrap();
            std::fs::remove_file(&path).unwrap();
            std::fs::rename(&replacement, &path).unwrap();
        });
        assert!(result.is_err());
        let retained = recovery_sources(dir.path());
        assert_eq!(retained.len(), 1);
        assert_eq!(
            std::fs::read(&retained[0]).unwrap(),
            b"latest replacement bytes"
        );
    }

    #[test]
    fn interrupted_metadata_copy_is_preserved_and_retry_is_not_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        seed_key(&db);
        let path = dir.path().join("source.bin");
        std::fs::write(&path, b"source plaintext").unwrap();
        seed_file(&db, &test_node("file", Some(&path)));
        let (cipher, metadata) = sidecar_paths(&path);
        std::fs::write(&cipher, b"partial encrypted bytes").unwrap();
        std::fs::write(&metadata, b"{partial").unwrap();
        let error = encrypt_one(&db, "file", "hybrid").unwrap_err();
        assert!(error.contains("incomplete encryption package"), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), b"source plaintext");
        assert!(!cipher.exists());
        assert!(!metadata.exists());
        let retained = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| {
                let directory = entry.unwrap().path();
                directory
                    .join("metadata.json")
                    .exists()
                    .then_some(directory)
            })
            .next()
            .unwrap();
        assert_eq!(
            std::fs::read(retained.join("metadata.json")).unwrap(),
            b"{partial"
        );
        assert_eq!(
            std::fs::read(retained.join("ciphertext.enc")).unwrap(),
            b"partial encrypted bytes"
        );
        encrypt_one(&db, "file", "hybrid").unwrap();
        decrypt_one(&db, "file").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"source plaintext");
    }

    #[test]
    fn cleanup_failure_keeps_metadata_for_a_safe_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.bin");
        let (cipher, metadata) = sidecar_paths(&path);
        std::fs::create_dir(&cipher).unwrap();
        std::fs::write(&metadata, b"verifier must survive").unwrap();
        assert!(remove_sidecars(&path).is_err());
        assert_eq!(std::fs::read(&metadata).unwrap(), b"verifier must survive");
        std::fs::remove_dir(&cipher).unwrap();
        remove_sidecars(&path).unwrap();
        remove_sidecars(&path).unwrap();
        assert!(!metadata.exists());
    }

    #[test]
    fn source_retirement_never_unlinks_a_new_public_pathname() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.bin");
        std::fs::write(&path, b"original").unwrap();
        let (bytes, metadata) = read_source_snapshot(&path).unwrap();
        let claim = SourceClaim::capture(&path, &bytes, &metadata).unwrap();
        std::fs::write(&path, b"new pathname").unwrap();
        claim.finish(&bytes).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new pathname");
    }

    #[test]
    fn edits_through_a_preexisting_handle_are_detected_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.bin");
        std::fs::write(&path, b"original").unwrap();
        let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let (bytes, metadata) = read_source_snapshot(&path).unwrap();
        let claim = SourceClaim::capture(&path, &bytes, &metadata).unwrap();
        let retained = claim.path.clone();
        writer.write_all(b"modified").unwrap();
        writer.sync_all().unwrap();
        assert!(claim.finish(&bytes).unwrap_err().starts_with("conflict:"));
        assert_eq!(std::fs::read(&retained).unwrap(), b"modified");
    }

    #[test]
    fn portable_install_does_not_require_links_and_never_overwrites_a_destination() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("destination");
        let stage = write_staged(&destination, b"complete staged bytes").unwrap();
        install_staged(&stage, &destination).unwrap();
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"complete staged bytes"
        );
        std::fs::write(&destination, b"another writer").unwrap();
        assert!(install_staged(&stage, &destination).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"another writer");
        assert_eq!(std::fs::read(&stage).unwrap(), b"complete staged bytes");
    }

    #[cfg(unix)]
    #[test]
    fn executable_mode_survives_encryption_decryption_and_rollback() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        seed_key(&db);
        let path = dir.path().join("script.sh");
        std::fs::write(&path, b"#!/bin/sh\necho protected\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o751)).unwrap();
        seed_file(&db, &test_node("file", Some(&path)));
        encrypt_one(&db, "file", "hybrid").unwrap();
        let (cipher, metadata_path) = sidecar_paths(&path);
        assert_eq!(
            std::fs::metadata(&cipher).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            read_restore_permissions(&metadata_path).unwrap().unix_mode,
            Some(0o751)
        );
        decrypt_one(&db, "file").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o751
        );
        let (bytes, metadata) = read_source_snapshot(&path).unwrap();
        let claim = SourceClaim::capture(&path, &bytes, &metadata).unwrap();
        let retained = claim.path.clone();
        restore_after_encrypt_failure(&path, &bytes, &metadata, "injected DB failure".to_string());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o751
        );
        assert_eq!(
            std::fs::metadata(&retained).unwrap().permissions().mode() & 0o777,
            0o751
        );
    }

    #[test]
    fn decrypt_cleanup_retry_handles_missing_ciphertext_and_repeated_success() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        seed_key(&db);
        let path = dir.path().join("source.bin");
        let plaintext = b"restored committed plaintext";
        std::fs::write(&path, plaintext).unwrap();
        seed_file(&db, &test_node("file", Some(&path)));
        encrypt_one(&db, "file", "hybrid").unwrap();
        let (cipher, metadata) = sidecar_paths(&path);
        std::fs::write(&path, plaintext).unwrap();
        let mut node = read_file(&db, "file").unwrap();
        node.encrypted = false;
        node.encryption_algorithm = None;
        write_file(&db, "file", &node).unwrap();
        // Simulate interruption after ciphertext deletion, before metadata deletion.
        std::fs::remove_file(&cipher).unwrap();
        decrypt_one(&db, "file").unwrap();
        decrypt_one(&db, "file").unwrap();
        assert!(!metadata.exists());
        assert_eq!(std::fs::read(&path).unwrap(), plaintext);
        encrypt_one(&db, "file", "hybrid").unwrap();
        decrypt_one(&db, "file").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), plaintext);
    }

    #[test]
    fn legacy_metadata_first_cleanup_preserves_orphan_and_unblocks_retry() {
        for reencrypt in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
            seed_key(&db);
            let path = dir.path().join("source.bin");
            let plaintext = b"restored plaintext";
            std::fs::write(&path, plaintext).unwrap();
            seed_file(&db, &test_node("file", Some(&path)));
            encrypt_one(&db, "file", "hybrid").unwrap();
            let (cipher, metadata) = sidecar_paths(&path);
            let ciphertext = std::fs::read(&cipher).unwrap();
            std::fs::write(&path, plaintext).unwrap();
            let mut node = read_file(&db, "file").unwrap();
            node.encrypted = false;
            node.encryption_algorithm = None;
            write_file(&db, "file", &node).unwrap();
            std::fs::remove_file(&metadata).unwrap();
            let result = if reencrypt {
                encrypt_one(&db, "file", "hybrid").map(|_| ())
            } else {
                decrypt_one(&db, "file")
            };
            let error = result.unwrap_err();
            assert!(error.contains("orphan.enc"), "{error}");
            assert_eq!(std::fs::read(&path).unwrap(), plaintext);
            let orphan = std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|entry| {
                    let candidate = entry.unwrap().path().join("orphan.enc");
                    candidate.exists().then_some(candidate)
                })
                .next()
                .unwrap();
            assert_eq!(std::fs::read(orphan).unwrap(), ciphertext);
            if reencrypt {
                encrypt_one(&db, "file", "hybrid").unwrap();
            }
            decrypt_one(&db, "file").unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), plaintext);
        }
    }

    #[test]
    fn missing_byte_pipeline_refuses_without_changing_encryption_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("db.redb").to_str().unwrap()).unwrap();
        seed_key(&db);
        seed_file(&db, &test_node("file-no-path", None));

        let error = encrypt_one(&db, "file-no-path", "hybrid").unwrap_err();
        assert!(error.starts_with("unsupported:"), "{error}");
        let node = read_file(&db, "file-no-path").unwrap();
        assert!(!node.encrypted);
        assert!(node.encryption_algorithm.is_none());
    }
}
