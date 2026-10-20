// CyberManju OS — transfer integrity & size preflight (AGENT-1)
//
//   * BLAKE3 helpers: hash before upload, verify after download, compare
//     against `Content-Length` / a provider checksum. A mismatch comes back
//     as `integrity: ...` — AGENT-2 gates `delete_raw_after_sync` on it.
//   * `preflight`: reject `too_large:` *before* the request is built, and
//     expose the threshold that decides inline vs chunked transport.
//
// <<< AGENT-1 TRANSFER >>>

use cybermanju_types::sync::SyncBackendType;
use std::io::Read;

// ─── Provider limits (bytes) ─────────────────────────────────────────

/// GitHub Contents API: inline write/download max (larger → release asset).
pub const GITHUB_INLINE_MAX: u64 = 1024 * 1024;
/// GitHub release asset transport max we are willing to use.
pub const GITHUB_RELEASE_MAX: u64 = 100 * 1024 * 1024;
/// GitHub release asset chunked-upload size (also the single-POST threshold).
pub const GITHUB_RELEASE_CHUNK: u64 = 8 * 1024 * 1024;
/// Google Drive multipart (one-shot) max — above this, resumable sessions.
pub const DRIVE_MULTIPART_MAX: u64 = 5 * 1024 * 1024;
/// Google Drive resumable upload chunk size.
pub const DRIVE_RESUMABLE_CHUNK: u64 = 5 * 1024 * 1024;
/// Google Drive supports files up to 5 TiB (user quota aside).
pub const DRIVE_MAX: u64 = 5 * 1024 * 1024 * 1024 * 1024;

/// Hard per-provider ceiling used by `preflight`.
///
/// `u64::MAX` means "no fixed ceiling known to us" (local disk; GitLab's
/// `push_file_size` is a per-project setting we cannot see).
pub fn max_size_bytes(backend_type: &SyncBackendType) -> u64 {
    match backend_type {
        SyncBackendType::Local => u64::MAX,
        SyncBackendType::GitHub => GITHUB_RELEASE_MAX,
        SyncBackendType::GitLab => u64::MAX,
        SyncBackendType::GoogleDrive => DRIVE_MAX,
    }
}

/// Reject an oversized file with `too_large:` before any request is sent.
pub fn preflight(backend_type: &SyncBackendType, size: u64) -> Result<(), String> {
    let limit = max_size_bytes(backend_type);
    if size > limit {
        return Err(format!(
            "too_large: {} rejects files larger than {} bytes (this file is {} bytes)",
            backend_type, limit, size
        ));
    }
    Ok(())
}

// ─── BLAKE3 ──────────────────────────────────────────────────────────

/// BLAKE3 of `bytes` as lowercase hex.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Streaming BLAKE3 of a file on disk (never loads the whole file).
pub fn hash_file(path: &str) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("integrity: cannot open '{}' for hashing: {}", path, e))?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|e| format!("integrity: failed to hash '{}': {}", path, e))?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Compare received bytes against an expected BLAKE3 hex digest.
pub fn verify_blake3(bytes: &[u8], expected_hex: &str) -> Result<(), String> {
    let expected = expected_hex.trim().to_ascii_lowercase();
    let actual = blake3_hex(bytes);
    if actual != expected {
        return Err(format!(
            "integrity: BLAKE3 mismatch (expected {}, got {})",
            expected, actual
        ));
    }
    Ok(())
}

/// Compare a file on disk against an expected BLAKE3 hex digest.
pub fn verify_file_blake3(path: &str, expected_hex: &str) -> Result<(), String> {
    let actual = hash_file(path)?;
    let expected = expected_hex.trim().to_ascii_lowercase();
    if actual != expected {
        return Err(format!(
            "integrity: BLAKE3 mismatch for '{}' (expected {}, got {})",
            path, expected, actual
        ));
    }
    Ok(())
}

/// Compare a byte count against the provider's promised length
/// (`Content-Length`, an API `size` field, …). `None` = nothing to compare.
pub fn verify_size(actual: u64, expected: Option<u64>, what: &str) -> Result<(), String> {
    match expected {
        Some(expected) if expected != actual => Err(format!(
            "integrity: {} returned {} bytes but promised {}",
            what, actual, expected
        )),
        _ => Ok(()),
    }
}

// ─── Download sink ───────────────────────────────────────────────────

/// Write downloaded bytes to `local_path` after verifying them against the
/// length the provider promised. The parent directory is created first, so a
/// failed verify leaves no partial file behind.
pub fn write_verified(
    local_path: &str,
    bytes: &[u8],
    expected_len: Option<u64>,
    what: &str,
) -> Result<(), String> {
    verify_size(bytes.len() as u64, expected_len, what)?;
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "network: cannot create directory '{}': {}",
                parent.display(),
                e
            )
        })?;
    }
    std::fs::write(local_path, bytes)
        .map_err(|e| format!("network: failed to write '{}': {}", local_path, e))?;
    log::debug!(
        "{}: wrote {} bytes (blake3={})",
        local_path,
        bytes.len(),
        blake3_hex(bytes)
    );
    Ok(())
}

/// Read the local size of `path` with a `not_found:` error on failure —
/// the first thing every `upload_file` does, before any network traffic.
pub fn local_size(path: &str) -> Result<u64, String> {
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| format!("not_found: Failed to read file '{}': {}", path, e))
}

/// Read a local file for upload (`not_found:` when it is missing).
pub fn read_local(path: &str) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("not_found: Failed to read file '{}': {}", path, e))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| format!("not_found: Failed to read file '{}': {}", path, e))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_rejects_only_real_limit_violations() {
        assert!(preflight(&SyncBackendType::GitHub, GITHUB_INLINE_MAX).is_ok());

        // No fixed cap: local disk and GitLab (project-configurable).
        assert!(preflight(&SyncBackendType::Local, u64::MAX).is_ok());
        assert!(preflight(&SyncBackendType::GitLab, 40 * 1024 * 1024 * 1024).is_ok());
    }

    #[test]
    fn size_verification_catches_truncation() {
        assert!(verify_size(10, Some(10), "mock").is_ok());
        assert!(verify_size(10, None, "mock").is_ok());
        let err = verify_size(0, Some(10), "mock").expect_err("0 != 10");
        assert!(err.starts_with("integrity: "), "{err}");
    }

    #[test]
    fn blake3_round_trip() {
        let digest = blake3_hex(b"hello");
        assert_eq!(digest.len(), 64);
        assert!(verify_blake3(b"hello", &digest).is_ok());
        let err = verify_blake3(b"hell0", &digest).expect_err("mismatch");
        assert!(err.starts_with("integrity: "), "{err}");
    }

    #[test]
    fn hash_file_matches_hash_of_the_bytes() {
        let dir = std::env::temp_dir().join(format!("cyb-transfer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("payload.bin");
        std::fs::write(&path, b"payload bytes").expect("write");
        let on_disk = hash_file(path.to_str().unwrap()).expect("hash");
        assert_eq!(on_disk, blake3_hex(b"payload bytes"));
        assert!(verify_file_blake3(path.to_str().unwrap(), &on_disk).is_ok());
        let err = verify_file_blake3(path.to_str().unwrap(), &blake3_hex(b"other"))
            .expect_err("mismatch");
        assert!(err.starts_with("integrity: "), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn local_size_reports_missing_files_as_not_found() {
        let err = local_size("/nonexistent/definitely-missing.txt").expect_err("must fail");
        assert!(err.starts_with("not_found: "), "{err}");
        assert!(err.contains("Failed to read file"), "{err}");
    }

    #[test]
    fn write_verified_rejects_short_writes() {
        let dir = std::env::temp_dir().join(format!("cyb-transfer-w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let target = dir.join("out.bin");
        let err = write_verified(target.to_str().unwrap(), b"abc", Some(10), "mock")
            .expect_err("short write");
        assert!(err.starts_with("integrity: "), "{err}");
        assert!(!target.exists(), "no partial file");
        write_verified(target.to_str().unwrap(), b"abc", None, "mock").expect("write");
        assert!(target.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
