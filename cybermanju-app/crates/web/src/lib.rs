// CyberManju OS — Web Dashboard Server (Security-Hardened Rewrite)
// Embedded HTTP server for any-device browser access
// Exposes REST API mirroring all Tauri IPC commands
//
// Uses only std::net::TcpListener + manual HTTP parsing (no external HTTP crate)
//
// Security hardening applied:
//   1. Binds to 127.0.0.1 ONLY — prevents remote network exposure
//   2. JWT-based authentication (HS256) on all endpoints except login/register/health
//   3. CORS restricted to localhost origins only (not wildcard)
//   4. Request body size limit of 100 MB to prevent DoS
//   5. Private keys stripped from encryption key list responses
//   6. HMAC-signed JWT tokens (shared secret generated at startup)
//   7. Proper shutdown via mpsc signal channel, thread join on Drop
//   8. Rate limiting: 100 requests per minute per IP address

pub mod api;
pub mod security;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cybermanju_compression::TripleCompressor;
use cybermanju_db::Database;
use cybermanju_search::SearchIndex;
use cybermanju_sync::SyncState;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use log::{error, info, warn};
use redb::{ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

// Table definitions come from the shared `cybermanju-db` crate
// (`Database::get_*_table()`), so the router can never drift from the schema.

// ─── Security constants ─────────────────────────────────────────────

/// Maximum request body size: 100 MB
#[allow(dead_code)]
pub const MAX_BODY_SIZE: usize = 104_857_600;

/// Rate limit: max requests per window per IP
#[allow(dead_code)]
pub const RATE_LIMIT_MAX: u32 = 100;

/// Default port for the web dashboard
#[allow(dead_code)]
pub const DEFAULT_PORT: u16 = 3456;

/// Rate limit window duration in seconds
#[allow(dead_code)]
pub const RATE_LIMIT_WINDOW_SECS: u64 = 60;

/// JWT token expiry: 24 hours
const JWT_EXPIRY_SECS: u64 = 86_400;

// <<< AGENT-4 OPS: process-wide counters behind GET /api/metrics >>>
/// Total HTTP requests routed (every response, including 4xx/5xx).
static REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Responses with a 4xx status.
static RESPONSES_4XX: AtomicU64 = AtomicU64::new(0);
/// Responses with a 5xx status.
static RESPONSES_5XX: AtomicU64 = AtomicU64::new(0);
/// Monotonic id used to build the per-request log correlation id.
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Allowed CORS origins (localhost only)
#[allow(dead_code)]
pub const ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost:3456",
    "http://127.0.0.1:3456",
    "http://localhost:3457",
    "http://127.0.0.1:3457",
];

// ─── JWT Claims ──────────────────────────────────────────────────────

/// Verified JWT payload. Defined in `security` so both transports and the
/// AGENT-4 test matrix share one type.
pub use security::Claims as JwtClaims;

// ─── WebDashboard struct ────────────────────────────────────────────

pub struct WebDashboard {
    pub port: u16,
    /// Bind address — "127.0.0.1" for desktop, "0.0.0.0" for Docker
    pub bind_addr: String,
    /// Random 256-bit secret generated at startup for HMAC-SHA256 JWT signing
    pub jwt_secret: [u8; 32],
    /// Shared database handle — opened exactly once and shared with the rest
    /// of the application (redb allows only one open handle per file).
    pub db: Arc<RwLock<Database>>,
    /// Shared sync progress / cancellation state for `/api/sync/*`.
    pub sync_state: Arc<SyncState>,
    /// Shared Tantivy full-text index. `None` until the host wires one in
    /// (`set_search_index`); search then falls back to a database scan.
    pub search_index: Option<Arc<RwLock<SearchIndex>>>,
    /// Shared triple compressor used by the sync pipeline.
    pub compression: TripleCompressor,
    pub running: AtomicBool,
    /// Live number of in-flight HTTP connections (reported by
    /// `GET /api/dashboard/status` and the Tauri `dashboard_status` command).
    pub active_connections: AtomicU64,
    /// Per-IP rate limit counters: IP → (count, window_start)
    pub rate_limits: Mutex<HashMap<String, (u32, Instant)>>,
    // <<< AGENT-3 AUTH STATE >>>
    /// Revoked JWT ids, in-flight OAuth handshakes and login backoff.
    pub auth: security::AuthState,
    // <<< /AGENT-3 AUTH STATE >>>
    pub server_thread: Mutex<Option<thread::JoinHandle<()>>>,
    pub shutdown_tx: Mutex<Option<mpsc::Sender<()>>>,
}

impl WebDashboard {
    /// Open (or create) the database at `db_path` and build a dashboard around it.
    /// Prefer `new_shared` when the application already owns a database handle.
    pub fn new(port: u16, db_path: &str) -> Self {
        let db = Database::new(db_path).expect("Failed to open web dashboard database");
        Self::build(
            port,
            Arc::new(RwLock::new(db)),
            "127.0.0.1",
            std::path::Path::new(db_path)
                .parent()
                .map(|p| p.to_path_buf()),
        )
    }

    /// Constructor that allows specifying a bind address (for Docker use case).
    pub fn new_with_bind_addr(port: u16, db_path: &str, bind_addr: &str) -> Self {
        let db = Database::new(db_path).expect("Failed to open web dashboard database");
        Self::build(
            port,
            Arc::new(RwLock::new(db)),
            bind_addr,
            std::path::Path::new(db_path)
                .parent()
                .map(|p| p.to_path_buf()),
        )
    }

    /// Build a dashboard around an already-open database handle so the same
    /// redb file is never opened twice (which would fail on the exclusive lock).
    pub fn new_shared(port: u16, db: Arc<RwLock<Database>>) -> Self {
        Self::new_shared_with_bind_addr(port, db, "127.0.0.1")
    }

    /// Like `new_shared`, but with an explicit bind address.
    pub fn new_shared_with_bind_addr(
        port: u16,
        db: Arc<RwLock<Database>>,
        bind_addr: &str,
    ) -> Self {
        Self::build(port, db, bind_addr, None)
    }

    // <<< AGENT-3 JWT SECRET >>>
    /// Shared constructor. The JWT secret is **not** regenerated on every
    /// start: it is sourced from `CYBERMANJU_JWT_SECRET` or persisted at
    /// `<data dir>/jwt_secret` (0600) so restarts keep sessions alive.
    /// See `security::load_or_create_jwt_secret_in` for the full order and
    /// `docs/SECURITY.md` for rotation.
    fn build(
        port: u16,
        db: Arc<RwLock<Database>>,
        bind_addr: &str,
        secret_dir: Option<std::path::PathBuf>,
    ) -> Self {
        let jwt_secret = security::load_or_create_jwt_secret_in(secret_dir.as_deref());

        // <<< AGENT-3 BOOTSTRAP ADMIN: registration never grants `admin`, so
        // headless deployments (Docker) need an out-of-band way to create the
        // first administrator. See docs/SECURITY.md. >>>
        if let (Ok(username), Ok(password)) = (
            std::env::var("CYBERMANJU_ADMIN_USERNAME"),
            std::env::var("CYBERMANJU_ADMIN_PASSWORD"),
        ) {
            if !username.trim().is_empty() && !password.is_empty() {
                let handle = Arc::clone(&db);
                match handle.write() {
                    Ok(guard) => {
                        if let Err(e) =
                            api::users::ensure_admin_provisioned(&guard, &username, &password)
                        {
                            warn!("Could not provision admin account: {}", e);
                        }
                    }
                    Err(_) => warn!("Could not provision admin account: database lock poisoned"),
                };
            }
        }

        let dashboard = Self {
            port,
            bind_addr: bind_addr.to_string(),
            jwt_secret,
            db,
            sync_state: Arc::new(SyncState::new()),
            search_index: None,
            compression: TripleCompressor::new(),
            running: AtomicBool::new(false),
            active_connections: AtomicU64::new(0),
            rate_limits: Mutex::new(HashMap::new()),
            auth: security::AuthState::new(),
            server_thread: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
        };
        // P0-5: revocations persist in the db, not just in memory — hydrate
        // the in-memory gate at boot so logouts survive restarts. A locked
        // or unreadable db degrades to an empty gate (fail-open only for
        // tokens revoked before an unclean shutdown; live revokes persist
        // on the logout path below).
        if let Ok(guard) = dashboard.db.read() {
            match guard.list_live_revocations(security::now_secs()) {
                Ok(rows) => {
                    for (jti, exp) in rows {
                        dashboard.auth.revoke(&jti, exp);
                    }
                }
                Err(e) => warn!("Could not hydrate revocations: {}", e),
            }
        }
        // Resume persisted work at backend startup, not only after a caller
        // visits the cron/sync API. Docker uses this constructor but runs its
        // own listener, so wiring this only in `start()` misses that host.
        cybermanju_os::scheduler::ensure_started(Arc::clone(&dashboard.db));
        cybermanju_sync::scheduler::ensure_started(Arc::clone(&dashboard.db));
        dashboard
    }
    // <<< /AGENT-3 JWT SECRET >>>

    /// Accessor for the shared database handle.
    pub fn db(&self) -> &Arc<RwLock<Database>> {
        &self.db
    }

    /// Accessor for the shared sync state.
    pub fn sync_state(&self) -> &Arc<SyncState> {
        &self.sync_state
    }

    /// Wire the application's Tantivy index into the dashboard so REST search
    /// hits the same index as the desktop app.
    pub fn set_search_index(&mut self, index: Arc<RwLock<SearchIndex>>) {
        self.search_index = Some(index);
    }

    /// Start the web dashboard HTTP server on a background thread.
    /// Binds to 127.0.0.1 only. Returns Ok(()) on successful bind.
    #[allow(dead_code)]
    pub fn start(self: &std::sync::Arc<Self>) -> std::io::Result<()> {
        // Stop any previously running server
        self.stop();

        self.running.store(true, Ordering::SeqCst);
        let this = std::sync::Arc::clone(self);

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
        {
            let mut tx_guard = self.shutdown_tx.lock().expect("shutdown_tx lock poisoned");
            *tx_guard = Some(shutdown_tx);
        }

        let handle = thread::spawn(move || {
            let addr = format!("{}:{}", this.bind_addr, this.port);
            let listener = match TcpListener::bind(&addr) {
                Ok(l) => {
                    info!(
                        "Web Dashboard listening on http://{} (localhost only)",
                        addr
                    );
                    l
                }
                Err(e) => {
                    error!("Web Dashboard failed to bind on {}: {}", addr, e);
                    this.running.store(false, Ordering::SeqCst);
                    return;
                }
            };

            // Use non-blocking accept so we can poll the shutdown channel
            listener.set_nonblocking(true).ok();

            loop {
                // Check shutdown signals
                if !this.running.load(Ordering::SeqCst) {
                    break;
                }
                match shutdown_rx.try_recv() {
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => {
                        info!("Web Dashboard received shutdown signal");
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }

                // Non-blocking accept with 50ms poll interval
                match listener.accept() {
                    Ok((stream, _addr)) => {
                        let this_clone = std::sync::Arc::clone(&this);
                        thread::spawn(move || {
                            handle_connection(&this_clone, stream);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    Err(e) => {
                        if this.running.load(Ordering::SeqCst) {
                            warn!("Web Dashboard accept error: {}", e);
                        }
                    }
                }
            }
            info!("Web Dashboard server stopped");
        });

        // Store the thread handle for later joining
        {
            let mut thread_guard = self
                .server_thread
                .lock()
                .expect("server_thread lock poisoned");
            *thread_guard = Some(handle);
        }

        Ok(())
    }

    /// Signal the server to stop and join the thread.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);

        // Send shutdown signal via channel
        {
            let mut tx_guard = self.shutdown_tx.lock().expect("shutdown_tx lock poisoned");
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(());
            }
        }

        // Also connect to ourselves to unblock accept() if it somehow
        // isn't in non-blocking mode (defensive)
        let _ = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", self.port)
                .parse::<std::net::SocketAddr>()
                .unwrap_or_else(|_| "127.0.0.1:3456".parse().unwrap()),
            Duration::from_secs(1),
        );

        // Join the server thread to ensure clean shutdown
        {
            let mut thread_guard = self
                .server_thread
                .lock()
                .expect("server_thread lock poisoned");
            if let Some(handle) = thread_guard.take() {
                if let Err(e) = handle.join() {
                    error!("Web Dashboard thread join error: {:?}", e);
                }
            }
        }
    }
}

impl Drop for WebDashboard {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─── Connection handler ──────────────────────────────────────────────

/// Decrements `active_connections` when a connection handler returns.
struct ActiveConnectionGuard<'a>(&'a AtomicU64);

impl Drop for ActiveConnectionGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn handle_connection(dashboard: &WebDashboard, mut stream: TcpStream) {
    // <<< AGENT-3 MAX CONNECTIONS >>>
    // Refuse new work once the cap is reached instead of spawning an
    // unbounded number of threads (thread-per-connection had no ceiling).
    if dashboard.active_connections.load(Ordering::SeqCst) >= security::MAX_CONCURRENT_CONNECTIONS {
        write_http_json(
            &mut stream,
            503,
            r#"{"error":true,"status":503,"message":"Too many connections"}"#,
        );
        return;
    }
    // <<< /AGENT-3 MAX CONNECTIONS >>>

    dashboard.active_connections.fetch_add(1, Ordering::SeqCst);
    let _active = ActiveConnectionGuard(&dashboard.active_connections);

    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

    // Extract client IP for rate limiting
    let client_ip = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // ── Rate limiting check ──
    if !check_rate_limit(&dashboard.rate_limits, &client_ip) {
        write_http_json(
            &mut stream,
            429,
            r#"{"error":true,"status":429,"message":"Rate limit exceeded"}"#,
        );
        return;
    }

    // Parse the HTTP request using a fresh BufReader over a shared reference.
    // The BufReader borrow ends when `parse_http_request` returns,
    // leaving `stream` available for writing the response.
    let parse_result = parse_http_request(&stream);
    let ParsedRequest {
        method,
        path,
        body,
        auth_header,
        effective_origin,
    } = match parse_result {
        Ok(r) => r,
        Err((status, msg)) => {
            write_http_json(&mut stream, status, &msg);
            return;
        }
    };

    let effective_origin = effective_origin.as_deref();

    // SSE job tail: a long-lived `text/event-stream` on the same socket.
    // It must branch before `handle_request` (which can only return one
    // complete `Content-Length` response) so the stream below owns the
    // socket: headers now, one flushed frame per job change, heartbeat
    // comments while idle. Polling `GET /api/agent/jobs/:id` stays the
    // fallback. `EventSource` cannot send the `Authorization` header, so a
    // `?token=` query credential is accepted here (P0-7) — header first,
    // query second, never logged — and enforced by the same JWT gate inside
    // `handle_sse_connection`. A missing credential answers 401, never a
    // stream.
    if let Some(job_id) = is_sse_events_path(&method, &path) {
        let authed: Option<String> = auth_header.clone().or_else(|| {
            path.split('?')
                .nth(1)
                .and_then(|q| parse_query_param(q, "token"))
                .filter(|t| !t.trim().is_empty())
                .map(|t| format!("Bearer {}", t.trim()))
        });
        handle_sse_connection(
            dashboard,
            stream,
            &job_id,
            authed.as_deref(),
            effective_origin,
        );
        return;
    }

    // Handle the request — the database lock is taken inside `handle_request`
    let response = handle_request(
        dashboard,
        &dashboard.db,
        &method,
        &path,
        &body,
        auth_header.as_deref(),
        effective_origin,
    );

    let _ = stream.write_all(response.as_bytes());
}

struct ParsedRequest {
    method: String,
    path: String,
    body: String,
    auth_header: Option<String>,
    effective_origin: Option<String>,
}

/// Parse an HTTP request from a TcpStream without writing to it.
///
/// Hardened: the request line, every header line and the header count are
/// capped, and `Content-Length` is validated with
/// `security::enforce_body_limit` before a single body byte is read.
fn parse_http_request(stream: &TcpStream) -> Result<ParsedRequest, (u16, String)> {
    let mut reader = BufReader::new(stream);

    // Read request line (capped — an unbounded line is a memory DoS)
    let mut request_line = String::new();
    let read = reader
        .by_ref()
        .take(security::MAX_REQUEST_LINE_BYTES as u64 + 2)
        .read_line(&mut request_line)
        .map_err(|_| {
            (
                400,
                r#"{"error":true,"status":400,"message":"Bad Request"}"#.to_string(),
            )
        })?;
    if read == 0 || request_line.len() > security::MAX_REQUEST_LINE_BYTES {
        return Err((
            400,
            r#"{"error":true,"status":400,"message":"Request line too long"}"#.to_string(),
        ));
    }
    let request_line = request_line.trim();

    // Parse method and path from "GET /path HTTP/1.1"
    let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err((
            400,
            r#"{"error":true,"status":400,"message":"Bad Request"}"#.to_string(),
        ));
    }
    let method = parts[0].to_string();
    let path = parts[1].to_string();

    // Read headers — extract Content-Length, Authorization, Origin
    let mut content_length: usize = 0;
    let mut auth_header: Option<String> = None;
    let mut origin_header: Option<String> = None;
    let mut header_lines: usize = 0;

    loop {
        if header_lines >= security::MAX_HEADER_LINES {
            return Err((
                400,
                r#"{"error":true,"status":400,"message":"Too many headers"}"#.to_string(),
            ));
        }
        let mut line = String::new();
        let read = reader
            .by_ref()
            .take(security::MAX_HEADER_LINE_BYTES as u64 + 2)
            .read_line(&mut line)
            .unwrap_or(0);
        if read == 0 {
            break;
        }
        if line.len() > security::MAX_HEADER_LINE_BYTES {
            return Err((
                400,
                r#"{"error":true,"status":400,"message":"Header line too long"}"#.to_string(),
            ));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        header_lines += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_lowercase();

        if lower.starts_with("content-length:") {
            content_length = trimmed[15..].trim().parse().unwrap_or(0);
        } else if lower.starts_with("authorization:") {
            if trimmed.len() > 14 + security::MAX_AUTH_HEADER_BYTES {
                return Err((
                    401,
                    r#"{"error":true,"status":401,"message":"Authorization header too long"}"#
                        .to_string(),
                ));
            }
            auth_header = Some(trimmed[14..].trim().to_string());
        } else if lower.starts_with("origin:") {
            origin_header = Some(trimmed[7..].trim().to_string());
        }
    }

    // ── Body size limit enforcement ──
    let content_length =
        security::enforce_body_limit(content_length).map_err(|(status, msg)| {
            (
                status,
                serde_json::json!({"error":true,"status":status,"message":msg}).to_string(),
            )
        })?;

    // Read body if present (capped to MAX_BODY_SIZE for safety)
    let body = if content_length > 0 {
        let read_size = content_length.min(MAX_BODY_SIZE);
        let mut buf = vec![0u8; read_size];
        if std::io::Read::read_exact(&mut reader, &mut buf).is_err() {
            String::new()
        } else {
            String::from_utf8(buf).unwrap_or_default()
        }
    } else {
        String::new()
    };

    // Determine effective CORS origin for response headers
    let effective_origin = security::cors_origin_allowed(origin_header.as_deref());

    Ok(ParsedRequest {
        method,
        path,
        body,
        auth_header,
        effective_origin,
    })
}

/// Write a JSON HTTP response (transport-level errors: 400/413/429/503).
fn write_http_json(stream: &mut TcpStream, status: u16, body: &str) {
    let resp = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: application/json\r\n\
         {}\
         Content-Length: {}\r\n\
         \r\n\
         {}",
        status,
        status_text(status),
        security::security_headers(),
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
}

// ─── Request router ──────────────────────────────────────────────────

/// Either flavour of database lock, so one code path can serve both readers
/// (GET/HEAD) and writers (POST/PUT/DELETE) with a single route table.
enum DbLock<'a> {
    Read(std::sync::RwLockReadGuard<'a, Database>),
    Write(std::sync::RwLockWriteGuard<'a, Database>),
}

impl<'a> DbLock<'a> {
    fn db(&self) -> &Database {
        match self {
            DbLock::Read(guard) => guard,
            DbLock::Write(guard) => guard,
        }
    }
}

/// Parse a JSON request body, returning an HTTP 400 response on failure.
macro_rules! json_body {
    ($body:expr, $origin:expr) => {
        match serde_json::from_str($body) {
            Ok(v) => v,
            Err(e) => return json_error(400, &format!("Invalid JSON: {}", e), $origin),
        }
    };
}

/// Map byte-pipeline failures without presenting a failed operation as success.
fn crypto_response<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Err(e) if e.starts_with("unsupported:") => json_error(501, &e, origin),
        Err(e) if e.starts_with("integrity:") || e.starts_with("conflict:") => {
            json_error(409, &e, origin)
        }
        Err(e) if e.starts_with("too_large:") => json_error(413, &e, origin),
        other => auth_response(other, origin),
    }
}

/// Serialize a value as a 200 JSON response.
fn json_ok<T: Serialize>(value: &T, origin: Option<&str>) -> String {
    let body = match serde_json::to_string(value) {
        Ok(body) => body,
        Err(_) => "null".to_string(),
    };
    http_response(200, "application/json", &body, origin)
}

/// Render a shared-API `Result` as an HTTP response: 200 on success, 404
/// when the message reports a missing entity, 403 when it reports an
/// `auth:` denial, 400 otherwise.
///
/// `api_response` would flatten `auth:` into 400 — every owner/object gate
/// in this router goes through here so denials stay 403.
fn auth_response<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => json_ok(&value, origin),
        Err(e) if e.starts_with("auth:") => json_error(403, &e, origin),
        Err(e) => api_response::<T>(Err(e), origin),
    }
}

/// P0-1 object gate: deny when `claims` may not use `file_id` at `required`
/// (`read`/`write`). Returns `Some(response)` when denied (`403 auth:` or
/// `404 not_found:`), `None` when allowed or when the route is public
/// (token-capability share bytes carry no claims).
fn deny_file(
    db: &Database,
    claims: &Option<security::Claims>,
    file_id: &str,
    required: &str,
    origin: Option<&str>,
) -> Option<String> {
    let claims = claims.as_ref()?;
    match api::files::check_access(db, claims, file_id, required) {
        Ok(()) => None,
        Err(e) if e.starts_with("not_found:") => Some(json_error(404, &e, origin)),
        Err(e) => Some(json_error(403, &e, origin)),
    }
}

/// `(user_id, is_admin)` for the P0-10 owner checks, from verified claims.
/// `None` claims (public share bytes) map to an empty untrusted id — scoped
/// helpers then deny unless the check is bypassed for trusted-local calls.
fn claims_owner(claims: &Option<security::Claims>) -> (String, bool) {
    match claims {
        Some(c) => (c.user_id.clone(), c.role == "admin"),
        None => (String::new(), false),
    }
}

/// Render a shared-API `Result` as an HTTP response: 200 on success, 404
/// when the message reports a missing entity, 400 otherwise.
fn api_response<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("auth: privileged agent configuration:") {
                403
            } else if message.to_lowercase().contains("not found")
                || message.starts_with("not_found:")
            {
                404
            } else {
                400
            };
            json_error(status, &message, origin)
        }
    }
}

// <<< AGENT-4 OPS: access log + metrics wrapper >>>
// Every request that enters `handle_request` leaves exactly one structured
// line (target `access`) with request id, method, path, status, latency and
// auth outcome, and bumps the counters exposed at `GET /api/metrics`.
// The routing itself lives in `route_request`.
pub fn handle_request(
    dashboard: &WebDashboard,
    db: &Arc<RwLock<Database>>,
    method: &str,
    path: &str,
    body: &str,
    auth_header: Option<&str>,
    origin: Option<&str>,
) -> String {
    let started = Instant::now();
    let request_id = format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    );

    let response = route_request(dashboard, db, method, path, body, auth_header, origin);
    let status = response_status(&response);

    REQUESTS_TOTAL.fetch_add(1, Ordering::SeqCst);
    match status {
        400..=499 => {
            RESPONSES_4XX.fetch_add(1, Ordering::SeqCst);
        }
        500..=599 => {
            RESPONSES_5XX.fetch_add(1, Ordering::SeqCst);
        }
        _ => {}
    }

    // `401` means the credential check ran and failed; without a header the
    // route was served anonymously (public endpoint); with a header and no
    // 401 the credentials were accepted.
    let auth = if status == 401 {
        "rejected"
    } else if auth_header.is_none() {
        "anonymous"
    } else {
        "accepted"
    };
    // Never log secrets: strip the query string (OAuth `?code=&state=`)
    // and mask share tokens (capability secrets in the path).
    let logged_path = match path.split_once('?') {
        Some((p, _)) => p.to_string(),
        None => path.to_string(),
    };
    let logged_path = match logged_path.strip_prefix("/api/shared/") {
        Some(_) => "/api/shared/<redacted>".to_string(),
        None => logged_path,
    };
    info!(
        target: "access",
        "rid={} method={} path={} status={} latency_ms={} auth={}",
        request_id,
        method,
        logged_path,
        status,
        started.elapsed().as_millis(),
        auth
    );

    response
}

/// HTTP status code from the first line of a raw HTTP response.
fn response_status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// Route a request to its handler. Called only by `handle_request`, which
/// wraps it with access logging and metrics.
fn route_request(
    dashboard: &WebDashboard,
    db: &Arc<RwLock<Database>>,
    method: &str,
    path: &str,
    body: &str,
    auth_header: Option<&str>,
    origin: Option<&str>,
) -> String {
    // Handle CORS preflight requests
    if method == "OPTIONS" {
        return cors_preflight_response(origin);
    }

    // Parse query string from path
    let (path_clean, query) = match path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path, ""),
    };

    let path_segments: Vec<&str> = path_clean
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    // Unknown path → 404, before any credential is inspected. Doing it the
    // other way round makes an anonymous probe of a typo'd route answer `401`,
    // which both hides the 404 arm below and tells a caller nothing it did
    // not already know (the gate rejects every unauthenticated path alike).
    if !security::is_known_route(&path_segments) {
        return json_error(404, &format!("Not found: {} {}", method, path), origin);
    }

    // <<< AGENT-3 AUTH GATE >>>
    // Route → required role, then verify the JWT and authorize its claims.
    // Default is `Authenticated`; `Public` covers health/login/register/
    // share-link/OAuth-callback/AGENT-4 probes, `Admin` is the narrow table
    // in `security::required_role`. P0-2: `authorize` also takes the method
    // so `viewer` stays read-only (writes denied even on `Authenticated`
    // routes).
    let required = security::required_role(method, &path_segments);
    let claims: Option<security::Claims> = match required {
        security::RequiredRole::Public => None,
        _ => match verify_jwt_auth(dashboard, auth_header, origin) {
            Ok(claims) => Some(claims),
            Err(resp) => return resp,
        },
    };
    if let Some(claims) = &claims {
        if let Err(reason) = security::authorize(claims, required, method) {
            return json_error(403, &format!("Forbidden: {}", reason), origin);
        }
    }
    // <<< /AGENT-3 AUTH GATE >>>

    // <<< AGENT-2 ROUTES: sync run lifecycle (lockless) >>>
    // The sync pipeline acquires its own per-file locks, so a run must not
    // hold the request lock (it would deadlock). These arms therefore return
    // before the database lock is taken; where a handler needs the database
    // it takes (and releases) its own short-lived lock internally.
    match path_segments.as_slice() {
        ["api", "sync", "status"] if method == "GET" => {
            // Lazily start the auto-sync scheduler (item 11): the first
            // contact with the sync API arms the background scan.
            cybermanju_sync::scheduler::ensure_started(Arc::clone(db));
            // P0-10: non-admins read their own latest run, never `current_file`
            // of someone else's run.
            let (owner_id, is_admin) = claims_owner(&claims);
            let progress =
                api::sync_api::latest_progress_for(dashboard.sync_state(), &owner_id, is_admin);
            // P0-10: non-admins see their own latest run's provider, never
            // another user's.
            let provider = if is_admin || owner_id.is_empty() {
                cybermanju_sync::state::RunRegistry::global()
                    .latest()
                    .map(|run| run.config_id.clone())
            } else {
                cybermanju_sync::state::RunRegistry::global()
                    .latest_for(&owner_id, false)
                    .map(|run| run.config_id.clone())
            };
            let status = serde_json::json!({
                "syncEnabled": true,
                "status": progress.status,
                "lastSync": progress.started_at,
                "provider": provider,
            });
            return http_response(
                200,
                "application/json",
                &serde_json::to_string(&status).unwrap_or_default(),
                origin,
            );
        }
        ["api", "sync", "progress"] if method == "GET" => {
            cybermanju_sync::scheduler::ensure_started(Arc::clone(db));
            // P0-10: non-admins read their own latest run, never `current_file`
            // of someone else's run.
            let (owner_id, is_admin) = claims_owner(&claims);
            return json_ok(
                &api::sync_api::latest_progress_for(dashboard.sync_state(), &owner_id, is_admin),
                origin,
            );
        }
        ["api", "sync", "cancel"] if method == "POST" => {
            // Empty body (and the legacy `{}`) means "cancel the latest run".
            // P0-10: scoped — `None` cancels the caller's latest own run,
            // never the global latest (admins keep legacy behaviour).
            let req: api::sync_api::CancelRequest = if body.trim().is_empty() {
                Default::default()
            } else {
                json_body!(body, origin)
            };
            let (owner_id, is_admin) = claims_owner(&claims);
            return json_ok(
                &api::sync_api::cancel_job_as(req.job_id.as_deref(), &owner_id, is_admin),
                origin,
            );
        }
        ["api", "sync", "start"] if method == "POST" => {
            // 202 + job id — the pipeline runs on a worker thread; the
            // request thread must never wait on a provider.
            let req: api::sync_api::StartRequest = json_body!(body, origin);
            let (owner_id, _) = claims_owner(&claims);
            // P0-1: every synced id must be readable by the caller.
            {
                let guard = match db.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(claims) = &claims {
                    if let Err(e) =
                        api::files::check_access_many(&guard, claims, &req.file_ids, "read")
                    {
                        let status = if e.starts_with("not_found:") {
                            404
                        } else {
                            403
                        };
                        return json_error(status, &e, origin);
                    }
                }
            }
            return match api::sync_api::start_job_for(db, &req.config_id, req.file_ids, &owner_id) {
                Ok(job) => http_response(
                    202,
                    "application/json",
                    &serde_json::to_string(&job).unwrap_or_else(|_| "{}".to_string()),
                    origin,
                ),
                // Legacy `auth:` here means "no provider key saved" (400 by
                // contract) — ownership is stored, never denied, on start.
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        ["api", "sync", "jobs", job_id] if method == "GET" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            // Foreign runs answer `not_found:` (indistinguishable by design),
            // so the plain mapper keeps every status exactly as before.
            return api_response(
                api::sync_api::job_for(db, job_id, &owner_id, is_admin),
                origin,
            );
        }
        ["api", "sync", "runs"] if method == "GET" => {
            // P0-10: admins see all rows, everyone else only their own runs
            // plus legacy ownerless rows.
            let (owner_id, is_admin) = claims_owner(&claims);
            return api_response(api::sync_api::runs_for(db, 20, &owner_id, is_admin), origin);
        }
        ["api", "sync", "restore"] if method == "POST" => {
            let req: api::sync_api::RestoreRequest = json_body!(body, origin);
            // P0-1: restoring by file rewrites local bytes — needs `write`
            // on that file. Remote-path restores carry no file id and rely
            // on the role gate.
            if let Some(file_id) = req.file_id.as_deref() {
                let guard = match db.read() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(claims) = &claims {
                    if let Err(e) = api::files::check_access(&guard, claims, file_id, "write") {
                        let status = if e.starts_with("not_found:") {
                            404
                        } else {
                            403
                        };
                        return json_error(status, &e, origin);
                    }
                }
            }
            return api_response(api::sync_api::restore(db, req), origin);
        }
        ["api", "sync", "remote"] if method == "DELETE" => {
            let req: api::sync_api::RemoteDeleteRequest = json_body!(body, origin);
            return match api::sync_api::delete_remote(db, req) {
                Ok(value) => json_ok(&value, origin),
                // Honest 501: a backend that cannot delete says
                // `unsupported: …` and must not be dressed up as a 400.
                Err(e) if e.starts_with("unsupported:") => json_error(501, &e, origin),
                Err(e) => api_response::<bool>(Err(e), origin),
            };
        }
        ["api", "sync", "usage", config_id] if method == "GET" => {
            return api_response(api::sync_api::usage(db, config_id), origin);
        }
        ["api", "sync", "test"] if method == "POST" => {
            let req: api::sync_api::ConfigRequest = json_body!(body, origin);
            return api_response(api::sync_api::test_connection(&req.config), origin);
        }
        ["api", "sync", "remote-files"] if method == "POST" => {
            let req: api::sync_api::RemoteFilesRequest = json_body!(body, origin);
            return api_response(
                api::sync_api::list_remote_files(&req.config, &req.prefix),
                origin,
            );
        }
        ["api", "sync", "create-repo"] if method == "POST" => {
            let req: api::sync_api::CreateRepoRequest = json_body!(body, origin);
            return match api::sync_api::create_repo(db, req) {
                Ok(value) => json_ok(&value, origin),
                Err(e) if e.starts_with("unsupported:") => json_error(501, &e, origin),
                Err(e) if e.starts_with("conflict:") => json_error(409, &e, origin),
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        ["api", "sync", "seed-repo"] if method == "POST" => {
            let req: api::sync_api::SeedRepoRequest = json_body!(body, origin);
            return match api::sync_api::seed_repo(req) {
                Ok(value) => json_ok(&value, origin),
                Err(e) if e.starts_with("unsupported:") => json_error(501, &e, origin),
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        ["api", "sync", "upload"] if method == "POST" => {
            let req: api::sync_api::UploadRequest = json_body!(body, origin);
            return match api::sync_api::upload_bytes(req) {
                Ok(value) => json_ok(&value, origin),
                Err(e) if e.starts_with("unsupported:") => json_error(501, &e, origin),
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        ["api", "sync", "move"] if method == "POST" => {
            let req: api::sync_api::MoveRequest = json_body!(body, origin);
            return match api::sync_api::move_file(db, req) {
                Ok(value) => json_ok(&value, origin),
                Err(e) if e.starts_with("conflict:") => json_error(409, &e, origin),
                Err(e) if e.starts_with("unsupported:") => json_error(501, &e, origin),
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        _ => {}
    }
    // <<< /AGENT-2 ROUTES >>>

    // <<< CRON RUN-NOW (lockless) >>>
    // `POST /api/cron/{id}/run` executes a script synchronously — the
    // request lock must not be held across the run. The Database is cloned
    // (shares the redb handle) and the script runs outside any app lock,
    // same contract as `POST /api/sync/start`.
    if path_segments.get(1) == Some(&"cron") {
        cybermanju_os::scheduler::ensure_started(Arc::clone(db));
    }
    if let ["api", "cron", id, "run"] = path_segments.as_slice() {
        if method == "POST" {
            let handle = match db.read() {
                Ok(guard) => guard.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            };
            return api_response(cybermanju_os::scheduler::run_now(&handle, id), origin);
        }
    }
    // <<< /CRON RUN-NOW >>>

    // <<< AI AGENT JOBS (lockless) >>>
    // Prompt/abort/approve must not hold the request lock: start_job takes
    // its own short reads (and spawns the worker), so running it under the
    // write lock would deadlock — same contract as `POST /api/sync/start`.
    // Reads/writes below take no database at all (in-memory registry).
    match path_segments.as_slice() {
        ["api", "agent", "prompt"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct PromptBody {
                #[serde(default)]
                config_id: String,
                #[serde(default)]
                session_id: Option<String>,
                #[serde(default)]
                prompt: String,
            }
            let req: PromptBody = json_body!(body, origin);
            // P0-10: the job is owned by the JWT caller.
            let (owner_id, is_admin) = claims_owner(&claims);
            return match api::agent_api::start_job_as(
                db,
                &req.config_id,
                req.session_id,
                req.prompt,
                &owner_id,
                is_admin,
            ) {
                Ok(job) => http_response(
                    202,
                    "application/json",
                    &serde_json::to_string(&job).unwrap_or_else(|_| "{}".to_string()),
                    origin,
                ),
                // Legacy `auth:` here means "no provider key saved" (400 by
                // contract) — ownership is stored, never denied, on start.
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        ["api", "agent", "jobs"] if method == "GET" => {
            // P0-10: admins see all jobs, everyone else only their own.
            let (owner_id, is_admin) = claims_owner(&claims);
            return json_ok(&api::agent_api::list_jobs_for(&owner_id, is_admin), origin);
        }
        ["api", "agent", "jobs", job_id] if method == "GET" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            return auth_response(
                api::agent_api::job_status_for(job_id, &owner_id, is_admin),
                origin,
            );
        }
        ["api", "agent", "jobs", job_id, "abort"] if method == "POST" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            return auth_response(
                api::agent_api::abort_job_for(job_id, &owner_id, is_admin),
                origin,
            );
        }
        ["api", "agent", "jobs", job_id, "approve"] if method == "POST" => {
            #[derive(Deserialize)]
            struct ApproveBody {
                #[serde(default)]
                approved: bool,
                #[serde(default)]
                answer: Option<String>,
                #[serde(default)]
                remember: bool,
            }
            let req: ApproveBody = json_body!(body, origin);
            let (owner_id, is_admin) = claims_owner(&claims);
            return auth_response(
                api::agent_api::approve_job_for(
                    db,
                    job_id,
                    req.approved,
                    req.answer,
                    req.remember,
                    &owner_id,
                    is_admin,
                ),
                origin,
            );
        }
        ["api", "agent", "init"] if method == "POST" => {
            // Lockless like prompt: spawns a worker with its own locks.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct InitBody {
                config_id: String,
            }
            let req: InitBody = json_body!(body, origin);
            let (owner_id, is_admin) = claims_owner(&claims);
            return match api::agent_api::start_init_job_as(db, &req.config_id, &owner_id, is_admin)
            {
                Ok(job) => http_response(
                    202,
                    "application/json",
                    &serde_json::to_string(&job).unwrap_or_else(|_| "{}".to_string()),
                    origin,
                ),
                Err(e) => api_response::<()>(Err(e), origin),
            };
        }
        ["api", "agent", "sessions", session_id, "compact"] if method == "POST" => {
            // Lockless: compaction is a full provider round trip — it must
            // never hold the request lock. Reads/writes inside are brief.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct CompactBody {
                config_id: String,
            }
            let req: CompactBody = json_body!(body, origin);
            let (owner_id, is_admin) = claims_owner(&claims);
            return api_response(
                api::agent_api::compact_session_for(
                    db,
                    &req.config_id,
                    session_id,
                    &owner_id,
                    is_admin,
                ),
                origin,
            );
        }
        // ─── semantic memory (redb `agent_memories`: text + vectors) ──
        ["api", "agent", "memories"] if method == "GET" => {
            // Newest-first, vectors stripped unless `?vectors=true`
            // (export/sync restores need them; list views never do).
            // Owner-scoped like sessions (P0-12).
            let config_id = parse_query_param(query, "configId");
            let vectors = parse_query_param(query, "vectors").as_deref() == Some("true");
            let (owner_id, is_admin) = claims_owner(&claims);
            return match db.read() {
                Ok(guard) => api_response(
                    api::agent_api::list_memories(
                        &guard,
                        config_id.as_deref().filter(|s| !s.is_empty()),
                        vectors,
                        &owner_id,
                        is_admin,
                    ),
                    origin,
                ),
                Err(e) => api_response::<Vec<cybermanju_types::agent::AgentMemory>>(
                    Err(e.to_string()),
                    origin,
                ),
            };
        }
        ["api", "agent", "memories"] if method == "POST" => {
            // Lockless: storing embeds via a provider round trip — it must
            // never hold the request lock. Reads/writes inside are brief.
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct MemoryStoreBody {
                #[serde(default)]
                config_id: String,
                #[serde(default)]
                session_id: Option<String>,
                #[serde(default)]
                text: String,
            }
            let req: MemoryStoreBody = json_body!(body, origin);
            let (owner_id, _) = claims_owner(&claims);
            return api_response(
                api::agent_api::store_memory_entry(
                    db,
                    &req.config_id,
                    req.session_id,
                    &req.text,
                    &owner_id,
                ),
                origin,
            );
        }
        ["api", "agent", "memories", memory_id] if method == "DELETE" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            return match db.read() {
                Ok(guard) => api_response(
                    api::agent_api::delete_memory(&guard, memory_id, &owner_id, is_admin),
                    origin,
                ),
                Err(e) => api_response::<bool>(Err(e.to_string()), origin),
            };
        }
        ["api", "agent", "memories", "recall"] if method == "POST" => {
            // Lockless: recall may spend one embeddings call, then ranks
            // off brief reads. `configId` optional — without it the recall
            // is keyword-only across every config (global memory search).
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct MemoryRecallBody {
                #[serde(default)]
                config_id: Option<String>,
                #[serde(default)]
                query: String,
                #[serde(default)]
                top_k: Option<u64>,
            }
            let req: MemoryRecallBody = json_body!(body, origin);
            let (owner_id, is_admin) = claims_owner(&claims);
            return api_response(
                api::agent_api::recall_memory_entries(
                    db,
                    req.config_id.as_deref().filter(|s| !s.is_empty()),
                    &req.query,
                    req.top_k.unwrap_or(3) as usize,
                    &owner_id,
                    is_admin,
                ),
                origin,
            );
        }
        ["api", "agent", "memories", "export"] if method == "GET" => {
            // Portable envelope for sync restore: Hermes-compatible markdown
            // plus the full rows (vectors included).
            let config_id = parse_query_param(query, "configId");
            return match db.read() {
                Ok(guard) => {
                    let scope = config_id.as_deref().filter(|s| !s.is_empty());
                    let (owner_id, is_admin) = claims_owner(&claims);
                    match api::agent_api::list_memories(&guard, scope, true, &owner_id, is_admin) {
                        Ok(memories) => {
                            let markdown =
                                cybermanju_agent::memory::render_export_markdown(&memories);
                            api_response(
                                Ok::<_, String>(serde_json::json!({
                                    "markdown": markdown,
                                    "memories": memories,
                                })),
                                origin,
                            )
                        }
                        Err(e) => api_response::<serde_json::Value>(Err(e), origin),
                    }
                }
                Err(e) => api_response::<serde_json::Value>(Err(e.to_string()), origin),
            };
        }
        ["api", "agent", "configs", config_id, "mcp", "tools"] if method == "GET" => {
            // Lockless: discovery spawns processes with 15s budgets each.
            // Config load is one brief read; the rest holds no lock.
            let config = match db.read() {
                Ok(guard) => api::agent_api::get_config(&guard, config_id),
                Err(e) => Err(e.to_string()),
            };
            return match config {
                Ok(config) => {
                    let (_, is_admin) = claims_owner(&claims);
                    api_response(api::agent_api::mcp_tools_for(&config, is_admin), origin)
                }
                Err(e) => api_response::<Vec<api::agent_api::McpToolView>>(Err(e), origin),
            };
        }
        ["api", "agent", "jobs", _, "events"] if method == "GET" => {
            // The live tail never reaches this router: `handle_connection`
            // intercepts the path and streams `text/event-stream` on the
            // socket. A caller that arrives here (tests, embedded use) gets
            // the honest one-shot equivalent instead of a silent 404.
            return json_error(
                400,
                "GET /api/agent/jobs/:id/events needs an SSE stream; \
                 poll GET /api/agent/jobs/:id instead",
                origin,
            );
        }
        _ => {}
    }
    // <<< /AI AGENT JOBS >>>

    // <<< CYBSH SYNC START (real, lockless) >>>
    // `sync start …` typed into the terminal arrives as POST /api/os/exec,
    // whose normal handler runs under the request write lock — under which
    // start_job would deadlock on its own config read. Intercept the line
    // here, before any lock is taken, and run it as a detached job exactly
    // like POST /api/sync/start. Every other line falls through untouched.
    // (One `if let`: nested `if` + `if let` trips `collapsible_if`, which is
    // denied workspace-wide.)
    if let Some(resp) = (method == "POST"
        && matches!(path_segments.as_slice(), ["api", "os", "exec"]))
    .then(|| api::os_api::try_sync_start_exec(db, body, origin, claims.as_ref()))
    .flatten()
    {
        return resp;
    }
    // <<< /CYBSH SYNC START >>>

    // <<< CYBSH AI (real, lockless) >>>
    // `ai ask …` typed into the terminal arrives as POST /api/os/exec. The
    // agent registry lives here (not in `cybermanju-os`, which cannot depend
    // back on this crate), so — like `sync start` above — the line is
    // intercepted before any lock is taken and run as a detached agent job.
    // `ai status/abort/sessions` are registry/database reads the normal
    // locked path cannot serve either (the job map is process-global), so
    // every `ai` line dispatches here; anything else falls through.
    if let Some(resp) = (method == "POST"
        && matches!(path_segments.as_slice(), ["api", "os", "exec"]))
    .then(|| {
        let (owner_id, is_admin) = claims_owner(&claims);
        api::agent_api::try_ai_exec(db, body, origin, &owner_id, is_admin)
    })
    .flatten()
    {
        return resp;
    }
    // <<< /CYBSH AI >>>

    // Take the database lock for the duration of the request. Readers share
    // the lock; writers (POST/PUT/DELETE) take it exclusively. A poisoned lock
    // is recovered from rather than propagated, so one panicking request can
    // not wedge every later one.
    //
    // P0-22 serialization discipline: the transfer paths that move bulk bytes
    // (sync restore/seed/upload/move/list) and the detached-job paths
    // (`sync start`, `ai …`, repair/scrub/gc) all run BEFORE this lock is
    // taken, with brief scoped locks inside — a long download/upload never
    // stalls writers here. Bundle export/import and the op-log exchange stay
    // guarded but are capped (10k ops per call, paged pulls), so no guarded
    // request clones unbounded state. What else stays guarded is deliberate:
    // os_exec shell lines and disk attach/check are read-modify-write
    // sequences whose logical atomicity the guard provides (redb serializes
    // storage txns, not app logic); repair status is capped (`status_paged`)
    // and the write-behind queues are bounded.
    let lock = if matches!(method, "GET" | "HEAD") {
        match db.read() {
            Ok(guard) => DbLock::Read(guard),
            Err(poisoned) => DbLock::Read(poisoned.into_inner()),
        }
    } else {
        match db.write() {
            Ok(guard) => DbLock::Write(guard),
            Err(poisoned) => DbLock::Write(poisoned.into_inner()),
        }
    };
    let db: &Database = lock.db();
    let (owner_id, is_admin) = claims_owner(&claims);

    // <<< CYBERMANJU OS PRE-WIRE: three route families, one file each >>>
    // Each `route()` returns `Some(response)` only for paths it owns and
    // `None` otherwise, so the families compose without anyone having to edit
    // this match. They run after the auth gate, like every other handler.
    // Long work (scrub/repair/compute) must be spawned, not run in here —
    // the database lock is held for the duration of the request.
    if let Some(resp) = api::disk_api::route(db, method, &path_segments, body, origin) {
        return resp;
    }
    if let Some(resp) = api::repair_api::route(db, method, &path_segments, body, origin) {
        return resp;
    }
    if let Some(resp) = api::os_api::route(db, method, &path_segments, query, body, origin) {
        return resp;
    }
    if let Some(resp) = api::cron_api::route(db, method, &path_segments, body, origin) {
        return resp;
    }
    if let Some(resp) = api::secrets::route(db, method, &path_segments, body, origin) {
        return resp;
    }
    // <<< ANALYSIS 100%: device identity (§2B.1) — enrollment is
    // idempotent; revocation is Admin-gated in `required_role` below. >>>
    if let Some(resp) = api::devices::route(
        db,
        method,
        &path_segments,
        body,
        &owner_id,
        is_admin,
        origin,
    ) {
        return resp;
    }
    // <<< ANALYSIS 100%: server todos (§4.9 agent bridge) — owner-scoped
    // for non-admins (P0-10 posture); admins see every owner. >>>
    if let Some(resp) = api::todos::route(
        db,
        method,
        &path_segments,
        body,
        origin,
        &owner_id,
        is_admin,
    ) {
        return resp;
    }
    // <<< ANALYSIS 100%: op-log exchange (§2C) — idempotent push, paged
    // pull, fork-pair conflicts for the UI. >>>
    if let Some(resp) = api::oplog::route(
        db,
        method,
        &path_segments,
        query,
        body,
        api::oplog::OplogAuth {
            owner_id: &owner_id,
            is_admin,
        },
        origin,
    ) {
        return resp;
    }
    // <<< ANALYSIS 100%: LAN peers (§2B.2) + sneakernet bundles (§2B.4).
    // The scan holds only the shared read lock and is capped at 2 s, so a
    // slow LAN never stalls writers (P0-22 discipline). >>>
    if method == "GET" && matches!(path_segments.as_slice(), ["api", "peers"]) {
        return lan_peers(origin);
    }
    if method == "POST"
        && path_segments.len() == 4
        && path_segments[0] == "api"
        && path_segments[1] == "peers"
        && path_segments[3] == "sync"
    {
        return peer_sync(db, path_segments[2], body, &owner_id, origin);
    }
    // Server-assisted presence + NAT rendezvous (§2B.2 PEX — the answer for
    // android/ios/docker/wasm, where mDNS multicast cannot go).
    if method == "GET" && matches!(path_segments.as_slice(), ["api", "peers", "known"]) {
        return peers_known(db, &owner_id, is_admin, origin);
    }
    if method == "POST" && matches!(path_segments.as_slice(), ["api", "peers", "announce"]) {
        return peers_announce(db, body, &owner_id, origin);
    }
    if method == "POST" && matches!(path_segments.as_slice(), ["api", "relay", "drop"]) {
        return relay_drop(db, body, &owner_id, origin);
    }
    if method == "POST" && matches!(path_segments.as_slice(), ["api", "relay", "poll"]) {
        return relay_poll(db, body, &owner_id, origin);
    }
    if method == "POST" && matches!(path_segments.as_slice(), ["api", "relay", "ack"]) {
        return relay_ack(db, body, &owner_id, origin);
    }
    if method == "POST" && matches!(path_segments.as_slice(), ["api", "sync", "bundle", "build"]) {
        return bundle_build(db, body, origin);
    }
    if method == "GET"
        && matches!(
            path_segments.as_slice(),
            ["api", "sync", "bundle", "export"]
        )
    {
        return bundle_export(db, query, origin);
    }
    if method == "POST"
        && matches!(
            path_segments.as_slice(),
            ["api", "sync", "bundle", "import"]
        )
    {
        return bundle_import(db, body, origin);
    }
    // <<< /CYBERMANJU OS PRE-WIRE >>>

    // Route to appropriate handler
    match path_segments.as_slice() {
        // ─── Auth endpoint (JWT login) ────────────────────────────
        ["api", "auth", "login"] | ["api", "users", "login"] if method == "POST" => {
            login_user(db, body, dashboard, origin)
        }

        // ─── Auth status (public first-run probe) ───────────────
        // `GET /api/auth/status → 200 { registrationOpen }` on every
        // device, no token needed. The Docker/web login gate uses it to
        // decide between "create the first account" and "sign in".
        ["api", "auth", "status"] if method == "GET" => match api::users::registration_open(db) {
            Ok(open) => json_ok(&serde_json::json!({ "registrationOpen": open }), origin),
            Err(e) => json_error(500, &e, origin),
        },

        // ─── User registration ────────────────────────────────────
        // Public, but bootstrap-only inside the handler and the granted
        // role can never be `admin` (see P0-1).
        ["api", "users", "register"] if method == "POST" => register_user_web(
            db,
            body,
            None,
            api::users::RegistrationMode::Bootstrap,
            origin,
        ),

        // ─── File endpoints ───────────────────────────────────────
        ["api", "files"] if method == "GET" => {
            list_visible_files(db, claims.as_ref(), false, origin)
        }
        ["api", "files", "folder"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct CreateFolderBody {
                name: String,
                parent_id: String,
            }
            let req: CreateFolderBody = json_body!(body, origin);
            api_response(
                api::files::create_folder(db, req.name, req.parent_id),
                origin,
            )
        }
        ["api", "files", "rebuild-index"] if method == "POST" => {
            api_response(api::files::rebuild_parent_index(db), origin)
        }
        ["api", "files", id] if method == "GET" => {
            // P0-1: object-level read gate (fail-closed).
            if let Some(denied) = deny_file(db, &claims, id, "read", origin) {
                return denied;
            }
            get_by_id(db, Database::get_files_table(), id, origin)
        }
        ["api", "files", id, "encrypt"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            #[derive(Deserialize)]
            struct EncryptBody {
                algorithm: String,
            }
            let req: EncryptBody = json_body!(body, origin);
            crypto_response(api::batch::encrypt_one(db, id, &req.algorithm).map(|algorithm| {
                serde_json::json!({ "fileId": id, "encrypted": true, "algorithm": algorithm })
            }), origin)
        }
        ["api", "files", id, "decrypt"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            crypto_response(
                api::batch::decrypt_one(db, id).map(
                    |()| serde_json::json!({ "fileId": id, "encrypted": false, "algorithm": null }),
                ),
                origin,
            )
        }
        ["api", "files", id] if method == "DELETE" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            api_response(api::files::delete(db, id), origin)
        }
        ["api", "files", id, "rename"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct RenameBody {
                new_name: String,
            }
            let req: RenameBody = json_body!(body, origin);
            api_response(api::files::rename(db, id, req.new_name), origin)
        }
        ["api", "files", id, "move"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct MoveBody {
                parent_id: String,
            }
            let req: MoveBody = json_body!(body, origin);
            api_response(api::files::move_to(db, id, req.parent_id), origin)
        }
        ["api", "files", id, "duplicate"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            api_response(api::files::duplicate(db, id), origin)
        }
        ["api", "files", id, "tags"] if method == "PUT" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct TagsBody {
                #[serde(default)]
                tags: Vec<String>,
            }
            let req: TagsBody = json_body!(body, origin);
            api_response(api::files::set_tags(db, id, req.tags), origin)
        }
        ["api", "files", id, "preview"] if method == "GET" => {
            if let Some(denied) = deny_file(db, &claims, id, "read", origin) {
                return denied;
            }
            api_response(api::files::preview(db, id), origin)
        }
        // ─── File text content (code editor) ────────────────────
        // Reads/writes the bytes at `context_data.original_path`, with a
        // version snapshot before every overwrite. Refusals carry prefixes
        // (`encrypted:`, `binary:`, `too_large:`, `not_found:`).
        ["api", "files", id, "content"] if method == "GET" => {
            if let Some(denied) = deny_file(db, &claims, id, "read", origin) {
                return denied;
            }
            api_response(api::files::read_content(db, id), origin)
        }
        ["api", "files", id, "content"] if method == "PUT" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            #[derive(Deserialize)]
            struct ContentBody {
                #[serde(default)]
                content: String,
            }
            let req: ContentBody = json_body!(body, origin);
            api_response(api::files::write_content(db, id, &req.content), origin)
        }
        ["api", "files", id, "versions"] if method == "GET" => {
            if let Some(denied) = deny_file(db, &claims, id, "read", origin) {
                return denied;
            }
            api_response(api::versions::list(db, id), origin)
        }
        ["api", "files", id, "versions"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            api_response(api::versions::create(db, id), origin)
        }
        ["api", "files", id, "versions", version_id, "revert"] if method == "POST" => {
            if let Some(denied) = deny_file(db, &claims, id, "write", origin) {
                return denied;
            }
            api_response(api::versions::revert(db, id, version_id), origin)
        }

        // ─── Trash endpoints ─────────────────────────────────────
        ["api", "trash"] if method == "GET" => api_response(api::trash::list(db), origin),
        // <<< AGENT-3 AUDIT: empty-trash is admin-only (RBAC table) and
        // now records who did it >>>
        ["api", "trash"] if method == "DELETE" => {
            let actor = claims.as_ref().map(|c| c.user_id.clone());
            let result = api::trash::empty(db).inspect(|count| {
                let _ = db.log_audit(
                    "trash_empty",
                    "trash",
                    "*",
                    actor.as_deref(),
                    Some(serde_json::json!({ "count": count })),
                );
            });
            api_response(result, origin)
        }
        ["api", "trash", id] if method == "DELETE" => {
            api_response(api::trash::delete(db, id), origin)
        }
        ["api", "trash", id, "restore"] if method == "POST" => {
            api_response(api::trash::restore(db, id), origin)
        }

        // ─── Version snapshots ───────────────────────────────────
        ["api", "versions", "snapshot-all"] if method == "POST" => {
            api_response(api::versions::snapshot_all(db), origin)
        }

        // ─── Vault backup export ─────────────────────────────────
        // Raw snapshot image (base64 JSON); the browser seals it into a
        // CYBMJ01 container with a passphrase that never leaves the
        // device. Admin-only (see `security.rs`).
        ["api", "backup", "export"] if method == "GET" => {
            api_response(api::backup::export_db(db), origin)
        }

        // ─── Audit log ───────────────────────────────────────────
        ["api", "audit"] if method == "GET" => {
            // <<< AGENT-3 VALIDATION: cap the page size >>>
            let limit = parse_query_param(query, "limit")
                .and_then(|v| v.parse::<u32>().ok())
                .map(|v| v.min(1_000));
            let entity_type = parse_query_param(query, "entityType");
            api_response(api::audit::list(db, limit, entity_type.as_deref()), origin)
        }

        // ─── Share links ─────────────────────────────────────────
        // <<< AGENT-3 SHARE: listing is admin-only, URLs are built from the
        // request origin instead of a hardcoded localhost >>>
        ["api", "share-links"] if method == "GET" => {
            api_response(api::share::list_with_base(db, origin), origin)
        }
        ["api", "share-links"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct ShareBody {
                file_id: String,
                expires_in_hours: Option<u64>,
            }
            let req: ShareBody = json_body!(body, origin);
            if let Err(e) = security::validate_id(&req.file_id) {
                return json_error(400, &e, origin);
            }
            // P0-1: sharing publishes bytes — the caller must read the file.
            if let Some(denied) = deny_file(db, &claims, &req.file_id, "read", origin) {
                return denied;
            }
            let actor = claims.as_ref().map(|c| c.user_id.clone());
            let result =
                api::share::generate(db, &req.file_id, req.expires_in_hours).inspect(|link| {
                    let _ = db.log_audit(
                        "share_grant",
                        "file",
                        &req.file_id,
                        actor.as_deref(),
                        Some(
                            serde_json::json!({ "shareId": link.id, "expiresAt": link.expires_at }),
                        ),
                    );
                });
            api_response(result, origin)
        }
        // <<< AGENT-3 SHARE: revoke (admin-only) >>>
        ["api", "share-links", id] if method == "DELETE" => {
            if let Err(e) = security::validate_id(id) {
                return json_error(400, &e, origin);
            }
            let actor = claims.as_ref().map(|c| c.user_id.clone());
            let result = api::share::revoke(db, id).inspect(|removed| {
                let _ = db.log_audit(
                    "share_revoke",
                    "share_link",
                    id,
                    actor.as_deref(),
                    Some(serde_json::json!({ "revoked": removed })),
                );
            });
            api_response(result, origin)
        }
        // <<< AGENT-3 SHARE: metadata (token-gated, public) >>>
        ["api", "shared", token] if method == "GET" => {
            if let Err(e) = security::validate_share_token(token) {
                return json_error(400, &e, origin);
            }
            match api::share::resolve(db, token) {
                Ok(Some(node)) => json_ok(&node, origin),
                Ok(None) => json_error(404, "Share link not found", origin),
                Err(e) if e.contains("expired") => json_error(410, &e, origin),
                Err(e) => json_error(400, &e, origin),
            }
        }
        // <<< AGENT-3 SHARE: the link actually serves bytes now >>>
        ["api", "shared", token, "content"] if method == "GET" => {
            if let Err(e) = security::validate_share_token(token) {
                return json_error(400, &e, origin);
            }
            match api::share::content(db, token) {
                Ok(Some(payload)) => {
                    http_response_bytes(200, &payload.mime_type, &payload.bytes, origin)
                }
                Ok(None) => json_error(404, "Share link not found", origin),
                Err(e) if e.contains("expired") => json_error(410, &e, origin),
                Err(e) => json_error(400, &e, origin),
            }
        }

        // ─── Batch operations ────────────────────────────────────
        // P0-1: every id is owner-checked up front (`write` — delete,
        // encrypt and compress all mutate), fail-closed on the first denial.
        ["api", "batch", "delete"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct BatchBody {
                file_ids: Vec<String>,
            }
            let req: BatchBody = json_body!(body, origin);
            if let Some(claims) = &claims {
                if let Err(e) = api::files::check_access_many(db, claims, &req.file_ids, "write") {
                    let status = if e.starts_with("not_found:") {
                        404
                    } else {
                        403
                    };
                    return json_error(status, &e, origin);
                }
            }
            api_response(api::batch::delete(db, &req.file_ids), origin)
        }
        ["api", "batch", "encrypt"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct BatchBody {
                file_ids: Vec<String>,
                algorithm: String,
            }
            let req: BatchBody = json_body!(body, origin);
            if let Some(claims) = &claims {
                if let Err(e) = api::files::check_access_many(db, claims, &req.file_ids, "write") {
                    let status = if e.starts_with("not_found:") {
                        404
                    } else {
                        403
                    };
                    return json_error(status, &e, origin);
                }
            }
            crypto_response(
                api::batch::encrypt(db, &req.file_ids, &req.algorithm),
                origin,
            )
        }
        ["api", "batch", "decrypt"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct BatchBody {
                file_ids: Vec<String>,
            }
            let req: BatchBody = json_body!(body, origin);
            let Some(caller) = claims.as_ref() else {
                return json_error(401, "Authentication required", origin);
            };
            if let Err(e) = api::files::check_access_many(db, caller, &req.file_ids, "write") {
                return auth_response::<()>(Err(e), origin);
            }
            crypto_response(api::batch::decrypt(db, &req.file_ids), origin)
        }
        ["api", "batch", "compress"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct BatchBody {
                file_ids: Vec<String>,
                layer: String,
            }
            let req: BatchBody = json_body!(body, origin);
            if let Some(claims) = &claims {
                if let Err(e) = api::files::check_access_many(db, claims, &req.file_ids, "write") {
                    let status = if e.starts_with("not_found:") {
                        404
                    } else {
                        403
                    };
                    return json_error(status, &e, origin);
                }
            }
            api_response(api::batch::compress(db, &req.file_ids, &req.layer), origin)
        }

        // ─── Account endpoints ────────────────────────────────────
        ["api", "accounts"] if method == "GET" => {
            list_all_json(db, Database::get_accounts_table(), origin)
        }
        ["api", "accounts"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct CreateAccountBody {
                name: String,
                account_type: String,
                path: Option<String>,
                color: Option<String>,
            }
            let req: CreateAccountBody = json_body!(body, origin);
            api_response(
                api::accounts::create(db, req.name, req.account_type, req.path, req.color),
                origin,
            )
        }
        ["api", "accounts", id, "switch"] if method == "POST" => {
            api_response(api::accounts::switch(db, id), origin)
        }
        ["api", "accounts", id] if method == "DELETE" => {
            api_response(api::accounts::delete(db, id), origin)
        }

        // ─── Collection endpoints ─────────────────────────────────
        ["api", "collections"] if method == "GET" => {
            list_all_json(db, Database::get_collections_table(), origin)
        }
        ["api", "collections"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct CreateCollectionBody {
                name: String,
                collection_type: String,
                color: String,
                description: Option<String>,
            }
            let req: CreateCollectionBody = json_body!(body, origin);
            api_response(
                api::collections::create(
                    db,
                    req.name,
                    req.collection_type,
                    req.color,
                    req.description,
                ),
                origin,
            )
        }
        ["api", "collections", id, "items"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct AddItemBody {
                file_id: String,
                note: Option<String>,
            }
            let req: AddItemBody = json_body!(body, origin);
            // P0-1: collecting a file needs `read` on it.
            if let Some(denied) = deny_file(db, &claims, &req.file_id, "read", origin) {
                return denied;
            }
            api_response(
                api::collections::add_item(db, id, &req.file_id, req.note),
                origin,
            )
        }
        ["api", "collections", id, "items", file_id] if method == "DELETE" => {
            if let Some(denied) = deny_file(db, &claims, file_id, "read", origin) {
                return denied;
            }
            api_response(api::collections::remove_item(db, id, file_id), origin)
        }
        ["api", "collection-items"] if method == "GET" => {
            list_all_json(db, Database::get_collection_items_table(), origin)
        }

        // ─── Face group endpoints ─────────────────────────────────
        ["api", "face-groups"] if method == "GET" => {
            list_all_json(db, Database::get_face_groups_table(), origin)
        }
        ["api", "faces", "detect"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct DetectFacesBody {
                file_id: String,
            }
            let req: DetectFacesBody = json_body!(body, origin);
            if let Some(denied) = deny_file(db, &claims, &req.file_id, "read", origin) {
                return denied;
            }
            api_response(api::faces_api::detect(db, &req.file_id), origin)
        }
        ["api", "faces", "detect-batch"] if method == "POST" => {
            api_response(api::faces_api::detect_batch(db), origin)
        }

        // ─── Loose group endpoints ────────────────────────────────
        ["api", "loose-groups"] if method == "GET" => {
            list_all_json(db, Database::get_loose_groups_table(), origin)
        }
        ["api", "loose-groups"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct CreateLooseGroupBody {
                name: String,
                color: Option<String>,
            }
            let req: CreateLooseGroupBody = json_body!(body, origin);
            api_response(
                api::files::create_loose_group(
                    db,
                    req.name,
                    req.color.unwrap_or_else(|| "#FFFFFF".to_string()),
                ),
                origin,
            )
        }
        ["api", "loose-groups", group_id, "files"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct AddLooseFileBody {
                file_id: String,
            }
            let req: AddLooseFileBody = json_body!(body, origin);
            // P0-1: grouping a file stamps its node — needs `read`.
            if let Some(denied) = deny_file(db, &claims, &req.file_id, "read", origin) {
                return denied;
            }
            api_response(
                api::files::add_to_loose_group(db, group_id, &req.file_id),
                origin,
            )
        }

        // ─── Encryption endpoints ─────────────────────────────────
        ["api", "encryption", "status"] if method == "GET" => {
            if let Some(file_id) = parse_query_param(query, "fileId") {
                if let Some(denied) = deny_file(db, &claims, &file_id, "read", origin) {
                    return denied;
                }
                return match db.get_file_node(&file_id) {
                    Ok(Some(node)) => {
                        let algorithm = node.encryption_algorithm.as_deref().unwrap_or("");
                        let level = if algorithm.contains("Hybrid") {
                            3
                        } else if algorithm.contains("1024") {
                            5
                        } else {
                            0
                        };
                        json_ok(
                            &serde_json::json!({
                                "isEncrypted": node.encrypted,
                                "algorithm": node.encryption_algorithm,
                                "nistLevel": level,
                                "encryptedAt": if node.encrypted { Some(node.modified_at) } else { None },
                                "available": true,
                            }),
                            origin,
                        )
                    }
                    Ok(None) => json_error(404, "not_found: file not found", origin),
                    Err(e) => json_error(500, &e.to_string(), origin),
                };
            }
            encryption_status(origin)
        }
        ["api", "encryption", "keys"] if method == "POST" => {
            #[derive(Deserialize)]
            struct KeyBody {
                algorithm: String,
            }
            let req: KeyBody = json_body!(body, origin);
            crypto_response(
                api::batch::generate_keypair(db, &req.algorithm)
                    .map(|key| encryption_key_info(&key)),
                origin,
            )
        }
        ["api", "encryption", "keys"] if method == "GET" => {
            // SECURITY: Never expose private keys
            list_encryption_keys_safe(db, origin)
        }

        // ─── Geo files ───────────────────────────────────────────
        ["api", "geo-files"] if method == "GET" => {
            list_visible_files(db, claims.as_ref(), true, origin)
        }

        // ─── Search ───────────────────────────────────────────────
        // <<< AGENT-3 VALIDATION: limit/offset are clamped — a caller can
        // no longer ask for an unbounded scan >>>
        ["api", "search", "suggest"] if method == "GET" => {
            let Some(caller) = claims.as_ref() else {
                return json_error(401, "Authentication required", origin);
            };
            let prefix = parse_query_param(query, "q").unwrap_or_default();
            let limit = security::clamp_limit(
                parse_query_param(query, "limit").and_then(|v| v.parse::<usize>().ok()),
                10,
                security::MAX_SEARCH_LIMIT,
            );
            api_response(
                api::search_api::suggest_for_caller(db, &prefix, limit, caller),
                origin,
            )
        }
        ["api", "search", "paginated"] if method == "GET" => {
            let Some(caller) = claims.as_ref() else {
                return json_error(401, "Authentication required", origin);
            };
            let q = parse_query_param(query, "q").unwrap_or_default();
            let limit = security::clamp_limit(
                parse_query_param(query, "limit").and_then(|v| v.parse::<usize>().ok()),
                20,
                security::MAX_SEARCH_LIMIT,
            );
            let offset = parse_query_param(query, "offset")
                .and_then(|v| v.parse::<usize>().ok())
                .map(|v| security::clamp_offset(Some(v)))
                .unwrap_or(0);
            api_response(
                api::search_api::search_paginated_for_caller(
                    &dashboard.search_index,
                    db,
                    &q,
                    limit,
                    offset,
                    caller,
                ),
                origin,
            )
        }
        ["api", "search"] if method == "GET" => {
            let Some(caller) = claims.as_ref() else {
                return json_error(401, "Authentication required", origin);
            };
            let q = parse_query_param(query, "q").unwrap_or_default();
            let limit = parse_query_param(query, "limit")
                .and_then(|v| v.parse::<usize>().ok())
                .map(|v| security::clamp_limit(Some(v), 20, security::MAX_SEARCH_LIMIT));
            let offset = parse_query_param(query, "offset")
                .and_then(|v| v.parse::<usize>().ok())
                .map(|v| security::clamp_offset(Some(v)));
            api_response(
                api::search_api::search_for_caller(
                    &dashboard.search_index,
                    db,
                    &q,
                    limit,
                    offset,
                    caller,
                ),
                origin,
            )
        }
        // ─── Cross-device search port ───────────────────────────
        // POST /api/search/federated { query, limit?, peers: [{baseUrl,
        // deviceId?, token?}], timeoutSecs? } → merged ranked hits (local
        // tagged `local`, remote tagged per peer). Explicit peer URLs are
        // what make android/ios/docker/wasm work where mDNS cannot; a dead
        // peer degrades to zero hits for that peer, never a failure.
        // Only an explicit per-peer token is sent to that peer. The inbound
        // caller bearer authorizes this request and is never forwarded.
        ["api", "search", "federated"] if method == "POST" => {
            let Some(caller) = claims.as_ref() else {
                return json_error(401, "Authentication required", origin);
            };
            match serde_json::from_str::<api::search_api::FederatedRequest>(body) {
                Ok(req) => json_ok(
                    &api::search_api::search_federated_for_caller(
                        &dashboard.search_index,
                        db,
                        &req,
                        caller,
                    ),
                    origin,
                ),
                Err(e) => json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
            }
        }

        // ─── Code intelligence ────────────────────────────────
        // Source-text parsing for web/Pages clients (the desktop Tauri
        // `parse_text` runs real grammars; the server runs the shared
        // heuristic core and labels it — same shape, honest `"engine"`).
        ["api", "code", "parse"] if method == "POST" => {
            match api::code::route(method, &path_segments, body, origin) {
                Some(resp) => resp,
                None => json_error(404, "not found: /api/code/parse", origin),
            }
        }

        // ─── AI agent: providers, configs (keyless rows), sessions ──
        // Secrets never serialize: keys live in `sync_secrets` as
        // `agent:key:<config_id>`; rows report `hasKey` only.
        ["api", "agent", "providers"] if method == "GET" => api_response(
            Ok::<_, String>(cybermanju_agent::providers::all_presets()),
            origin,
        ),
        ["api", "agent", "configs"] if method == "GET" => {
            api_response(api::agent_api::list_configs(db), origin)
        }
        ["api", "agent", "configs"] if method == "POST" => {
            #[derive(Deserialize)]
            struct ConfigBody {
                config: cybermanju_types::agent::AgentConfig,
            }
            let req: ConfigBody = json_body!(body, origin);
            // P0-8: `mcp_servers` / `auto_approve` / broad `bash` allow need
            // admin (process spawn + unattended shell). Everyone else saves
            // only unprivileged configs — `auth:` denials are 403.
            let (_, is_admin) = claims_owner(&claims);
            auth_response(
                api::agent_api::save_config_as(db, req.config, is_admin),
                origin,
            )
        }
        ["api", "agent", "configs", id] if method == "GET" => {
            api_response(api::agent_api::get_config(db, id), origin)
        }
        ["api", "agent", "configs", id] if method == "DELETE" => {
            api_response(api::agent_api::delete_config(db, id), origin)
        }
        ["api", "agent", "configs", id, "key"] if method == "PUT" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct KeyBody {
                #[serde(default)]
                api_key: String,
            }
            let req: KeyBody = json_body!(body, origin);
            api_response(api::agent_api::save_key(db, id, &req.api_key), origin)
        }
        ["api", "agent", "configs", id, "models"] if method == "GET" => {
            api_response(api::agent_api::list_models(db, id), origin)
        }
        // MCP servers spawn processes — attaching/detaching is admin-gated
        // (see `security::required_role`); reads stay authenticated-only.
        ["api", "agent", "configs", id, "mcp"] if method == "POST" => {
            #[derive(Deserialize)]
            struct McpAddBody {
                name: String,
                server: cybermanju_types::agent::McpServerConfig,
            }
            let req: McpAddBody = json_body!(body, origin);
            api_response(
                api::agent_api::mcp_add(db, id, req.name, req.server),
                origin,
            )
        }
        ["api", "agent", "configs", id, "mcp", name] if method == "DELETE" => {
            api_response(api::agent_api::mcp_remove(db, id, name), origin)
        }
        ["api", "agent", "sessions"] if method == "GET" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            api_response(
                api::agent_api::list_sessions_for(db, &owner_id, is_admin),
                origin,
            )
        }
        ["api", "agent", "sessions"] if method == "POST" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct NewSessionBody {
                config_id: String,
                #[serde(default)]
                title: Option<String>,
            }
            let req: NewSessionBody = json_body!(body, origin);
            let (owner_id, _) = claims_owner(&claims);
            api_response(
                api::agent_api::create_session_as(db, &req.config_id, req.title, &owner_id),
                origin,
            )
        }
        ["api", "agent", "sessions", id] if method == "GET" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            api_response(
                api::agent_api::get_session_for(db, id, &owner_id, is_admin),
                origin,
            )
        }
        ["api", "agent", "sessions", id] if method == "DELETE" => {
            let (owner_id, is_admin) = claims_owner(&claims);
            api_response(
                api::agent_api::delete_session_for(db, id, &owner_id, is_admin),
                origin,
            )
        }
        ["api", "agent", "sessions", "import"] if method == "POST" => {
            #[derive(Deserialize)]
            struct ImportBody {
                session: cybermanju_types::agent::AgentSession,
            }
            let req: ImportBody = json_body!(body, origin);
            let (owner_id, _) = claims_owner(&claims);
            api_response(
                api::agent_api::import_session_as(db, req.session, &owner_id),
                origin,
            )
        }

        // ─── Location endpoints ───────────────────────────────────
        ["api", "locations"] if method == "GET" => {
            list_all_json(db, Database::get_locations_table(), origin)
        }

        // ─── User endpoints ──────────────────────────────────────
        // Listing is any authenticated session; *management* (create /
        // delete / role change) is admin-only via the RBAC table.
        ["api", "users"] if method == "GET" => list_users_safe(db, origin),
        // <<< AGENT-3 RBAC: admin-created user, role recorded in the audit log >>>
        ["api", "users"] if method == "POST" => register_user_web(
            db,
            body,
            claims.as_ref(),
            api::users::RegistrationMode::AdminCreated,
            origin,
        ),
        ["api", "users", id] if method == "DELETE" => {
            if let Err(e) = security::validate_id(id) {
                return json_error(400, &e, origin);
            }
            let actor = claims.as_ref().map(|c| c.user_id.clone());
            let result = api::users::delete(db, id).inspect(|_| {
                let _ = db.log_audit("user_delete", "user", id, actor.as_deref(), None);
            });
            api_response(result, origin)
        }
        ["api", "users", id, "role"] if method == "POST" => {
            #[derive(Deserialize)]
            struct RoleBody {
                role: String,
            }
            let req: RoleBody = json_body!(body, origin);
            if let Err(e) = security::validate_role(&req.role) {
                return json_error(400, &e, origin);
            }
            let actor = claims.as_ref().map(|c| c.user_id.clone());
            match api::users::update_role(db, id, req.role.clone()) {
                Ok(mut user) => {
                    let _ = db.log_audit(
                        "user_role_change",
                        "user",
                        id,
                        actor.as_deref(),
                        Some(serde_json::json!({ "role": req.role })),
                    );
                    user.password_hash.clear();
                    json_ok(&user, origin)
                }
                Err(e) => api_response::<cybermanju_types::schema::User>(Err(e), origin),
            }
        }

        // ─── Permission endpoints ─────────────────────────────────
        ["api", "permissions"] if method == "POST" => {
            set_permission_web(db, claims.as_ref(), body, origin)
        }
        ["api", "permissions", "verify"] if method == "POST" => {
            verify_access_web(db, claims.as_ref(), body, origin)
        }
        ["api", "permissions", file_id] if method == "GET" => {
            get_permissions_for_file(db, claims.as_ref(), file_id, origin)
        }

        // ─── Sync config endpoints ────────────────────────────────
        ["api", "sync", "configs"] if method == "GET" => {
            api_response(api::sync_api::list_configs(db), origin)
        }
        ["api", "sync", "configs"] if method == "POST" => {
            let req: api::sync_api::ConfigRequest = json_body!(body, origin);
            // P0-6: the token travels in the request body but is stored ONLY
            // in the `sync_secrets` side table (`save_config` routes it
            // there; the row is `skip_serializing`). No row write-back here.
            api_response::<cybermanju_types::sync::SyncConfig>(
                api::sync_api::save_config(db, req.config),
                origin,
            )
        }
        ["api", "sync", "configs", id] if method == "DELETE" => {
            // Admin-only (RBAC table) and audited — deleting a config
            // removes the provider binding for good.
            let actor = claims.as_ref().map(|c| c.user_id.clone());
            let result = api::sync_api::delete_config(db, id).inspect(|_| {
                let _ = db.log_audit(
                    "sync_config_delete",
                    "sync_config",
                    id,
                    actor.as_deref(),
                    None,
                );
            });
            api_response(result, origin)
        }

        // ─── Dashboard status ────────────────────────────────────
        ["api", "dashboard", "status"] if method == "GET" => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            // Shape matches the Tauri `DashboardStatus` command so the
            // frontend can use one mapping in every transport.
            let status = serde_json::json!({
                "service": "CyberManju OS Web Dashboard",
                "running": dashboard.running.load(Ordering::SeqCst),
                "port": dashboard.port,
                "url": format!("http://localhost:{}", dashboard.port),
                "activeConnections": dashboard.active_connections.load(Ordering::SeqCst),
                "bindAddress": dashboard.bind_addr,
                "timestamp": now,
                // Single-sourced from the crate manifest — see AGENT-4 item 15.
                "version": env!("CARGO_PKG_VERSION"),
            });
            http_response(
                200,
                "application/json",
                &serde_json::to_string(&status).unwrap_or_default(),
                origin,
            )
        }

        // <<< AGENT-4 OPS: metrics (text exposition format, no extra deps) >>>
        ["api", "metrics"] if method == "GET" => http_response(
            200,
            "text/plain; version=0.0.4; charset=utf-8",
            &metrics_exposition(dashboard),
            origin,
        ),

        // <<< AGENT-4 OPS: readiness — 200 only when db + index + disk are good >>>
        ["api", "readyz"] if method == "GET" => {
            let checks = readiness_checks(dashboard, db);
            let ready = is_ready(&checks);
            http_response(
                if ready { 200 } else { 503 },
                "application/json",
                &serde_json::to_string(&serde_json::json!({
                    "ready": ready,
                    "checks": checks,
                }))
                .unwrap_or_default(),
                origin,
            )
        }

        // ─── Root / health check ─────────────────────────────────
        // Liveness: always 200 while the process is serving, with the same
        // `service`/`status`/`timestamp` fields as before plus the real
        // readiness checks. Readiness lives on GET /readyz (503 when red).
        ["api"] | ["api", "health"] if method == "GET" => {
            let checks = readiness_checks(dashboard, db);
            let ready = is_ready(&checks);
            let health = serde_json::json!({
                "service": "CyberManju OS Web Dashboard",
                "status": if ready { "ok" } else { "degraded" },
                "ready": ready,
                "checks": checks,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            });
            http_response(
                200,
                "application/json",
                &serde_json::to_string(&health).unwrap_or_default(),
                origin,
            )
        }

        // ═══ AGENT-3 ROUTES ═════════════════════════════════════════
        // Session revocation.
        //
        // `POST /api/auth/logout` is Authenticated by default, so `claims`
        // is always present; the `None` arm is defensive only.
        ["api", "auth", "logout"] if method == "POST" => match claims {
            Some(claims) => {
                dashboard.auth.revoke(&claims.jti, claims.exp);
                // P0-5: persist the revocation — memory alone forgets it on
                // restart. Best-effort: the in-memory gate above already
                // holds it for this process lifetime (`db` here is the
                // request guard's `&Database`, which opens its own txn).
                if let Err(e) = db.save_revocation(&claims.jti, claims.exp) {
                    warn!("Could not persist revocation: {}", e);
                }
                let _ = db.log_audit(
                    "logout",
                    "user",
                    &claims.user_id,
                    Some(&claims.user_id),
                    None,
                );
                json_ok(&serde_json::json!({ "ok": true }), origin)
            }
            None => json_error(401, "Not authenticated", origin),
        },

        // OAuth authorization-code + PKCE — begin the flow.
        ["api", "sync", "oauth", provider, "start"] if method == "GET" => {
            let config_id = parse_query_param(query, "configId").unwrap_or_default();
            if let Err(e) = security::validate_id(&config_id) {
                return json_error(400, &e, origin);
            }
            if !api::oauth::is_supported_provider(provider) {
                return json_error(404, "Unknown OAuth provider", origin);
            }
            match api::oauth::start(&dashboard.auth, dashboard.port, provider, &config_id) {
                Ok(start) => json_ok(&start, origin),
                Err(e) => json_error(400, &e, origin),
            }
        }

        // OAuth authorization-code + PKCE — redeem the redirect.
        ["api", "sync", "oauth", provider, "callback"] if method == "GET" => {
            let code = parse_query_param(query, "code").unwrap_or_default();
            let state = parse_query_param(query, "state").unwrap_or_default();
            if !api::oauth::is_supported_provider(provider) {
                return http_response(
                    400,
                    "text/html; charset=utf-8",
                    &api::oauth::error_page("Unknown OAuth provider"),
                    origin,
                );
            }
            match api::oauth::callback(db, &dashboard.auth, provider, &code, &state) {
                Ok(html) => http_response(200, "text/html; charset=utf-8", &html, origin),
                Err(e) => http_response(
                    400,
                    "text/html; charset=utf-8",
                    &api::oauth::error_page(&e),
                    origin,
                ),
            }
        }
        // ═══ /AGENT-3 ROUTES ═══════════════════════════════════════

        // ─── 404 ─────────────────────────────────────────────────
        _ => json_error(404, &format!("Not found: {} {}", method, path), origin),
    }
}

// ─── <<< AGENT-4 OPS: readiness, metrics, encryption capabilities ────

/// Real readiness checks for `GET /readyz` and `GET /api/health`:
/// `database` (redb opens a read transaction), `searchIndex` (the Tantivy
/// index is attached and readable) and `diskWritable` (a probe file can be
/// created in the data directory). Every value is `"ok"`, `"error"` or
/// `"not-configured"`; only an all-`"ok"` map is ready.
fn readiness_checks(dashboard: &WebDashboard, db: &Database) -> serde_json::Value {
    let database = if db.begin_read().is_ok() {
        "ok"
    } else {
        "error"
    };

    let search_index = match &dashboard.search_index {
        Some(index) => {
            let open = index
                .read()
                .map(|guard| guard.doc_count().is_ok())
                .unwrap_or(false);
            if open {
                "ok"
            } else {
                "error"
            }
        }
        None => "not-configured",
    };

    let disk_writable = if data_dir_writable() { "ok" } else { "error" };

    serde_json::json!({
        "database": database,
        "searchIndex": search_index,
        "diskWritable": disk_writable,
    })
}

/// True when every readiness check reports `"ok"`.
fn is_ready(checks: &serde_json::Value) -> bool {
    checks
        .as_object()
        .map(|map| map.values().all(|v| v == "ok"))
        .unwrap_or(false)
}

/// Write and delete a probe file in the data directory to prove it is
/// writable at runtime. The directory is the parent of `DB_PATH` (set by the
/// container image) and falls back to the process temp dir on desktop.
fn data_dir_writable() -> bool {
    let dir = std::env::var("DB_PATH")
        .ok()
        .map(std::path::PathBuf::from)
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);

    let probe = dir.join(format!(".readyz-{}", std::process::id()));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => std::fs::remove_file(&probe).is_ok(),
        Err(_) => false,
    }
}

/// Prometheus text exposition of the process-wide counters. Deliberately
/// dependency-free: plain `log`/`std` counters, no metrics crate.
fn metrics_exposition(dashboard: &WebDashboard) -> String {
    format!(
        "# HELP cybermanju_http_requests_total Requests routed by the REST API.\n\
         # TYPE cybermanju_http_requests_total counter\n\
         cybermanju_http_requests_total {requests}\n\
         # HELP cybermanju_http_responses_4xx_total Responses with a 4xx status.\n\
         # TYPE cybermanju_http_responses_4xx_total counter\n\
         cybermanju_http_responses_4xx_total {r4xx}\n\
         # HELP cybermanju_http_responses_5xx_total Responses with a 5xx status.\n\
         # TYPE cybermanju_http_responses_5xx_total counter\n\
         cybermanju_http_responses_5xx_total {r5xx}\n\
         # HELP cybermanju_active_connections In-flight HTTP connections.\n\
         # TYPE cybermanju_active_connections gauge\n\
         cybermanju_active_connections {active}\n",
        requests = REQUESTS_TOTAL.load(Ordering::SeqCst),
        r4xx = RESPONSES_4XX.load(Ordering::SeqCst),
        r5xx = RESPONSES_5XX.load(Ordering::SeqCst),
        active = dashboard.active_connections.load(Ordering::SeqCst),
    )
}

/// `GET /api/encryption/status` — capabilities of the crypto engine that is
/// **linked into this binary**. The list is derived from
/// `cybermanju_crypto::EncryptionAlgo`, so it can only describe algorithms
/// that are really compiled in: dropping the `cybermanju-crypto` dependency
/// makes this function stop compiling instead of quietly lying.
fn encryption_status(origin: Option<&str>) -> String {
    use cybermanju_crypto::EncryptionAlgo;

    let algorithms = [
        EncryptionAlgo::Kyber1024,
        EncryptionAlgo::Hybrid,
        EncryptionAlgo::MlDsa44,
        EncryptionAlgo::MlDsa65,
        EncryptionAlgo::MlDsa87,
        EncryptionAlgo::ClassicalSign,
        EncryptionAlgo::Aes256,
    ];

    let status = serde_json::json!({
        "available": true,
        "supported_algorithms": algorithms.iter().map(algorithm_id).collect::<Vec<_>>(),
        "engine": "cybermanju-crypto — ML-KEM (FIPS 203), ML-DSA (FIPS 204), ChaCha20Poly1305",
        "post_quantum": algorithms
            .iter()
            .filter(|a| !matches!(**a, EncryptionAlgo::ClassicalSign | EncryptionAlgo::Aes256))
            .map(algorithm_id)
            .collect::<Vec<_>>(),
    });

    http_response(
        200,
        "application/json",
        &serde_json::to_string(&status).unwrap_or_default(),
        origin,
    )
}

/// Stable identifier for each algorithm the linked crypto crate implements.
fn algorithm_id(algo: &cybermanju_crypto::EncryptionAlgo) -> &'static str {
    use cybermanju_crypto::EncryptionAlgo as Algo;
    match algo {
        Algo::Kyber1024 => "ml_kem_1024",
        Algo::Hybrid => "hybrid_ml_kem_768_x25519",
        Algo::MlDsa44 => "ml_dsa_44",
        Algo::MlDsa65 => "ml_dsa_65",
        Algo::MlDsa87 => "ml_dsa_87",
        Algo::ClassicalSign => "hmac_sha512",
        Algo::Aes256 => "chacha20poly1305",
    }
}

// ─── JWT Authentication ─────────────────────────────────────────────

// <<< AGENT-3 RBAC >>>
/// Verify the JWT from the Authorization header and return its [`Claims`].
/// Returns `Err(http_response_string)` on any failure — missing/malformed
/// header, bad signature, expiry, or a token already revoked by logout.
fn verify_jwt_auth(
    dashboard: &WebDashboard,
    auth_header: Option<&str>,
    origin: Option<&str>,
) -> Result<security::Claims, String> {
    let token = match auth_header {
        Some(h) => {
            // Expected format: "Bearer <token>"
            if let Some(t) = h
                .strip_prefix("Bearer ")
                .or_else(|| h.strip_prefix("bearer "))
            {
                let t = t.trim();
                if t.len() > security::MAX_AUTH_HEADER_BYTES {
                    return Err(json_error(401, "Authorization header too long", origin));
                }
                t
            } else {
                return Err(json_error(
                    401,
                    "Missing or invalid Authorization header format. Expected: Bearer <token>",
                    origin,
                ));
            }
        }
        None => {
            return Err(json_error(401, "Authorization header required", origin));
        }
    };

    let decoding_key = DecodingKey::from_secret(&dashboard.jwt_secret);
    let mut validation = Validation::default();
    validation.set_required_spec_claims(&["exp", "sub"]);
    match decode::<security::Claims>(token, &decoding_key, &validation) {
        Ok(token_data) => {
            let claims = token_data.claims;
            if claims.is_expired() {
                return Err(json_error(401, "Token expired", origin));
            }
            if dashboard.auth.is_revoked(&claims.jti) {
                return Err(json_error(401, "Token has been revoked", origin));
            }
            // A signed token is not a snapshot of permanent authority. Reject
            // deleted/inactive accounts and changed roles on every request.
            let db = dashboard
                .db
                .read()
                .map_err(|_| json_error(401, "Account validation unavailable", origin))?;
            api::users::validate_token_identity(&db, &claims)
                .map_err(|_| json_error(401, "Account or role changed; sign in again", origin))?;
            Ok(claims)
        }
        Err(e) => Err(json_error(
            401,
            &format!("Invalid or expired token: {}", e),
            origin,
        )),
    }
}

/// Create a JWT token for an authenticated user. Every token carries a
/// unique `jti` so `POST /api/auth/logout` can revoke it individually.
fn create_jwt(
    jwt_secret: &[u8; 32],
    user_id: &str,
    username: &str,
    role: &str,
) -> Result<String, String> {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let claims = security::Claims {
        sub: username.to_string(),
        role: role.to_string(),
        user_id: user_id.to_string(),
        iat: now_secs,
        exp: now_secs + JWT_EXPIRY_SECS,
        jti: uuid::Uuid::new_v4().to_string(),
    };

    let encoding_key = EncodingKey::from_secret(jwt_secret);
    encode(&Header::default(), &claims, &encoding_key)
        .map_err(|e| format!("JWT encoding error: {}", e))
}
// <<< /AGENT-3 RBAC >>>

// ─── Rate Limiting ──────────────────────────────────────────────────

/// Check and update the rate limit for a client IP.
/// Returns true if the request is allowed, false if rate limited.
///
/// Thin wrapper over `security::enforce_rate_limit` (the single
/// implementation shared with the Docker transport).
pub fn check_rate_limit(
    rate_limits: &Mutex<HashMap<String, (u32, Instant)>>,
    client_ip: &str,
) -> bool {
    security::enforce_rate_limit(rate_limits, client_ip)
}

// ─── CORS ───────────────────────────────────────────────────────────

/// Build the CORS preflight response (OPTIONS).
///
/// The origin is re-checked against `ALLOWED_ORIGINS` here rather than
/// trusted from the caller, so a transport that forwards a raw `Origin`
/// header (the Docker server) can never get a reflected preflight.
fn cors_preflight_response(origin: Option<&str>) -> String {
    let cors_headers = match security::cors_origin_allowed(origin) {
        Some(o) => format!(
            "Access-Control-Allow-Origin: {}\r\n\
             Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
             Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
             Access-Control-Max-Age: 86400\r\n",
            o
        ),
        None => String::new(),
    };

    format!(
        "HTTP/1.1 204 No Content\r\n\
         {cors_headers}\
         Content-Length: 0\r\n\
         \r\n"
    )
}

// ─── Generic table operations ────────────────────────────────────────

/// List all JSON values from a table.
fn list_all_json(
    db: &Database,
    table_def: TableDefinition<'static, &'static str, &'static str>,
    origin: Option<&str>,
) -> String {
    let tx = match db.begin_read() {
        Ok(tx) => tx,
        Err(e) => return json_error(500, &format!("Read error: {}", e), origin),
    };
    let table = match tx.open_table(table_def) {
        Ok(t) => t,
        Err(e) => return json_error(500, &format!("Table open error: {}", e), origin),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    let iter = match table.iter() {
        Ok(i) => i,
        Err(e) => return json_error(500, &format!("Iteration error: {}", e), origin),
    };
    for entry in iter {
        match entry {
            Ok((key, value)) => {
                let key_str = key.value().to_string();
                let val_str = value.value().to_string();
                if let Ok(mut obj) = serde_json::from_str::<serde_json::Value>(&val_str) {
                    if let Some(map) = obj.as_object_mut() {
                        map.insert("_key".to_string(), serde_json::json!(key_str));
                    }
                    results.push(obj);
                } else {
                    results.push(serde_json::json!({
                        "_key": key_str,
                        "_raw": val_str,
                    }));
                }
            }
            Err(e) => {
                return json_error(500, &format!("Iteration error: {}", e), origin);
            }
        }
    }
    let body = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
    http_response(200, "application/json", &body, origin)
}

/// Return only public key information, never sealed or legacy private values.
fn encryption_key_info(key: &cybermanju_types::schema::EncryptionKey) -> serde_json::Value {
    let algorithm = cybermanju_crypto::pqc::algorithm_from_str(&key.algorithm);
    serde_json::json!({
        "id": key.id,
        "algorithm": key.algorithm,
        "algorithmDisplay": algorithm.as_ref().map(|a| a.display_name()).unwrap_or("Unknown"),
        "nistLevel": algorithm.as_ref().map(|a| a.nist_level()).unwrap_or(0),
        "color": "#00FF41",
        "publicKey": key.public_key,
        "publicKeyPreview": key.public_key.chars().take(32).collect::<String>(),
        "hasPrivateKey": !key.private_key.is_empty(),
        "createdAt": key.created_at,
        "x25519PublicKey": key.x25519_public_key,
    })
}

/// Malformed key rows fail closed, rather than returning an unredacted `_raw`.
fn list_encryption_keys_safe(db: &Database, origin: Option<&str>) -> String {
    let list = || -> Result<Vec<serde_json::Value>, String> {
        let tx = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx
            .open_table(Database::get_encryption_keys_table())
            .map_err(|e| e.to_string())?;
        let mut keys = Vec::new();
        for row in table.iter().map_err(|e| e.to_string())? {
            let (_, value) = row.map_err(|e| e.to_string())?;
            let key: cybermanju_types::schema::EncryptionKey = serde_json::from_str(value.value())
                .map_err(|_| "integrity: invalid encryption key record".to_string())?;
            keys.push(encryption_key_info(&key));
        }
        Ok(keys)
    };
    crypto_response(list(), origin)
}

/// Get a single entry by key from a table.
fn get_by_id(
    db: &Database,
    table_def: TableDefinition<'static, &'static str, &'static str>,
    id: &str,
    origin: Option<&str>,
) -> String {
    let tx = match db.begin_read() {
        Ok(tx) => tx,
        Err(e) => return json_error(500, &format!("Read error: {}", e), origin),
    };
    let table = match tx.open_table(table_def) {
        Ok(t) => t,
        Err(e) => return json_error(500, &format!("Table open error: {}", e), origin),
    };

    match table.get(id) {
        Ok(Some(value)) => {
            let val_str = value.value().to_string();
            http_response(200, "application/json", &val_str, origin)
        }
        Ok(None) => json_error(404, &format!("Not found: {}", id), origin),
        Err(e) => json_error(500, &format!("Get error: {}", e), origin),
    }
}

// ─── Domain-specific handlers ────────────────────────────────────────

/// List only readable files; GPS coordinates are protected metadata too.
fn list_visible_files(
    db: &Database,
    claims: Option<&security::Claims>,
    geo_only: bool,
    origin: Option<&str>,
) -> String {
    let Some(claims) = claims else {
        return json_error(401, "Authentication required", origin);
    };
    let tx = match db.begin_read() {
        Ok(tx) => tx,
        Err(e) => return json_error(500, &format!("Read error: {}", e), origin),
    };
    let table = match tx.open_table(Database::get_files_table()) {
        Ok(t) => t,
        Err(e) => return json_error(500, &format!("Table open error: {}", e), origin),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    let iter = match table.iter() {
        Ok(i) => i,
        Err(e) => return json_error(500, &format!("Iteration error: {}", e), origin),
    };
    for entry in iter {
        let (key, value) = match entry {
            Ok(row) => row,
            Err(e) => return json_error(500, &format!("Iteration error: {}", e), origin),
        };
        if api::files::check_access(db, claims, key.value(), "read").is_err() {
            continue;
        }
        if let Ok(mut obj) = serde_json::from_str::<serde_json::Value>(value.value()) {
            let has_lat = obj.get("gpsLat").and_then(|v| v.as_f64()).is_some();
            let has_lng = obj.get("gpsLon").and_then(|v| v.as_f64()).is_some();
            if !geo_only || (has_lat && has_lng) {
                if !geo_only {
                    if let Some(map) = obj.as_object_mut() {
                        map.insert("_key".to_string(), serde_json::json!(key.value()));
                    }
                }
                results.push(obj);
            }
        }
    }
    let body = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
    http_response(200, "application/json", &body, origin)
}

// ─── User management handlers ────────────────────────────────────────

/// List users (without password hashes).
fn list_users_safe(db: &Database, origin: Option<&str>) -> String {
    let tx = match db.begin_read() {
        Ok(tx) => tx,
        Err(e) => return json_error(500, &format!("Read error: {}", e), origin),
    };
    let table = match tx.open_table(Database::get_users_table()) {
        Ok(t) => t,
        Err(e) => return json_error(500, &format!("Table open error: {}", e), origin),
    };

    let mut results: Vec<serde_json::Value> = Vec::new();
    let iter = match table.iter() {
        Ok(i) => i,
        Err(e) => return json_error(500, &format!("Iteration error: {}", e), origin),
    };
    for (_, value) in iter.flatten() {
        if let Ok(mut obj) = serde_json::from_str::<serde_json::Value>(value.value()) {
            // Strip password hashes for safety
            if let Some(map) = obj.as_object_mut() {
                map.remove("passwordHash");
                map.remove("password_hash");
            }
            results.push(obj);
        }
    }
    let body = serde_json::to_string(&results).unwrap_or_else(|_| "[]".to_string());
    http_response(200, "application/json", &body, origin)
}

/// Login endpoint — expects JSON body: { "username": "...", "password": "..." }
///
/// <<< AGENT-3 LOGIN: per-account backoff, the shared argon2id verification
/// path (including legacy BLAKE3 migration) and audit events for both
/// success and failure. >>>
fn login_user(db: &Database, body: &str, dashboard: &WebDashboard, origin: Option<&str>) -> String {
    let req: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json_error(400, &format!("Invalid JSON: {}", e), origin),
    };

    let username = req.get("username").and_then(|v| v.as_str()).unwrap_or("");
    let password = req.get("password").and_then(|v| v.as_str()).unwrap_or("");

    if username.is_empty() || password.is_empty() {
        return json_error(400, "username and password are required", origin);
    }

    let backoff_key = username.to_ascii_lowercase();
    let locked_for = dashboard.auth.login_locked_secs(&backoff_key);
    if locked_for > 0 {
        return json_error(
            429,
            &format!("Too many failed attempts; retry in {} seconds", locked_for),
            origin,
        );
    }

    match api::users::authenticate(db, username, password) {
        Ok(outcome) => {
            dashboard.auth.clear_login_failures(&backoff_key);
            if outcome.upgraded {
                info!(
                    "Upgraded legacy BLAKE3 password hash to argon2id for user '{}'",
                    username
                );
            }

            let user = outcome.user;
            let token =
                match create_jwt(&dashboard.jwt_secret, &user.id, &user.username, &user.role) {
                    Ok(t) => t,
                    Err(e) => return json_error(500, &e, origin),
                };

            let _ = db.log_audit(
                "login",
                "user",
                &user.id,
                Some(&user.id),
                Some(serde_json::json!({ "username": user.username })),
            );

            let response = serde_json::json!({
                "userId": user.id,
                "username": user.username,
                "role": user.role,
                "displayName": user.display_name,
                "token": token,
                "tokenType": "Bearer",
                "expiresIn": JWT_EXPIRY_SECS,
            });
            http_response(
                200,
                "application/json",
                &serde_json::to_string(&response).unwrap_or_default(),
                origin,
            )
        }
        Err(e) => {
            dashboard.auth.record_login_failure(&backoff_key);
            let _ = db.log_audit(
                "login_failed",
                "user",
                username,
                None,
                Some(serde_json::json!({ "username": username })),
            );
            if e == "Invalid credentials" {
                // Identical message for unknown user and wrong password.
                json_error(401, "Invalid credentials", origin)
            } else if e.contains("deactivated") {
                json_error(403, &e, origin)
            } else {
                json_error(400, &e, origin)
            }
        }
    }
}

/// Register a user from a JSON body — shared by `/api/users/register`
/// (first-run setup) and `POST /api/users` (admin creation).
///
/// `mode` decides whether registration is open at all and whether the role
/// may be `admin`; `claims` is the acting admin for the audit trail.
fn register_user_web(
    db: &Database,
    body: &str,
    claims: Option<&security::Claims>,
    mode: api::users::RegistrationMode,
    origin: Option<&str>,
) -> String {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RegisterBody {
        username: String,
        password: String,
        display_name: Option<String>,
        role: Option<String>,
    }

    let req: RegisterBody = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return json_error(400, &format!("Invalid JSON: {}", e), origin),
    };

    let requested_role = req.role.clone();
    match api::users::register(
        db,
        req.username,
        req.password,
        req.display_name,
        req.role,
        mode,
    ) {
        Ok(mut user) => {
            // SECURITY: never return the password hash to a client
            user.password_hash.clear();
            let actor = claims.map(|c| c.user_id.clone());
            let _ = db.log_audit(
                "user_register",
                "user",
                &user.id,
                actor.as_deref(),
                Some(serde_json::json!({
                    "username": user.username,
                    "role": user.role,
                    "requestedRole": requested_role,
                    "mode": match mode {
                        api::users::RegistrationMode::Bootstrap => "bootstrap",
                        api::users::RegistrationMode::AdminCreated => "adminCreated",
                        api::users::RegistrationMode::LocalIpc => "localIpc",
                    },
                })),
            );
            http_response(
                201,
                "application/json",
                &serde_json::to_string(&user).unwrap_or_default(),
                origin,
            )
        }
        Err(e) => {
            let status = if e.contains("already exists") {
                409
            } else if e.contains("Registration is closed")
                || e.contains("cannot grant the admin role")
            {
                403
            } else {
                400
            };
            json_error(status, &e, origin)
        }
    }
}

/// GET /api/peers — active mDNS scan for CyberManju devices on the LAN
/// (§2B.2). Capped at 2 s and lock-free of writers (called under the shared
/// read lock); an empty list is a real answer (no devices around), never an
/// error.
fn lan_peers(origin: Option<&str>) -> String {
    match cybermanju_sync::lan::discover(
        std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
            cybermanju_sync::lan::MDNS_ADDR,
            cybermanju_sync::lan::MDNS_PORT,
        )),
        std::time::Duration::from_secs(2),
    ) {
        Ok(peers) => json_ok(
            &peers
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "deviceId": p.device_id,
                        "name": p.name,
                        "host": p.host,
                        "port": p.port,
                        "lastSeenSecs": p.last_seen_secs,
                    })
                })
                .collect::<Vec<_>>(),
            origin,
        ),
        Err(e) => json_error(502, &e, origin),
    }
}

fn require_owned_device(
    db: &Database,
    device_id: &str,
    owner_id: &str,
) -> Result<cybermanju_types::schema::DeviceRow, String> {
    if device_id.trim().is_empty() || owner_id.trim().is_empty() {
        return Err("auth: an authenticated owner and registered device are required".to_string());
    }
    let row = db
        .get_device(device_id)
        .map_err(|e| format!("internal: device lookup failed: {e}"))?
        .ok_or_else(|| format!("not_found: no device '{device_id}'"))?;
    if row.revoked {
        return Err(format!("auth: device '{device_id}' is revoked"));
    }
    if row.owner_id.is_empty() || row.owner_id != owner_id {
        return Err("auth: device is not owned by the authenticated account".to_string());
    }
    Ok(row)
}

/// POST /api/peers/{id}/sync — direct device↔device op-log exchange (§2B.2
/// follow-up). Peer-authenticated against the device registry: unknown ids
/// are `not_found:`, revoked rows fail closed with `auth:` — an unknown peer
/// never merges. Body `{ ops: OpRow[], signature, algorithm }` is mandatory
/// and is verified against the active sender's registry key; every op must
/// name that device. Unsigned bodies, owner mismatches and bad signatures are
/// rejected (`auth:`) — a JWT alone cannot impersonate a device. A successful
/// response reports `verified: true`. The caller pulls `since` from the peer
/// first and renders `GET /api/oplog/conflicts` after.
fn peer_sync(
    db: &Database,
    device_id: &str,
    body: &str,
    owner_id: &str,
    origin: Option<&str>,
) -> String {
    let peer = match require_owned_device(db, device_id, owner_id) {
        Ok(row) => row,
        Err(e) if e.starts_with("not_found:") => return json_error(404, &e, origin),
        Err(e) if e.starts_with("internal:") => return json_error(500, &e, origin),
        Err(e) => return json_error(403, &e, origin),
    };
    let req: cybermanju_sync::auth::SignedSyncBody = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
    };
    if req.ops.len() > 10_000 {
        return json_error(
            413,
            &format!(
                "too_large: sync holds {} ops (cap 10000 per call — page it)",
                req.ops.len()
            ),
            origin,
        );
    }
    for op in &req.ops {
        if op.kind.trim().is_empty() {
            return json_error(400, "invalid: every synced op needs a kind", origin);
        }
        if op.device_id.trim().is_empty() {
            return json_error(400, "invalid: every synced op needs a device_id", origin);
        }
        if op.device_id != device_id {
            return json_error(
                403,
                "auth: every synced op must belong to the attested device",
                origin,
            );
        }
    }
    let verified = match cybermanju_sync::auth::verify_envelope(
        &req.ops,
        &req.signature,
        &req.algorithm,
        &peer.public_key,
        &peer.key_algorithm,
    ) {
        Ok(v) => v,
        Err(e) => return json_error(403, &e, origin),
    };
    match db
        .merge_ops(&req.ops)
        .and_then(|(appended, skipped)| Ok((appended, skipped, db.oplog_head()?)))
    {
        Ok((appended, skipped, head)) => json_ok(
            &serde_json::json!({ "appended": appended, "skipped": skipped, "head": head, "verified": verified }),
            origin,
        ),
        Err(e) => json_error(500, &format!("internal: sync merge failed: {e}"), origin),
    }
}

/// GET /api/peers/known — server-assisted presence (§2B.2 PEX): the fresh
/// (5-minute TTL) `POST /api/peers/announce` heartbeats, newest-reader merge
/// material for `GET /api/peers` mDNS scans. What makes android/ios/docker
/// work where multicast cannot.
fn peers_known(db: &Database, owner_id: &str, is_admin: bool, origin: Option<&str>) -> String {
    let entries = cybermanju_sync::peers::known_presence(
        db,
        cybermanju_sync::peers::presence_now(),
        cybermanju_sync::peers::PRESENCE_TTL_SECS,
    );
    // Presence reveals network locations; return only this account's active
    // devices (admins may audit all active devices).
    let devices = match db.list_devices() {
        Ok(rows) => rows,
        Err(e) => return json_error(500, &format!("internal: device list failed: {e}"), origin),
    };
    let visible: std::collections::HashSet<String> = devices
        .into_iter()
        .filter(|d| !d.revoked && (is_admin || (!owner_id.is_empty() && d.owner_id == owner_id)))
        .map(|d| d.id)
        .collect();
    json_ok(
        &entries
            .into_iter()
            .filter(|e| visible.contains(&e.device_id))
            .collect::<Vec<_>>(),
        origin,
    )
}

/// POST /api/peers/announce { deviceId, host, port } — heartbeat this
/// device's reachable `/api` (call once per launch, refresh every few
/// minutes). Authenticated and restricted to an active device owned by the
/// caller; presence is advisory, sync still verifies the device signature.
fn peers_announce(db: &Database, body: &str, owner_id: &str, origin: Option<&str>) -> String {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AnnounceRequest {
        #[serde(default)]
        device_id: String,
        #[serde(default)]
        host: String,
        #[serde(default)]
        port: u16,
    }
    let req: AnnounceRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
    };
    if let Err(e) = require_owned_device(db, &req.device_id, owner_id) {
        let status = if e.starts_with("not_found:") {
            404
        } else {
            403
        };
        return json_error(status, &e, origin);
    }
    match cybermanju_sync::peers::announce_presence(
        db,
        &req.device_id,
        &req.host,
        req.port,
        cybermanju_sync::peers::presence_now(),
    ) {
        Ok(entry) => json_ok(&entry, origin),
        Err(e) => json_error(400, &e, origin),
    }
}

/// POST /api/relay/drop { from, to, kind, payload } — rendezvous a NAT-
/// traversed envelope (op-log delta or WebRTC handshake fragment). Both
/// sides make outbound HTTPS here, so no inbound port is needed. Caps:
/// 256 KiB per message (`too_large:` → ship bulk bytes as bundle payloads).
fn relay_drop(db: &Database, body: &str, owner_id: &str, origin: Option<&str>) -> String {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DropRequest {
        #[serde(default)]
        from: String,
        #[serde(default)]
        to: String,
        #[serde(default)]
        kind: String,
        #[serde(default)]
        payload: serde_json::Value,
    }
    let req: DropRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
    };
    if let Err(e) = require_owned_device(db, &req.from, owner_id) {
        let status = if e.starts_with("not_found:") {
            404
        } else {
            403
        };
        return json_error(status, &e, origin);
    }
    // Mailboxes are tenant-private: an account cannot enqueue messages for a
    // different owner's device, even when its id is known.
    if let Err(e) = require_owned_device(db, &req.to, owner_id) {
        let status = if e.starts_with("not_found:") {
            404
        } else {
            403
        };
        return json_error(status, &e, origin);
    }
    match cybermanju_sync::peers::relay_drop(
        db,
        &req.from,
        &req.to,
        &req.kind,
        req.payload,
        cybermanju_sync::peers::presence_now(),
    ) {
        Ok(id) => json_ok(&serde_json::json!({ "accepted": true, "id": id }), origin),
        Err(e) if e.starts_with("too_large:") => json_error(413, &e, origin),
        Err(e) => json_error(400, &e, origin),
    }
}

/// POST /api/relay/poll { for } — list a recipient's queued envelopes,
/// oldest first. Non-destructive (messages stay queued until ack), so a
/// dropped poll never loses mail.
fn relay_poll(db: &Database, body: &str, owner_id: &str, origin: Option<&str>) -> String {
    #[derive(serde::Deserialize)]
    struct PollRequest {
        #[serde(default, rename = "for")]
        for_device: String,
    }
    let req: PollRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
    };
    if let Err(e) = require_owned_device(db, &req.for_device, owner_id) {
        let status = if e.starts_with("not_found:") {
            404
        } else {
            403
        };
        return json_error(status, &e, origin);
    }
    match cybermanju_sync::peers::relay_poll(db, &req.for_device) {
        Ok(messages) => json_ok(&serde_json::json!({ "messages": messages }), origin),
        Err(e) => json_error(400, &e, origin),
    }
}

/// POST /api/relay/ack { for, ids } — clear delivered envelopes. Returns
/// the cleared count.
fn relay_ack(db: &Database, body: &str, owner_id: &str, origin: Option<&str>) -> String {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct AckRequest {
        #[serde(default, rename = "for")]
        for_device: String,
        #[serde(default)]
        ids: Vec<String>,
    }
    let req: AckRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
    };
    if let Err(e) = require_owned_device(db, &req.for_device, owner_id) {
        let status = if e.starts_with("not_found:") {
            404
        } else {
            403
        };
        return json_error(status, &e, origin);
    }
    match cybermanju_sync::peers::relay_ack(db, &req.for_device, &req.ids) {
        Ok(cleared) => json_ok(&serde_json::json!({ "cleared": cleared }), origin),
        Err(e) => json_error(400, &e, origin),
    }
}

/// POST /api/sync/bundle/build { deviceId, since?, files? } — build a bundle
/// carrying file bytes (`[{ fileId, name, bytesBase64 }]` supplied by the
/// caller, which already holds them). Ops-only callers keep using
/// `GET /api/sync/bundle/export`. Admin-only (same table as the other
/// bundle routes in `security::required_role`).
fn bundle_build(db: &Database, body: &str, origin: Option<&str>) -> String {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FileInput {
        #[serde(default)]
        file_id: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        bytes_base64: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BuildRequest {
        #[serde(default)]
        device_id: String,
        #[serde(default)]
        since: u64,
        #[serde(default)]
        files: Vec<FileInput>,
    }
    let req: BuildRequest = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return json_error(400, &format!("invalid: bad JSON body: {e}"), origin),
    };
    if req.files.len() > cybermanju_sync::bundle::BUNDLE_MAX_PAYLOADS {
        return json_error(
            413,
            &format!(
                "too_large: bundle holds {} files (cap {})",
                req.files.len(),
                cybermanju_sync::bundle::BUNDLE_MAX_PAYLOADS
            ),
            origin,
        );
    }
    let mut files = Vec::with_capacity(req.files.len());
    for f in req.files {
        if f.bytes_base64.len() > 8 * 1024 * 1024 {
            return json_error(
                413,
                &format!(
                    "too_large: payload '{}' exceeds the 5 MiB file cap — ship it via the sync backends",
                    f.file_id
                ),
                origin,
            );
        }
        match base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            f.bytes_base64.trim(),
        ) {
            Ok(bytes) => files.push((f.file_id, f.name, bytes)),
            Err(e) => {
                return json_error(
                    400,
                    &format!("invalid: payload '{}' is not base64: {e}", f.file_id),
                    origin,
                )
            }
        }
    }
    match cybermanju_sync::bundle::export_bundle_with_files(db, &req.device_id, req.since, files) {
        Ok(bundle) => json_ok(&bundle, origin),
        Err(e) if e.starts_with("too_large:") => json_error(413, &e, origin),
        Err(e) => json_error(400, &e, origin),
    }
}
/// as JSON. Admin-only (it walks the whole op-log); the caller places the
/// GET /api/sync/bundle/export?since=N&deviceId=X — sneakernet delta (§2B.4)
/// as JSON. Admin-only (it walks the whole op-log); the caller places the
/// bytes on the USB stick / provider object.
fn bundle_export(db: &Database, query: &str, origin: Option<&str>) -> String {
    let since = parse_query_param(query, "since")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let device_id = parse_query_param(query, "deviceId")
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            std::env::var("CYBERMANJU_DEVICE")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "dashboard".to_string())
        });
    match cybermanju_sync::bundle::export_bundle(db, &device_id, since) {
        Ok(bundle) => json_ok(&bundle, origin),
        Err(e) if e.starts_with("too_large:") => json_error(413, &e, origin),
        Err(e) => json_error(400, &e, origin),
    }
}

/// POST /api/sync/bundle/import — validate + merge a received bundle
/// (idempotent; same op_ids skip). Riding file payloads are BLAKE3-verified
/// and returned as metadata (`payloads: [{ fileId, name, size, blake3 }]`)
/// for the caller to place — import itself merges ops only. Admin-only.
fn bundle_import(db: &Database, body: &str, origin: Option<&str>) -> String {
    match cybermanju_sync::bundle::import_bundle_detailed(db, body.as_bytes()) {
        Ok(out) => json_ok(
            &serde_json::json!({
                "appended": out.appended,
                "skipped": out.skipped,
                "from": out.from_device,
                "payloads": out.payloads.iter().map(|p| serde_json::json!({
                    "fileId": p.file_id,
                    "name": p.name,
                    "size": p.size,
                    "blake3": p.blake3,
                })).collect::<Vec<_>>(),
            }),
            origin,
        ),
        Err(e) if e.starts_with("too_large:") => json_error(413, &e, origin),
        Err(e) if e.starts_with("integrity:") => json_error(409, &e, origin),
        Err(e) => json_error(400, &e, origin),
    }
}

fn set_permission_web(
    db: &Database,
    claims: Option<&security::Claims>,
    body: &str,
    origin: Option<&str>,
) -> String {
    let Some(caller) = claims else {
        return json_error(401, "Authentication required", origin);
    };
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct GrantBody {
        user_id: String,
        file_id: String,
        access: String,
    }
    let req: GrantBody = json_body!(body, origin);
    match api::permissions::grant_permission(db, caller, &req.user_id, &req.file_id, &req.access) {
        Ok(permission) => http_response(
            201,
            "application/json",
            &serde_json::to_string(&permission).unwrap_or_default(),
            origin,
        ),
        Err(e) => auth_response::<()>(Err(e), origin),
    }
}

/// Access checks are caller-bound; inspecting another user requires file-admin.
fn verify_access_web(
    db: &Database,
    claims: Option<&security::Claims>,
    body: &str,
    origin: Option<&str>,
) -> String {
    let Some(caller) = claims else {
        return json_error(401, "Authentication required", origin);
    };
    let req: serde_json::Value = json_body!(body, origin);
    let user_id = req
        .get("userId")
        .and_then(|v| v.as_str())
        .unwrap_or(&caller.user_id);
    let file_id = req.get("fileId").and_then(|v| v.as_str()).unwrap_or("");
    let required = req
        .get("requiredAccess")
        .and_then(|v| v.as_str())
        .unwrap_or("read");
    match api::permissions::verify_permission(db, caller, user_id, file_id, required) {
        Ok(granted) => json_ok(
            &serde_json::json!({
                "userId": user_id,
                "fileId": file_id,
                "requiredAccess": required,
                "granted": granted,
            }),
            origin,
        ),
        Err(e) => auth_response::<()>(Err(e), origin),
    }
}

fn get_permissions_for_file(
    db: &Database,
    claims: Option<&security::Claims>,
    file_id: &str,
    origin: Option<&str>,
) -> String {
    let Some(caller) = claims else {
        return json_error(401, "Authentication required", origin);
    };
    auth_response(
        api::permissions::list_permissions_for_file(db, caller, file_id),
        origin,
    )
}
// ─── HTTP response builder ────────────────────────────────────────────

/// Reason phrase for a status code.
fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        507 => "Insufficient Storage",
        _ => "Error",
    }
}

// <<< AGENT-3 HEADERS >>>
/// Build an HTTP response with restricted CORS headers and the hardening
/// header block (`security::security_headers`).
///
/// The origin is re-checked against `ALLOWED_ORIGINS` here, so any caller —
/// including the Docker transport, which forwards a raw `Origin` header —
/// can never get a reflected `Access-Control-Allow-Origin`.
pub fn http_response(status: u16, content_type: &str, body: &str, origin: Option<&str>) -> String {
    let cors_headers = security::cors_response_headers(origin);
    let extra_headers = security::security_headers();

    format!(
        "HTTP/1.1 {status} {}\r\n\
         Content-Type: {content_type}\r\n\
         {cors_headers}\
         {extra_headers}\
         Content-Length: {}\r\n\
         \r\n\
         {body}",
        status_text(status),
        body.len()
    )
}

/// Binary-safe variant of [`http_response`] used by the share-link content
/// stream (an empty `content_type` falls back to `application/octet-stream`).
pub fn http_response_bytes(
    status: u16,
    content_type: &str,
    body: &[u8],
    origin: Option<&str>,
) -> String {
    let ctype = if content_type.is_empty() {
        "application/octet-stream"
    } else {
        content_type
    };
    format!(
        "HTTP/1.1 {status} {}\r\n\
         Content-Type: {ctype}\r\n\
         Content-Disposition: inline\r\n\
         {}{}Content-Length: {}\r\n\
         \r\n",
        status_text(status),
        security::cors_response_headers(origin),
        security::security_headers(),
        body.len()
    )
}
// <<< /AGENT-3 HEADERS >>>

/// Build a JSON error response.
fn json_error(status: u16, message: &str, origin: Option<&str>) -> String {
    let body = serde_json::json!({
        "error": true,
        "status": status,
        "message": message,
    });
    let body_str = serde_json::to_string(&body).unwrap_or_else(|_| message.to_string());
    http_response(status, "application/json", &body_str, origin)
}

// ─── SSE job tail (`GET /api/agent/jobs/:id/events`) ─────────────────
//
// The hand-rolled server speaks one complete `Content-Length` response
// per request everywhere else; SSE is the one deliberate exception — a
// long-lived `text/event-stream` with no `Content-Length`, flushed per
// frame. All framing bytes come from `cybermanju-agent::stream` (pure,
// unit-tested); this block owns only sockets, timeouts and shutdown:
//
// * the 5 s write timeout set in `handle_connection` is **cleared** on
//   entry — a stream that lives minutes would otherwise die on its first
//   idle gap longer than 5 s;
// * the connection-cap slot taken in `handle_connection` is held for the
//   whole stream (the `ActiveConnectionGuard` lives in the caller), so 256
//   slow readers can never spawn a 257th thread — the cap is ample
//   because each stream is bounded by `SSE_MAX_STREAM_SECS`;
// * heartbeats keep proxies and read timeouts from closing idle streams;
// * any failed write ends the stream immediately (half-open clients must
//   not spin a thread forever);
// * shutdown (`running == false`) ends the stream at the next poll tick.

/// Match `GET /api/agent/jobs/<id>/events` (query string ignored) and
/// return the job id. `None` for every other method/path. Pub so the
/// Docker transport reuses the exact same match.
pub fn is_sse_events_path(method: &str, path: &str) -> Option<String> {
    if method != "GET" {
        return None;
    }
    let clean = path.split('?').next().unwrap_or(path);
    let mut segments = clean
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty());
    match (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) {
        (Some("api"), Some("agent"), Some("jobs"), Some(id), Some("events")) => {
            if segments.next().is_none() && !id.is_empty() {
                Some(id.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// SSE response headers: event-stream content type, no buffering, CORS
/// and hardening included, deliberately no `Content-Length` — the socket
/// stays open until `[DONE]`, the stream cap, or a client disconnect.
pub fn sse_response_headers(origin: Option<&str>) -> String {
    format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Connection: keep-alive\r\n\
         X-Accel-Buffering: no\r\n\
         {}{}\
         \r\n",
        security::cors_response_headers(origin),
        security::security_headers()
    )
}

/// Serve one SSE job tail on an already-accepted socket. Returns when the
/// job reaches a terminal state, the stream cap elapses, the client goes
/// away, or the server stops.
///
/// Pub so the Docker transport serves the identical stream (same timeout
/// clearing, same cap accounting by the caller, same heartbeat) instead
/// of drifting into a second implementation.
pub fn handle_sse_connection(
    dashboard: &WebDashboard,
    mut stream: TcpStream,
    job_id: &str,
    auth_header: Option<&str>,
    origin: Option<&str>,
) {
    // Same gate as every other `Authenticated` route: unknown id shape is
    // a 404-shaped JSON error, bad credentials a 401 — both as a normal
    // one-shot response, never as an event stream.
    let claims = match verify_jwt_auth(dashboard, auth_header, origin) {
        Ok(claims) => claims,
        Err(resp) => {
            let _ = stream.write_all(resp.as_bytes());
            return;
        }
    };
    if security::authorize(&claims, security::RequiredRole::Authenticated, "GET").is_err() {
        let _ = stream.write_all(json_error(403, "Forbidden", origin).as_bytes());
        return;
    }
    if let Err(message) = api::agent_api::job_status_for(job_id, &claims.user_id, claims.is_admin())
    {
        let status = if message.contains("not found") || message.starts_with("not_found:") {
            404
        } else if message.starts_with("auth:") {
            403
        } else {
            400
        };
        let _ = stream.write_all(json_error(status, &message, origin).as_bytes());
        return;
    }

    // Clearing the write timeout is the whole point of this branch: the
    // 5 s timeout that protects normal request/response exchanges would
    // otherwise kill any stream idle longer than one heartbeat gap.
    stream.set_write_timeout(None).ok();
    if stream
        .write_all(sse_response_headers(origin).as_bytes())
        .is_err()
    {
        return;
    }
    let started = Instant::now();
    let max_stream = Duration::from_secs(cybermanju_agent::stream::SSE_MAX_STREAM_SECS);
    let heartbeat_every = Duration::from_secs(cybermanju_agent::stream::SSE_HEARTBEAT_SECS);
    let poll_every = Duration::from_millis(500);
    let mut last_body = String::new();
    let mut last_beat = Instant::now();
    loop {
        if !dashboard.running.load(Ordering::SeqCst) {
            break;
        }
        if started.elapsed() >= max_stream {
            break;
        }
        let snapshot =
            match api::agent_api::job_status_for(job_id, &claims.user_id, claims.is_admin()) {
                Ok(snapshot) => snapshot,
                Err(_) => break,
            };
        let body = serde_json::to_string(&snapshot).unwrap_or_default();
        if body != last_body {
            last_body = body.clone();
            let frame = cybermanju_agent::stream::format_event("job", &body);
            if stream.write_all(frame.as_bytes()).is_err() {
                break;
            }
            last_beat = Instant::now();
        } else if last_beat.elapsed() >= heartbeat_every {
            let frame = cybermanju_agent::stream::heartbeat_frame();
            if stream.write_all(frame.as_bytes()).is_err() {
                break;
            }
            last_beat = Instant::now();
        }
        let terminal = matches!(snapshot.status.as_str(), "done" | "error" | "cancelled");
        if terminal {
            let _ = stream.write_all(cybermanju_agent::stream::done_frame().as_bytes());
            break;
        }
        thread::sleep(poll_every);
    }
}

// ─── Query string parser ────────────────────────────────────────────

/// Parse a query parameter value from a query string (e.g., "q=search&limit=10").
fn parse_query_param(query: &str, param: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == param {
                return Some(url_decode(value));
            }
        }
    }
    None
}

/// Basic URL percent-decoding.
fn url_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

// ─── Static file serving (for Docker use case) ─────────────────────

/// MIME type lookup for common file extensions
pub fn mime_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("webp") => "image/webp",
        Some("webm") => "video/webm",
        Some("mp4") => "video/mp4",
        Some("mp3") => "audio/mpeg",
        Some("wasm") => "application/wasm",
        Some("xml") => "application/xml; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("gz") | Some("gzip") => "application/gzip",
        Some("map") => "application/json",
        _ => "application/octet-stream",
    }
}

/// Serve a static file, writing binary-safe HTTP response directly to the stream.
/// Returns true if the file was served, false if a text-based error was written.
#[allow(dead_code)]
pub fn serve_static_file(stream: &mut TcpStream, static_dir: &std::path::Path, request_path: &str) {
    use std::fs;

    let file_path = if request_path == "/" || request_path.ends_with('/') {
        static_dir.join("index.html")
    } else {
        static_dir.join(request_path.trim_start_matches('/'))
    };

    // Security: prevent path traversal
    let resolved = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\n\
                  Content-Type: text/html; charset=utf-8\r\n\
                  Content-Length: 44\r\n\
                  \r\n\
                  <html><body><h1>404 Not Found</h1></body></html>",
            );
            return;
        }
    };

    let static_resolved = match static_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            let _ = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\n\
                  Content-Type: text/html; charset=utf-8\r\n\
                  Content-Length: 52\r\n\
                  \r\n\
                  <html><body><h1>500 Internal Server Error</h1></body></html>",
            );
            return;
        }
    };

    if !resolved.starts_with(&static_resolved) {
        let _ = stream.write_all(
            b"HTTP/1.1 403 Forbidden\r\n\
              Content-Type: text/html; charset=utf-8\r\n\
              Content-Length: 44\r\n\
              \r\n\
              <html><body><h1>403 Forbidden</h1></body></html>",
        );
        return;
    }

    // SPA fallback: if the file doesn't exist, serve index.html
    let actual_path = if resolved.is_file() {
        resolved
    } else {
        let index = static_dir.join("index.html");
        if index.is_file() {
            index
        } else {
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\n\
                  Content-Type: text/html; charset=utf-8\r\n\
                  Content-Length: 44\r\n\
                  \r\n\
                  <html><body><h1>404 Not Found</h1></body></html>",
            );
            return;
        }
    };

    let contents = match fs::read(&actual_path) {
        Ok(c) => c,
        Err(_) => {
            let _ = stream.write_all(
                b"HTTP/1.1 500 Internal Server Error\r\n\
                  Content-Type: text/html; charset=utf-8\r\n\
                  Content-Length: 52\r\n\
                  \r\n\
                  <html><body><h1>500 Internal Server Error</h1></body></html>",
            );
            return;
        }
    };

    let ctype = mime_type(&actual_path.to_string_lossy());
    let content_length = contents.len();

    // Static assets are public, so they keep the wildcard CORS; they still
    // get the hardening header block (nosniff / frame / CSP).
    let header = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: {ctype}\r\n\
         Content-Length: {content_length}\r\n\
         Cache-Control: public, max-age=3600\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
         {}\r\n",
        security::security_headers()
    );

    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&contents);
}
