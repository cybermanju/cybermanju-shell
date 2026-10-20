// CyberManju OS — AGENT-7 durability & repair tests
//
// Pre-created and registered by the supervisor. Acceptance lives in
// `scripts/os-acceptance.sh` Tier 1; unit/contract cases belong here.

use cybermanju_erasure::{decode, encode};
use cybermanju_sync::lease;

fn temp_db() -> (cybermanju_db::Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("repair-tests.redb");
    let db = cybermanju_db::Database::new(path.to_str().expect("path")).expect("db");
    (db, dir)
}

#[test]
fn default_parity_survives_one_provider_loss() {
    // The shipped default is `parity: 1` — a config written before RS landed
    // must still mean "tolerate one loss".
    assert_eq!(cybermanju_erasure::DEFAULT_PARITY, 1);
}

#[test]
fn reed_solomon_rebuilds_after_two_shard_losses() {
    // k=4 data + m=2 parity: any four shards carry the whole chunk, so a
    // repair can restore a striped file after two providers have vanished.
    let payload = b"cybermanju chunk payload that must come back byte-identical";
    let encoded = encode(payload, 4, 2).expect("encode");
    assert_eq!(encoded.shards.len(), 6);

    let present: Vec<(u8, Vec<u8>)> = encoded
        .shards
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 1 && *i != 4) // lose two shards
        .map(|(i, shard)| (i as u8, shard.clone()))
        .collect();
    let rebuilt = decode(4, 2, payload.len() as u64, &present).expect("decode");
    assert_eq!(rebuilt, payload);
}

#[test]
fn too_many_shard_losses_is_an_integrity_error() {
    // The same failure must never look like success: m=1 can only absorb one
    // loss, so the decode errors with the shared `integrity:` prefix.
    let encoded = encode(b"payload", 4, 1).expect("encode");
    let present: Vec<(u8, Vec<u8>)> = encoded
        .shards
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 0 && *i != 1)
        .map(|(i, shard)| (i as u8, shard.clone()))
        .collect();
    let err = decode(4, 1, 7, &present).expect_err("must fail");
    assert!(err.to_string().starts_with("integrity:"), "{}", err);
}

#[test]
fn an_expired_lease_is_stolen_not_silently_taken() {
    // Two writers, one volume: a live lease is a conflict, and the moment its
    // TTL passes the next acquirer records who it took the volume from.
    let (db, _dir) = temp_db();

    lease::acquire_lease_in(&db, "writer-a", "volume", 30).expect("a acquires");
    let conflict = lease::acquire_lease_in(&db, "writer-b", "volume", 30)
        .expect_err("b must not take a live lease");
    assert!(conflict.starts_with("conflict:"), "{}", conflict);

    // ttl = 0 → the lease is dead on arrival, so b steals it immediately.
    lease::acquire_lease_in(&db, "writer-a", "temp", 0).expect("a takes temp");
    let stolen = lease::acquire_lease_in(&db, "writer-b", "temp", 30).expect("b steals");
    assert_eq!(
        stolen.stolen_from.as_deref(),
        Some("writer-a"),
        "{:?}",
        stolen
    );

    // The old holder cannot release a lease it no longer owns.
    let release = lease::release_lease(&db, "writer-a", "temp").expect_err("not yours");
    assert!(release.starts_with("conflict:"), "{}", release);
    assert!(
        lease::release_lease(&db, "writer-b", "temp").expect("release"),
        "true"
    );
    assert!(
        lease::inspect_lease(&db, "temp")
            .expect("inspect")
            .is_none(),
        "released lease is gone"
    );
}

#[test]
fn repair_status_route_answers_200_with_camel_case_json() {
    // The REST contract: `GET /api/repair/status` is wired through
    // `repair_api::route` and answers with the status document.
    let (db, _dir) = temp_db();
    let response =
        cybermanju_web::api::repair_api::route(&db, "GET", &["api", "repair", "status"], "", None)
            .expect("owned path must be handled");
    assert!(response.starts_with("HTTP/1.1 200"), "{}", &response[..60]);
    let body = response.split("\r\n\r\n").nth(1).expect("body");
    let doc: serde_json::Value = serde_json::from_str(body).expect("json");
    assert!(doc.get("queuedFindings").is_some(), "{}", body);
    assert!(doc.get("repairs").is_some(), "{}", body);
}

#[test]
fn repair_routes_reject_the_wrong_method_and_foreign_paths() {
    let (db, _dir) = temp_db();
    // Wrong verb on our own path → 405, never a silent pass-through.
    let response =
        cybermanju_web::api::repair_api::route(&db, "GET", &["api", "lease", "acquire"], "", None)
            .expect("owned path");
    assert!(response.starts_with("HTTP/1.1 405"), "{}", &response[..40]);
    // Someone else's path → not ours, the router keeps looking.
    assert!(
        cybermanju_web::api::repair_api::route(&db, "GET", &["api", "files"], "", None).is_none(),
        "foreign paths fall through"
    );
}

#[test]
fn durability_routes_are_auth_gated_and_then_work_end_to_end() {
    // Through the shared router, not the raw handler: 401 without a session,
    // the real document with one, and the lease round trip it protects.
    let (_dir, d) = crate::web::mk_dashboard(0);

    let denied = crate::web::call(&d, "GET", "/api/repair/status", "", None);
    assert_eq!(crate::web::status_of(&denied), 401, "{denied}");
    let denied_scrub = crate::web::call(&d, "GET", "/api/scrub/runs", "", None);
    assert_eq!(crate::web::status_of(&denied_scrub), 401, "{denied_scrub}");

    let token = crate::web::bootstrap_session(&d, "durability", "correct horse battery");
    let auth = crate::web::bearer(&token);

    let status = crate::web::call(&d, "GET", "/api/repair/status", "", Some(&auth));
    assert_eq!(crate::web::status_of(&status), 200, "{status}");
    let doc: serde_json::Value =
        serde_json::from_str(crate::web::body_of(&status)).expect("status json");
    assert!(doc.get("queuedFindings").is_some(), "{doc}");

    let acquired = crate::web::call(
        &d,
        "POST",
        "/api/lease/acquire",
        r#"{"holder":"agent-7","ttlSecs":60}"#,
        Some(&auth),
    );
    assert_eq!(crate::web::status_of(&acquired), 200, "{acquired}");
    let lease: serde_json::Value =
        serde_json::from_str(crate::web::body_of(&acquired)).expect("lease json");
    assert_eq!(lease["holder"], "agent-7", "{lease}");

    let conflict = crate::web::call(
        &d,
        "POST",
        "/api/lease/acquire",
        r#"{"holder":"someone-else","ttlSecs":60}"#,
        Some(&auth),
    );
    assert_eq!(crate::web::status_of(&conflict), 409, "{conflict}");

    let runs = crate::web::call(&d, "GET", "/api/scrub/runs", "", Some(&auth));
    assert_eq!(crate::web::status_of(&runs), 200, "{runs}");
}
