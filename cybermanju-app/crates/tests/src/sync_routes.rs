// CyberManju OS — Sync routes (crates/tests/src/sync_routes.rs).
// AGENT-4 item 9 — sync job REST contract (AGENT-2's engine, my status codes).
//
// These tests do not start a sync run: they pin the transport — auth gate,
// body contract, config CRUD and the fact that a run without a config fails
// as a 4xx instead of taking the engine down.

use crate::web::{bearer, body_of, call, mint, mk_dashboard, now_secs, status_of};

const SYNC_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/sync/status"),
    ("GET", "/api/sync/progress"),
    ("GET", "/api/sync/configs"),
    ("POST", "/api/sync/cancel"),
    ("POST", "/api/sync/start"),
    ("POST", "/api/sync/test"),
    ("POST", "/api/sync/remote-files"),
];

#[test]
fn sync_routes_require_a_session() {
    let (_dir, d) = mk_dashboard(3456);
    for (method, path) in SYNC_ROUTES {
        let resp = call(&d, method, path, "{}", None);
        assert_eq!(status_of(&resp), 401, "{method} {path}: {resp}");
    }
}

#[test]
fn sync_status_and_progress_answer_without_a_provider() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-sync-read"));

    let status = call(&d, "GET", "/api/sync/status", "", Some(auth.as_str()));
    assert_eq!(status_of(&status), 200, "{status}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&status)).expect("status json");
    assert_eq!(payload["syncEnabled"], true);
    assert!(payload["status"].is_string(), "{payload}");
    assert!(
        payload["lastSync"].is_null() || payload["lastSync"].is_string(),
        "{payload}"
    );

    let progress = call(&d, "GET", "/api/sync/progress", "", Some(auth.as_str()));
    assert_eq!(status_of(&progress), 200, "{progress}");
    let payload: serde_json::Value =
        serde_json::from_str(body_of(&progress)).expect("progress json");
    assert!(payload.is_object(), "{payload}");

    let configs = call(&d, "GET", "/api/sync/configs", "", Some(auth.as_str()));
    assert_eq!(status_of(&configs), 200, "{configs}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&configs)).expect("configs json");
    assert!(payload.is_array(), "{payload}");
    assert_eq!(payload.as_array().expect("array").len(), 0, "{payload}");
}

#[test]
fn sync_jobs_reject_bodies_that_do_not_match_the_contract() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-sync-body"));

    for path in [
        "/api/sync/start",
        "/api/sync/test",
        "/api/sync/remote-files",
    ] {
        let missing_fields = call(&d, "POST", path, "{}", Some(auth.as_str()));
        assert_eq!(status_of(&missing_fields), 400, "{path}: {missing_fields}");

        let malformed = call(&d, "POST", path, "not json", Some(auth.as_str()));
        assert_eq!(status_of(&malformed), 400, "{path}: {malformed}");
    }
}

#[test]
fn sync_cancel_is_idempotent_when_nothing_is_running() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-sync-cancel"));

    for _ in 0..2 {
        let resp = call(&d, "POST", "/api/sync/cancel", "", Some(auth.as_str()));
        assert_eq!(status_of(&resp), 200, "{resp}");
    }
}

#[test]
fn sync_config_round_trip_never_returns_a_provider_token() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-sync-config"));

    // A raw token is accepted on the way in (it is a credential, not a
    // display field) and must never come back out. The body is the
    // `ConfigRequest { config }` envelope the route deserializes — the same
    // shape `/api/sync/test` and `/api/sync/remote-files` use.
    let config = r#"{
        "config": {
            "id": "",
            "backendType": "local",
            "enabled": true,
            "name": "test-config",
            "basePath": "/tmp/cybermanju-sync-test",
            "autoSync": false,
            "compressBeforeUpload": false,
            "createPreviews": false,
            "deleteRawAfterSync": false,
            "maxConcurrentUploads": 1,
            "token": "super-secret-provider-token"
        }
    }"#;

    let created = call(&d, "POST", "/api/sync/configs", config, Some(auth.as_str()));
    assert_eq!(status_of(&created), 200, "{created}");
    assert!(
        !created.contains("super-secret-provider-token"),
        "{created}"
    );
    let saved: serde_json::Value = serde_json::from_str(body_of(&created)).expect("config json");
    let config_id = saved["id"].as_str().expect("config id").to_string();
    assert!(!config_id.is_empty(), "{saved}");
    assert_eq!(saved["name"], "test-config");

    let listed = call(&d, "GET", "/api/sync/configs", "", Some(auth.as_str()));
    assert_eq!(status_of(&listed), 200, "{listed}");
    assert!(!listed.contains("super-secret-provider-token"), "{listed}");
    let rows: serde_json::Value = serde_json::from_str(body_of(&listed)).expect("configs json");
    assert_eq!(rows.as_array().expect("array").len(), 1, "{rows}");

    // Deleting a provider binding is admin-only.
    let denied = call(
        &d,
        "DELETE",
        &format!("/api/sync/configs/{config_id}"),
        "",
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&denied), 403, "{denied}");

    let admin = bearer(&mint(
        &d,
        "admin",
        now_secs() + 3_600,
        "jti-sync-config-admin",
    ));
    let removed = call(
        &d,
        "DELETE",
        &format!("/api/sync/configs/{config_id}"),
        "",
        Some(admin.as_str()),
    );
    assert_eq!(status_of(&removed), 200, "{removed}");

    let after = call(&d, "GET", "/api/sync/configs", "", Some(auth.as_str()));
    let rows: serde_json::Value = serde_json::from_str(body_of(&after)).expect("configs json");
    assert_eq!(rows.as_array().expect("array").len(), 0, "{rows}");
}
