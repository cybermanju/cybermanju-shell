// CyberManju OS — HTTP authorization regression tests.
// Role: exercise the shared router rather than static source assertions.
// Integrates: cybermanju-web, redb, jsonwebtoken; no network/provider calls.
// Contracts: grants are caller-authorized and stale tokens fail closed.
use cybermanju_db::Database;
use cybermanju_web::{handle_request, security::Claims, WebDashboard};
use serde_json::{json, Value};

fn dashboard() -> (tempfile::TempDir, WebDashboard) {
    let dir = tempfile::tempdir().unwrap();
    let d = WebDashboard::new(0, dir.path().join("test.redb").to_str().unwrap());
    (dir, d)
}

fn user(d: &WebDashboard, id: &str, role: &str) -> String {
    let db = d.db.read().unwrap();
    let tx = db.begin_write().unwrap();
    {
        let mut table = tx.open_table(Database::get_users_table()).unwrap();
        let row = json!({"id": id, "username": id, "passwordHash": "unused",
            "role": role, "isActive": true, "displayName": null,
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"});
        table.insert(id, row.to_string().as_str()).unwrap();
    }
    tx.commit().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let claims = Claims {
        sub: id.into(),
        user_id: id.into(),
        role: role.into(),
        iat: now,
        exp: now + 3600,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&d.jwt_secret),
    )
    .unwrap();
    format!("Bearer {token}")
}

fn file(d: &WebDashboard, id: &str, name: &str) {
    let db = d.db.read().unwrap();
    let tx = db.begin_write().unwrap();
    {
        let mut table = tx.open_table(Database::get_files_table()).unwrap();
        let row = json!({"id": id, "name": name, "fileType": "file", "parentId": null,
            "sizeBytes": 0, "mimeType": "text/plain", "hashBlake3": null,
            "encrypted": false, "encryptionAlgorithm": null, "compressionLayers": [],
            "thumbnailPath": null, "contextData": null, "tags": [], "collectionIds": [],
            "faceGroupIds": [], "looseGroupIds": [], "gpsLat": 1.0, "gpsLon": 2.0,
            "createdAt": "2026-01-01T00:00:00Z", "modifiedAt": "2026-01-01T00:00:00Z"});
        table.insert(id, row.to_string().as_str()).unwrap();
    }
    tx.commit().unwrap();
}

fn request(d: &WebDashboard, auth: &str, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = handle_request(d, &d.db, method, path, &body.to_string(), Some(auth), None);
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    (status, body)
}

#[test]
fn cannot_self_grant_and_lists_and_search_obey_acl() {
    let (_dir, d) = dashboard();
    let admin = user(&d, "admin", "admin");
    let alice = user(&d, "alice", "user");
    file(&d, "visible", "needle-public.txt");
    file(&d, "private", "needle-secret.txt");
    let grant = json!({"userId": "alice", "fileId": "visible", "access": "read"});
    assert_eq!(
        request(&d, &alice, "POST", "/api/permissions", grant.clone()).0,
        403
    );
    assert_eq!(
        request(&d, &admin, "POST", "/api/permissions", grant).0,
        201
    );
    for path in ["/api/files", "/api/geo-files", "/api/search?q=needle"] {
        let (status, body) = request(&d, &alice, "GET", path, Value::Null);
        assert_eq!(status, 200, "{path}: {body}");
        let rows = body.as_array().unwrap();
        assert_eq!(rows.len(), 1, "{path}: {body}");
        assert!(
            !body.to_string().contains("needle-secret"),
            "{path}: {body}"
        );
    }
    let (status, body) = request(
        &d,
        &alice,
        "GET",
        "/api/search/paginated?q=needle&limit=1",
        Value::Null,
    );
    assert_eq!(status, 200);
    assert_eq!(body["total"], 1);
    assert_eq!(body["results"].as_array().unwrap().len(), 1);
    let (status, body) = request(
        &d,
        &alice,
        "GET",
        "/api/search/suggest?q=needle",
        Value::Null,
    );
    assert_eq!(status, 200);
    assert!(!body.to_string().contains("secret"));
    assert_eq!(
        request(&d, &alice, "GET", "/api/permissions/private", Value::Null).0,
        403
    );
    // A readable file does not convey authority to grant stronger access.
    assert_eq!(
        request(
            &d,
            &alice,
            "POST",
            "/api/permissions",
            json!({"userId": "alice", "fileId": "visible", "access": "admin"})
        )
        .0,
        403
    );
}

#[test]
fn tokens_stop_working_after_role_change_or_deletion() {
    let (_dir, d) = dashboard();
    let old_admin = user(&d, "admin", "admin");
    assert_eq!(
        request(&d, &old_admin, "GET", "/api/files", Value::Null).0,
        200
    );
    {
        let db = d.db.read().unwrap();
        cybermanju_web::api::users::update_role(&db, "admin", "user".into()).unwrap();
    }
    assert_eq!(
        request(&d, &old_admin, "GET", "/api/files", Value::Null).0,
        401
    );
    let current = user(&d, "admin", "user");
    {
        let db = d.db.read().unwrap();
        cybermanju_web::api::users::delete(&db, "admin").unwrap();
    }
    assert_eq!(
        request(&d, &current, "GET", "/api/files", Value::Null).0,
        401
    );
}

#[test]
fn privileged_configs_are_denied_before_provider_or_mcp_execution() {
    let (_dir, d) = dashboard();
    let token = user(&d, "member", "user");
    {
        let db = d.db.read().unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_agent_configs_table()).unwrap();
            let config = json!({
                "id": "privileged", "name": "Privileged", "providerId": "openai",
                "model": "gpt-4.1-mini", "autoApprove": true
            });
            table
                .insert("privileged", config.to_string().as_str())
                .unwrap();
        }
        tx.commit().unwrap();
    }
    for (method, path, body) in [
        (
            "POST",
            "/api/agent/prompt",
            json!({"configId": "privileged", "prompt": "hello"}),
        ),
        ("POST", "/api/agent/init", json!({"configId": "privileged"})),
        (
            "GET",
            "/api/agent/configs/privileged/mcp/tools",
            Value::Null,
        ),
        (
            "POST",
            "/api/agent/sessions/session/compact",
            json!({"configId": "privileged"}),
        ),
    ] {
        let (status, body) = request(&d, &token, method, path, body);
        assert_eq!(status, 403, "{path}: {body}");
        assert!(
            body.to_string().contains("privileged agent configuration"),
            "{body}"
        );
    }
}

#[test]
fn backend_keypair_provisioning_is_admin_only() {
    let (_dir, d) = dashboard();
    let token = user(&d, "member", "user");
    let (status, body) = request(
        &d,
        &token,
        "POST",
        "/api/encryption/keys",
        json!({"algorithm": "hybrid"}),
    );
    assert_eq!(status, 403, "{body}");
}

#[test]
fn malformed_encryption_key_records_are_not_echoed_as_raw_secrets() {
    let (_dir, d) = dashboard();
    let token = user(&d, "admin", "admin");
    {
        let db = d.db.read().unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx
                .open_table(Database::get_encryption_keys_table())
                .unwrap();
            table
                .insert("broken", "{private_key: MUST-NOT-LEAK}")
                .unwrap();
        }
        tx.commit().unwrap();
    }
    let (status, body) = request(&d, &token, "GET", "/api/encryption/keys", Value::Null);
    assert_eq!(status, 409, "{body}");
    assert!(!body.to_string().contains("MUST-NOT-LEAK"), "{body}");
}
