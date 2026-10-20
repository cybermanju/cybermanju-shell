// CyberManju OS — Spine (crates/tests/src/spine.rs).
// ANALYSIS 100% pass: device identity (§2B.1), server todos (§4.9), op-log
// exchange (§2C) incl. resolve verdicts + mutation wiring, direct LAN peer
// sync (§2B.2 follow-up, registry-authenticated), LAN peers, sneakernet
// bundles (§2B.4) and the P0-6 no-row-token regression — one module pinning
// the transport: auth gates, body contracts, owner scoping, idempotent
// merges and error prefixes.

use crate::web::{
    bearer, body_of, bootstrap_session, call, login_session, mint, mk_dashboard, now_secs,
    status_of,
};

fn admin(d: &cybermanju_web::WebDashboard) -> String {
    bearer(&mint(d, "admin", now_secs() + 3600, "spine-admin"))
}

fn user(d: &cybermanju_web::WebDashboard) -> String {
    bearer(&mint(d, "user", now_secs() + 3600, "spine-user"))
}

fn signing_identity() -> (String, String) {
    use base64::Engine;
    let mut engine = cybermanju_crypto::pqc::PqcEngine::new();
    let keypair = engine
        .generate_keypair(cybermanju_crypto::pqc::EncryptionAlgo::MlDsa65)
        .expect("ML-DSA keygen");
    (
        base64::engine::general_purpose::STANDARD.encode(&keypair.private_key),
        base64::engine::general_purpose::STANDARD.encode(&keypair.public_key),
    )
}

fn enroll_signing_device(d: &cybermanju_web::WebDashboard, auth: &str, id: &str) -> String {
    let (seed, public_key) = signing_identity();
    let response = call(
        d,
        "POST",
        "/api/devices",
        &serde_json::json!({
            "id": id,
            "name": id,
            "publicKey": public_key,
            "keyAlgorithm": "ml-dsa-65"
        })
        .to_string(),
        Some(auth),
    );
    assert_eq!(status_of(&response), 200, "{response}");
    seed
}

fn op_row(
    device_id: &str,
    op_id: &str,
    kind: &str,
    file_id: &str,
) -> cybermanju_types::schema::OpRow {
    cybermanju_types::schema::OpRow {
        seq: 0,
        op_id: op_id.to_string(),
        device_id: device_id.to_string(),
        created_at: "2026-10-11T00:00:00Z".to_string(),
        kind: kind.to_string(),
        file_id: Some(file_id.to_string()),
        payload: serde_json::json!({}),
        bases: Default::default(),
    }
}

fn signed_envelope(device_id: &str, seed: &str, ops: &[cybermanju_types::schema::OpRow]) -> String {
    let signature =
        cybermanju_sync::auth::sign_envelope(seed, "ml-dsa-65", ops).expect("sign ML-DSA envelope");
    serde_json::json!({
        "deviceId": device_id,
        "ops": ops,
        "signature": signature,
        "algorithm": "ml-dsa-65"
    })
    .to_string()
}

#[test]
fn new_families_require_a_session() {
    let (_dir, d) = mk_dashboard(3456);
    for (method, path, body) in [
        ("GET", "/api/devices", ""),
        ("POST", "/api/devices", "{}"),
        ("GET", "/api/todos", ""),
        ("POST", "/api/todos", "{}"),
        ("GET", "/api/oplog/since", ""),
        ("POST", "/api/oplog/push", "{}"),
        ("GET", "/api/oplog/conflicts", ""),
        ("POST", "/api/oplog/resolve", "{}"),
        ("GET", "/api/peers", ""),
        ("POST", "/api/peers/ghost/sync", "{}"),
        ("GET", "/api/sync/bundle/export", ""),
        ("POST", "/api/sync/bundle/import", "{}"),
    ] {
        let resp = call(&d, method, path, body, None);
        assert_eq!(status_of(&resp), 401, "{method} {path}: {resp}");
    }
}

#[test]
fn devices_register_idempotently_and_revoke_admin_only() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let reg = call(
        &d,
        "POST",
        "/api/devices",
        r#"{"name":"laptop","publicKey":"QUJD","keyAlgorithm":"ml-dsa-65"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&reg), 200, "{reg}");
    let first: serde_json::Value = serde_json::from_str(body_of(&reg)).expect("json");
    let id = first["id"].as_str().expect("id").to_string();
    assert_eq!(first["revoked"], false);

    // Re-enrollment with the same id heartbeats instead of duplicating.
    let again = call(
        &d,
        "POST",
        "/api/devices",
        &format!(r#"{{"id":"{id}","name":"laptop-2"}}"#),
        Some(&auth),
    );
    assert_eq!(status_of(&again), 200, "{again}");
    let listed = call(&d, "GET", "/api/devices", "", Some(&auth));
    let rows: serde_json::Value = serde_json::from_str(body_of(&listed)).expect("json");
    assert_eq!(rows.as_array().map(|a| a.len()), Some(1));

    // Non-admins cannot revoke or delete.
    let plain = user(&d);
    let denied = call(
        &d,
        "POST",
        &format!("/api/devices/{id}/revoke"),
        "",
        Some(&plain),
    );
    assert_eq!(status_of(&denied), 403, "{denied}");
    let revoked = call(
        &d,
        "POST",
        &format!("/api/devices/{id}/revoke"),
        "",
        Some(&auth),
    );
    assert_eq!(status_of(&revoked), 200, "{revoked}");
    let row: serde_json::Value = serde_json::from_str(body_of(&revoked)).expect("json");
    assert_eq!(row["revoked"], true);

    let gone = call(&d, "DELETE", &format!("/api/devices/{id}"), "", Some(&auth));
    assert_eq!(status_of(&gone), 200, "{gone}");
    let missing = call(&d, "DELETE", &format!("/api/devices/{id}"), "", Some(&auth));
    assert_eq!(status_of(&missing), 404, "{missing}");
}

#[test]
fn todos_scope_to_owners_and_validate() {
    let (_dir, d) = mk_dashboard(3456);
    // Two ordinary users through an admin (minted ids collide per role).
    let admin_token = admin(&d);
    for user in ["alice", "bob"] {
        let created = call(
            &d,
            "POST",
            "/api/users",
            &format!(r#"{{"username":"{user}","password":"correct horse battery","role":"user"}}"#),
            Some(admin_token.as_str()),
        );
        assert_eq!(status_of(&created), 201, "{created}");
    }
    let alice = bearer(&login_session(&d, "alice", "correct horse battery"));
    let bob = bearer(&login_session(&d, "bob", "correct horse battery"));
    let created = call(
        &d,
        "POST",
        "/api/todos",
        r#"{"title":"Buy milk"}"#,
        Some(&alice),
    );
    assert_eq!(status_of(&created), 200, "{created}");
    let row: serde_json::Value = serde_json::from_str(body_of(&created)).expect("json");
    let id = row["id"].as_str().expect("id").to_string();

    let blank = call(&d, "POST", "/api/todos", r#"{"title":"  "}"#, Some(&alice));
    assert_eq!(status_of(&blank), 400, "{blank}");

    // Bob sees none of Alice's rows; touching them 404s (never 403-leaks).
    let bobs = call(&d, "GET", "/api/todos", "", Some(&bob));
    assert_eq!(body_of(&bobs), "[]", "cross-owner leak: {bobs}");
    let touch = call(
        &d,
        "PUT",
        &format!("/api/todos/{id}"),
        r#"{"done":true}"#,
        Some(&bob),
    );
    assert_eq!(status_of(&touch), 404, "{touch}");

    let done = call(
        &d,
        "PUT",
        &format!("/api/todos/{id}"),
        r#"{"done":true}"#,
        Some(&alice),
    );
    assert_eq!(status_of(&done), 200, "{done}");
    let row: serde_json::Value = serde_json::from_str(body_of(&done)).expect("json");
    assert_eq!(row["done"], true);

    let removed = call(&d, "DELETE", &format!("/api/todos/{id}"), "", Some(&alice));
    assert_eq!(status_of(&removed), 200, "{removed}");
}

#[test]
fn oplog_push_pull_and_conflicts() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let empty = call(&d, "GET", "/api/oplog/since", "", Some(&auth));
    assert_eq!(status_of(&empty), 200, "{empty}");
    assert_eq!(body_of(&empty), r#"{"head":0,"ops":[]}"#, "{empty}");

    let bad = call(
        &d,
        "POST",
        "/api/oplog/push",
        r#"{"ops":[{"kind":""}]}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&bad), 400, "{bad}");

    let seed1 = enroll_signing_device(&d, &auth, "d1");
    let seed2 = enroll_signing_device(&d, &auth, "d2");
    let op1 = op_row("d1", "d1-0", "tag", "f1");
    let op2 = op_row("d2", "d2-0", "rename", "f1");
    let unsigned = call(
        &d,
        "POST",
        "/api/oplog/push",
        &serde_json::json!({ "deviceId": "d1", "ops": [op1.clone()], "algorithm": "ml-dsa-65" })
            .to_string(),
        Some(&auth),
    );
    assert_eq!(status_of(&unsigned), 403, "{unsigned}");
    assert!(body_of(&unsigned).contains("auth:"), "{unsigned}");

    let push1 = call(
        &d,
        "POST",
        "/api/oplog/push",
        &signed_envelope("d1", &seed1, std::slice::from_ref(&op1)),
        Some(&auth),
    );
    assert_eq!(status_of(&push1), 200, "{push1}");
    let report: serde_json::Value = serde_json::from_str(body_of(&push1)).expect("json");
    assert_eq!(report["appended"], 1);
    assert_eq!(report["verified"], true);

    let push2 = call(
        &d,
        "POST",
        "/api/oplog/push",
        &signed_envelope("d2", &seed2, &[op2]),
        Some(&auth),
    );
    assert_eq!(status_of(&push2), 200, "{push2}");

    // Re-push is idempotent.
    let again = call(
        &d,
        "POST",
        "/api/oplog/push",
        &signed_envelope("d1", &seed1, &[op1]),
        Some(&auth),
    );
    assert_eq!(status_of(&again), 200, "{again}");
    let report: serde_json::Value = serde_json::from_str(body_of(&again)).expect("json");
    assert_eq!(
        (report["appended"].as_u64(), report["skipped"].as_u64()),
        (Some(0), Some(1))
    );

    let page = call(
        &d,
        "GET",
        "/api/oplog/since?since=0&limit=500",
        "",
        Some(&auth),
    );
    let page: serde_json::Value = serde_json::from_str(body_of(&page)).expect("json");
    assert_eq!(page["ops"].as_array().map(|a| a.len()), Some(2));

    // Same file, two devices, neither vector dominates → one fork pair.
    let conflicts = call(&d, "GET", "/api/oplog/conflicts", "", Some(&auth));
    let conflicts: serde_json::Value = serde_json::from_str(body_of(&conflicts)).expect("json");
    assert_eq!(conflicts["conflicts"].as_array().map(|a| a.len()), Some(1));
}

#[test]
fn oplog_resolve_appends_a_verdict_without_deleting_history() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let seed1 = enroll_signing_device(&d, &auth, "r1");
    let seed2 = enroll_signing_device(&d, &auth, "r2");
    let first = call(
        &d,
        "POST",
        "/api/oplog/push",
        &signed_envelope("r1", &seed1, &[op_row("r1", "r1-0", "rename", "f9")]),
        Some(&auth),
    );
    assert_eq!(status_of(&first), 200, "{first}");
    let second = call(
        &d,
        "POST",
        "/api/oplog/push",
        &signed_envelope("r2", &seed2, &[op_row("r2", "r2-0", "rename", "f9")]),
        Some(&auth),
    );
    assert_eq!(status_of(&second), 200, "{second}");
    // Unknown choices and ghost ops refuse honestly.
    let bad_choice = call(
        &d,
        "POST",
        "/api/oplog/resolve",
        r#"{"opA":"r1-0","opB":"r2-0","choice":"delete"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&bad_choice), 400, "{bad_choice}");
    let ghost = call(
        &d,
        "POST",
        "/api/oplog/resolve",
        r#"{"opA":"nope","opB":"r2-0","choice":"merge"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&ghost), 404, "{ghost}");

    let done = call(
        &d,
        "POST",
        "/api/oplog/resolve",
        r#"{"opA":"r1-0","opB":"r2-0","choice":"keep-a","deviceId":"r1"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&done), 200, "{done}");
    let verdict: serde_json::Value = serde_json::from_str(body_of(&done)).expect("json");
    assert_eq!(verdict["resolved"], true);
    assert_eq!(verdict["op"]["kind"], "resolve");
    assert_eq!(verdict["op"]["payload"]["choice"], "keep-a");
    // Both forks survive under the verdict — resolution never deletes.
    let page = call(
        &d,
        "GET",
        "/api/oplog/since?since=0&limit=500",
        "",
        Some(&auth),
    );
    let page: serde_json::Value = serde_json::from_str(body_of(&page)).expect("json");
    assert_eq!(page["ops"].as_array().map(|a| a.len()), Some(3));
}

#[test]
fn mutations_append_ops_for_queue_and_replay() {
    // §2C rollout: a todo write (REST, Tauri twin and agent tool share the
    // `db.save_todo` chokepoint) appends a `todo` op for offline replay.
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let created = call(
        &d,
        "POST",
        "/api/todos",
        r#"{"title":"queue me"}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&created), 200, "{created}");
    let page = call(
        &d,
        "GET",
        "/api/oplog/since?since=0&limit=500",
        "",
        Some(&auth),
    );
    let page: serde_json::Value = serde_json::from_str(body_of(&page)).expect("json");
    let ops = page["ops"].as_array().expect("ops");
    assert!(
        ops.iter().any(|op| op["kind"] == "todo"),
        "todo write appended no op: {page}"
    );
}

#[test]
fn peer_sync_is_peer_authenticated() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let plain = user(&d);
    // Unknown peers never merge.
    let ghost = call(
        &d,
        "POST",
        "/api/peers/ghost/sync",
        r#"{"ops":[]}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&ghost), 404, "{ghost}");
    assert!(body_of(&ghost).contains("not_found:"), "{ghost}");

    // Enroll a device with a real keypair, then reject unsigned and
    // cross-owner requests before accepting its verified envelope.
    let seed = enroll_signing_device(&d, &auth, "phone");
    let op = op_row("phone", "p-0", "star", "f3");
    let unsigned = call(
        &d,
        "POST",
        "/api/peers/phone/sync",
        r#"{"ops":[]}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&unsigned), 403, "{unsigned}");
    assert!(body_of(&unsigned).contains("auth:"), "{unsigned}");

    let wrong_owner = call(
        &d,
        "POST",
        "/api/peers/phone/sync",
        &signed_envelope("phone", &seed, std::slice::from_ref(&op)),
        Some(&plain),
    );
    assert_eq!(status_of(&wrong_owner), 403, "{wrong_owner}");

    let ok = call(
        &d,
        "POST",
        "/api/peers/phone/sync",
        &signed_envelope("phone", &seed, &[op]),
        Some(&auth),
    );
    assert_eq!(status_of(&ok), 200, "{ok}");
    let report: serde_json::Value = serde_json::from_str(body_of(&ok)).expect("json");
    assert_eq!(report["appended"], 1);
    assert_eq!(report["skipped"], 0);
    assert!(report["head"].as_u64().is_some(), "{report}");
    assert_eq!(report["verified"], true);

    call(&d, "POST", "/api/devices/phone/revoke", "", Some(&auth));
    let shut = call(
        &d,
        "POST",
        "/api/peers/phone/sync",
        r#"{"ops":[]}"#,
        Some(&auth),
    );
    assert_eq!(status_of(&shut), 403, "{shut}");
    assert!(body_of(&shut).contains("auth:"), "{shut}");
}

#[test]
fn relay_mailboxes_are_owner_scoped() {
    let (_dir, d) = mk_dashboard(3456);
    let admin = admin(&d);
    let plain = user(&d);
    enroll_signing_device(&d, &admin, "relay-sender");
    enroll_signing_device(&d, &admin, "relay-receiver");
    enroll_signing_device(&d, &plain, "other-owner-device");

    let cross_owner = call(
        &d,
        "POST",
        "/api/relay/drop",
        r#"{"from":"relay-sender","to":"other-owner-device","kind":"offer","payload":{}}"#,
        Some(&admin),
    );
    assert_eq!(status_of(&cross_owner), 403, "{cross_owner}");
    assert!(body_of(&cross_owner).contains("auth:"), "{cross_owner}");

    let accepted = call(
        &d,
        "POST",
        "/api/relay/drop",
        r#"{"from":"relay-sender","to":"relay-receiver","kind":"offer","payload":{}}"#,
        Some(&admin),
    );
    assert_eq!(status_of(&accepted), 200, "{accepted}");

    let cross_poll = call(
        &d,
        "POST",
        "/api/relay/poll",
        r#"{"for":"other-owner-device"}"#,
        Some(&admin),
    );
    assert_eq!(status_of(&cross_poll), 403, "{cross_poll}");

    let poll = call(
        &d,
        "POST",
        "/api/relay/poll",
        r#"{"for":"relay-receiver"}"#,
        Some(&admin),
    );
    assert_eq!(status_of(&poll), 200, "{poll}");
    assert!(body_of(&poll).contains("offer"), "{poll}");
}

#[test]
fn bundle_export_import_round_trips_admin_only() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let plain = user(&d);
    let denied = call(
        &d,
        "GET",
        "/api/sync/bundle/export?since=0",
        "",
        Some(&plain),
    );
    assert_eq!(status_of(&denied), 403, "{denied}");

    let seed = enroll_signing_device(&d, &auth, "d9");
    let push = call(
        &d,
        "POST",
        "/api/oplog/push",
        &signed_envelope("d9", &seed, &[op_row("d9", "d9-0", "star", "f2")]),
        Some(&auth),
    );
    assert_eq!(status_of(&push), 200, "{push}");
    let export = call(
        &d,
        "GET",
        "/api/sync/bundle/export?since=0&deviceId=d9",
        "",
        Some(&auth),
    );
    assert_eq!(status_of(&export), 200, "{export}");
    let bundle: serde_json::Value = serde_json::from_str(body_of(&export)).expect("json");
    assert_eq!(bundle["format"], "cyb-bundle/1");

    // Tampered bytes fail closed with integrity:, never merge.
    let tampered = body_of(&export).replace("\"kind\":\"star\"", "\"kind\":\"delete\"");
    let bad = call(
        &d,
        "POST",
        "/api/sync/bundle/import",
        &tampered,
        Some(&auth),
    );
    assert_eq!(status_of(&bad), 409, "{bad}");
    assert!(body_of(&bad).contains("integrity:"), "{bad}");
}

#[test]
fn peers_answers_200_with_an_array() {
    // CI sandboxes rarely have mDNS peers; the contract is the shape —
    // 200 + array (possibly empty), never a refusal or a hang.
    let (_dir, d) = mk_dashboard(3456);
    let auth = admin(&d);
    let resp = call(&d, "GET", "/api/peers", "", Some(&auth));
    assert_eq!(status_of(&resp), 200, "{resp}");
    assert!(body_of(&resp).starts_with('['), "{resp}");
}

#[test]
fn sync_config_tokens_never_reach_rows_or_clients() {
    // P0-6 regression: the request token lands in the side table only.
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "cfgowner", "correct horse battery");
    let auth = bearer(&token);
    let saved = call(
        &d,
        "POST",
        "/api/sync/configs",
        r#"{"config":{"id":"","name":"vault","backendType":"local","token":"sekret-token"}}}"#,
        Some(&auth),
    );
    assert!(
        status_of(&saved) == 200 || status_of(&saved) == 400,
        "{saved}"
    );
    if status_of(&saved) == 200 {
        assert!(
            !body_of(&saved).contains("sekret-token"),
            "token echoed: {saved}"
        );
        let listed = call(&d, "GET", "/api/sync/configs", "", Some(&auth));
        assert!(
            !body_of(&listed).contains("sekret-token"),
            "token listed: {listed}"
        );
    }
}
