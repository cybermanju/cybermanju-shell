// CyberManju OS — Share links (shared by Tauri IPC and REST)
//
// <<< AGENT-3 SHARE: request-derived URLs, revocation and a content stream
// that actually serves the bytes behind a link. >>>

use cybermanju_db::Database;
use cybermanju_types::schema::{FileNode, ShareLink};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

/// Share link payload returned to the frontend.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareLinkResult {
    pub id: String,
    pub file_id: String,
    pub token: String,
    pub expires_at: String,
    pub url: String,
}

/// Bytes served by [`content`].
#[derive(Debug)]
pub struct ShareContent {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// Create a share link for a file.
pub fn generate(
    db: &Database,
    file_id: &str,
    expires_in_hours: Option<u64>,
) -> Result<ShareLinkResult, String> {
    if db
        .get_file_node(file_id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Err(format!("File not found: {}", file_id));
    }
    let link = db
        .create_share_link(file_id, expires_in_hours.unwrap_or(0))
        .map_err(|e| e.to_string())?;
    let token = link.token.clone();
    Ok(ShareLinkResult {
        id: link.id,
        file_id: link.file_id,
        token,
        expires_at: link.expires_at,
        url: format!("/api/shared/{}", link.token),
    })
}

/// Remove a share link by id. Returns `true` when something was deleted.
pub fn revoke(db: &Database, share_id: &str) -> Result<bool, String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_share_links_table())
            .map_err(|e| e.to_string())?;
        let removed = table.remove(share_id).map_err(|e| e.to_string())?.is_some();
        if !removed {
            return Err(format!("Share link not found: {}", share_id));
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(true)
}

/// Resolve a share token to its file, enforcing expiry.
pub fn resolve(db: &Database, token: &str) -> Result<Option<FileNode>, String> {
    let link = find_by_token(db, token)?;
    match link {
        Some(link) => {
            reject_if_expired(&link)?;
            db.get_file_node(&link.file_id).map_err(|e| e.to_string())
        }
        None => Ok(None),
    }
}

/// Stream the bytes behind a share token.
///
/// The on-disk location lives in `FileNode.context_data.original_path`
/// (written by the importer), so no new column is needed in `crates/db`.
pub fn content(db: &Database, token: &str) -> Result<Option<ShareContent>, String> {
    let link = match find_by_token(db, token)? {
        Some(link) => link,
        None => return Ok(None),
    };
    reject_if_expired(&link)?;

    let node = db
        .get_file_node(&link.file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Share link target no longer exists".to_string())?;

    let path = node
        .context_data
        .as_ref()
        .and_then(|ctx| ctx.get("original_path"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Shared file has no readable location".to_string())?;

    let bytes = std::fs::read(path).map_err(|e| format!("Could not read shared file: {}", e))?;
    let mime_type = node
        .mime_type
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| guess_mime(path));

    Ok(Some(ShareContent { mime_type, bytes }))
}

/// List every share link, with a path-relative URL filled in.
pub fn list(db: &Database) -> Result<Vec<ShareLink>, String> {
    list_inner(db, None)
}

/// Like [`list`], but the URL is prefixed with the caller's origin so the
/// frontend can copy a link that works from anywhere.
pub fn list_with_base(db: &Database, origin: Option<&str>) -> Result<Vec<ShareLink>, String> {
    list_inner(db, origin)
}

fn list_inner(db: &Database, origin: Option<&str>) -> Result<Vec<ShareLink>, String> {
    let base = origin
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(|o| o.trim_end_matches('/').to_string());
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_share_links_table())
        .map_err(|e| e.to_string())?;
    let mut links = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        if let Ok(link) = serde_json::from_str::<ShareLink>(value.value()) {
            let mut link = link.with_url();
            if let (Some(base), Some(url)) = (base.as_deref(), link.url.as_mut()) {
                *url = format!("{}{}", base, url);
            }
            links.push(link);
        }
    }
    Ok(links)
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn find_by_token(db: &Database, token: &str) -> Result<Option<ShareLink>, String> {
    db.get_share_link_by_token(token).map_err(|e| e.to_string())
}

/// Expired links answer `410 Gone`, not `404`, so a revoked/expired link is
/// distinguishable from a typo without leaking whether the token ever existed.
fn reject_if_expired(link: &ShareLink) -> Result<(), String> {
    if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&link.expires_at) {
        if chrono::Utc::now() > expires {
            return Err("Share link has expired".to_string());
        }
    }
    Ok(())
}

/// Minimal extension → MIME table (no extra dependency, no network lookup).
fn guess_mime(path: &str) -> String {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let mime: &str = match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "txt" | "log" | "md" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "yaml" | "yml" => "application/yaml",
        "xml" => "application/xml",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    };
    mime.to_string()
}
