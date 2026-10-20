// CyberManju OS — Backup routes (crates/tests/src/backup.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: crate::web harness (no live providers).
// Contracts: `GET /api/backup/export` is Admin-only; the payload carries
// the raw image base64'd with a BLAKE3 handle the UI verifies before
// sealing it into a CYBMJ01 container.

use crate::web::{bearer, body_of, call, mint, mk_dashboard, now_secs, status_of};

#[test]
fn backup_export_is_admin_only() {
    let (_dir, d) = mk_dashboard(3456);
    // Unknown-route and missing-session behavior first.
    let anon = call(&d, "GET", "/api/backup/export", "", None);
    assert_eq!(status_of(&anon), 401, "{anon}");
    let member = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-backup-user"));
    let denied = call(&d, "GET", "/api/backup/export", "", Some(&member));
    assert_eq!(status_of(&denied), 403, "{denied}");
}

#[test]
fn backup_export_reports_a_verifiable_image() {
    let (_dir, d) = mk_dashboard(3456);
    let admin = bearer(&mint(&d, "admin", now_secs() + 3_600, "jti-backup-admin"));
    let resp = call(&d, "GET", "/api/backup/export", "", Some(&admin));
    assert_eq!(status_of(&resp), 200, "{resp}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("json");
    let name = payload["name"].as_str().expect("name");
    assert!(name.starts_with("cybermanju-"), "{name}");
    assert!(name.ends_with(".cybermanju"), "{name}");
    let image_bytes = payload["imageBytes"].as_u64().expect("imageBytes");
    assert!(image_bytes > 0, "{payload}");
    assert_eq!(payload["blake3"].as_str().map(|s| s.len()), Some(64));
    // Base64 length must agree with the reported image size.
    let b64_len = payload["imageB64"].as_str().expect("imageB64").len() as u64;
    assert_eq!(
        b64_len,
        image_bytes.div_ceil(3) * 4,
        "{image_bytes} vs {b64_len}"
    );
}
