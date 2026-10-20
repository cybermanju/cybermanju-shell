// CyberManju OS — Share (crates/tests/src/share.rs).
// AGENT-4 item 9 — share-link REST contract.
//
// Share links are the one deliberately unauthenticated read path, so the
// interesting cases are "public but narrow": a token resolves metadata, an
// unknown token is a 404, an expired one is a 410, and administering links
// stays behind the admin role.

use crate::web::{
    bearer, body_of, bootstrap_session, call, grant_file_access, mint, mk_dashboard, now_secs,
    status_of,
};
use cybermanju_web::WebDashboard;

/// Create a folder over the REST API and return its id (folders are the only
/// node type the API can mint without a sync run or a desktop import).
fn create_folder(d: &WebDashboard, auth: &str, name: &str) -> String {
    let body = format!("{{\"name\":\"{name}\",\"parentId\":\"root\"}}");
    let resp = call(d, "POST", "/api/files/folder", &body, Some(auth));
    assert_eq!(status_of(&resp), 200, "create folder: {resp}");
    let node: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("folder json");
    node["id"].as_str().expect("folder id").to_string()
}

#[test]
fn creating_a_share_link_requires_a_session() {
    let (_dir, d) = mk_dashboard(3456);
    let resp = call(
        &d,
        "POST",
        "/api/share-links",
        r#"{"fileId":"anything"}"#,
        None,
    );
    assert_eq!(status_of(&resp), 401, "{resp}");
}

#[test]
fn share_link_round_trip_is_public() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "ida", "correct horse battery");
    let auth = bearer(&token);
    let folder_id = create_folder(&d, &auth, "Holiday");
    // P0-1: sharing publishes bytes — the grant comes first (no grant: 403).
    let denied = call(
        &d,
        "POST",
        "/api/share-links",
        &format!(r#"{{"fileId":"{folder_id}"}}"#),
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&denied), 403, "{denied}");
    grant_file_access(&d, &auth, "ida", &folder_id, "read");

    let created = call(
        &d,
        "POST",
        "/api/share-links",
        &format!(r#"{{"fileId":"{folder_id}"}}"#),
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&created), 200, "{created}");
    let link: serde_json::Value = serde_json::from_str(body_of(&created)).expect("share json");
    let share_token = link["token"].as_str().expect("share token").to_string();
    assert_eq!(link["fileId"], folder_id);
    assert!(
        link["url"]
            .as_str()
            .unwrap_or_default()
            .ends_with(&share_token),
        "{link}"
    );

    // Resolving a share needs no session at all — that is the point of a link.
    let metadata = call(&d, "GET", &format!("/api/shared/{share_token}"), "", None);
    assert_eq!(status_of(&metadata), 200, "{metadata}");
    let node: serde_json::Value = serde_json::from_str(body_of(&metadata)).expect("node json");
    assert_eq!(node["name"], "Holiday");
    assert_eq!(node["id"], folder_id);
}

#[test]
fn unknown_share_token_is_404() {
    let (_dir, d) = mk_dashboard(3456);
    let resp = call(&d, "GET", "/api/shared/not-a-real-token", "", None);
    assert_eq!(status_of(&resp), 404, "{resp}");
}

#[test]
fn expired_share_link_is_gone() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "jan", "correct horse battery");
    let auth = bearer(&token);
    let folder_id = create_folder(&d, &auth, "Old photos");

    // Write an already-expired link straight into the store: no test should
    // ever have to sleep for an expiry to elapse.
    let link = cybermanju_types::schema::ShareLink {
        id: "expired-share".to_string(),
        file_id: folder_id,
        token: "expired-token".to_string(),
        expires_at: "2000-01-01T00:00:00+00:00".to_string(),
        created_at: "2000-01-01T00:00:00+00:00".to_string(),
        url: None,
    };
    {
        let db = d.db.write().expect("db write lock");
        let tx = db.begin_write().expect("write tx");
        {
            let mut table = tx
                .open_table(cybermanju_db::Database::get_share_links_table())
                .expect("share_links table");
            let serialized = serde_json::to_string(&link).expect("serialize share link");
            table
                .insert(link.id.as_str(), serialized.as_str())
                .expect("insert share link");
        }
        tx.commit().expect("commit");
    }

    // Metadata: an expired link is a client error, never a 200. AGENT-3 owns
    // whether that is 400 or a strict 410, so accept either.
    let metadata = call(&d, "GET", "/api/shared/expired-token", "", None);
    let metadata_status = status_of(&metadata);
    assert!(
        metadata_status == 400 || metadata_status == 410,
        "expired metadata: {metadata}"
    );

    // The byte stream is explicit: 410 Gone, so a client can tell "expired"
    // from "typo" without probing the file itself.
    let content = call(&d, "GET", "/api/shared/expired-token/content", "", None);
    assert_eq!(status_of(&content), 410, "{content}");
}

#[test]
fn share_listing_and_revoke_are_admin_only() {
    let (_dir, d) = mk_dashboard(3456);
    let user = bootstrap_session(&d, "lena", "correct horse battery");
    let user_auth = bearer(&user);
    let folder_id = create_folder(&d, &user_auth, "Private");
    // P0-1: sharing needs the object grant even for the folder creator.
    grant_file_access(&d, &user_auth, "lena", &folder_id, "read");

    let created = call(
        &d,
        "POST",
        "/api/share-links",
        &format!(r#"{{"fileId":"{folder_id}"}}"#),
        Some(user_auth.as_str()),
    );
    assert_eq!(status_of(&created), 200, "{created}");
    let link: serde_json::Value = serde_json::from_str(body_of(&created)).expect("share json");
    let share_id = link["id"].as_str().expect("share id").to_string();
    let share_token = link["token"].as_str().expect("share token").to_string();

    // A plain session can mint a link but cannot administer them.
    let denied_list = call(&d, "GET", "/api/share-links", "", Some(user_auth.as_str()));
    assert_eq!(status_of(&denied_list), 403, "{denied_list}");
    let denied_revoke = call(
        &d,
        "DELETE",
        &format!("/api/share-links/{share_id}"),
        "",
        Some(user_auth.as_str()),
    );
    assert_eq!(status_of(&denied_revoke), 403, "{denied_revoke}");

    let admin = bearer(&mint(&d, "admin", now_secs() + 3_600, "jti-share-admin"));
    let listed = call(&d, "GET", "/api/share-links", "", Some(admin.as_str()));
    assert_eq!(status_of(&listed), 200, "{listed}");
    let links: serde_json::Value = serde_json::from_str(body_of(&listed)).expect("links json");
    assert!(!links.as_array().expect("array").is_empty(), "{links}");

    let revoked = call(
        &d,
        "DELETE",
        &format!("/api/share-links/{share_id}"),
        "",
        Some(admin.as_str()),
    );
    assert_eq!(status_of(&revoked), 200, "{revoked}");

    let after = call(&d, "GET", &format!("/api/shared/{share_token}"), "", None);
    assert_eq!(status_of(&after), 404, "{after}");
}
