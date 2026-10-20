// CyberManju OS — unified-disk relocation (single-copy move A→B).
//
// Shared by `POST /api/sync/move` (via `cybermanju-web`) and `cybsh sync move`
// (via `cybermanju-os`): bytes travel as-is — the sealed `.cyb3` artifact is
// backend-agnostic, so no recompress and no re-encrypt — verified by BLAKE3
// on the way back. Order: download A → upload B → verify B → delete A →
// retarget the `sync_files` record (`config_id` + `home_config_id`).

use crate::backends::{create_backend, is_protected_remote_path};
use crate::transfer;
use cybermanju_db::Database;
use cybermanju_types::sync::{SyncFile, SyncStatus};
use serde::Serialize;

/// Verified relocation result (mirrored by the web `MoveOutcome`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateOutcome {
    pub file_id: String,
    pub from_config_id: String,
    pub to_config_id: String,
    pub remote_path: String,
    pub bytes: u64,
    pub noop: bool,
}

fn validate_id(v: &str) -> Result<(), String> {
    if v.trim().is_empty() {
        return Err("invalid: id is required".to_string());
    }
    if v.len() > 128 || v.contains(['/', '\\', '\0']) {
        return Err(format!("invalid: id '{}' is malformed", v));
    }
    Ok(())
}

/// Relocate one file's single copy. Striped manifests move via re-place,
/// vault/secret paths never move, and an already-home file is a noop.
pub fn relocate(
    db: &Database,
    file_id: &str,
    from_config_id: &str,
    to_config_id: &str,
) -> Result<RelocateOutcome, String> {
    validate_id(file_id)?;
    validate_id(from_config_id)?;
    validate_id(to_config_id)?;

    if from_config_id == to_config_id {
        let remote = db
            .get_sync_file(file_id, from_config_id)
            .map_err(|e| e.to_string())?
            .and_then(|r| r.remote_path.clone())
            .unwrap_or_default();
        return Ok(RelocateOutcome {
            file_id: file_id.to_string(),
            from_config_id: from_config_id.to_string(),
            to_config_id: to_config_id.to_string(),
            remote_path: remote,
            bytes: 0,
            noop: true,
        });
    }

    let from_config = db
        .list_sync_configs()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == from_config_id)
        .ok_or_else(|| format!("not_found: sync config '{}' not found", from_config_id))?;
    let to_config = db
        .list_sync_configs()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|c| c.id == to_config_id)
        .ok_or_else(|| format!("not_found: sync config '{}' not found", to_config_id))?;
    if !to_config.enabled {
        return Err(format!(
            "unsupported: destination provider '{}' is disabled",
            to_config.id
        ));
    }

    let mut record: SyncFile = db
        .get_sync_file(file_id, from_config_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "not_found: no sync record for file '{}' on provider '{}'",
                file_id, from_config_id
            )
        })?;
    if record.manifest_ref.is_some() {
        return Err(
            "unsupported: striped files move via re-place, not single-copy move".to_string(),
        );
    }
    let remote_path = record.remote_path.clone().ok_or_else(|| {
        format!(
            "not_found: sync record for file '{}' has no remote locator",
            file_id
        )
    })?;
    if is_protected_remote_path(&remote_path) {
        return Err(format!(
            "integrity: '{}' is a vault/secret path and is never moved",
            remote_path
        ));
    }

    let from_backend = create_backend(&from_config)?;
    let to_backend = create_backend(&to_config)?;

    let mut candidates = vec![remote_path.clone()];
    if let Some(url) = record.remote_url.clone() {
        if !candidates.contains(&url) {
            candidates.push(url);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let staging = std::env::temp_dir().join(format!(
        "cybermanju-move-{}-{}.cyb3",
        std::process::id(),
        nonce
    ));
    let staging_str = staging.to_string_lossy().to_string();
    let mut last_err: Option<String> = None;
    let mut downloaded = false;
    for candidate in &candidates {
        match from_backend.download_file(candidate, &staging_str) {
            Ok(()) => {
                downloaded = true;
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }
    if !downloaded {
        let _ = std::fs::remove_file(&staging);
        return Err(last_err
            .unwrap_or_else(|| "not_found: source copy could not be downloaded".to_string()));
    }
    let bytes_len = std::fs::metadata(&staging).map(|m| m.len()).unwrap_or(0);

    if let Ok(Some(_)) = to_backend.stat(&remote_path) {
        let _ = std::fs::remove_file(&staging);
        return Err(format!(
            "conflict: destination provider already holds '{}'",
            remote_path
        ));
    }
    let remote_url = to_backend
        .upload_file(&staging_str, &remote_path)
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&staging);
        })?;

    let verify_path = format!("{}.verify", staging_str);
    if let Err(e) = to_backend.download_file(&remote_path, &verify_path) {
        let _ = std::fs::remove_file(&staging);
        let _ = to_backend.delete_file(&remote_path);
        return Err(format!("integrity: moved copy did not verify: {}", e));
    }
    let staged_hash = transfer::blake3_hex(
        &std::fs::read(&staging).map_err(|e| format!("integrity: staged read failed: {}", e))?,
    );
    let verify_hash = transfer::blake3_hex(
        &std::fs::read(&verify_path)
            .map_err(|e| format!("integrity: verify read failed: {}", e))?,
    );
    let _ = std::fs::remove_file(&verify_path);
    let _ = std::fs::remove_file(&staging);
    if staged_hash != verify_hash {
        let _ = to_backend.delete_file(&remote_path);
        return Err("integrity: moved copy BLAKE3 mismatch — source left untouched".to_string());
    }

    if let Err(e) = from_backend.delete_file(&candidates[0]) {
        log::warn!("move: source delete failed (B verified): {}", e);
    }

    record.config_id = Some(to_config_id.to_string());
    record.home_config_id = Some(to_config_id.to_string());
    record.remote_url = Some(remote_url);
    record.backend_type = to_config.backend_type.clone();
    record.synced_at = Some(chrono::Utc::now().to_rfc3339());
    record.status = SyncStatus::Completed;
    {
        let _ = db.remove_sync_file(file_id, from_config_id);
        if let Err(e) = db.upsert_sync_file(&record).map_err(|e| e.to_string()) {
            let _ = to_backend.delete_file(&remote_path);
            return Err(e);
        }
    }

    Ok(RelocateOutcome {
        file_id: file_id.to_string(),
        from_config_id: from_config_id.to_string(),
        to_config_id: to_config_id.to_string(),
        remote_path,
        bytes: bytes_len,
        noop: false,
    })
}

#[cfg(test)]
mod tests {
    use super::is_protected_remote_path;

    #[test]
    fn vault_paths_never_relocate() {
        assert!(is_protected_remote_path("vault.cybermanju"));
        assert!(!is_protected_remote_path("notes.txt"));
    }
}
