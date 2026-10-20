// CyberManju OS — Sync Pipeline
// Orchestrates the full sync flow: scan → compress → encrypt → upload →
// verify → link → clean
//
// <<< AGENT-2: items 1–4, 8–9 live in this file — verified deletion, the
// durable `sync_files` locator, single-point byte accounting, collision-safe
// remote paths, encrypt-before-upload and conflict policy. >>>

use crate::backends::create_backend;
use crate::manifest;
use crate::quota;
use crate::rate_limit;
use crate::retry;
use crate::state::SyncState;
use crate::transfer;
use chrono::Utc;
use cybermanju_compression::TripleCompressor;
use cybermanju_crypto::keystore;
use cybermanju_db::Database;
use cybermanju_types::schema::FileNode;
use cybermanju_types::sync::*;
use log::{error, info, warn};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Removes a staging file when it drops, so retry-exhausted or cancelled
/// uploads never leak `cybermanju-*.cyb3` temps (error paths included —
/// the explicit success-path removals stay as the fast path).
struct RemoveOnDrop<'a> {
    path: &'a Path,
}

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.path);
    }
}

/// Best-effort removal of every landed striped chunk copy. Shared by the
/// link/record failure paths (both run after chunk copies landed on
/// providers with no manifest pointing at them).
fn remove_striped_copies(
    backends: &HashMap<String, Box<dyn StorageBackend>>,
    manifest_obj: &manifest::ChunkManifest,
) {
    for chunk in &manifest_obj.chunks {
        for loc in std::iter::once(&chunk.primary).chain(chunk.replicas.iter()) {
            if let Some(backend) = backends.get(&loc.config_id) {
                if let Err(remove_err) = backend.delete_file(&loc.remote_path) {
                    warn!(
                        "orphaned striped chunk '{}' could not be removed: {}",
                        loc.remote_path, remove_err
                    );
                }
            }
        }
    }
}

/// Magic prefix on artifacts whose payload is a `keystore::seal` blob.
/// Restores key off this, so a `remotePath` restore works even without a
/// local `sync_files` record.
pub const CYBE_MAGIC: &[u8; 5] = b"CYBE1";

/// Keystore handle id used for sync encryption (salt + wrap live in the
/// keystore; only this id is stored in `sync_files.key_handle`).
const SYNC_KEY_HANDLE: &str = "sync";

/// Result of `SyncPipeline::transform_payload`:
/// `(payload, compressed, encrypted, key_handle, bytes_saved)`.
type TransformedPayload = (Vec<u8>, bool, bool, Option<String>, u64);

// ===========================================================================
// SyncPipeline
// ===========================================================================

/// Per-file sync result returned by the parallel inner worker.
struct FileSyncResult {
    file_id: String,
    bytes_uploaded: u64,
    bytes_saved: u64,
    error: Option<String>,
}

/// The main sync orchestrator. Holds configuration and shared progress state.
pub struct SyncPipeline {
    config: SyncConfig,
    state: Arc<SyncState>,
}

impl SyncPipeline {
    /// Create a new pipeline for the given config, sharing `state` for
    /// live progress reporting and cancellation.
    pub fn new(config: SyncConfig, state: Arc<SyncState>) -> Self {
        Self { config, state }
    }

    /// Cancel an in-progress sync.
    pub fn cancel(&self) {
        self.state.cancel();
    }

    /// Check if the sync has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    /// Get a snapshot of the current progress.
    pub fn get_progress(&self) -> SyncProgress {
        self.state.snapshot()
    }

    // -----------------------------------------------------------------------
    // Main entry points
    // -----------------------------------------------------------------------

    /// Sync all the given file IDs to the configured backend.
    /// Uses rayon parallel iterators when `max_concurrent_uploads > 1`,
    /// otherwise falls back to sequential processing.
    pub fn sync_all(
        &self,
        file_ids: Vec<String>,
        db: &RwLock<Database>,
        compressor: &TripleCompressor,
    ) -> Result<SyncResult, String> {
        self.reset_progress(file_ids.len() as u32);
        self.state.set_started_at(Some(Utc::now().to_rfc3339()));

        let backend = create_backend(&self.config)?;

        if self.config.max_concurrent_uploads > 1 {
            self.sync_all_parallel(&file_ids, backend.as_ref(), db, compressor)
        } else {
            self.sync_all_sequential(&file_ids, backend.as_ref(), db, compressor)
        }
    }

    /// Sequential sync — one file at a time (fallback).
    fn sync_all_sequential(
        &self,
        file_ids: &[String],
        backend: &dyn StorageBackend,
        db: &RwLock<Database>,
        compressor: &TripleCompressor,
    ) -> Result<SyncResult, String> {
        let mut total_bytes_uploaded: u64 = 0;
        let mut bytes_saved: u64 = 0;
        let mut files_synced: u32 = 0;
        let start = std::time::Instant::now();

        for file_id in file_ids {
            if self.is_cancelled() {
                warn!("Sync cancelled by user");
                break;
            }

            match self.sync_single_file_inner(file_id, backend, compressor, db) {
                Ok((uploaded, saved)) => {
                    total_bytes_uploaded += uploaded;
                    bytes_saved += saved;
                    files_synced += 1;
                    // Single byte-accounting point for sequential runs
                    // (item 3): the inner worker never touches the counter.
                    self.state.add_bytes(uploaded);
                }
                Err(e) => {
                    error!("Failed to sync file {}: {}", file_id, e);
                    self.state.add_error(e);
                }
            }

            self.state.inc_processed();
        }

        let final_status = if self.is_cancelled() {
            SyncStatus::Cancelled
        } else {
            SyncStatus::Completed
        };
        self.state.set_status(final_status);

        Ok(SyncResult {
            files_synced,
            bytes_uploaded: total_bytes_uploaded,
            bytes_saved_by_compression: bytes_saved,
            errors: self.state.snapshot().errors,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Parallel sync using rayon — each file is independent.
    fn sync_all_parallel(
        &self,
        file_ids: &[String],
        backend: &dyn StorageBackend,
        db: &RwLock<Database>,
        compressor: &TripleCompressor,
    ) -> Result<SyncResult, String> {
        let start = std::time::Instant::now();

        // Configure the rayon thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.max_concurrent_uploads as usize)
            .build()
            .map_err(|e| format!("Failed to create thread pool: {}", e))?;

        let results: Vec<FileSyncResult> = pool.install(|| {
            file_ids
                .par_iter()
                .filter(|_| !self.is_cancelled())
                .map(|file_id| {
                    match self.sync_single_file_inner(file_id, backend, compressor, db) {
                        Ok((uploaded, saved)) => {
                            self.state.inc_processed();
                            self.state.add_bytes(uploaded);
                            FileSyncResult {
                                file_id: file_id.clone(),
                                bytes_uploaded: uploaded,
                                bytes_saved: saved,
                                error: None,
                            }
                        }
                        Err(e) => {
                            error!("Failed to sync file {}: {}", file_id, e);
                            self.state.inc_processed();
                            FileSyncResult {
                                file_id: file_id.clone(),
                                bytes_uploaded: 0,
                                bytes_saved: 0,
                                error: Some(e),
                            }
                        }
                    }
                })
                .collect()
        });

        // Aggregate results
        let mut total_bytes_uploaded: u64 = 0;
        let mut bytes_saved: u64 = 0;
        let mut files_synced: u32 = 0;

        for r in &results {
            if let Some(ref err) = r.error {
                self.state.add_error(format!("{}: {}", r.file_id, err));
            } else {
                total_bytes_uploaded += r.bytes_uploaded;
                bytes_saved += r.bytes_saved;
                files_synced += 1;
            }
        }

        let final_status = if self.is_cancelled() {
            SyncStatus::Cancelled
        } else {
            SyncStatus::Completed
        };
        self.state.set_status(final_status);

        Ok(SyncResult {
            files_synced,
            bytes_uploaded: total_bytes_uploaded,
            bytes_saved_by_compression: bytes_saved,
            errors: self.state.snapshot().errors,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Sync a single file by ID.
    pub fn sync_single_file(
        &self,
        file_id: String,
        db: &RwLock<Database>,
        compressor: &TripleCompressor,
    ) -> Result<SyncResult, String> {
        self.reset_progress(1);
        self.state.set_started_at(Some(Utc::now().to_rfc3339()));

        let backend = create_backend(&self.config)?;
        let start = std::time::Instant::now();

        let (bytes_uploaded, bytes_saved) =
            self.sync_single_file_inner(&file_id, backend.as_ref(), compressor, db)?;

        self.state.inc_processed();
        self.state.add_bytes(bytes_uploaded);
        self.state.set_status(SyncStatus::Completed);

        Ok(SyncResult {
            files_synced: 1,
            bytes_uploaded,
            bytes_saved_by_compression: bytes_saved,
            errors: self.state.snapshot().errors,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    // -----------------------------------------------------------------------
    // Internal: sync a single file
    // -----------------------------------------------------------------------

    /// Returns (bytes_uploaded, bytes_saved_by_compression).
    ///
    /// Order of operations (documented once, here):
    /// conflict check → compress → encrypt → preflight → upload →
    /// verify (when deletion is at stake) → link → persist locator → clean.
    fn sync_single_file_inner(
        &self,
        file_id: &str,
        backend: &dyn StorageBackend,
        compressor: &TripleCompressor,
        db: &RwLock<Database>,
    ) -> Result<(u64, u64), String> {
        // 1. Read file node from DB
        let file_node = self.get_file_node(file_id, db)?;

        // Get the actual file path from context_data
        let original_path = file_node
            .context_data
            .as_ref()
            .and_then(|ctx| ctx.get("original_path").and_then(|v| v.as_str()))
            .unwrap_or(&file_node.name)
            .to_string();

        // One-folder loop guard: the vault container + secret sidecars are
        // never sync sources. Skip quietly — the vault re-hashes every save,
        // so erroring here would spam every auto-sync tick forever.
        if crate::backends::is_protected_remote_path(&original_path)
            || original_path
                .rsplit('/')
                .next()
                .unwrap_or(&original_path)
                .to_lowercase()
                .ends_with(".cybermanju")
        {
            info!("{} is a vault/secret path — skipping sync", original_path);
            return Ok((0, 0));
        }

        // A `deleteRawAfterSync` original that is already gone is the
        // *intended* end state, not a failure: the verified remote copy is
        // the live one and restore brings the bytes back. Skip quietly —
        // otherwise every later run (and every auto-sync tick) would spam
        // "File not found" for files this config deliberately removed.
        if self.config.delete_raw_after_sync && !Path::new(&original_path).exists() {
            if let Some(record) = self.load_sync_file(file_id, db) {
                let placed = record.remote_path.is_some() || record.manifest_ref.is_some();
                if placed && record.hash_blake3.is_some() {
                    info!(
                        "{} already synced and its original removed — skipping",
                        original_path
                    );
                    return Ok((0, 0));
                }
            }
        }

        if !Path::new(&original_path).exists() {
            return Err(format!(
                "not_found: file not found on disk: {}",
                original_path
            ));
        }

        // <<< AGENT-6 ADMISSION >>>
        // D1 — police the write *before* anything is built, encrypted or
        // uploaded, so a full volume leaves nothing partial behind. Bytes a
        // previous sync of this same file already occupies only need their
        // growth to fit: an idempotent re-sync is never refused for space.
        let payload_bytes = fs::metadata(&original_path).map(|m| m.len()).unwrap_or(0);
        let already_synced = self
            .load_sync_file(file_id, db)
            .map(|rec| rec.size_bytes)
            .unwrap_or(0);
        let needed = payload_bytes.saturating_sub(already_synced);
        if needed > 0 {
            let guard = db.read().map_err(|e| e.to_string())?;
            quota::admit_write(&guard, needed)?;
        }
        // <<< /AGENT-6 ADMISSION >>>

        self.state.set_current(Some(original_path.clone()));

        // 2. Plaintext BLAKE3 — the restore-verify baseline and the
        //    idempotency signal for conflict detection.
        let local_hash = transfer::hash_file(&original_path)?;

        // Striped placement (item 10) takes over when opted in and at least
        // two enabled configs exist; otherwise `Ok(None)` degrades to the
        // whole-file path below with a WARNING recorded by the callee.
        if self.config.placement == PlacementMode::Striped {
            if let Some(handled) =
                self.sync_single_file_striped(file_id, &original_path, &local_hash, compressor, db)?
            {
                return Ok(handled);
            }
        }

        // 3. Collision-safe, stable remote locator (item 4; basename
        //    hashed when the config opts into obfuscation).
        let mut remote_name = remote_path_for(&original_path, self.config.obfuscate_names);

        // 4. Conflict policy (item 9). Same file + same content + same
        //    locator as the stored record is an idempotent re-sync, not a
        //    conflict — it must not be skipped.
        let existing = self.load_sync_file(file_id, db);
        let idempotent = existing.as_ref().is_some_and(|rec| {
            rec.remote_path.as_deref() == Some(remote_name.as_str())
                && rec.hash_blake3.as_deref() == Some(local_hash.as_str())
        });
        if !idempotent {
            match backend.stat(&remote_name) {
                Ok(Some(_remote)) => match self.config.conflict_policy {
                    ConflictPolicy::Skip => {
                        return Err(format!(
                            "conflict: remote already holds '{}' (policy=skip — set \
                             conflictPolicy to overwrite or keepBoth to proceed)",
                            remote_name
                        ));
                    }
                    ConflictPolicy::Overwrite => {}
                    ConflictPolicy::KeepBoth => {
                        remote_name = keep_both_path(&remote_name, &local_hash);
                    }
                },
                // Unknown metadata — never invent a conflict.
                Ok(None) => {}
                // Confirmed missing → clean first upload.
                Err(e) if e.starts_with("not_found:") => {}
                Err(e) => warn!(
                    "conflict stat for '{}' failed, treating as unknown: {}",
                    remote_name, e
                ),
            }
        }

        // 5. Build the artifact: compress → encrypt (item 8; compress first
        //    because ciphertext does not compress).
        let artifact = self.build_artifact(&original_path, compressor)?;

        // 6. Reject over-size payloads before a single byte leaves.
        transfer::preflight(&self.config.backend_type, artifact.len)?;

        // 7. Preview (if configured)
        let preview_path = if self.config.create_previews {
            self.state.set_status(SyncStatus::Linking);
            self.create_preview(&original_path).ok()
        } else {
            None
        };

        // 8. Upload — per-provider concurrency permit + classified retry.
        self.state.set_status(SyncStatus::Uploading);
        let remote_url = {
            let _permit = rate_limit::acquire(&self.config.backend_type)?;
            retry::with_retry(&retry::RetryPolicy::default(), || {
                backend.upload_file(&artifact.path, &remote_name)
            })?
        };

        // <<< AGENT-6 ADMISSION >>>
        // The bytes are on the disk: account them so `df` sees them. An
        // upload that already landed is never failed by bookkeeping.
        if needed > 0 {
            let guard = db.read().map_err(|e| e.to_string())?;
            if let Err(e) = quota::charge_disk(&guard, &self.config.id, needed) {
                warn!("volume accounting for '{}' failed: {}", original_path, e);
            }
        }
        // <<< /AGENT-6 ADMISSION >>>

        // 9. Verified delete gate (item 1): the local original is only ever
        //    removed after a download-back BLAKE3 match. On any doubt the
        //    original stays and a WARNING lands in `SyncResult.errors`.
        let mut last_verified_at: Option<String> = None;
        if self.config.delete_raw_after_sync {
            match self.verify_remote_copy(backend, &remote_name, &artifact.hash) {
                Ok(()) => last_verified_at = Some(Utc::now().to_rfc3339()),
                Err(e) => self.state.add_error(format!(
                    "WARNING: local original '{}' kept — remote copy not verified: {}",
                    original_path, e
                )),
            }
        }
        let keep_local_artifact = self.config.delete_raw_after_sync && last_verified_at.is_some();

        // 10. Link in FileNode's context_data + persist the durable locator
        //     (item 2: remote_path, backend, hashes, verification stamp).
        //     A link/record failure after a landed upload must not leave a
        //     verified-but-unreferenced remote object behind: compensate
        //     with a best-effort delete so GC never has to guess.
        self.state.set_status(SyncStatus::Linking);
        if let Err(e) = self.create_link(file_id, &remote_url, db) {
            if let Err(remove_err) = backend.delete_file(&remote_name) {
                warn!(
                    "orphaned remote copy '{}' could not be removed: {}",
                    remote_name, remove_err
                );
            }
            return Err(e);
        }
        let record = SyncFile {
            id: file_id.to_string(),
            config_id: Some(self.config.id.clone()),
            home_config_id: Some(self.config.id.clone()),
            original_path: original_path.clone(),
            compressed_path: keep_local_artifact.then(|| artifact.path.clone()),
            preview_path,
            remote_url: Some(remote_url.clone()),
            remote_path: Some(remote_name.clone()),
            size_bytes: artifact.original_size,
            compressed_size_bytes: (!artifact.is_original).then_some(artifact.len),
            hash_blake3: Some(local_hash),
            artifact_hash: Some(artifact.hash.clone()),
            manifest_ref: None,
            last_verified_at,
            key_handle: artifact.key_handle.clone(),
            compressed: Some(artifact.compressed),
            encrypted: Some(artifact.encrypted),
            backend_type: self.config.backend_type.clone(),
            synced_at: Some(Utc::now().to_rfc3339()),
            status: SyncStatus::Completed,
            error_message: None,
        };
        if let Err(e) = write_sync_file(&record, db) {
            if let Err(remove_err) = backend.delete_file(&remote_name) {
                warn!(
                    "orphaned remote copy '{}' could not be removed: {}",
                    remote_name, remove_err
                );
            }
            return Err(e);
        }

        // 11. Cleanup — delete the raw original only behind a verified
        //     remote copy; otherwise drop the temporary artifact (the
        //     remote copy is authoritative, restore brings it back).
        if keep_local_artifact {
            self.state.set_status(SyncStatus::Cleaning);
            let _ = self.delete_raw_uncompressed(&original_path, &artifact.path)?;
        } else if !artifact.is_original {
            if let Err(e) = fs::remove_file(&artifact.path) {
                warn!(
                    "could not remove temporary artifact '{}': {}",
                    artifact.path, e
                );
            }
        }

        self.state.set_current(None);

        Ok((artifact.len, artifact.bytes_saved))
    }

    // -----------------------------------------------------------------------
    // Artifact: compress → encrypt
    // -----------------------------------------------------------------------

    /// Build the bytes that will be uploaded for `original_path`.
    ///
    /// When neither compression nor encryption is in play the original file
    /// itself is the artifact (no copy is made). Otherwise the payload is
    /// compressed first, then sealed with the machine master passphrase
    /// behind [`CYBE_MAGIC`], and written next to the original as `.cyb3`.
    fn build_artifact(
        &self,
        original_path: &str,
        compressor: &TripleCompressor,
    ) -> Result<Artifact, String> {
        let want_compress = self.config.compress_before_upload;
        let want_encrypt = self.config.encrypt_before_upload;
        if !want_compress && !want_encrypt {
            let len = fs::metadata(original_path)
                .map_err(|e| format!("Failed to get file metadata: {}", e))?
                .len();
            return Ok(Artifact {
                path: original_path.to_string(),
                original_size: len,
                len,
                bytes_saved: 0,
                hash: transfer::hash_file(original_path)?,
                compressed: false,
                encrypted: false,
                key_handle: None,
                is_original: true,
            });
        }

        self.state.set_status(SyncStatus::Compressing);
        let raw = fs::read(original_path)
            .map_err(|e| format!("Failed to read file for compression: {}", e))?;
        let original_size = raw.len() as u64;

        let (bytes, compressed, encrypted, key_handle, bytes_saved) =
            self.transform_payload(raw, compressor, original_path)?;

        let hash = transfer::blake3_hex(&bytes);
        let path = format!("{}.cyb3", original_path);
        fs::write(&path, &bytes).map_err(|e| format!("Failed to write sync artifact: {}", e))?;
        info!(
            "Built sync artifact for {} ({} → {} bytes, compressed={}, encrypted={})",
            original_path,
            original_size,
            bytes.len(),
            compressed,
            encrypted
        );

        Ok(Artifact {
            path,
            original_size,
            len: bytes.len() as u64,
            bytes_saved,
            hash,
            compressed,
            encrypted,
            key_handle,
            is_original: false,
        })
    }

    /// compress → encrypt one payload. Shared by whole-file artifacts and
    /// striped chunks so both travel the exact same transform order
    /// (compress first — ciphertext does not compress). `label` is only used
    /// in the no-passphrase WARNING.
    ///
    /// Returns `(payload, compressed, encrypted, key_handle, bytes_saved)`.
    fn transform_payload(
        &self,
        raw: Vec<u8>,
        compressor: &TripleCompressor,
        label: &str,
    ) -> Result<TransformedPayload, String> {
        let want_compress = self.config.compress_before_upload;
        let want_encrypt = self.config.encrypt_before_upload;
        let original_size = raw.len() as u64;

        let (payload, bytes_saved, compressed) = if want_compress {
            let (compressed_bytes, _stats) = compressor
                .compress_triple(&raw)
                .map_err(|e| format!("integrity: triple compression failed: {}", e))?;
            let saved = original_size.saturating_sub(compressed_bytes.len() as u64);
            (compressed_bytes, saved, true)
        } else {
            (raw, 0, false)
        };

        let (bytes, encrypted, key_handle) = if want_encrypt {
            match keystore::master_passphrase() {
                Some(passphrase) => {
                    let sealed = keystore::seal(&passphrase, &payload)
                        .map_err(|e| format!("encryption failed: {}", e))?;
                    let mut out = Vec::with_capacity(CYBE_MAGIC.len() + sealed.len());
                    out.extend_from_slice(CYBE_MAGIC);
                    out.extend(sealed);
                    (out, true, Some(SYNC_KEY_HANDLE.to_string()))
                }
                None if self.config.require_encryption => {
                    // Encrypt-or-fail: refuse the upload instead of sending
                    // plaintext with a warning. The error is per-file, so one
                    // locked file never blocks the rest of the run.
                    return Err(format!(
                        "auth: no master passphrase available — '{}' refused \
                         (requireEncryption is on; set a master passphrase or turn it off)",
                        label
                    ));
                }
                None => {
                    // Never block a sync on key material, never pretend it
                    // happened: upload and say so.
                    self.state.add_error(format!(
                        "WARNING: no master passphrase available — '{}' uploaded \
                         WITHOUT encryption",
                        label
                    ));
                    (payload, false, None)
                }
            }
        } else {
            (payload, false, None)
        };

        Ok((bytes, compressed, encrypted, key_handle, bytes_saved))
    }

    // -----------------------------------------------------------------------
    // Striped placement (item 10)
    // -----------------------------------------------------------------------

    /// Split `original_path` into 4 MiB chunks, place them round-robin
    /// across every enabled config (with replicas per `parity`), persist the
    /// manifest in `sync_files.manifest_ref`, and — only when every chunk
    /// came back verified — honour `delete_raw_after_sync`.
    ///
    /// Returns:
    ///  - `Ok(Some((uploaded, saved)))` — handled; the record is written;
    ///  - `Ok(None)` — degraded to the caller's whole-file path (fewer than
    ///    two enabled configs, or an empty file);
    ///  - `Err` — hard failure. The caller must **not** fall back to
    ///    whole-file (chunks may already be uploaded; retrying would
    ///    double-place content).
    fn sync_single_file_striped(
        &self,
        file_id: &str,
        original_path: &str,
        local_hash: &str,
        compressor: &TripleCompressor,
        db: &RwLock<Database>,
    ) -> Result<Option<(u64, u64)>, String> {
        let total_size = fs::metadata(original_path)
            .map_err(|e| format!("Failed to get file metadata: {}", e))?
            .len();
        if total_size == 0 {
            return Ok(None);
        }

        // Idempotent re-sync: this exact content is already placed (and its
        // manifest recorded) — nothing to upload. Chunks are
        // content-addressed, so only a hash change can matter here; this is
        // also what keeps `auto_sync` ticks from re-sending the library.
        if let Some(record) = self.load_sync_file(file_id, db) {
            if record.manifest_ref.is_some() && record.hash_blake3.as_deref() == Some(local_hash) {
                self.state.set_current(None);
                return Ok(Some((0, 0)));
            }
        }

        let already = self
            .load_sync_file(file_id, db)
            .map(|rec| rec.size_bytes)
            .unwrap_or(0);

        // Participants: this config first (it owns the `sync_files` record),
        // then every other enabled config in stable id order.
        let participants: Vec<SyncConfig> = {
            let guard = db.read().map_err(|e| e.to_string())?;
            let mut others = guard.list_sync_configs().map_err(|e| e.to_string())?;
            others.retain(|c| c.enabled && c.id != self.config.id);
            others.sort_by_key(|a| a.id.clone());
            let mut list = vec![self.config.clone()];
            list.extend(others);
            list
        };
        if participants.len() < 2 {
            self.state.add_error(format!(
                "WARNING: placement striped needs 2+ enabled configs — '{}' \
                 fell back to whole-file mode",
                original_path
            ));
            return Ok(None);
        }

        self.state.set_current(Some(original_path.to_string()));
        let n = participants.len();
        let parity = self.config.parity;

        // Per-call nonce so two concurrent runs syncing the same file (one
        // per config) never share an upload sidecar.
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let mut backends: HashMap<String, Box<dyn StorageBackend>> = HashMap::new();
        for config in &participants {
            backends.insert(config.id.clone(), create_backend(config)?);
        }

        let mut file =
            fs::File::open(original_path).map_err(|e| format!("Failed to open file: {}", e))?;
        let mut entries: Vec<manifest::ChunkEntry> = Vec::new();
        let mut index: u32 = 0;
        let mut offset: u64 = 0;
        let mut total_uploaded: u64 = 0;
        let mut total_saved: u64 = 0;
        let mut any_compressed = false;
        let mut any_encrypted = false;

        loop {
            if self.is_cancelled() {
                // Partial chunks are content-addressed and no record was
                // written — a later run overwrites them in place. Nothing
                // claims them meanwhile.
                return Err("sync cancelled".to_string());
            }

            let want = (total_size - offset).min(manifest::CHUNK_SIZE) as usize;
            if want == 0 {
                break;
            }
            let mut buf = vec![0u8; want];
            file.read_exact(&mut buf)
                .map_err(|e| format!("Failed to read chunk: {}", e))?;

            let chunk_hash = transfer::blake3_hex(&buf);
            self.state.set_status(SyncStatus::Compressing);
            let (payload, compressed, encrypted, _key_handle, saved) =
                self.transform_payload(buf, compressor, original_path)?;
            any_compressed |= compressed;
            any_encrypted |= encrypted;
            total_saved += saved;
            let artifact_hash = transfer::blake3_hex(&payload);
            let remote = manifest::chunk_remote_path(&chunk_hash);

            // Unique across every concurrent run: pid + per-call nonce +
            // chunk index.
            let tmp = std::env::temp_dir().join(format!(
                "cybermanju-up-{}-{}-{}.cyb3",
                std::process::id(),
                nonce,
                index
            ));
            let tmp_str = tmp.to_string_lossy().to_string();
            // Guard first: every `?` below (quota, backend lookup,
            // rate-limit, exhausted retries) drops through here.
            let _cleanup = RemoveOnDrop { path: &tmp };
            fs::write(&tmp, &payload)
                .map_err(|e| format!("integrity: cannot write chunk artifact: {}", e))?;

            let (primary_idx, replica_idxs) = manifest::placements(index as usize, n, parity);
            let mut locs: Vec<manifest::ChunkLoc> = Vec::with_capacity(1 + replica_idxs.len());
            for &cfg_idx in std::iter::once(&primary_idx).chain(replica_idxs.iter()) {
                let config_id = participants[cfg_idx].id.clone();
                let backend = backends
                    .get(&config_id)
                    .ok_or_else(|| format!("not_found: no backend for config '{}'", config_id))?;
                self.state.set_status(SyncStatus::Uploading);
                let _permit = rate_limit::acquire(&backend.backend_type())?;
                retry::with_retry(&retry::RetryPolicy::default(), || {
                    backend.upload_file(&tmp_str, &remote)
                })?;
                // Exactly one accounting point per copy actually sent.
                total_uploaded += payload.len() as u64;
                locs.push(manifest::ChunkLoc {
                    config_id,
                    remote_path: remote.clone(),
                    artifact_hash: artifact_hash.clone(),
                });
            }
            let _ = fs::remove_file(&tmp);

            entries.push(manifest::ChunkEntry {
                index,
                hash: chunk_hash,
                size: want as u64,
                primary: locs[0].clone(),
                replicas: locs[1..].to_vec(),
            });
            index += 1;
            offset += want as u64;
        }

        // Verified delete gate (item 1): same contract as whole-file —
        // download-back every chunk (first copy that answers) before the
        // original is touched; any doubt keeps it and records a WARNING.
        let mut last_verified_at: Option<String> = None;
        if self.config.delete_raw_after_sync {
            match self.verify_striped(&entries, &backends) {
                Ok(()) => last_verified_at = Some(Utc::now().to_rfc3339()),
                Err(e) => self.state.add_error(format!(
                    "WARNING: local original '{}' kept — striped copy not verified: {}",
                    original_path, e
                )),
            }
        }
        let keep_local = self.config.delete_raw_after_sync && last_verified_at.is_some();

        let manifest_obj = manifest::ChunkManifest {
            version: manifest::MANIFEST_VERSION,
            file_hash: local_hash.to_string(),
            chunk_size: manifest::CHUNK_SIZE,
            total_size,
            chunks: entries,
            parity,
        };
        let manifest_json = serde_json::to_string(&manifest_obj).map_err(|e| e.to_string())?;

        self.state.set_status(SyncStatus::Linking);
        if let Err(e) = self.create_link(file_id, &format!("striped:{}", local_hash), db) {
            remove_striped_copies(&backends, &manifest_obj);
            return Err(e);
        }

        let record = SyncFile {
            id: file_id.to_string(),
            config_id: Some(self.config.id.clone()),
            home_config_id: Some(self.config.id.clone()),
            original_path: original_path.to_string(),
            // No sidecar artifact exists in striped mode: chunk temps are
            // removed after upload, and restore reassembles from providers.
            compressed_path: None,
            preview_path: None,
            remote_url: None,
            remote_path: None,
            size_bytes: total_size,
            compressed_size_bytes: Some(total_uploaded),
            hash_blake3: Some(local_hash.to_string()),
            // Per-chunk artifact hashes live in the manifest.
            artifact_hash: None,
            manifest_ref: Some(manifest_json),
            last_verified_at,
            key_handle: any_encrypted.then(|| SYNC_KEY_HANDLE.to_string()),
            compressed: Some(any_compressed),
            encrypted: Some(any_encrypted),
            backend_type: self.config.backend_type.clone(),
            synced_at: Some(Utc::now().to_rfc3339()),
            status: SyncStatus::Completed,
            error_message: None,
        };
        if let Err(e) = write_sync_file(&record, db) {
            remove_striped_copies(&backends, &manifest_obj);
            return Err(e);
        }

        // <<< AGENT-6 ADMISSION >>>
        // Same ledger as the whole-file path: only growth is charged, and the
        // placement is already durable here.
        let needed = total_size.saturating_sub(already);
        if needed > 0 {
            let guard = db.read().map_err(|e| e.to_string())?;
            if let Err(e) = quota::charge_disk(&guard, &self.config.id, needed) {
                warn!("volume accounting for '{}' failed: {}", original_path, e);
            }
        }
        // <<< /AGENT-6 ADMISSION >>>

        if keep_local {
            self.state.set_status(SyncStatus::Cleaning);
            fs::remove_file(original_path)
                .map_err(|e| format!("Failed to delete original file: {}", e))?;
            info!(
                "Deleted original file {} behind {} verified striped chunks",
                original_path, index
            );
        }

        self.state.set_current(None);
        Ok(Some((total_uploaded, total_saved)))
    }

    /// Download-back verification for striped placement: every chunk must
    /// return its plaintext hash from at least one copy (primary, then
    /// replicas). Any doubt → `Err`, and the caller keeps the original.
    fn verify_striped(
        &self,
        entries: &[manifest::ChunkEntry],
        backends: &HashMap<String, Box<dyn StorageBackend>>,
    ) -> Result<(), String> {
        // Per-call nonce: two files verifying chunk 0 at the same time must
        // not share a sidecar (content hashes can legitimately repeat
        // across files).
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for entry in entries {
            let mut ok = false;
            for loc in std::iter::once(&entry.primary).chain(entry.replicas.iter()) {
                let backend = match backends.get(&loc.config_id) {
                    Some(backend) => backend,
                    None => continue,
                };
                let tmp = std::env::temp_dir().join(format!(
                    "cybermanju-verify-{}-{}-{}.part",
                    std::process::id(),
                    nonce,
                    entry.index
                ));
                let tmp_str = tmp.to_string_lossy().to_string();
                let outcome = retry::with_retry(&retry::RetryPolicy::default(), || {
                    backend.download_file(&loc.remote_path, &tmp_str)
                })
                .and_then(|()| fs::read(&tmp).map_err(|e| format!("read failed: {}", e)))
                .and_then(manifest::decode_artifact)
                .and_then(|plain| {
                    if transfer::blake3_hex(&plain) == entry.hash {
                        Ok(())
                    } else {
                        Err("plaintext hash mismatch".to_string())
                    }
                });
                let _ = fs::remove_file(&tmp);
                if outcome.is_ok() {
                    ok = true;
                    break;
                }
            }
            if !ok {
                return Err(format!(
                    "chunk {} failed download-back on every copy",
                    entry.index
                ));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Verification (item 1)
    // -----------------------------------------------------------------------

    /// Download the remote copy back and compare its BLAKE3 against the
    /// bytes we uploaded. Any doubt (network, unsupported, mismatch) is an
    /// `Err` — the caller keeps the local original and records a WARNING.
    fn verify_remote_copy(
        &self,
        backend: &dyn StorageBackend,
        remote_path: &str,
        expected_hash: &str,
    ) -> Result<(), String> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!(
            "cybermanju-verify-{}-{}.part",
            std::process::id(),
            unique
        ));
        let tmp_str = tmp.to_string_lossy().to_string();
        let outcome = retry::with_retry(&retry::RetryPolicy::default(), || {
            backend.download_file(remote_path, &tmp_str)
        })
        .and_then(|()| transfer::verify_file_blake3(&tmp_str, expected_hash));
        let _ = fs::remove_file(&tmp);
        outcome
    }

    // -----------------------------------------------------------------------
    // sync_files persistence (item 2)
    // -----------------------------------------------------------------------

    fn load_sync_file(&self, file_id: &str, db: &RwLock<Database>) -> Option<SyncFile> {
        let db = db.read().ok()?;
        db.get_sync_file(file_id, &self.config.id).ok().flatten()
    }

    // -----------------------------------------------------------------------
    // Compression
    // -----------------------------------------------------------------------

    /// Compress a file using the triple compressor.
    /// Returns (compressed_path, original_size, compressed_size).
    pub fn compress_file(
        &self,
        file_path: &str,
        compressor: &TripleCompressor,
    ) -> Result<(String, u64, u64), String> {
        let data = fs::read(file_path)
            .map_err(|e| format!("Failed to read file for compression: {}", e))?;
        let original_size = data.len() as u64;

        let (compressed, _stats) = compressor
            .compress_triple(&data)
            .map_err(|e| format!("integrity: triple compression failed: {}", e))?;
        let compressed_size = compressed.len() as u64;

        // Write compressed file next to the original with .cyb3 extension
        let compressed_path = format!("{}.cyb3", file_path);
        fs::write(&compressed_path, &compressed)
            .map_err(|e| format!("Failed to write compressed file: {}", e))?;

        info!(
            "Compressed {} → {} ({} → {} bytes)",
            file_path, compressed_path, original_size, compressed_size,
        );

        Ok((compressed_path, original_size, compressed_size))
    }

    // -----------------------------------------------------------------------
    // Preview generation
    // -----------------------------------------------------------------------

    /// Generate a thumbnail preview for an image file.
    /// Returns the path to the generated preview.
    pub fn create_preview(&self, file_path: &str) -> Result<String, String> {
        let data =
            fs::read(file_path).map_err(|e| format!("Failed to read file for preview: {}", e))?;

        let img = image::load_from_memory(&data)
            .map_err(|e| format!("Failed to decode image for preview: {}", e))?;

        let (w, h) = (img.width(), img.height());
        let max_size: u32 = 512;
        let scale = if w > h {
            max_size as f64 / w as f64
        } else {
            max_size as f64 / h as f64
        };
        let new_w = (w as f64 * scale) as u32;
        let new_h = (h as f64 * scale) as u32;

        let thumbnail = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);

        let preview_path = format!("{}.preview.png", file_path);
        let mut out_file = fs::File::create(&preview_path)
            .map_err(|e| format!("Failed to create preview file: {}", e))?;
        thumbnail
            .write_to(&mut out_file, image::ImageFormat::Png)
            .map_err(|e| format!("Failed to write preview: {}", e))?;

        info!("Created preview: {}", preview_path);
        Ok(preview_path)
    }

    // -----------------------------------------------------------------------
    // Link creation
    // -----------------------------------------------------------------------

    /// Update a FileNode's context_data with the remote URL.
    pub fn create_link(
        &self,
        file_id: &str,
        remote_url: &str,
        db: &RwLock<Database>,
    ) -> Result<(), String> {
        let db = db.write().map_err(|e| e.to_string())?;

        // Read current file node
        let tx_read = db.begin_read().map_err(|e| e.to_string())?;
        let table_read = tx_read
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        let value = table_read
            .get(file_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("File not found: {}", file_id))?;
        let mut file_node: FileNode =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        drop(tx_read);

        // Update context_data with sync link
        let mut context = file_node
            .context_data
            .clone()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        if let Some(obj) = context.as_object_mut() {
            obj.insert("sync_url".to_string(), serde_json::json!(remote_url));
            obj.insert(
                "sync_backend".to_string(),
                serde_json::json!(self.config.backend_type.clone()),
            );
            obj.insert(
                "synced_at".to_string(),
                serde_json::json!(Utc::now().to_rfc3339()),
            );
        }
        file_node.context_data = Some(context);
        file_node.modified_at = Utc::now().to_rfc3339();

        // Write back
        let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
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

        info!("Created sync link for file {}: {}", file_id, remote_url);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Cleanup
    // -----------------------------------------------------------------------

    /// Delete the original file if a compressed version exists and config allows it.
    /// Returns true if the original was deleted.
    pub fn delete_raw_uncompressed(
        &self,
        file_path: &str,
        compressed_path: &str,
    ) -> Result<bool, String> {
        let original = Path::new(file_path);
        let compressed = Path::new(compressed_path);

        // Only delete if the compressed version exists
        if !compressed.exists() {
            return Ok(false);
        }

        // Don't delete if the "compressed" file is the same as the original
        // (i.e. compression was not enabled)
        if original == compressed {
            return Ok(false);
        }

        if original.exists() {
            fs::remove_file(original)
                .map_err(|e| format!("Failed to delete original file: {}", e))?;
            info!("Deleted original file: {}", file_path);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn reset_progress(&self, total: u32) {
        self.state.reset(total);
    }

    fn get_file_node(&self, file_id: &str, db: &RwLock<Database>) -> Result<FileNode, String> {
        let db = db.read().map_err(|e| e.to_string())?;
        let tx = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        let value = table
            .get(file_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("File not found in DB: {}", file_id))?;
        let node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        Ok(node)
    }
}

// ===========================================================================
// Artifact + locator helpers
// ===========================================================================

/// The bytes a single file's upload will carry, and what it cost.
struct Artifact {
    path: String,
    /// Size of the original plaintext file.
    original_size: u64,
    /// Size of the uploaded bytes (what `SyncResult.bytes_uploaded` counts).
    len: u64,
    bytes_saved: u64,
    /// BLAKE3 of the uploaded bytes — the download-back verify baseline.
    hash: String,
    compressed: bool,
    encrypted: bool,
    key_handle: Option<String>,
    /// The original file itself is the artifact (nothing was built).
    is_original: bool,
}

/// Persist one `sync_files` locator record (item 2).
fn write_sync_file(record: &SyncFile, db: &RwLock<Database>) -> Result<(), String> {
    let db = db.write().map_err(|e| e.to_string())?;
    db.upsert_sync_file(record).map_err(|e| e.to_string())
}

/// Collision-safe remote locator for a local path (item 4).
///
/// `cybermanju_sync/{parent_hash8}/{name}` — two files with the same
/// basename in different directories never collide, and the mapping is a
/// pure function of the path, so a re-sync targets the same remote object
/// (idempotency preserved).
///
/// With `obfuscate` the basename is replaced by the first 16 hex chars of
/// its BLAKE3 (`cybermanju_sync/{parent_hash8}/{name_hash16}`): providers
/// never see real filenames, and the mapping stays deterministic so
/// idempotent re-syncs still converge. The original name is kept in the
/// local `sync_files` record, which is what restore reads back.
pub fn remote_path_for(original_path: &str, obfuscate: bool) -> String {
    let path = Path::new(original_path);
    let parent = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent_key = if parent.is_empty() {
        ".".to_string()
    } else {
        parent
    };
    let dir_hash = &transfer::blake3_hex(parent_key.as_bytes())[..8];
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "unnamed".to_string());
    if obfuscate {
        let name_hash = &transfer::blake3_hex(name.as_bytes())[..16];
        format!("cybermanju_sync/{}/{}", dir_hash, name_hash)
    } else {
        format!("cybermanju_sync/{}/{}", dir_hash, name)
    }
}

/// Derive the `keepBoth` variant of a locator: deterministic in the local
/// content hash, so repeated keep-both runs converge on one remote object
/// instead of littering a new name per attempt.
fn keep_both_path(remote_path: &str, local_hash: &str) -> String {
    let short = &local_hash[..8.min(local_hash.len())];
    let (dir, name) = match remote_path.rsplit_once('/') {
        Some((d, n)) => (Some(d), n),
        None => (None, remote_path),
    };
    let candidate = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{}.keep-{}.{}", stem, short, ext),
        _ => format!("{}.keep-{}", name, short),
    };
    match dir {
        Some(d) => format!("{}/{}", d, candidate),
        None => candidate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_never_collide_across_directories() {
        let a = remote_path_for("/home/alice/report.txt", false);
        let b = remote_path_for("/home/bob/report.txt", false);
        assert_ne!(a, b, "same basename, different parents must differ");
        assert!(a.starts_with("cybermanju_sync/"));
        assert!(a.ends_with("/report.txt"));
    }

    #[test]
    fn remote_paths_are_stable_for_idempotent_resync() {
        let first = remote_path_for("/data/photo.png", false);
        let second = remote_path_for("/data/photo.png", false);
        assert_eq!(first, second);
    }

    #[test]
    fn bare_relative_paths_still_produce_a_locator() {
        let p = remote_path_for("photo.png", false);
        assert!(p.starts_with("cybermanju_sync/"));
        assert!(p.ends_with("/photo.png"));
    }

    #[test]
    fn obfuscated_paths_hide_the_basename_but_stay_stable() {
        let plain = remote_path_for("/data/photo.png", false);
        let hidden = remote_path_for("/data/photo.png", true);
        assert!(plain.ends_with("/photo.png"));
        assert!(
            !hidden.contains("photo"),
            "obfuscated locator must not leak the name"
        );
        assert!(
            !hidden.contains("png"),
            "obfuscated locator must not leak the extension"
        );
        assert_eq!(hidden, remote_path_for("/data/photo.png", true));
        // Same basename elsewhere still maps elsewhere (dir hash preserved).
        let other = remote_path_for("/elsewhere/photo.png", true);
        assert_ne!(hidden, other);
    }

    #[test]
    fn keep_both_is_deterministic_and_keeps_the_extension() {
        let hash = "0123456789abcdef";
        let k1 = keep_both_path("cybermanju_sync/abcd/report.pdf", hash);
        let k2 = keep_both_path("cybermanju_sync/abcd/report.pdf", hash);
        assert_eq!(k1, k2);
        assert_eq!(k1, "cybermanju_sync/abcd/report.keep-01234567.pdf");
        let no_ext = keep_both_path("cybermanju_sync/abcd/README", hash);
        assert_eq!(no_ext, "cybermanju_sync/abcd/README.keep-01234567");
    }
}
