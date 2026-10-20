// CyberManju OS — Trash operations (shared by Tauri IPC and REST)
//
// The caller holds the application database lock for the duration of the
// call, exactly as the original Tauri commands did.

use cybermanju_db::Database;
use cybermanju_types::schema::TrashItem;

/// List all items currently in the trash.
pub fn list(db: &Database) -> Result<Vec<TrashItem>, String> {
    db.list_trash().map_err(|e| e.to_string())
}

/// Restore a file or folder from trash to its original location.
pub fn restore(db: &Database, file_id: &str) -> Result<TrashItem, String> {
    db.restore_from_trash(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Item not found in trash: {}", file_id))
}

/// Permanently delete all items from the trash.
pub fn empty(db: &Database) -> Result<u32, String> {
    db.empty_trash().map_err(|e| e.to_string())
}

/// Permanently delete a single item from the trash (no restore).
pub fn delete(db: &Database, file_id: &str) -> Result<bool, String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let removed = {
        let mut table = tx
            .open_table(Database::get_trash_table())
            .map_err(|e| e.to_string())?;
        let removed = table.remove(file_id).map_err(|e| e.to_string())?.is_some();
        removed
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(removed)
}
