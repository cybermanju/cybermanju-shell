// CyberManju OS — Db (crates/tests/src/db.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: cybermanju_db, cybermanju_types.
// Contracts: none special (pure logic; errors bubble to caller).
use cybermanju_db::Database;
use cybermanju_types::FileNode;

fn temp_db() -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.redb");
    let db = Database::new(path.to_str().unwrap()).unwrap();
    (db, dir)
}

fn file_node(id: &str, parent_id: Option<&str>) -> FileNode {
    FileNode {
        id: id.to_string(),
        name: format!("{id}.txt"),
        file_type: "file".to_string(),
        parent_id: parent_id.map(str::to_string),
        size_bytes: 4,
        mime_type: Some("text/plain".to_string()),
        hash_blake3: None,
        encrypted: false,
        encryption_algorithm: None,
        compression_layers: vec![],
        thumbnail_path: None,
        context_data: None,
        tags: vec![],
        collection_ids: vec![],
        face_group_ids: vec![],
        loose_group_ids: vec![],
        gps_lat: None,
        gps_lon: None,
        created_at: "2026-10-09T00:00:00Z".to_string(),
        modified_at: "2026-10-09T00:00:00Z".to_string(),
    }
}

#[test]
fn test_database_creation() {
    let (_db, _dir) = temp_db();
}

#[test]
fn test_table_accessors_exist() {
    let _ = Database::get_files_table();
    let _ = Database::get_accounts_table();
    let _ = Database::get_collections_table();
    let _ = Database::get_collection_items_table();
    let _ = Database::get_face_groups_table();
    let _ = Database::get_loose_groups_table();
    let _ = Database::get_encryption_keys_table();
    let _ = Database::get_locations_table();
    let _ = Database::get_users_table();
    let _ = Database::get_user_file_perms_table();
    let _ = Database::get_sync_configs_table();
    let _ = Database::get_parent_index_table();
}

#[test]
fn test_begin_read_write() {
    let (db, _dir) = temp_db();
    let _read_txn = db.begin_read().unwrap();
    let _write_txn = db.begin_write().unwrap();
}

#[test]
fn test_insert_file_with_index() {
    let (db, _dir) = temp_db();
    let file_json = r#"{"id":"f1","name":"test.txt"}"#;
    db.insert_file_with_index("f1", file_json, Some("root"))
        .unwrap();

    let children = db.list_by_parent("root").unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0], "f1");
}

#[test]
fn test_insert_multiple_files_same_parent() {
    let (db, _dir) = temp_db();
    db.insert_file_with_index("f1", "j1", Some("root")).unwrap();
    db.insert_file_with_index("f2", "j2", Some("root")).unwrap();
    db.insert_file_with_index("f3", "j3", Some("root")).unwrap();

    let children = db.list_by_parent("root").unwrap();
    assert_eq!(children.len(), 3);
}

#[test]
fn test_insert_file_no_parent() {
    let (db, _dir) = temp_db();
    db.insert_file_with_index("f1", "j1", None).unwrap();
    let children = db.list_by_parent("root").unwrap();
    assert!(children.is_empty());
}

#[test]
fn test_remove_file_with_index() {
    let (db, _dir) = temp_db();
    db.insert_file_with_index("f1", "j1", Some("root")).unwrap();
    let removed = db.remove_file_with_index("f1", Some("root")).unwrap();
    assert!(removed);
    let children = db.list_by_parent("root").unwrap();
    assert!(children.is_empty());
}

#[test]
fn test_remove_nonexistent_file() {
    let (db, _dir) = temp_db();
    let removed = db.remove_file_with_index("nonexistent", None).unwrap();
    assert!(!removed);
}

#[test]
fn test_add_to_parent_index() {
    let (db, _dir) = temp_db();
    db.add_to_parent_index("f1", "parent1").unwrap();
    db.add_to_parent_index("f2", "parent1").unwrap();

    let children = db.list_by_parent("parent1").unwrap();
    assert_eq!(children, vec!["f1".to_string(), "f2".to_string()]);
}

#[test]
fn test_add_duplicate_to_parent_index() {
    let (db, _dir) = temp_db();
    db.add_to_parent_index("f1", "parent1").unwrap();
    db.add_to_parent_index("f1", "parent1").unwrap();

    let children = db.list_by_parent("parent1").unwrap();
    assert_eq!(children.len(), 1);
}

#[test]
fn test_remove_from_parent_index() {
    let (db, _dir) = temp_db();
    db.add_to_parent_index("f1", "parent1").unwrap();
    db.add_to_parent_index("f2", "parent1").unwrap();
    db.remove_from_parent_index("f1", "parent1").unwrap();

    let children = db.list_by_parent("parent1").unwrap();
    assert_eq!(children, vec!["f2".to_string()]);
}

#[test]
fn test_remove_last_from_parent_index() {
    let (db, _dir) = temp_db();
    db.add_to_parent_index("f1", "parent1").unwrap();
    db.remove_from_parent_index("f1", "parent1").unwrap();

    let children = db.list_by_parent("parent1").unwrap();
    assert!(children.is_empty());
}

#[test]
fn test_trash_and_restore_keeps_parent_index_consistent() {
    let (db, _dir) = temp_db();
    let file = file_node("f1", Some("parent1"));
    let serialized = serde_json::to_string(&file).unwrap();
    db.insert_file_with_index("f1", &serialized, Some("parent1"))
        .unwrap();
    db.insert_file_with_index("f2", "{}", Some("parent1"))
        .unwrap();

    db.trash_file("f1", &file, None).unwrap();
    assert_eq!(
        db.list_by_parent("parent1").unwrap(),
        vec!["f2".to_string()]
    );
    assert!(db.get_file_node("f1").unwrap().is_none());

    let restored = db.restore_from_trash("f1").unwrap().unwrap();
    assert_eq!(restored.original_file, file);
    let children = db.list_by_parent("parent1").unwrap();
    assert_eq!(children.len(), 2);
    assert!(children.iter().any(|id| id.as_str() == "f1"));
    assert!(children.iter().any(|id| id.as_str() == "f2"));
    assert_eq!(children.iter().filter(|id| id.as_str() == "f1").count(), 1);
    assert!(db.restore_from_trash("f1").unwrap().is_none());
    assert_eq!(db.list_by_parent("parent1").unwrap().len(), 2);
}

#[test]
fn test_trash_and_restore_file_without_parent() {
    let (db, _dir) = temp_db();
    let file = file_node("root-file", None);
    let serialized = serde_json::to_string(&file).unwrap();
    db.insert_file_with_index(&file.id, &serialized, None)
        .unwrap();

    db.trash_file(&file.id, &file, None).unwrap();
    assert!(db.list_by_parent("root").unwrap().is_empty());
    db.restore_from_trash(&file.id).unwrap().unwrap();

    assert_eq!(db.get_file_node(&file.id).unwrap(), Some(file));
    assert!(db.list_by_parent("root").unwrap().is_empty());
}

#[test]
fn test_list_by_parent_empty() {
    let (db, _dir) = temp_db();
    let children = db.list_by_parent("nonexistent").unwrap();
    assert!(children.is_empty());
}

#[test]
fn test_move_file_with_index() {
    let (db, _dir) = temp_db();
    db.insert_file_with_index("f1", "j1", Some("old_parent"))
        .unwrap();

    db.move_file_with_index("f1", "j1_updated", Some("old_parent"), "new_parent")
        .unwrap();

    let old_children = db.list_by_parent("old_parent").unwrap();
    assert!(old_children.is_empty());

    let new_children = db.list_by_parent("new_parent").unwrap();
    assert_eq!(new_children, vec!["f1".to_string()]);
}

#[test]
fn test_move_file_no_old_parent() {
    let (db, _dir) = temp_db();
    db.insert_file_with_index("f1", "j1", None).unwrap();

    db.move_file_with_index("f1", "j1", None, "new_parent")
        .unwrap();

    let children = db.list_by_parent("new_parent").unwrap();
    assert_eq!(children, vec!["f1".to_string()]);
}

#[test]
fn test_file_index_across_multiple_parents() {
    let (db, _dir) = temp_db();
    db.add_to_parent_index("f1", "p1").unwrap();
    db.add_to_parent_index("f2", "p1").unwrap();
    db.add_to_parent_index("f3", "p2").unwrap();
    db.add_to_parent_index("f4", "p2").unwrap();
    db.add_to_parent_index("f5", "p2").unwrap();

    assert_eq!(db.list_by_parent("p1").unwrap().len(), 2);
    assert_eq!(db.list_by_parent("p2").unwrap().len(), 3);
    assert!(db.list_by_parent("p3").unwrap().is_empty());
}
