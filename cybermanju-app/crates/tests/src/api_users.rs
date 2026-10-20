// CyberManju OS — Api users (crates/tests/src/api_users.rs).
// AGENT-4 item 9 — user-management REST contract.
//
// The route → role table lives in AGENT-3's `security::required_role`; these
// tests pin the status codes an operator actually sees when registering,
// logging in and administering accounts.

use crate::web::{
    bearer, body_of, bootstrap_session, call, mint, mk_dashboard, now_secs, status_of,
};

#[test]
fn list_users_requires_a_session() {
    let (_dir, d) = mk_dashboard(3456);
    let resp = call(&d, "GET", "/api/users", "", None);
    assert_eq!(status_of(&resp), 401, "{resp}");
}

#[test]
fn list_users_returns_an_array_and_never_leaks_a_password_hash() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "alice", "correct horse battery");
    let auth = bearer(&token);

    let resp = call(&d, "GET", "/api/users", "", Some(auth.as_str()));
    assert_eq!(status_of(&resp), 200, "{resp}");

    let payload: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("users json");
    let users = payload.as_array().expect("a JSON array");
    assert_eq!(users.len(), 1, "{users:?}");
    assert_eq!(users[0]["username"], "alice");
    assert!(
        !body_of(&resp).contains("passwordHash") && !body_of(&resp).contains("password_hash"),
        "password hashes must never reach a client: {}",
        body_of(&resp)
    );
}

#[test]
fn admin_session_can_create_an_account_that_can_then_log_in() {
    let (_dir, d) = mk_dashboard(3456);
    let admin = bearer(&mint(&d, "admin", now_secs() + 3_600, "jti-admin-create"));

    let create = call(
        &d,
        "POST",
        "/api/users",
        r#"{"username":"frank","password":"correct horse battery","role":"viewer"}"#,
        Some(admin.as_str()),
    );
    assert_eq!(status_of(&create), 201, "{create}");
    let created: serde_json::Value = serde_json::from_str(body_of(&create)).expect("user json");
    assert_eq!(created["username"], "frank");
    assert_eq!(created["role"], "viewer");

    let session = call(
        &d,
        "POST",
        "/api/auth/login",
        r#"{"username":"frank","password":"correct horse battery"}"#,
        None,
    );
    assert_eq!(status_of(&session), 200, "{session}");
    let login: serde_json::Value = serde_json::from_str(body_of(&session)).expect("login json");
    assert_eq!(login["role"], "viewer");
    assert!(login["token"].as_str().is_some(), "{login}");

    // A second admin creation with the same username is a conflict, not a 500.
    let duplicate = call(
        &d,
        "POST",
        "/api/users",
        r#"{"username":"frank","password":"correct horse battery"}"#,
        Some(admin.as_str()),
    );
    assert_eq!(status_of(&duplicate), 409, "{duplicate}");
}

#[test]
fn user_session_is_denied_every_user_management_write() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "grace", "correct horse battery");
    let auth = bearer(&token);

    let writes: &[(&str, &str, &str)] = &[
        (
            "POST",
            "/api/users",
            r#"{"username":"mallory","password":"correct horse battery"}"#,
        ),
        ("DELETE", "/api/users/some-id", "{}"),
        ("POST", "/api/users/some-id/role", r#"{"role":"admin"}"#),
    ];
    for (method, path, body) in writes {
        let resp = call(&d, method, path, body, Some(auth.as_str()));
        assert_eq!(status_of(&resp), 403, "{method} {path}: {resp}");
        assert!(resp.contains("Forbidden"), "{resp}");
    }
}

#[test]
fn registration_rejects_weak_credentials_and_unknown_roles() {
    let (_dir, d) = mk_dashboard(3456);

    let short_password = call(
        &d,
        "POST",
        "/api/users/register",
        r#"{"username":"pat","password":"short"}"#,
        None,
    );
    assert_eq!(status_of(&short_password), 400, "{short_password}");

    let bad_username = call(
        &d,
        "POST",
        "/api/users/register",
        r#"{"username":"not a valid name","password":"correct horse battery"}"#,
        None,
    );
    assert_eq!(status_of(&bad_username), 400, "{bad_username}");

    let bad_role = call(
        &d,
        "POST",
        "/api/users/register",
        r#"{"username":"sam","password":"correct horse battery","role":"wizard"}"#,
        None,
    );
    assert_eq!(status_of(&bad_role), 400, "{bad_role}");

    // None of the rejected attempts may create an account.
    let login = call(
        &d,
        "POST",
        "/api/auth/login",
        r#"{"username":"pat","password":"short"}"#,
        None,
    );
    assert_eq!(status_of(&login), 401, "{login}");
}
