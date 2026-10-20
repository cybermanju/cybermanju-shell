// CyberManju OS — Web (crates/tests/src/web.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: cybermanju_web.
// Contracts: signals: AGENT-.
use cybermanju_web::WebDashboard;
use redb::ReadableTable;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn test_web_dashboard_creation() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let _d = WebDashboard::new(3456, db_path.to_str().unwrap());
}

#[test]
fn test_web_dashboard_new_with_bind_addr() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let d = WebDashboard::new_with_bind_addr(8080, db_path.to_str().unwrap(), "0.0.0.0");
    assert_eq!(d.port, 8080);
    assert_eq!(d.bind_addr, "0.0.0.0");
}

#[test]
fn test_web_dashboard_db_accessor() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let d = WebDashboard::new(3456, db_path.to_str().unwrap());
    let _guard = d.db().read().unwrap();
}

#[test]
fn test_security_constants() {
    const { assert!(cybermanju_web::MAX_BODY_SIZE > 0) };
    const { assert!(cybermanju_web::RATE_LIMIT_MAX > 0) };
    const { assert!(cybermanju_web::RATE_LIMIT_WINDOW_SECS > 0) };
    assert!(!cybermanju_web::ALLOWED_ORIGINS.is_empty());
}

#[test]
fn test_handle_request_health() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let d = WebDashboard::new(3456, db_path.to_str().unwrap());
    let resp = cybermanju_web::handle_request(&d, &d.db, "GET", "/api/health", "", None, None);
    assert!(resp.contains("200"));
}

#[test]
fn test_handle_request_404() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let d = WebDashboard::new(3456, db_path.to_str().unwrap());
    let resp = cybermanju_web::handle_request(&d, &d.db, "GET", "/api/nonexistent", "", None, None);
    assert!(resp.contains("404"));
}

#[test]
fn test_handle_request_options_cors() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let d = WebDashboard::new(3456, db_path.to_str().unwrap());
    let resp = cybermanju_web::handle_request(
        &d,
        &d.db,
        "OPTIONS",
        "/api/health",
        "",
        None,
        Some("http://localhost:3456"),
    );
    assert!(resp.contains("204"));
    assert!(resp.contains("Access-Control-Allow-Origin"));
}

#[test]
fn test_rate_limit_check() {
    use std::collections::HashMap;
    use std::sync::Mutex;

    let limits = Mutex::new(HashMap::new());
    for _ in 0..100 {
        assert!(cybermanju_web::check_rate_limit(&limits, "1.2.3.4"));
    }
}

#[test]
fn test_rate_limit_exceeded() {
    use std::collections::HashMap;
    use std::sync::Mutex;

    let limits = Mutex::new(HashMap::new());
    for _ in 0..101 {
        cybermanju_web::check_rate_limit(&limits, "5.6.7.8");
    }
    assert!(!cybermanju_web::check_rate_limit(&limits, "5.6.7.8"));
}

#[test]
fn test_rate_limit_different_ips() {
    use std::collections::HashMap;
    use std::sync::Mutex;

    let limits = Mutex::new(HashMap::new());
    for _ in 0..100 {
        cybermanju_web::check_rate_limit(&limits, "ip1");
    }
    assert!(cybermanju_web::check_rate_limit(&limits, "ip2"));
}

#[test]
fn test_mime_type() {
    assert_eq!(
        cybermanju_web::mime_type("test.html"),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        cybermanju_web::mime_type("style.css"),
        "text/css; charset=utf-8"
    );
    assert_eq!(
        cybermanju_web::mime_type("app.js"),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(
        cybermanju_web::mime_type("data.json"),
        "application/json; charset=utf-8"
    );
    assert_eq!(cybermanju_web::mime_type("image.png"), "image/png");
    assert_eq!(cybermanju_web::mime_type("photo.jpg"), "image/jpeg");
    assert_eq!(cybermanju_web::mime_type("photo.jpeg"), "image/jpeg");
    assert_eq!(cybermanju_web::mime_type("anim.gif"), "image/gif");
    assert_eq!(cybermanju_web::mime_type("icon.svg"), "image/svg+xml");
    assert_eq!(cybermanju_web::mime_type("icon.ico"), "image/x-icon");
    assert_eq!(cybermanju_web::mime_type("font.woff"), "font/woff");
    assert_eq!(cybermanju_web::mime_type("font.woff2"), "font/woff2");
    assert_eq!(cybermanju_web::mime_type("font.ttf"), "font/ttf");
    assert_eq!(cybermanju_web::mime_type("doc.pdf"), "application/pdf");
    assert_eq!(cybermanju_web::mime_type("archive.zip"), "application/zip");
    assert_eq!(cybermanju_web::mime_type("code.wasm"), "application/wasm");
    assert_eq!(
        cybermanju_web::mime_type("data.xml"),
        "application/xml; charset=utf-8"
    );
    assert_eq!(
        cybermanju_web::mime_type("readme.txt"),
        "text/plain; charset=utf-8"
    );
    assert_eq!(
        cybermanju_web::mime_type("data.csv"),
        "text/csv; charset=utf-8"
    );
    assert_eq!(
        cybermanju_web::mime_type("unknown.xyz"),
        "application/octet-stream"
    );
}

#[test]
fn test_mime_type_no_extension() {
    assert_eq!(
        cybermanju_web::mime_type("Makefile"),
        "application/octet-stream"
    );
}

#[test]
fn test_serve_static_file_exists() {
    let _ = cybermanju_web::serve_static_file;
}

#[test]
fn test_default_port() {
    assert_eq!(cybermanju_web::DEFAULT_PORT, 3456);
}

// ─── AGENT-4 item 9 — REST/auth status-code matrix ─────────────────────
//
// The router (`cybermanju_web::handle_request`) is the single place every
// transport goes through, so pinning it here pins Docker and desktop too.
// AGENT-3 owns the route → role table (`security::required_role`); these
// tests fail loudly if the public surface grows without an auth check.

/// Fresh dashboard backed by a throwaway database. The tempdir is returned so
/// it outlives the dashboard (redb keeps the file open).
pub(crate) fn mk_dashboard(port: u16) -> (tempfile::TempDir, Arc<WebDashboard>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("matrix.redb");
    let d = Arc::new(WebDashboard::new(port, db_path.to_str().unwrap()));
    (dir, d)
}

/// One request through the shared router (no transport, no sockets).
pub(crate) fn call(
    d: &WebDashboard,
    method: &str,
    path: &str,
    body: &str,
    auth: Option<&str>,
) -> String {
    cybermanju_web::handle_request(d, &d.db, method, path, body, auth, None)
}

/// Status code from the first line of a raw HTTP response.
pub(crate) fn status_of(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// Response body — everything after the header terminator.
pub(crate) fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Sign a token with the dashboard's own secret so role gating and expiry can
/// be pinned without depending on how a given role is *obtained*.
pub(crate) fn mint(d: &WebDashboard, role: &str, exp: u64, jti: &str) -> String {
    let user_id = format!("user-{role}");
    let username = format!("{role}-user");
    // Most auth tests need a coherent principal now that the router checks
    // current account state. Never repair an existing row: tests for stale
    // tokens must retain intentional role changes, deactivation, or deletion.
    let db = d.db.write().expect("test database lock");
    let tx = db.begin_write().expect("test user transaction");
    {
        let mut users = tx
            .open_table(cybermanju_db::Database::get_users_table())
            .expect("users table");
        if users.get(user_id.as_str()).expect("user lookup").is_none() {
            let user = cybermanju_types::schema::User {
                id: user_id.clone(),
                username: username.clone(),
                password_hash: String::new(),
                display_name: None,
                role: role.to_string(),
                is_active: true,
                created_at: "test-fixture".to_string(),
                updated_at: "test-fixture".to_string(),
            };
            users
                .insert(
                    user_id.as_str(),
                    serde_json::to_string(&user)
                        .expect("serialize test user")
                        .as_str(),
                )
                .expect("insert test user");
        }
    }
    tx.commit().expect("commit test user transaction");
    drop(db);
    let claims = cybermanju_web::security::Claims {
        sub: username,
        user_id,
        role: role.to_string(),
        iat: now_secs(),
        exp,
        jti: jti.to_string(),
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&d.jwt_secret),
    )
    .expect("jwt encode")
}

pub(crate) fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Bootstrap the first account over the public endpoint and return a real
/// session token, so Argon2 verification and JWT issuance are covered too.
pub(crate) fn bootstrap_session(d: &WebDashboard, username: &str, password: &str) -> String {
    let register = call(
        d,
        "POST",
        "/api/users/register",
        &format!("{{\"username\":\"{username}\",\"password\":\"{password}\"}}"),
        None,
    );
    assert_eq!(status_of(&register), 201, "register: {register}");
    login_session(d, username, password)
}

pub(crate) fn login_session(d: &WebDashboard, username: &str, password: &str) -> String {
    let login = call(
        d,
        "POST",
        "/api/auth/login",
        &format!("{{\"username\":\"{username}\",\"password\":\"{password}\"}}"),
        None,
    );
    assert_eq!(status_of(&login), 200, "login: {login}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&login)).expect("login json");
    payload["token"].as_str().expect("token").to_string()
}

/// P0-1 object-gate setup: grant `username` `access` (`read`/`write`) on
/// `file_id` by writing a canonical permission row straight into the store
/// (the same shape both the Tauri and REST setters persist). Tests that
/// touch file bytes/links must call this first — object access is
/// fail-closed and no grant means 403.
pub(crate) fn grant_file_access(
    d: &WebDashboard,
    auth: &str,
    username: &str,
    file_id: &str,
    access: &str,
) {
    let listed = call(d, "GET", "/api/users", "", Some(auth));
    assert_eq!(status_of(&listed), 200, "{listed}");
    let users: serde_json::Value = serde_json::from_str(body_of(&listed)).expect("users json");
    let user_id = users
        .as_array()
        .expect("users array")
        .iter()
        .find(|u| u["username"] == username)
        .and_then(|u| u["id"].as_str())
        .expect("user id")
        .to_string();
    let perm = cybermanju_types::schema::UserFilePermission {
        id: format!("perm-{file_id}-{access}"),
        user_id,
        file_id: file_id.to_string(),
        access: access.to_string(),
        granted_by: "test".to_string(),
        granted_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let guard = d.db.write().expect("db write lock");
    let tx = guard.begin_write().expect("write tx");
    {
        let mut table = tx
            .open_table(cybermanju_db::Database::get_user_file_perms_table())
            .expect("perms table");
        table
            .insert(
                perm.id.as_str(),
                serde_json::to_string(&perm).expect("perm json").as_str(),
            )
            .expect("insert perm");
    }
    tx.commit().expect("commit perm");
}

/// Ephemeral port nothing is holding right now.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("probe bind")
        .local_addr()
        .expect("probe addr")
        .port()
}

/// Send one raw HTTP/1.1 request to a locally started server and read until it
/// closes the connection (the server answers once and drops the socket).
fn http_exchange(port: u16, request: &str) -> String {
    let mut stream = None;
    let mut last = None;
    for _ in 0..300 {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let mut stream =
        stream.unwrap_or_else(|| panic!("server on port {port} never came up: {last:?}"));
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
    stream.write_all(request.as_bytes()).expect("write");
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).into_owned()
}

/// A connected socket pair — drives `serve_static_file` without a server.
fn socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("pair bind");
    let addr = listener.local_addr().expect("pair addr");
    let client = TcpStream::connect(addr).expect("pair client");
    let (server, _) = listener.accept().expect("pair accept");
    (client, server)
}

/// Prometheus text exposition: `name value` on its own line.
fn counter_of(text: &str, name: &str) -> u64 {
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(' ') {
            if key == name {
                return value.trim().parse::<u64>().unwrap_or(0);
            }
        }
    }
    0
}

/// Routes that must never be reachable without a session.
const PROTECTED_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/files"),
    ("GET", "/api/files/any-id"),
    ("GET", "/api/trash"),
    ("GET", "/api/users"),
    ("GET", "/api/share-links"),
    ("GET", "/api/dashboard/status"),
    ("GET", "/api/sync/status"),
    ("GET", "/api/sync/progress"),
    ("GET", "/api/sync/configs"),
    ("GET", "/api/audit"),
    ("GET", "/api/search?q=anything"),
    ("GET", "/api/encryption/status"),
    ("GET", "/api/accounts"),
    ("GET", "/api/collections"),
    ("GET", "/api/permissions/any-file"),
    ("POST", "/api/files/folder"),
    ("POST", "/api/trash"),
    ("POST", "/api/users"),
    ("DELETE", "/api/users/some-id"),
    ("DELETE", "/api/trash"),
    ("DELETE", "/api/share-links/share-1"),
    ("DELETE", "/api/sync/configs/cfg-1"),
];

#[test]
fn rest_401_on_every_protected_route_without_credentials() {
    let (_dir, d) = mk_dashboard(3456);
    for (method, path) in PROTECTED_ROUTES {
        let resp = call(&d, method, path, "{}", None);
        assert_eq!(status_of(&resp), 401, "{method} {path}: {resp}");
    }
}

#[test]
fn rest_401_for_malformed_credentials() {
    let (_dir, d) = mk_dashboard(3456);
    let good = mint(&d, "user", now_secs() + 3_600, "jti-shape");
    let headers = [
        "Token nonsense".to_string(),
        "Bearer".to_string(),
        "Bearer not.a.jwt".to_string(),
        bearer(&format!("{good}x")),
        bearer(&"a".repeat(5_000)),
    ];
    for auth in headers {
        let resp = call(&d, "GET", "/api/files", "", Some(&auth));
        assert_eq!(status_of(&resp), 401, "{auth}: {resp}");
    }
}

#[test]
fn rest_public_routes_serve_without_credentials() {
    let (_dir, d) = mk_dashboard(3456);
    for path in ["/api", "/api/health", "/api/metrics"] {
        let resp = call(&d, "GET", path, "", None);
        assert_eq!(status_of(&resp), 200, "{path}: {resp}");
    }
    // OPTIONS is answered before the auth gate — preflight must never need a
    // token or the browser cannot call the API at all.
    let preflight = call(&d, "OPTIONS", "/api/files", "", None);
    assert_eq!(status_of(&preflight), 204, "{preflight}");
}

#[test]
fn health_keeps_the_old_shape_and_stays_green_when_not_ready() {
    let (_dir, d) = mk_dashboard(3456);
    let resp = call(&d, "GET", "/api/health", "", None);
    assert_eq!(status_of(&resp), 200, "{resp}");
    let health: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("health json");
    assert_eq!(health["service"], "CyberManju OS Web Dashboard");
    assert!(health["status"].is_string(), "status must stay a string");
    assert!(health["timestamp"].is_u64());
    // Liveness (200) and readiness are separate: this dashboard has no search
    // index wired, so it is degraded but still serving.
    assert_eq!(health["ready"], false);
    assert_eq!(health["checks"]["searchIndex"], "not-configured");
    assert_eq!(health["checks"]["database"], "ok");
}

#[test]
fn readyz_is_503_without_a_search_index_and_200_with_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("readyz.redb");
    let mut d = WebDashboard::new(3456, db_path.to_str().unwrap());

    let red = cybermanju_web::handle_request(&d, &d.db, "GET", "/api/readyz", "", None, None);
    assert_eq!(status_of(&red), 503, "{red}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&red)).expect("readyz json");
    assert_eq!(payload["ready"], false);
    assert_eq!(payload["checks"]["searchIndex"], "not-configured");

    let index_dir = tempfile::tempdir().expect("index dir");
    let index = cybermanju_search::SearchIndex::new(index_dir.path().to_str().unwrap())
        .expect("search index");
    d.set_search_index(Arc::new(std::sync::RwLock::new(index)));

    let green = cybermanju_web::handle_request(&d, &d.db, "GET", "/api/readyz", "", None, None);
    assert_eq!(status_of(&green), 200, "{green}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&green)).expect("readyz json");
    assert_eq!(payload["ready"], true);
    assert_eq!(payload["checks"]["database"], "ok");
    assert_eq!(payload["checks"]["searchIndex"], "ok");
    assert_eq!(payload["checks"]["diskWritable"], "ok");
}

#[test]
fn metrics_exposes_prometheus_counters_and_counts_4xx() {
    let (_dir, d) = mk_dashboard(3456);
    let first = call(&d, "GET", "/api/metrics", "", None);
    assert_eq!(status_of(&first), 200, "{first}");
    assert!(first.contains("text/plain"), "{first}");
    let before = counter_of(body_of(&first), "cybermanju_http_responses_4xx_total");

    let missing = call(&d, "GET", "/api/definitely-not-a-route", "", None);
    assert_eq!(status_of(&missing), 404, "{missing}");

    let second = call(&d, "GET", "/api/metrics", "", None);
    assert_eq!(status_of(&second), 200, "{second}");
    let after = counter_of(body_of(&second), "cybermanju_http_responses_4xx_total");
    assert!(
        after > before,
        "4xx counter did not move: {before} → {after}"
    );
    let text = body_of(&second);
    assert!(text.contains("cybermanju_http_requests_total"), "{text}");
    assert!(text.contains("cybermanju_active_connections"), "{text}");
}

#[test]
fn dashboard_status_reports_a_version_string() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-status"));
    let resp = call(&d, "GET", "/api/dashboard/status", "", Some(auth.as_str()));
    assert_eq!(status_of(&resp), 200, "{resp}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("status json");
    let version = payload["version"].as_str().expect("version field");
    assert_eq!(
        version.split('.').count(),
        3,
        "version must be MAJOR.MINOR.PATCH, got {version}"
    );
    assert_eq!(payload["service"], "CyberManju OS Web Dashboard");
}

#[test]
fn encryption_status_reports_the_linked_crypto_engine() {
    let (_dir, d) = mk_dashboard(3456);
    let auth = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-enc"));
    let resp = call(&d, "GET", "/api/encryption/status", "", Some(auth.as_str()));
    assert_eq!(status_of(&resp), 200, "{resp}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&resp)).expect("crypto json");
    assert_eq!(payload["available"], true);
    let algorithms = payload["supported_algorithms"]
        .as_array()
        .expect("supported_algorithms array");
    let ids: Vec<&str> = algorithms.iter().filter_map(|a| a.as_str()).collect();
    assert_eq!(ids.len(), 7, "{ids:?}");
    assert!(ids.contains(&"ml_kem_1024"), "{ids:?}");
    assert!(ids.contains(&"ml_dsa_87"), "{ids:?}");
    assert!(
        payload["engine"].as_str().is_some(),
        "engine must be a string"
    );
    assert_eq!(
        payload["post_quantum"]
            .as_array()
            .expect("post_quantum")
            .len(),
        5
    );
}

#[test]
fn cors_allowlist_only_reflects_allowed_origins() {
    let (_dir, d) = mk_dashboard(3456);

    let allowed = cybermanju_web::handle_request(
        &d,
        &d.db,
        "OPTIONS",
        "/api/health",
        "",
        None,
        Some("http://localhost:3456"),
    );
    assert_eq!(status_of(&allowed), 204, "{allowed}");
    assert!(
        allowed.contains("Access-Control-Allow-Origin: http://localhost:3456"),
        "{allowed}"
    );

    let denied = cybermanju_web::handle_request(
        &d,
        &d.db,
        "OPTIONS",
        "/api/health",
        "",
        None,
        Some("https://evil.example"),
    );
    assert_eq!(status_of(&denied), 204, "{denied}");
    assert!(!denied.contains("evil.example"), "{denied}");
    assert!(!denied.contains("Access-Control-Allow-Origin:"), "{denied}");

    // A permissive origin on a real API call must never be echoed back.
    let json = cybermanju_web::handle_request(
        &d,
        &d.db,
        "GET",
        "/api/health",
        "",
        None,
        Some("https://evil.example"),
    );
    assert_eq!(status_of(&json), 200, "{json}");
    assert!(!json.contains("evil.example"), "{json}");

    assert_eq!(
        cybermanju_web::security::cors_origin_allowed(Some("http://127.0.0.1:3456")),
        Some("http://127.0.0.1:3456".to_string())
    );
    assert_eq!(
        cybermanju_web::security::cors_origin_allowed(Some("https://evil.example")),
        None
    );
}

#[test]
fn rest_400_on_malformed_or_incomplete_json() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "alice", "correct horse battery");
    let auth = bearer(&token);

    let malformed = call(
        &d,
        "POST",
        "/api/files/folder",
        "{ not json",
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&malformed), 400, "{malformed}");

    let incomplete = call(
        &d,
        "POST",
        "/api/files/folder",
        r#"{"name":"documents"}"#,
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&incomplete), 400, "{incomplete}");

    let bad_login = call(&d, "POST", "/api/auth/login", "not json", None);
    assert_eq!(status_of(&bad_login), 400, "{bad_login}");

    let empty_login = call(&d, "POST", "/api/auth/login", "{}", None);
    assert_eq!(status_of(&empty_login), 400, "{empty_login}");
}

#[test]
fn rest_404_for_missing_entities_and_400_for_bad_requests() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "bob", "correct horse battery");
    let auth = bearer(&token);

    let unknown_route = call(&d, "GET", "/api/nonexistent", "", Some(auth.as_str()));
    assert_eq!(status_of(&unknown_route), 404, "{unknown_route}");

    let missing_file = call(
        &d,
        "GET",
        "/api/files/does-not-exist",
        "",
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&missing_file), 404, "{missing_file}");

    let missing_share = call(
        &d,
        "POST",
        "/api/share-links",
        r#"{"fileId":"no-such-file"}"#,
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&missing_share), 404, "{missing_share}");

    // Well-formed JSON that describes an invalid request is a 400, not a 404.
    let bad_request = call(
        &d,
        "POST",
        "/api/files/folder",
        r#"{"name":"documents"}"#,
        Some(auth.as_str()),
    );
    assert_eq!(status_of(&bad_request), 400, "{bad_request}");
}

#[test]
fn body_size_guard_boundaries() {
    assert!(cybermanju_web::security::enforce_body_limit(cybermanju_web::MAX_BODY_SIZE).is_ok());
    let err = cybermanju_web::security::enforce_body_limit(cybermanju_web::MAX_BODY_SIZE + 1)
        .expect_err("one byte over the cap must be rejected");
    assert_eq!(err.0, 413);
    assert_eq!(err.1, "Request body too large");
}

#[test]
fn rest_413_when_content_length_exceeds_the_body_cap() {
    let port = free_port();
    let (_dir, d) = mk_dashboard(port);
    d.start().expect("start server");
    let request = format!(
        "POST /api/users/register HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        cybermanju_web::MAX_BODY_SIZE + 1
    );
    let resp = http_exchange(port, &request);
    assert_eq!(status_of(&resp), 413, "{resp}");
    assert!(resp.contains("Request body too large"), "{resp}");
    d.stop();
}

#[test]
fn rest_429_once_the_rate_limit_window_is_exhausted() {
    let port = free_port();
    let (_dir, d) = mk_dashboard(port);
    d.start().expect("start server");

    let mut served: u32 = 0;
    let mut limited: u32 = 0;
    for _ in 0..=cybermanju_web::RATE_LIMIT_MAX {
        let resp = http_exchange(port, "GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n");
        match status_of(&resp) {
            200 => served += 1,
            429 => limited += 1,
            other => panic!("unexpected status {other}: {resp}"),
        }
    }
    assert_eq!(served, cybermanju_web::RATE_LIMIT_MAX);
    assert_eq!(limited, 1);
    d.stop();
}

#[test]
fn static_files_refuse_path_traversal() {
    let root = tempfile::tempdir().expect("root");
    let static_dir = root.path().join("dist");
    fs::create_dir_all(&static_dir).expect("static dir");
    fs::write(root.path().join("secret.txt"), b"top secret").expect("secret");
    fs::write(static_dir.join("index.html"), b"<html>ok</html>").expect("index");

    // A file that exists *outside* the static root must not be served.
    let (mut client, mut server) = socket_pair();
    cybermanju_web::serve_static_file(&mut server, &static_dir, "/../secret.txt");
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).expect("read");
    let traversal = String::from_utf8_lossy(&out);
    assert!(traversal.starts_with("HTTP/1.1 403"), "{traversal}");

    // The same shape pointing at nothing is a 404 (canonicalize fails).
    let (mut client, mut server) = socket_pair();
    cybermanju_web::serve_static_file(&mut server, &static_dir, "/../absent.txt");
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).expect("read");
    let absent = String::from_utf8_lossy(&out);
    assert!(absent.starts_with("HTTP/1.1 404"), "{absent}");

    // Sanity check: a file inside the root is still served normally.
    let (mut client, mut server) = socket_pair();
    cybermanju_web::serve_static_file(&mut server, &static_dir, "/");
    drop(server);
    let mut out = Vec::new();
    client.read_to_end(&mut out).expect("read");
    let index = String::from_utf8_lossy(&out);
    assert!(index.starts_with("HTTP/1.1 200"), "{index}");
    assert!(index.ends_with("<html>ok</html>"), "{index}");
}

#[test]
fn argon2_password_verification_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("argon2.redb");
    let db = cybermanju_db::Database::new(db_path.to_str().unwrap()).expect("db");

    let user = cybermanju_web::api::users::register(
        &db,
        "carol".to_string(),
        "correct horse battery".to_string(),
        None,
        None,
        cybermanju_web::api::users::RegistrationMode::LocalIpc,
    )
    .expect("register");
    assert!(
        user.password_hash.starts_with("$argon2"),
        "expected an argon2id hash, got {}",
        user.password_hash
    );

    assert!(
        cybermanju_web::api::users::authenticate(&db, "carol", "correct horse battery").is_ok()
    );
    assert!(cybermanju_web::api::users::authenticate(&db, "carol", "wrong password").is_err());
    assert!(
        cybermanju_web::api::users::authenticate(&db, "nobody", "correct horse battery").is_err()
    );
}

#[test]
fn rest_register_then_login_issues_a_usable_token() {
    let (_dir, d) = mk_dashboard(3456);
    let token = bootstrap_session(&d, "dave", "correct horse battery");
    let auth = bearer(&token);

    let files = call(&d, "GET", "/api/files", "", Some(auth.as_str()));
    assert_eq!(status_of(&files), 200, "{files}");

    // Bootstrap registration is closed once an account exists.
    let second = call(
        &d,
        "POST",
        "/api/users/register",
        r#"{"username":"eve","password":"correct horse battery"}"#,
        None,
    );
    assert_eq!(status_of(&second), 403, "{second}");
    assert!(second.contains("closed"), "{second}");

    // Wrong password → 401 through Argon2, never a 500.
    let bad = call(
        &d,
        "POST",
        "/api/auth/login",
        r#"{"username":"dave","password":"nope nope"}"#,
        None,
    );
    assert_eq!(status_of(&bad), 401, "{bad}");

    // Unknown user → 401 as well, so the status code cannot enumerate users.
    let unknown = call(
        &d,
        "POST",
        "/api/auth/login",
        r#"{"username":"ghost","password":"nope nope"}"#,
        None,
    );
    assert_eq!(status_of(&unknown), 401, "{unknown}");
}

#[test]
fn auth_status_reports_bootstrap_state_without_credentials() {
    let (_dir, d) = mk_dashboard(3456);
    // Empty database → registration open, no token needed.
    let open = call(&d, "GET", "/api/auth/status", "", None);
    assert_eq!(status_of(&open), 200, "{open}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&open)).expect("status json");
    assert_eq!(payload["registrationOpen"], true, "{payload}");

    // After the first account exists → registration closed.
    let _token = bootstrap_session(&d, "first", "correct horse battery");
    let closed = call(&d, "GET", "/api/auth/status", "", None);
    assert_eq!(status_of(&closed), 200, "{closed}");
    let payload: serde_json::Value = serde_json::from_str(body_of(&closed)).expect("status json");
    assert_eq!(payload["registrationOpen"], false, "{payload}");
}

#[test]
fn rest_bootstrap_registration_never_grants_admin() {
    let (_dir, d) = mk_dashboard(3456);
    let resp = call(
        &d,
        "POST",
        "/api/users/register",
        r#"{"username":"root","password":"correct horse battery","role":"admin"}"#,
        None,
    );
    assert_eq!(status_of(&resp), 403, "{resp}");
    assert!(resp.contains("admin"), "{resp}");

    // …and the account must not exist afterwards.
    let login = call(
        &d,
        "POST",
        "/api/auth/login",
        r#"{"username":"root","password":"correct horse battery"}"#,
        None,
    );
    assert_eq!(status_of(&login), 401, "{login}");
}

#[test]
fn jwt_expired_tokens_are_rejected() {
    let (_dir, d) = mk_dashboard(3456);
    let expired = mint(&d, "user", now_secs().saturating_sub(3_600), "jti-expired");
    let auth = bearer(&expired);
    let resp = call(&d, "GET", "/api/files", "", Some(auth.as_str()));
    assert_eq!(status_of(&resp), 401, "{resp}");
}

#[test]
fn jwt_tokens_are_revoked_by_logout() {
    let (_dir, d) = mk_dashboard(3456);
    let token = mint(&d, "user", now_secs() + 3_600, "jti-revoke-me");
    let auth = bearer(&token);

    let before = call(&d, "GET", "/api/files", "", Some(auth.as_str()));
    assert_eq!(status_of(&before), 200, "{before}");

    let logout = call(&d, "POST", "/api/auth/logout", "", Some(auth.as_str()));
    assert_eq!(status_of(&logout), 200, "{logout}");

    let replay = call(&d, "GET", "/api/files", "", Some(auth.as_str()));
    assert_eq!(status_of(&replay), 401, "{replay}");
}

#[test]
fn rbac_admin_routes_reject_non_admin_sessions() {
    let (_dir, d) = mk_dashboard(3456);
    let user = bearer(&mint(&d, "user", now_secs() + 3_600, "jti-rbac-user"));
    let admin = bearer(&mint(&d, "admin", now_secs() + 3_600, "jti-rbac-admin"));

    let admin_only: &[(&str, &str)] = &[
        ("POST", "/api/users"),
        ("DELETE", "/api/users/some-id"),
        ("POST", "/api/users/some-id/role"),
        ("DELETE", "/api/trash"),
        ("GET", "/api/share-links"),
        ("DELETE", "/api/share-links/share-1"),
        ("DELETE", "/api/sync/configs/cfg-1"),
    ];
    for (method, path) in admin_only {
        let denied = call(&d, method, path, "{}", Some(user.as_str()));
        assert_eq!(status_of(&denied), 403, "{method} {path}: {denied}");
        assert!(denied.contains("Forbidden"), "{denied}");
    }

    // Ordinary routes stay open to a plain session…
    let files = call(&d, "GET", "/api/files", "", Some(user.as_str()));
    assert_eq!(status_of(&files), 200, "{files}");
    let users = call(&d, "GET", "/api/users", "", Some(user.as_str()));
    assert_eq!(status_of(&users), 200, "{users}");

    // …and an admin session passes the gate on the admin table.
    let links = call(&d, "GET", "/api/share-links", "", Some(admin.as_str()));
    assert_eq!(status_of(&links), 200, "{links}");
    let admin_users = call(&d, "GET", "/api/users", "", Some(admin.as_str()));
    assert_eq!(status_of(&admin_users), 200, "{admin_users}");
}
