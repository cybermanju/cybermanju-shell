// CyberManju OS — Trash (src-tauri/src/commands/trash.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: list_trash, restore_from_trash, empty_trash, delete_from_trash.
// Integrates: cybermanju_web, tauri.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Trash commands (thin wrappers over the shared API)

use cybermanju_web::api;
use tauri::State;

use crate::db::schema::TrashItem;
use crate::AppState;

/// List all items currently in the trash.
#[tauri::command(async)]
pub fn list_trash(state: State<'_, AppState>) -> Result<Vec<TrashItem>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::trash::list(&db)
}

/// Restore a file or folder from trash to its original location.
#[tauri::command(async)]
pub fn restore_from_trash(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<TrashItem, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::trash::restore(&db, &file_id)
}

/// Permanently delete all items from the trash.
#[tauri::command(async)]
pub fn empty_trash(state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::trash::empty(&db)
}

/// Permanently delete a single item from the trash (no restore).
#[tauri::command(async)]
pub fn delete_from_trash(file_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::trash::delete(&db, &file_id)
}
