// CyberManju OS — Collections (src-tauri/src/commands/collections.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: list_collections, create_collection, add_to_collection, remove_from_collection.
// Integrates: tauri.
// Contracts: none special (pure logic; errors bubble to caller).
use tauri::State;

use crate::db::schema::{Collection, CollectionItem};
use crate::AppState;

/// List all collections from the database.
#[tauri::command(async)]
pub fn list_collections(state: State<'_, AppState>) -> Result<Vec<Collection>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    cybermanju_web::api::collections::list(&db)
}

/// Create a new collection.
#[tauri::command(async)]
pub fn create_collection(
    name: String,
    collection_type: String,
    color: String,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Collection, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::collections::create(&db, name, collection_type, color, description)
}

/// Add a file to a collection.
#[tauri::command(async)]
pub fn add_to_collection(
    collection_id: String,
    file_id: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<CollectionItem, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::collections::add_item(&db, &collection_id, &file_id, note)
}

/// Remove a file from a collection.
#[tauri::command(async)]
pub fn remove_from_collection(
    collection_id: String,
    file_id: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    cybermanju_web::api::collections::remove_item(&db, &collection_id, &file_id)
}
