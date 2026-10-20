// CyberManju OS — Versions (crates/web/src/api/versions.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: list, create, revert, snapshot_all.
// Integrates: cybermanju_db, cybermanju_types, redb.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — File version operations (shared by Tauri IPC and REST)

use cybermanju_db::Database;
use cybermanju_types::schema::{FileNode, FileVersion};
use redb::ReadableTable;

/// List all versions of a specific file.
pub fn list(db: &Database, file_id: &str) -> Result<Vec<FileVersion>, String> {
    db.list_file_versions(file_id).map_err(|e| e.to_string())
}

/// Create a new version snapshot for a file.
pub fn create(db: &Database, file_id: &str) -> Result<FileVersion, String> {
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx_read
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let file_node: FileNode = table
        .get(file_id)
        .map_err(|e| e.to_string())?
        .and_then(|v| serde_json::from_str(v.value()).ok())
        .ok_or_else(|| format!("File not found: {}", file_id))?;
    drop(tx_read);

    let version = db
        .create_file_version(&file_node, None)
        .map_err(|e| e.to_string())?;

    db.log_audit("create_version", "file_version", &version.id, None, None)
        .map_err(|e| e.to_string())?;

    Ok(version)
}

/// Revert a file to a previous version.
pub fn revert(db: &Database, file_id: &str, version_id: &str) -> Result<FileVersion, String> {
    let version = db
        .revert_file_version(file_id, version_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Version not found: {}", version_id))?;

    db.log_audit("revert_version", "file_version", version_id, None, None)
        .map_err(|e| e.to_string())?;

    Ok(version)
}

/// Create a version snapshot for every file in the database.
pub fn snapshot_all(db: &Database) -> Result<u32, String> {
    let mut count = 0u32;

    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx_read
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let file_nodes: Vec<FileNode> = table
        .iter()
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let (_, value) = entry.ok()?;
            serde_json::from_str::<FileNode>(value.value()).ok()
        })
        .collect();
    drop(tx_read);

    for node in &file_nodes {
        if node.file_type == "file" {
            db.create_file_version(node, None)
                .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok(count)
}
