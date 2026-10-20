// CyberManju OS — Files (crates/web/src/api/files.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: create_folder, delete, rename, duplicate, move_to, rebuild_parent_index, create_loose_group, add_to_loose_group.
// Integrates: cybermanju_db, cybermanju_types, redb.
// Contracts: signals: too_large, not_found.
// CyberManju OS — File tree operations (shared by Tauri IPC and REST)

use cybermanju_db::Database;
use cybermanju_types::schema::FileNode;
use redb::ReadableTable;

/// Create a new folder entry in the database.
pub fn create_folder(db: &Database, name: String, parent_id: String) -> Result<FileNode, String> {
    let folder_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let folder = FileNode {
        id: folder_id.clone(),
        name,
        file_type: "folder".to_string(),
        parent_id: Some(parent_id.clone()),
        size_bytes: 0,
        mime_type: None,
        hash_blake3: None,
        encrypted: false,
        encryption_algorithm: None,
        compression_layers: Vec::new(),
        thumbnail_path: None,
        created_at: now.clone(),
        modified_at: now,
        context_data: None,
        tags: Vec::new(),
        collection_ids: Vec::new(),
        face_group_ids: Vec::new(),
        loose_group_ids: Vec::new(),
        gps_lat: None,
        gps_lon: None,
    };

    let serialized = serde_json::to_string(&folder).map_err(|e| e.to_string())?;
    db.insert_file_with_index(&folder_id, serialized.as_str(), Some(&parent_id))
        .map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "create",
        Some(&folder_id),
        serde_json::json!({ "name": folder.name }),
    );

    Ok(folder)
}

/// Soft-delete a file or folder (moves it to the trash).
pub fn delete(db: &Database, file_id: &str) -> Result<bool, String> {
    let node = read_file(db, file_id)?;

    db.trash_file(file_id, &node, None)
        .map_err(|e| e.to_string())?;

    db.log_audit("delete", "file", file_id, None, None)
        .map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "delete",
        Some(file_id),
        serde_json::json!({ "name": node.name }),
    );

    Ok(true)
}

/// Rename a file or folder.
pub fn rename(db: &Database, file_id: &str, new_name: String) -> Result<FileNode, String> {
    let mut file_node = read_file(db, file_id)?;
    let old_name = file_node.name.clone();
    file_node.name = new_name;
    file_node.modified_at = chrono::Utc::now().to_rfc3339();

    write_file(db, file_id, &file_node)?;
    let _ = db.record_mutation_op(
        "rename",
        Some(file_id),
        serde_json::json!({ "from": old_name, "to": file_node.name }),
    );
    Ok(file_node)
}

/// Context-preserving duplication: copies a file node and preserves context_data.
pub fn duplicate(db: &Database, file_id: &str) -> Result<FileNode, String> {
    let original = read_file(db, file_id)?;

    let new_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // A duplicate contains identical bytes — its content hash is the same.
    let new_hash = original.hash_blake3.clone();

    let link_preview = serde_json::json!({
        "type": "duplicate_link",
        "source_file_id": file_id,
        "source_hash": original.hash_blake3,
        "duplicated_at": now,
    });

    let mut context_data = original
        .context_data
        .clone()
        .unwrap_or(serde_json::Value::Null);
    if let Some(obj) = context_data.as_object_mut() {
        obj.insert("duplicated_from".to_string(), serde_json::json!(file_id));
        obj.insert("duplicate_created_at".to_string(), serde_json::json!(now));
    }

    let tags = original.tags.clone();

    let mut duplicated = original;
    duplicated.id = new_id.clone();
    duplicated.name = format!("{} (copy)", duplicated.name);
    duplicated.hash_blake3 = new_hash;
    duplicated.thumbnail_path = Some(link_preview.to_string());
    duplicated.created_at = now.clone();
    duplicated.modified_at = now;
    duplicated.context_data = Some(context_data);
    duplicated.tags = tags;
    duplicated.collection_ids = Vec::new();

    let serialized = serde_json::to_string(&duplicated).map_err(|e| e.to_string())?;
    db.insert_file_with_index(
        &new_id,
        serialized.as_str(),
        duplicated.parent_id.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "create",
        Some(&new_id),
        serde_json::json!({ "name": duplicated.name, "from": file_id }),
    );

    Ok(duplicated)
}

/// Move a file to a new parent.
pub fn move_to(db: &Database, file_id: &str, new_parent_id: String) -> Result<FileNode, String> {
    let mut file_node = read_file(db, file_id)?;

    let old_parent = file_node.parent_id.clone();
    file_node.parent_id = Some(new_parent_id.clone());
    file_node.modified_at = chrono::Utc::now().to_rfc3339();

    let serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    db.move_file_with_index(
        file_id,
        serialized.as_str(),
        old_parent.as_deref(),
        &new_parent_id,
    )
    .map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "move",
        Some(file_id),
        serde_json::json!({ "from": old_parent, "to": new_parent_id }),
    );

    Ok(file_node)
}

/// Rebuild the parent index from all FileNodes in the files table.
pub fn rebuild_parent_index(db: &Database) -> Result<u32, String> {
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

    let tx_write = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut index_table = tx_write
            .open_table(Database::get_parent_index_table())
            .map_err(|e| e.to_string())?;
        let keys: Vec<String> = index_table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|(k, _)| k.value().to_string()))
            .collect();
        for key in keys {
            index_table
                .remove(key.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx_write.commit().map_err(|e| e.to_string())?;

    let mut count = 0u32;
    for node in &file_nodes {
        if let Some(ref parent_id) = node.parent_id {
            db.add_to_parent_index(&node.id, parent_id)
                .map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok(count)
}

/// Create a loose group (ad-hoc file grouping).
pub fn create_loose_group(
    db: &Database,
    name: String,
    color: String,
) -> Result<cybermanju_types::schema::LooseGroup, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("invalid: group name is required".to_string());
    }
    let group = cybermanju_types::schema::LooseGroup {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        color,
        file_ids: Vec::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let serialized = serde_json::to_string(&group).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_loose_groups_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(group.id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(group)
}

/// Add a file to a loose group (also stamps the file node's id list).
pub fn add_to_loose_group(
    db: &Database,
    group_id: &str,
    file_id: &str,
) -> Result<cybermanju_types::schema::LooseGroup, String> {
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let group_table = tx_read
        .open_table(Database::get_loose_groups_table())
        .map_err(|e| e.to_string())?;
    let group_value = group_table
        .get(group_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Loose group not found: {group_id}"))?;
    let mut group: cybermanju_types::schema::LooseGroup =
        serde_json::from_str(group_value.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    let mut file_node = read_file(db, file_id)?;
    if !group.file_ids.contains(&file_id.to_string()) {
        group.file_ids.push(file_id.to_string());
    }
    if !file_node.loose_group_ids.contains(&group_id.to_string()) {
        file_node.loose_group_ids.push(group_id.to_string());
    }

    let group_serialized = serde_json::to_string(&group).map_err(|e| e.to_string())?;
    let file_serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut gt = tx
            .open_table(Database::get_loose_groups_table())
            .map_err(|e| e.to_string())?;
        gt.insert(group_id, group_serialized.as_str())
            .map_err(|e| e.to_string())?;
        let mut ft = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        ft.insert(file_id, file_serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "group",
        Some(file_id),
        serde_json::json!({ "group": group_id }),
    );
    Ok(group)
}

/// Replace a file's user tags (powers search/parse refinement + scene
/// matching on every transport). Normalized: trimmed, de-duplicated,
/// capped — never throws on messy input, just cleans it.
pub fn set_tags(db: &Database, file_id: &str, tags: Vec<String>) -> Result<FileNode, String> {
    let mut seen = std::collections::HashSet::new();
    let mut clean: Vec<String> = Vec::new();
    for raw in tags {
        let t = raw.trim().to_string();
        if t.is_empty() || t.chars().count() > 48 {
            continue;
        }
        let key = t.to_lowercase();
        if seen.insert(key) {
            clean.push(t);
        }
        if clean.len() >= 24 {
            break;
        }
    }
    let mut file_node = read_file(db, file_id)?;
    file_node.tags = clean;
    file_node.modified_at = chrono::Utc::now().to_rfc3339();
    write_file(db, file_id, &file_node)?;
    let _ = db.record_mutation_op(
        "tag",
        Some(file_id),
        serde_json::json!({ "tags": file_node.tags }),
    );
    Ok(file_node)
}

/// Preview metadata for a file.
pub fn preview(db: &Database, file_id: &str) -> Result<serde_json::Value, String> {
    let file_node = read_file(db, file_id)?;

    Ok(serde_json::json!({
        "file_id": file_node.id,
        "name": file_node.name,
        "file_type": file_node.file_type,
        "mime_type": file_node.mime_type,
        "size_bytes": file_node.size_bytes,
        "thumbnail_path": file_node.thumbnail_path,
        "encrypted": file_node.encrypted,
        "compression_layers": file_node.compression_layers,
        "tags": file_node.tags,
        "gps_lat": file_node.gps_lat,
        "gps_lon": file_node.gps_lon,
        "context_data": file_node.context_data,
    }))
}

// ─── Object-level access (P0-1) ───────────────────────────────────────────
//
// Port of the Tauri `verify_file_access` gate (`src-tauri/.../users.rs`)
// for the REST transport, which previously never called it.
//
// Fail-closed: any unknown (missing file row, missing user row, database
// error, no matching permission) is `Err` — callers map that to 403/404
// and never serve bytes. `admin` bypasses like the Tauri helper, but only
// for active admin accounts. Permission rows use the shared ACL decoder so
// canonical and legacy camelCase/snake_case rows have identical semantics in
// authorization, verification, and listing.

/// Required access level ranks (`read` < `write` < `admin`).
fn access_rank(access: &str) -> u8 {
    match access {
        "admin" => 3,
        "write" => 2,
        "read" => 1,
        _ => 0,
    }
}

/// Enforce that `claims` may use `file_id` at `required` (`read`/`write`).
pub fn check_access(
    db: &Database,
    claims: &crate::security::Claims,
    file_id: &str,
    required: &str,
) -> Result<(), String> {
    if !claims.is_expired() && claims.role == "admin" {
        // Admins hold global access — but only when the account still
        // exists and is active (a deleted/deactivated admin loses it).
        let tx = db
            .begin_read()
            .map_err(|_| "auth: access check unavailable".to_string())?;
        let users = tx
            .open_table(Database::get_users_table())
            .map_err(|_| "auth: access check unavailable".to_string())?;
        for entry in users
            .iter()
            .map_err(|_| "auth: access check unavailable".to_string())?
        {
            let (_, value) = entry.map_err(|_| "auth: access check unavailable".to_string())?;
            if let Ok(user) = serde_json::from_str::<cybermanju_types::schema::User>(value.value())
            {
                if user.id == claims.user_id && user.role == "admin" && user.is_active {
                    if db
                        .get_file_node(file_id)
                        .map_err(|_| "auth: access check unavailable".to_string())?
                        .is_none()
                    {
                        return Err(format!("not_found: file '{file_id}' not found"));
                    }
                    return Ok(());
                }
            }
        }
        return Err("auth: admin account is not active".to_string());
    }

    let tx = db
        .begin_read()
        .map_err(|_| "auth: access check unavailable".to_string())?;
    // Fail closed when the caller itself is unknown or deactivated.
    let users = tx
        .open_table(Database::get_users_table())
        .map_err(|_| "auth: access check unavailable".to_string())?;
    let mut caller_active = false;
    for entry in users
        .iter()
        .map_err(|_| "auth: access check unavailable".to_string())?
    {
        let (_, value) = entry.map_err(|_| "auth: access check unavailable".to_string())?;
        if let Ok(user) = serde_json::from_str::<cybermanju_types::schema::User>(value.value()) {
            if user.id == claims.user_id {
                caller_active = user.is_active;
                break;
            }
        }
    }
    if !caller_active {
        return Err("auth: unknown or deactivated account".to_string());
    }

    // The file must exist — unknown ids deny, never pass.
    if db
        .get_file_node(file_id)
        .map_err(|_| "auth: access check unavailable".to_string())?
        .is_none()
    {
        return Err(format!("not_found: file '{file_id}' not found"));
    }

    let table = tx
        .open_table(Database::get_user_file_perms_table())
        .map_err(|_| "auth: access check unavailable".to_string())?;
    let need = access_rank(required);
    for entry in table
        .iter()
        .map_err(|_| "auth: access check unavailable".to_string())?
    {
        let (_, value) = entry.map_err(|_| "auth: access check unavailable".to_string())?;
        if let Some(perm) = crate::api::permissions::parse_permission(value.value()) {
            if perm.user_id == claims.user_id
                && perm.file_id == file_id
                && access_rank(&perm.access) >= need
            {
                return Ok(());
            }
        }
    }
    Err(format!(
        "auth: '{required}' access to file '{file_id}' denied"
    ))
}

/// Enforce [`check_access`] over a batch of ids (fail-closed on the first
/// denial — one unreadable id fails the whole batch).
pub fn check_access_many(
    db: &Database,
    claims: &crate::security::Claims,
    file_ids: &[String],
    required: &str,
) -> Result<(), String> {
    for id in file_ids {
        crate::security::validate_id(id).map_err(|e| format!("invalid: {e}"))?;
        check_access(db, claims, id, required)?;
    }
    Ok(())
}

fn read_file(db: &Database, file_id: &str) -> Result<FileNode, String> {
    db.get_file_node(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File not found: {}", file_id))
}

fn write_file(db: &Database, file_id: &str, node: &FileNode) -> Result<(), String> {
    let serialized = serde_json::to_string(node).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(file_id, serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

// ─── Text content (code editor) ────────────────────────────────────────────

/// Largest file the editor will read or write in one call. Editing is for
/// source text, not media: oversized files get an honest `too_large:` and
/// the panel offers read-only preview instead.
pub const MAX_CONTENT_BYTES: usize = 1024 * 1024;

/// Resolve where a file's bytes live on this machine.
fn stored_path(node: &FileNode) -> Result<&str, String> {
    node.context_data
        .as_ref()
        .and_then(|ctx| ctx.get("original_path"))
        .and_then(|v| v.as_str())
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            format!(
                "not_found: '{}' has no stored bytes on this machine",
                node.name
            )
        })
}

/// Read a file's text content for the editor.
///
/// Refuses encrypted files (`encrypted:` — ciphertext is not editable text),
/// missing bytes (`not_found:`), oversized files (`too_large:`) and
/// non-UTF-8 bytes (`binary:`) — never a silent mojibake.
pub fn read_content(db: &Database, file_id: &str) -> Result<serde_json::Value, String> {
    let node = read_file(db, file_id)?;
    if node.file_type == "folder" {
        return Err(format!("invalid: '{}' is a folder", node.name));
    }
    if node.encrypted {
        return Err(format!(
            "encrypted: '{}' is encrypted — decrypt it before editing",
            node.name
        ));
    }
    let path = stored_path(&node)?;
    let bytes =
        std::fs::read(path).map_err(|e| format!("not_found: stored file is unreadable: {e}"))?;
    if bytes.len() > MAX_CONTENT_BYTES {
        return Err(format!(
            "too_large: '{}' is {} bytes, editor limit is {}",
            node.name,
            bytes.len(),
            MAX_CONTENT_BYTES
        ));
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| format!("binary: '{}' is not valid UTF-8 text", node.name))?;
    Ok(serde_json::json!({
        "fileId": node.id,
        "name": node.name,
        "content": content,
        "sizeBytes": node.size_bytes,
        "truncated": false,
        "hashBlake3": node.hash_blake3,
    }))
}

/// Overwrite a file's text content from the editor.
///
/// Snapshots a version first (best effort — a failed snapshot never blocks
/// the save), then writes the bytes, refreshes `size_bytes` / `hash_blake3`
/// / `modified_at`, and returns the updated summary. Same refusals as
/// `read_content`, plus empty-write protection is left to the caller.
pub fn write_content(
    db: &Database,
    file_id: &str,
    content: &str,
) -> Result<serde_json::Value, String> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(format!(
            "too_large: content is {} bytes, editor limit is {}",
            content.len(),
            MAX_CONTENT_BYTES
        ));
    }
    let mut node = read_file(db, file_id)?;
    if node.file_type == "folder" {
        return Err(format!("invalid: '{}' is a folder", node.name));
    }
    if node.encrypted {
        return Err(format!(
            "encrypted: '{}' is encrypted — decrypt it before editing",
            node.name
        ));
    }
    let path = stored_path(&node)?.to_string();

    // Snapshot first so every save is undoable from File Versions.
    let _ = super::versions::create(db, file_id);

    std::fs::write(&path, content.as_bytes())
        .map_err(|e| format!("not_found: stored file is not writable: {e}"))?;
    node.size_bytes = content.len() as u64;
    node.hash_blake3 = Some(blake3::hash(content.as_bytes()).to_hex().to_string());
    node.modified_at = chrono::Utc::now().to_rfc3339();
    write_file(db, file_id, &node)?;
    let _ = db.record_mutation_op(
        "edit",
        Some(file_id),
        serde_json::json!({ "sizeBytes": node.size_bytes, "hashBlake3": node.hash_blake3 }),
    );

    Ok(serde_json::json!({
        "fileId": node.id,
        "name": node.name,
        "sizeBytes": node.size_bytes,
        "hashBlake3": node.hash_blake3,
        "modifiedAt": node.modified_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_with_bytes(id: &str, name: &str, dir: &std::path::Path, bytes: &[u8]) -> FileNode {
        let path = dir.join(format!("{id}.txt"));
        std::fs::write(&path, bytes).expect("fixture");
        FileNode {
            id: id.to_string(),
            name: name.to_string(),
            file_type: "file".to_string(),
            parent_id: None,
            size_bytes: bytes.len() as u64,
            mime_type: Some("text/plain".to_string()),
            hash_blake3: None,
            encrypted: false,
            encryption_algorithm: None,
            compression_layers: Vec::new(),
            thumbnail_path: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            context_data: Some(serde_json::json!({
                "original_path": path.to_string_lossy().to_string(),
            })),
            tags: Vec::new(),
            collection_ids: Vec::new(),
            face_group_ids: Vec::new(),
            loose_group_ids: Vec::new(),
            gps_lat: None,
            gps_lon: None,
        }
    }

    fn temp_db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("t.redb").to_str().expect("utf8")).expect("db");
        (db, dir)
    }

    #[test]
    fn content_round_trips_and_rejects_non_text() {
        let (db, dir) = temp_db();
        let files_dir = dir.path().join("files");
        std::fs::create_dir_all(&files_dir).expect("mkdir");
        let node = node_with_bytes("f1", "a.rs", &files_dir, b"fn a() {}");
        write_file(&db, "f1", &node).expect("seed");

        let out = read_content(&db, "f1").expect("read");
        assert_eq!(out["content"], "fn a() {}");

        let saved = write_content(&db, "f1", "fn b() {}").expect("write");
        assert_eq!(saved["sizeBytes"], 9);
        let out = read_content(&db, "f1").expect("re-read");
        assert_eq!(out["content"], "fn b() {}");

        let bin = node_with_bytes("f2", "b.bin", &files_dir, &[0xff, 0xfe, 0x00]);
        write_file(&db, "f2", &bin).expect("seed");
        let err = read_content(&db, "f2").expect_err("binary");
        assert!(err.starts_with("binary:"), "{err}");
    }

    #[test]
    fn content_refuses_encrypted_and_missing_bytes() {
        let (db, dir) = temp_db();
        let files_dir = dir.path().join("files");
        std::fs::create_dir_all(&files_dir).expect("mkdir");
        let mut node = node_with_bytes("f1", "a.rs", &files_dir, b"hi");
        node.encrypted = true;
        write_file(&db, "f1", &node).expect("seed");
        assert!(read_content(&db, "f1")
            .expect_err("enc")
            .starts_with("encrypted:"));
        assert!(write_content(&db, "f1", "x")
            .expect_err("enc")
            .starts_with("encrypted:"));

        node.encrypted = false;
        node.context_data = None;
        write_file(&db, "f1", &node).expect("seed");
        assert!(read_content(&db, "f1")
            .expect_err("gone")
            .starts_with("not_found:"));
    }
}
