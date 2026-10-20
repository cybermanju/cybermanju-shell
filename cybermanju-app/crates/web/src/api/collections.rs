// CyberManju OS — Collections (crates/web/src/api/collections.rs).
// Role: REST dashboard (:3456): routes, auth, security headers.
// Exports: list, create, add_item, remove_item.
// Integrates: cybermanju_db, cybermanju_types, redb.
// Contracts: none special (pure logic; errors bubble to caller).
// CyberManju OS — Collections (shared by Tauri IPC and REST)

use cybermanju_db::Database;
use cybermanju_types::schema::{Collection, CollectionItem, FileNode};
use redb::ReadableTable;

/// List all collections from the database.
pub fn list(db: &Database) -> Result<Vec<Collection>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_collections_table())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        let collection: Collection =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        results.push(collection);
    }

    Ok(results)
}

/// Create a new collection.
pub fn create(
    db: &Database,
    name: String,
    collection_type: String,
    color: String,
    description: Option<String>,
) -> Result<Collection, String> {
    let collection_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let collection = Collection {
        id: collection_id.clone(),
        name,
        collection_type,
        color,
        description,
        item_ids: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    let serialized = serde_json::to_string(&collection).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_collections_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(collection_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "collection",
        None,
        serde_json::json!({ "id": collection_id, "name": collection.name }),
    );
    Ok(collection)
}

/// Add a file to a collection.
pub fn add_item(
    db: &Database,
    collection_id: &str,
    file_id: &str,
    note: Option<String>,
) -> Result<CollectionItem, String> {
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let coll_table = tx_read
        .open_table(Database::get_collections_table())
        .map_err(|e| e.to_string())?;
    let coll_value = coll_table
        .get(collection_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Collection not found: {}", collection_id))?;
    let mut collection: Collection =
        serde_json::from_str(coll_value.value()).map_err(|e| e.to_string())?;

    let file_table = tx_read
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let file_value = file_table
        .get(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("File not found: {}", file_id))?;
    let mut file_node: FileNode =
        serde_json::from_str(file_value.value()).map_err(|e| e.to_string())?;
    drop(tx_read);

    let now = chrono::Utc::now().to_rfc3339();

    let item_id = uuid::Uuid::new_v4().to_string();
    let collection_item = CollectionItem {
        id: item_id.clone(),
        collection_id: collection_id.to_string(),
        file_id: file_id.to_string(),
        note,
        added_at: now.clone(),
    };

    if !collection.item_ids.contains(&item_id) {
        collection.item_ids.push(item_id.clone());
    }
    collection.updated_at = now.clone();

    if !file_node
        .collection_ids
        .iter()
        .any(|id| id.as_str() == collection_id)
    {
        file_node.collection_ids.push(collection_id.to_string());
    }
    file_node.modified_at = now;

    let coll_serialized = serde_json::to_string(&collection).map_err(|e| e.to_string())?;
    let file_serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
    let item_serialized = serde_json::to_string(&collection_item).map_err(|e| e.to_string())?;

    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut ct = tx
            .open_table(Database::get_collections_table())
            .map_err(|e| e.to_string())?;
        ct.insert(collection_id, coll_serialized.as_str())
            .map_err(|e| e.to_string())?;

        let mut ft = tx
            .open_table(Database::get_files_table())
            .map_err(|e| e.to_string())?;
        ft.insert(file_id, file_serialized.as_str())
            .map_err(|e| e.to_string())?;

        let mut it = tx
            .open_table(Database::get_collection_items_table())
            .map_err(|e| e.to_string())?;
        it.insert(item_id.as_str(), item_serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "collection",
        Some(file_id),
        serde_json::json!({ "collection": collection_id, "item": item_id }),
    );
    Ok(collection_item)
}

/// Remove a file from a collection.
pub fn remove_item(db: &Database, collection_id: &str, file_id: &str) -> Result<bool, String> {
    let tx_read = db.begin_read().map_err(|e| e.to_string())?;
    let coll_table = tx_read
        .open_table(Database::get_collections_table())
        .map_err(|e| e.to_string())?;
    let coll_value = coll_table
        .get(collection_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Collection not found: {}", collection_id))?;
    let mut collection: Collection =
        serde_json::from_str(coll_value.value()).map_err(|e| e.to_string())?;

    let items_table = tx_read
        .open_table(Database::get_collection_items_table())
        .map_err(|e| e.to_string())?;
    let mut item_id_to_remove: Option<String> = None;
    for entry in items_table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        let item: CollectionItem =
            serde_json::from_str(value.value()).map_err(|e| e.to_string())?;
        if item.collection_id == collection_id && item.file_id == file_id {
            item_id_to_remove = Some(key.value().to_string());
            break;
        }
    }

    let file_table = tx_read
        .open_table(Database::get_files_table())
        .map_err(|e| e.to_string())?;
    let file_value = file_table.get(file_id).map_err(|e| e.to_string())?;
    drop(tx_read);

    let now = chrono::Utc::now().to_rfc3339();

    collection.item_ids.retain(|id| {
        item_id_to_remove
            .as_ref()
            .is_none_or(|to_remove| id != to_remove)
    });
    collection.updated_at = now.clone();

    let coll_serialized = serde_json::to_string(&collection).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut ct = tx
            .open_table(Database::get_collections_table())
            .map_err(|e| e.to_string())?;
        ct.insert(collection_id, coll_serialized.as_str())
            .map_err(|e| e.to_string())?;

        if let Some(ref item_id) = item_id_to_remove {
            let mut it = tx
                .open_table(Database::get_collection_items_table())
                .map_err(|e| e.to_string())?;
            it.remove(item_id.as_str()).map_err(|e| e.to_string())?;
        }

        if let Some(fv) = file_value {
            let mut file_node: FileNode =
                serde_json::from_str(fv.value()).map_err(|e| e.to_string())?;
            file_node
                .collection_ids
                .retain(|id| id.as_str() != collection_id);
            file_node.modified_at = now;
            let file_serialized = serde_json::to_string(&file_node).map_err(|e| e.to_string())?;
            let mut ft = tx
                .open_table(Database::get_files_table())
                .map_err(|e| e.to_string())?;
            ft.insert(file_id, file_serialized.as_str())
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    let _ = db.record_mutation_op(
        "collection",
        Some(file_id),
        serde_json::json!({ "collection": collection_id, "removed": true }),
    );
    Ok(true)
}
