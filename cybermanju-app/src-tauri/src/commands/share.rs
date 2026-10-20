// CyberManju OS — Share (src-tauri/src/commands/share.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: generate_share_link, get_shared_file, list_share_links.
// Integrates: cybermanju_web, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Share link commands (thin wrappers over the shared API)

use cybermanju_web::api;
use tauri::State;

use crate::db::schema::{FileNode, ShareLink};
use crate::AppState;

/// Share link payload returned to the frontend.
pub use cybermanju_web::api::share::ShareLinkResult;

/// Create a share link for a file.
#[tauri::command(async)]
pub fn generate_share_link(
    file_id: String,
    expires_in_hours: Option<u64>,
    state: State<'_, AppState>,
) -> Result<ShareLinkResult, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::share::generate(&db, &file_id, expires_in_hours)
}

/// Resolve a share token to its file, enforcing expiry.
#[tauri::command(async)]
pub fn get_shared_file(
    token: String,
    state: State<'_, AppState>,
) -> Result<Option<FileNode>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::share::resolve(&db, &token)
}

/// List every share link, with its public URL filled in.
#[tauri::command(async)]
pub fn list_share_links(state: State<'_, AppState>) -> Result<Vec<ShareLink>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::share::list(&db)
}
