// CyberManju OS — File permissions (crates/web/src/api/permissions.rs).
// Role: authenticated per-file ACL grant, verification, and listing helpers.
// Exports: grant_permission, verify_permission, list_permissions_for_file.
// Integrates: Claims, user identity validation, file access, and redb ACL rows.
// Contracts: file-admin only for grants/lists and cross-user checks; fail closed.

use cybermanju_db::Database;
use cybermanju_types::schema::{User, UserFilePermission};
use redb::ReadableTable;

use crate::security::{self, Claims};

/// Grant or replace one user's access to an existing file.
///
/// Only a caller with `admin` access to this file may change its ACL. The
/// actor recorded in `granted_by` always comes from the verified caller token.
pub fn grant_permission(
    db: &Database,
    caller: &Claims,
    user_id: &str,
    file_id: &str,
    access: &str,
) -> Result<UserFilePermission, String> {
    security::validate_id(user_id).map_err(|e| format!("invalid: {e}"))?;
    security::validate_id(file_id).map_err(|e| format!("invalid: {e}"))?;
    validate_access(access)?;
    crate::api::users::validate_token_identity(db, caller)?;
    crate::api::files::check_access(db, caller, file_id, "admin")?;
    require_active_user(db, user_id)?;
    require_file(db, file_id)?;

    let permission = UserFilePermission {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        file_id: file_id.to_string(),
        access: access.to_string(),
        granted_by: caller.user_id.clone(),
        granted_at: chrono::Utc::now().to_rfc3339(),
    };
    let serialized = serde_json::to_string(&permission).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_user_file_perms_table())
            .map_err(|e| e.to_string())?;
        // Collect matching row keys before mutating the table. Decode both
        // canonical structs and the legacy JSON shapes without trusting them.
        let stale_keys = table
            .iter()
            .map_err(|e| e.to_string())?
            .filter_map(|entry| {
                let (key, value) = entry.ok()?;
                let raw = value.value();
                let matches = parse_permission(raw)
                    .is_some_and(|row| row.user_id == user_id && row.file_id == file_id);
                matches.then(|| key.value().to_string())
            })
            .collect::<Vec<_>>();
        for key in stale_keys {
            table.remove(key.as_str()).map_err(|e| e.to_string())?;
        }
        table
            .insert(permission.id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(permission)
}

/// Determine whether `user_id` has `required_access` on `file_id`.
///
/// A caller may check their own access; checking any other account requires
/// `admin` access to the file, preventing arbitrary ACL/account introspection.
pub fn verify_permission(
    db: &Database,
    caller: &Claims,
    user_id: &str,
    file_id: &str,
    required_access: &str,
) -> Result<bool, String> {
    security::validate_id(user_id).map_err(|e| format!("invalid: {e}"))?;
    security::validate_id(file_id).map_err(|e| format!("invalid: {e}"))?;
    validate_access(required_access)?;
    crate::api::users::validate_token_identity(db, caller)?;
    if caller.user_id != user_id {
        crate::api::files::check_access(db, caller, file_id, "admin")?;
    }
    let target = require_active_user(db, user_id)?;
    require_file(db, file_id)?;

    if caller.user_id == user_id {
        // Includes the global-admin bypass, which itself checks the current
        // active admin record.
        return Ok(crate::api::files::check_access(db, caller, file_id, required_access).is_ok());
    }
    if target.role == "admin" {
        return Ok(true);
    }

    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_user_file_perms_table())
        .map_err(|e| e.to_string())?;
    let needed = access_rank(required_access);
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        if let Some(permission) = parse_permission(value.value()) {
            if permission.user_id == user_id
                && permission.file_id == file_id
                && access_rank(&permission.access) >= needed
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// List the grants for a file; only a caller with file-admin access may do so.
pub fn list_permissions_for_file(
    db: &Database,
    caller: &Claims,
    file_id: &str,
) -> Result<Vec<UserFilePermission>, String> {
    security::validate_id(file_id).map_err(|e| format!("invalid: {e}"))?;
    crate::api::users::validate_token_identity(db, caller)?;
    crate::api::files::check_access(db, caller, file_id, "admin")?;
    require_file(db, file_id)?;

    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_user_file_perms_table())
        .map_err(|e| e.to_string())?;
    let mut permissions = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_, value) = entry.map_err(|e| e.to_string())?;
        if let Some(permission) = parse_permission(value.value()) {
            if permission.file_id == file_id {
                permissions.push(permission);
            }
        }
    }
    Ok(permissions)
}

fn require_active_user(db: &Database, user_id: &str) -> Result<User, String> {
    let tx = db
        .begin_read()
        .map_err(|_| "auth: user lookup unavailable".to_string())?;
    let table = tx
        .open_table(Database::get_users_table())
        .map_err(|_| "auth: user lookup unavailable".to_string())?;
    let raw = table
        .get(user_id)
        .map_err(|_| "auth: user lookup unavailable".to_string())?
        .ok_or_else(|| format!("not_found: user '{user_id}' not found"))?;
    let user: User = serde_json::from_str(raw.value())
        .map_err(|_| "auth: user record is invalid".to_string())?;
    if user.id != user_id || !user.is_active {
        return Err("auth: target user is inactive or invalid".to_string());
    }
    Ok(user)
}

fn require_file(db: &Database, file_id: &str) -> Result<(), String> {
    match db
        .get_file_node(file_id)
        .map_err(|_| "auth: file lookup unavailable".to_string())?
    {
        Some(_) => Ok(()),
        None => Err(format!("not_found: file '{file_id}' not found")),
    }
}

fn validate_access(access: &str) -> Result<(), String> {
    if matches!(access, "read" | "write" | "admin") {
        Ok(())
    } else {
        Err("invalid: access must be 'read', 'write', or 'admin'".to_string())
    }
}

fn access_rank(access: &str) -> u8 {
    match access {
        "admin" => 3,
        "write" => 2,
        "read" => 1,
        _ => 0,
    }
}

/// Decode the canonical and legacy ACL JSON representations consistently.
/// Identity and access are required for authorization; descriptive metadata
/// was optional in older rows and defaults to empty strings for API output.
pub(crate) fn parse_permission(raw: &str) -> Option<UserFilePermission> {
    let row = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    let user_id = json_str(&row, "userId", "user_id", true)?;
    let file_id = json_str(&row, "fileId", "file_id", true)?;
    let access = json_str(&row, "access", "access", true)?;
    if user_id.is_empty() || file_id.is_empty() || access_rank(&access) == 0 {
        return None;
    }
    Some(UserFilePermission {
        id: json_str(&row, "id", "id", false).unwrap_or_default(),
        user_id,
        file_id,
        access,
        granted_by: json_str(&row, "grantedBy", "granted_by", false).unwrap_or_default(),
        granted_at: json_str(&row, "grantedAt", "granted_at", false).unwrap_or_default(),
    })
}

/// Read either spelling, rejecting conflicting aliases and non-string values.
/// Required fields cannot be absent; metadata fields may be absent or null.
fn json_str(value: &serde_json::Value, camel: &str, snake: &str, required: bool) -> Option<String> {
    let camel_value = value.get(camel);
    let snake_value = value.get(snake);
    let as_string = |field: Option<&serde_json::Value>| -> Option<Option<String>> {
        match field {
            None | Some(serde_json::Value::Null) => Some(None),
            Some(serde_json::Value::String(s)) => Some(Some(s.clone())),
            _ => None,
        }
    };
    let camel_text = as_string(camel_value)?;
    let snake_text = if snake == camel {
        camel_text.clone()
    } else {
        as_string(snake_value)?
    };
    match (camel_text, snake_text) {
        (Some(left), Some(right)) if left != right => None,
        (Some(text), _) | (_, Some(text)) => Some(text),
        (None, None) if required => None,
        (None, None) => Some(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cybermanju_types::schema::FileNode;

    fn temp_db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("permissions.redb").to_str().unwrap()).unwrap();
        (db, dir)
    }

    fn seed_user(db: &Database, id: &str, username: &str, role: &str, active: bool) {
        let user = User {
            id: id.to_string(),
            username: username.to_string(),
            password_hash: String::new(),
            display_name: None,
            role: role.to_string(),
            is_active: active,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_users_table()).unwrap();
            table
                .insert(id, serde_json::to_string(&user).unwrap().as_str())
                .unwrap();
        }
        tx.commit().unwrap();
    }

    fn seed_file(db: &Database, id: &str) {
        let node = FileNode {
            id: id.to_string(),
            name: "test.txt".to_string(),
            file_type: "file".to_string(),
            parent_id: None,
            size_bytes: 0,
            mime_type: None,
            hash_blake3: None,
            encrypted: false,
            encryption_algorithm: None,
            compression_layers: Vec::new(),
            thumbnail_path: None,
            created_at: "now".to_string(),
            modified_at: "now".to_string(),
            context_data: None,
            tags: Vec::new(),
            collection_ids: Vec::new(),
            face_group_ids: Vec::new(),
            loose_group_ids: Vec::new(),
            gps_lat: None,
            gps_lon: None,
        };
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_files_table()).unwrap();
            table
                .insert(id, serde_json::to_string(&node).unwrap().as_str())
                .unwrap();
        }
        tx.commit().unwrap();
    }

    fn claims(id: &str, username: &str, role: &str) -> Claims {
        Claims {
            sub: username.to_string(),
            user_id: id.to_string(),
            role: role.to_string(),
            iat: 1,
            exp: u64::MAX,
            jti: "test-jti".to_string(),
        }
    }

    fn seed_raw_permission(db: &Database, key: &str, raw: &str) {
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx
                .open_table(Database::get_user_file_perms_table())
                .unwrap();
            table.insert(key, raw).unwrap();
        }
        tx.commit().unwrap();
    }

    #[test]
    fn grant_replaces_prior_pair_and_records_real_actor() {
        let (db, _dir) = temp_db();
        seed_user(&db, "admin-id", "admin", "admin", true);
        seed_user(&db, "target-id", "target", "user", true);
        seed_file(&db, "file-id");
        let admin = claims("admin-id", "admin", "admin");
        assert!(
            crate::api::files::check_access(&db, &admin, "missing-file", "read")
                .unwrap_err()
                .starts_with("not_found:")
        );

        let tx = db.begin_write().unwrap();
        {
            let mut table = tx
                .open_table(Database::get_user_file_perms_table())
                .unwrap();
            table
                .insert(
                    "legacy",
                    r#"{"user_id":"target-id","file_id":"file-id","access":"admin"}"#,
                )
                .unwrap();
        }
        tx.commit().unwrap();

        let first = grant_permission(&db, &admin, "target-id", "file-id", "read").unwrap();
        let second = grant_permission(&db, &admin, "target-id", "file-id", "write").unwrap();
        assert_eq!(second.granted_by, "admin-id");
        assert_ne!(first.id, second.id);
        let rows = list_permissions_for_file(&db, &admin, "file-id").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].access, "write");
        assert!(verify_permission(&db, &admin, "target-id", "file-id", "read").unwrap());
        assert!(!verify_permission(&db, &admin, "target-id", "file-id", "admin").unwrap());
    }

    #[test]
    fn acl_helpers_deny_non_admin_grants_and_cross_user_introspection() {
        let (db, _dir) = temp_db();
        seed_user(&db, "member-id", "member", "user", true);
        seed_user(&db, "target-id", "target", "user", true);
        seed_file(&db, "file-id");
        let member = claims("member-id", "member", "user");
        assert!(grant_permission(&db, &member, "target-id", "file-id", "read").is_err());
        assert!(verify_permission(&db, &member, "target-id", "file-id", "read").is_err());
        assert!(grant_permission(&db, &member, "target-id", "file-id", "invalid").is_err());
    }

    #[test]
    fn malformed_and_ambiguous_legacy_grants_fail_closed() {
        for raw in [
            r#"{"userId":"a","user_id":"b","fileId":"file","access":"admin"}"#,
            r#"{"userId":"a","fileId":"file","file_id":"other","access":"admin"}"#,
            r#"{"userId":7,"fileId":"file","access":"admin"}"#,
            r#"{"userId":"a","fileId":"file","access":"owner"}"#,
            r#"{"userId":"","fileId":"file","access":"read"}"#,
        ] {
            assert!(parse_permission(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn grants_reject_inactive_targets() {
        let (db, _dir) = temp_db();
        seed_user(&db, "admin-id", "admin", "admin", true);
        seed_user(&db, "target-id", "target", "user", false);
        seed_file(&db, "file-id");
        let admin = claims("admin-id", "admin", "admin");
        assert!(grant_permission(&db, &admin, "target-id", "file-id", "read").is_err());
    }

    #[test]
    fn legacy_acl_casing_and_missing_metadata_are_shared_across_acl_operations() {
        let (db, _dir) = temp_db();
        seed_user(&db, "admin-id", "admin", "admin", true);
        seed_user(&db, "reader-id", "reader", "user", true);
        seed_file(&db, "camel-file");
        seed_file(&db, "snake-file");
        seed_raw_permission(
            &db,
            "legacy-camel",
            r#"{"userId":"reader-id","fileId":"camel-file","access":"read"}"#,
        );
        seed_raw_permission(
            &db,
            "legacy-snake",
            r#"{"user_id":"reader-id","file_id":"snake-file","access":"write"}"#,
        );

        let reader = claims("reader-id", "reader", "user");
        assert!(crate::api::files::check_access(&db, &reader, "camel-file", "read").is_ok());
        assert!(crate::api::files::check_access(&db, &reader, "snake-file", "write").is_ok());
        assert!(!crate::api::files::check_access(&db, &reader, "camel-file", "write").is_ok());
        assert!(verify_permission(&db, &reader, "reader-id", "camel-file", "read").unwrap());
        assert!(verify_permission(&db, &reader, "reader-id", "snake-file", "write").unwrap());

        let admin = claims("admin-id", "admin", "admin");
        let camel = list_permissions_for_file(&db, &admin, "camel-file").unwrap();
        let snake = list_permissions_for_file(&db, &admin, "snake-file").unwrap();
        assert_eq!(camel.len(), 1);
        assert_eq!(snake.len(), 1);
        for permission in [camel.first().unwrap(), snake.first().unwrap()] {
            assert_eq!(permission.user_id, "reader-id");
            assert!(permission.id.is_empty());
            assert!(permission.granted_by.is_empty());
            assert!(permission.granted_at.is_empty());
        }
    }
}
