// CyberManju OS — Files (src-tauri/src/commands/files.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: list_files, get_file, create_folder, delete_file, rename_file, set_file_tags, duplicate_file_context, move_file.
// Integrates: chrono, redb, tauri, cybermanju_web.
// Contracts: signals: not_found.
use chrono::Utc;
use redb::ReadableTable;
use tauri::State;

use crate::db::schema::FileNode;
use crate::db::schema::LooseGroup;
use cybermanju_web::api;

use crate::AppState;

/// List all file nodes whose parent_id matches the given parent_path.
/// Uses the parent_index secondary index for O(1) lookup instead of O(N) full scan.
#[tauri::command(async)]
pub fn list_files(
    parent_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileNode>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;

    // Use the parent index for O(1) lookup
    let file_ids = db.list_by_parent(&parent_path).map_err(|e| e.to_string())?;

    if file_ids.is_empty() {
        // No entries in the parent index — return empty
        return Ok(Vec::new());
    }

    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let read_table = tx
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for file_id in &file_ids {
        match read_table
            .get(file_id.as_str())
            .map_err(|e| e.to_string())?
        {
            Some(value) => {
                match serde_json::from_str::<FileNode>(value.value()) {
                    Ok(node) => results.push(node),
                    Err(_) => {
                        // Stale index entry — file was deleted but index wasn't updated.
                        // Clean up the index entry in the background.
                        log::warn!("Stale parent index entry for file_id={}", file_id);
                    }
                }
            }
            None => {
                // File was deleted but parent index wasn't updated — skip
                log::warn!("Parent index references non-existent file_id={}", file_id);
            }
        }
    }

    Ok(results)
}

/// Get a single file node by its ID.
#[tauri::command(async)]
pub fn get_file(file_id: String, state: State<'_, AppState>) -> Result<FileNode, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;

    let value = table
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File not found: {}", file_id))?;

    let file_node: FileNode = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
    Ok(file_node)
}

/// Create a new folder entry in the database.
#[tauri::command(async)]
pub fn create_folder(
    name: String,
    parent_id: String,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::create_folder(&db, name, parent_id)
}

/// Delete a file or folder by its ID (soft-delete to trash).
#[tauri::command(async)]
pub fn delete_file(file_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::delete(&db, &file_id)
}

/// Rename a file or folder.
#[tauri::command(async)]
pub fn rename_file(
    file_id: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::rename(&db, &file_id, new_name)
}

/// Replace a file's user tags (search/parse refinement + scene matching).
#[tauri::command(async)]
pub fn set_file_tags(
    file_id: String,
    tags: Vec<String>,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::set_tags(&db, &file_id, tags)
}

/// Context-preserving duplication: copies a file node and preserves context_data.
#[tauri::command(async)]
pub fn duplicate_file_context(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::duplicate(&db, &file_id)
}

/// Move a file to a new parent.
#[tauri::command(async)]
pub fn move_file(
    file_id: String,
    new_parent_id: String,
    state: State<'_, AppState>,
) -> Result<FileNode, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::move_to(&db, &file_id, new_parent_id)
}

/// Get preview metadata for a file.
#[tauri::command(async)]
pub fn get_preview(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::files::preview(&db, &file_id)
}

/// Read a file's text content for the code editor.
/// Same refusals as REST (`encrypted:`, `binary:`, `too_large:`, `not_found:`).
#[tauri::command(async)]
pub fn read_file_content(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    api::files::read_content(&db, &file_id)
}

/// Overwrite a file's text content from the code editor.
/// Snapshots a version first (best effort), then writes bytes and refreshes
/// size/hash/modified metadata.
#[tauri::command(async)]
pub fn write_file_content(
    file_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // Write guard like the version commands: the save snapshots a version
    // and rewrites the row, which must not interleave with another writer.
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::write_content(&db, &file_id, &content)
}

#[tauri::command(async)]
/// Create an empty ad-hoc file group (see [`LooseGroup`]).
pub fn create_loose_group(
    name: String,
    color: String,
    state: State<'_, AppState>,
) -> Result<LooseGroup, String> {
    let group_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let group = LooseGroup {
        id: group_id.clone(),
        name,
        color,
        file_ids: Vec::new(),
        created_at: now,
    };

    let db = state.db.write().map_err(|e| e.to_string())?;
    let serialized = serde_json::to_string(&group).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(crate::db::Database::get_loose_groups_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(group_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(group)
}

/// Add a file to a loose group. Also updates the file node's loose_group_ids.
#[tauri::command(async)]
pub fn add_to_loose_group(
    group_id: String,
    file_id: String,
    state: State<'_, AppState>,
) -> Result<LooseGroup, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;

    // Read the group
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let group_table = tx_read
        .open_table(crate::db::Database::get_loose_groups_table())
        .map_err(|e| e.to_string())?;
    let group_value = group_table
        .get(group_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Loose group not found: {}", group_id))?;
    let mut group: LooseGroup =
        serde_json::from_str(group_value.value()).map_err(|e| e.to_string())?;

    if !group.file_ids.contains(&file_id) {
        group.file_ids.push(file_id.clone());
    }

    // Read the file node
    let file_table = tx_read
        .open_table(crate::db::Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let file_value = file_table
        .get(file_id.as_str())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File not found: {}", file_id))?;
    let mut file_node: FileNode =
        serde_json::from_str(file_value.value()).map_err(|e| e.to_string())?;

    if !file_node.loose_group_ids.contains(&group_id) {
        file_node.loose_group_ids.push(group_id.clone());
    }
    drop(tx_read);

    // Write both back in a single transaction
    let group_serialized = serde_json::to_string(&group).map_err(|e| e.to_string())?;
    let file_serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut gt = tx
            .open_table(crate::db::Database::get_loose_groups_table())
            .map_err(|e| e.to_string())?;
        gt.insert(group_id.as_str(), group_serialized.as_str())
            .map_err(|e| e.to_string())?;

        let mut ft = tx
            .open_table(crate::db::Database::get_files_table())
            .map_err(|e| e.to_string())?;
        ft.insert(file_id.as_str(), file_serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;

    Ok(group)
}

/// List all loose groups.
#[tauri::command(async)]
pub fn list_loose_groups(state: State<'_, AppState>) -> Result<Vec<LooseGroup>, String> {
    let db = state.db.read().map_err(|e| e.to_string())?;
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(crate::db::Database::get_loose_groups_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let group: LooseGroup = serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        results.push(group);
    }

    Ok(results)
}

/// Rebuild the parent index from all FileNodes in the files table.
/// Useful after restoring from a backup or index corruption.
#[tauri::command(async)]
pub fn rebuild_parent_index(state: State<'_, AppState>) -> Result<u32, String> {
    let db = state.db.write().map_err(|e| e.to_string())?;
    api::files::rebuild_parent_index(&db)
}
