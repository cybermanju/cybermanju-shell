// CyberManju OS — Editor (crates/tests/src/editor.rs).
// Editor + code-parse REST contract.
//
// The VS-like editor reads and writes managed file bytes over REST
// (`GET|PUT /api/files/{id}/content`) and parses text on every transport
// (`POST /api/code/parse`). Bytes live at `context_data.original_path`,
// seeded here directly since REST cannot mint byte-backed nodes.

use crate::web::{
    bearer, body_of, bootstrap_session, call, grant_file_access, mk_dashboard, status_of,
};
use cybermanju_types::schema::FileNode;
use cybermanju_web::WebDashboard;
use std::sync::Arc;

fn seed_file(
    d: &Arc<WebDashboard>,
    dir: &tempfile::TempDir,
    id: &str,
    name: &str,
    bytes: &[u8],
    encrypted: bool,
) {
    let path = dir.path().join(format!("{id}.txt"));
    std::fs::write(&path, bytes).expect("fixture");
    let node = FileNode {
        id: id.to_string(),
        name: name.to_string(),
        file_type: "file".to_string(),
        parent_id: None,
        size_bytes: bytes.len() as u64,
        mime_type: Some("text/plain".to_string()),
        hash_blake3: None,
        encrypted,
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
    };
    let guard = d.db.read().expect("db lock");
    let tx = guard.begin_write().expect("write tx");
    {
        let mut table = tx
            .open_table(cybermanju_db::Database::get_files_table())
            .expect("files table");
        table
            .insert(id, serde_json::to_string(&node).expect("json").as_str())
            .expect("insert");
    }
    tx.commit().expect("commit");
}

#[test]
fn content_endpoints_are_auth_gated() {
    let (_dir, d) = mk_dashboard(3456);
    let denied = call(&d, "GET", "/api/files/anything/content", "", None);
    assert_eq!(status_of(&denied), 401, "{denied}");
    let denied = call(
        &d,
        "PUT",
        "/api/files/anything/content",
        r#"{"content":"x"}"#,
        None,
    );
    assert_eq!(status_of(&denied), 401, "{denied}");
    let denied = call(
        &d,
        "POST",
        "/api/code/parse",
        r#"{"fileName":"a.rs","content":"fn a() {}"}"#,
        None,
    );
    assert_eq!(status_of(&denied), 401, "{denied}");
}

#[test]
fn content_round_trip_versions_the_save() {
    let (dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "ed", "correct horse battery");
    let auth = bearer(&token);
    seed_file(&d, &dir, "f1", "a.rs", b"fn a() {}", false);
    // P0-1: file bytes need an object grant — no grant means 403.
    grant_file_access(&d, &auth, "ed", "f1", "write");

    let got = call(&d, "GET", "/api/files/f1/content", "", Some(&auth));
    assert_eq!(status_of(&got), 200, "{got}");
    let body: serde_json::Value = serde_json::from_str(body_of(&got)).expect("json");
    assert_eq!(body["content"], "fn a() {}");
    assert_eq!(body["truncated"], false);

    let saved = call(
        &d,
        "PUT",
        "/api/files/f1/content",
        r#"{"content":"fn b() {}"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&saved), 200, "{saved}");
    let saved: serde_json::Value = serde_json::from_str(body_of(&saved)).expect("json");
    assert_eq!(saved["sizeBytes"], 9);

    let got = call(&d, "GET", "/api/files/f1/content", "", Some(&auth));
    let body: serde_json::Value = serde_json::from_str(body_of(&got)).expect("json");
    assert_eq!(body["content"], "fn b() {}");

    // Every overwrite snapshots a version first — saves stay undoable.
    let versions = call(&d, "GET", "/api/files/f1/versions", "", Some(&auth));
    assert_eq!(status_of(&versions), 200, "{versions}");
    let versions: serde_json::Value = serde_json::from_str(body_of(&versions)).expect("json");
    assert!(
        versions.as_array().map(|v| !v.is_empty()).unwrap_or(false),
        "{versions}"
    );
}

#[test]
fn content_refuses_honestly() {
    let (dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "ed2", "correct horse battery");
    let auth = bearer(&token);
    seed_file(&d, &dir, "enc", "s.rs", b"ciphertext", true);
    seed_file(&d, &dir, "bin", "b.bin", &[0xff, 0xfe, 0x00], false);
    // P0-1: reach the content layer through object grants so the refusal
    // prefixes below are exercised (without grants these are 403).
    grant_file_access(&d, &auth, "ed2", "enc", "write");
    grant_file_access(&d, &auth, "ed2", "bin", "read");

    let resp = call(&d, "GET", "/api/files/enc/content", "", Some(&auth));
    assert_eq!(status_of(&resp), 400, "{resp}");
    assert!(body_of(&resp).contains("encrypted:"), "{resp}");

    let resp = call(&d, "GET", "/api/files/bin/content", "", Some(&auth));
    assert_eq!(status_of(&resp), 400, "{resp}");
    assert!(body_of(&resp).contains("binary:"), "{resp}");

    let resp = call(&d, "GET", "/api/files/missing/content", "", Some(&auth));
    assert_eq!(status_of(&resp), 404, "{resp}");

    let big = "x".repeat(1024 * 1024 + 1);
    let resp = call(
        &d,
        "PUT",
        "/api/files/enc/content",
        &format!(r#"{{"content":"{big}"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 400, "{resp}");
    assert!(body_of(&resp).contains("too_large:"), "{resp}");
}

#[test]
fn code_parse_serves_shared_shape_on_every_transport_shape() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "ed3", "correct horse battery");
    let auth = bearer(&token);

    let resp = call(
        &d,
        "POST",
        "/api/code/parse",
        r#"{"fileName":"main.rs","content":"fn alpha() {}\nstruct Beta;\n"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&resp), 200, "{resp}");
    let body: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    assert_eq!(body["language"], "rust");
    assert_eq!(body["engine"], "heuristic");
    let names: Vec<&str> = body["symbols"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert!(names.contains(&"alpha"), "{names:?}");

    let resp = call(&d, "POST", "/api/code/parse", "{}", Some(&auth));
    assert_eq!(status_of(&resp), 400, "{resp}");
}
