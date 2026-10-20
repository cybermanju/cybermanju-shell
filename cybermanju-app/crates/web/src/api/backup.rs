// CyberManju OS — Backup export (crates/web/src/api/backup.rs).
// Role: Vault snapshot export for the dashboard download path.
// Exports: export_db, ExportReport.
// Integrates: cybermanju_db, cybermanju_os::backup (file naming).
// Contracts: Admin-only (the image holds sealed secrets + transcripts);
//   the wire carries the RAW image base64'd — the browser seals it into a
//   CYBMJ01 container with a passphrase that never leaves the device.
//   Images over the HTTP cap refuse with `too_large:` (use
//   `scripts/backup.sh` on the host or `backup export` in cybsh instead).

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use cybermanju_db::Database;

/// HTTP-safe ceiling: the hand-rolled server flushes one complete
/// `Content-Length` response (5 s write timeout), so giant vaults must use
/// the host script or the shell verb instead of this endpoint.
const EXPORT_CAP: usize = 32 * 1024 * 1024;

/// Raw snapshot image, base64'd for the JSON transport.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub name: String,
    pub image_bytes: u64,
    pub blake3: String,
    pub image_b64: String,
}

/// Point-in-time database image for `GET /api/backup/export`.
pub fn export_db(db: &Database) -> Result<ExportReport, String> {
    let image = db.snapshot_bytes_with_limit(EXPORT_CAP).map_err(|e| {
        if e.to_string().starts_with("too_large:") {
            "too_large: vault image exceeds the 32 MiB HTTP export cap — run scripts/backup.sh on the host or `backup export` in cybsh".to_string()
        } else {
            format!("integrity: database snapshot failed: {e:#}")
        }
    })?;
    Ok(ExportReport {
        name: format!(
            "cybermanju-{}.cybermanju",
            cybermanju_os::backup::utc_stamp()
        ),
        image_bytes: image.len() as u64,
        blake3: blake3::hash(&image).to_hex().to_string(),
        image_b64: BASE64.encode(&image),
    })
}
