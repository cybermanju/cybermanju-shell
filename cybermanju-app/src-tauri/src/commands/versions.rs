// CyberManju OS — Versions (src-tauri/src/commands/versions.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: list_file_versions, create_file_version, revert_file_version, snapshot_all_versions.
// Integrates: cybermanju_web, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — File version commands (thin wrappers over the shared API)

use cybermanju_web::api;
use tauri::State;

use crate::db::schema::FileVersion;
use crate::AppState;

/// List all versions of a specific file.
#[tauri::command(async)]
pub fn list_file_versions(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileVersion>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::versions::list(&db, &file_id)
}

/// Create a new version snapshot for a file.
#[tauri::command(async)]
pub fn create_file_version(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FileVersion, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::versions::create(&db, &file_id)
}

/// Revert a file to a previous version.
#[tauri::command(async)]
pub fn revert_file_version(
    file_id: String,
    version_id: String,
    state: State<'_, AppState>,
) -> Result<FileVersion, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::versions::revert(&db, &file_id, &version_id)
}

/// Create a version snapshot for every file in the database.
#[tauri::command(async)]
pub fn snapshot_all_versions(state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::versions::snapshot_all(&db)
}
