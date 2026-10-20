// CyberManju OS — reusable authentication / authorization / HTTP guards
//
// This module is the single source of truth for the security primitives that
// both HTTP transports share:
//
//   * `crates/web/src/lib.rs`   — desktop + in-process REST router
//   * `docker/server/src/main.rs` — standalone Docker server (wired by AGENT-4)
//
// Everything here is deliberately dependency-light and synchronous so that
// AGENT-4 can wire each guard with exactly one call.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rand_core::RngCore;
use serde::{Deserialize, Serialize};

use crate::{ALLOWED_ORIGINS, MAX_BODY_SIZE, RATE_LIMIT_MAX, RATE_LIMIT_WINDOW_SECS};

// ─── Parser / connection limits ─────────────────────────────────────

/// Maximum accepted request line (`GET /path HTTP/1.1`) size in bytes.
pub const MAX_REQUEST_LINE_BYTES: usize = 8_192;
/// Maximum number of header lines accepted per request.
pub const MAX_HEADER_LINES: usize = 100;
/// Maximum size of a single header line in bytes.
pub const MAX_HEADER_LINE_BYTES: usize = 8_192;
/// Maximum simultaneously accepted connections before the server starts
/// refusing (or shedding) work.
pub const MAX_CONCURRENT_CONNECTIONS: u64 = 256;
/// Maximum accepted `Authorization` header size in bytes.
pub const MAX_AUTH_HEADER_BYTES: usize = 4_096;

// ─── Validation limits ──────────────────────────────────────────────

/// Maximum username length accepted at registration.
pub const MAX_USERNAME_LEN: usize = 64;
/// Maximum display-name length accepted at registration.
pub const MAX_DISPLAY_NAME_LEN: usize = 128;
/// Maximum password length (defends against Argon2 DoS via huge inputs).
pub const MAX_PASSWORD_LEN: usize = 1_024;
/// Minimum password length.
pub const MIN_PASSWORD_LEN: usize = 8;
/// Maximum generic name (folder/collection/…) length.
pub const MAX_NAME_LEN: usize = 255;
/// Maximum `limit` accepted by search endpoints.
pub const MAX_SEARCH_LIMIT: usize = 100;
/// Maximum `limit` accepted by paginated listing endpoints.
pub const MAX_PAGE_LIMIT: usize = 500;

/// Roles accepted anywhere in the system.
pub const VALID_ROLES: &[&str] = &["admin", "user", "viewer"];

// ─── JWT claims ─────────────────────────────────────────────────────

/// Decoded, signature-verified JWT payload. Returned by `verify_jwt_auth` and
/// threaded into authenticated handlers; handlers must also validate its
/// current account identity via `api::users::validate_token_identity`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — username.
    pub sub: String,
    /// User id (UUID).
    pub user_id: String,
    /// Role string (`admin` / `user` / `viewer`).
    pub role: String,
    /// Issued-at, seconds since epoch.
    pub iat: u64,
    /// Expiration, seconds since epoch.
    pub exp: u64,
    /// Unique token id — used for logout / revocation.
    pub jti: String,
}

impl Claims {
    /// True when the claims still carry validity time.
    pub fn is_expired(&self) -> bool {
        now_secs() >= self.exp
    }

    /// True when the principal holds the `admin` role.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

// ─── Route → required-role table ────────────────────────────────────

/// The minimum role a route demands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredRole {
    /// No authentication at all (health, login, bootstrap registration,
    /// public share links, OAuth callback).
    Public,
    /// Any valid, unexpired, unrevoked session.
    Authenticated,
    /// Only `role == "admin"`.
    Admin,
}

impl RequiredRole {
    /// Whether a caller holding `role` satisfies this requirement. `Public`
    /// and `Authenticated` are decided by token presence; this only encodes
    /// the role gate.
    pub fn is_satisfied_by(&self, role: &str) -> bool {
        match self {
            RequiredRole::Public => true,
            RequiredRole::Authenticated => true,
            RequiredRole::Admin => role == "admin",
        }
    }
}

/// True for HTTP methods that mutate state. `GET`/`HEAD`/`OPTIONS` are
/// reads; everything else (including `PATCH`, unknown verbs fail closed as
/// writes at the call site via `authorize`) is a write.
pub fn is_write_method(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "DELETE" | "PATCH")
}

/// Second path segments the router in `route_request` actually matches.
///
/// Kept beside the role table on purpose: both answer "does this path exist?",
/// so a route added to one belongs in the other.
const ROUTED_SEGMENTS: &[&str] = &[
    "accounts",
    "audit",
    "agent",
    "auth",
    "batch",
    "backup",
    "collection-items",
    "collections",
    "code",
    "dashboard",
    "encryption",
    "face-groups",
    // Parity sweep (ANALYSIS.md §3): `POST /api/faces/detect*` is served by
    // the router (`["api", "faces", …]` arms in `route_request`) but was
    // missing here, so `is_known_route` 404d it before auth ran. Every
    // `REST_ROUTES` second segment must have an entry in this table.
    "faces",
    "files",
    "geo-files",
    "health",
    "locations",
    // <<< CYBERMANJU OS PUSH: second-level segments for the three new route
    // families (AGENT-6 disk/volume, AGENT-7 repair/scrub/lease, AGENT-8 os).
    // Without these, `is_known_route` 404s the family before auth runs. >>>
    "disk",
    "volume",
    "repair",
    "scrub",
    "lease",
    "os",
    // Scheduler: recurring `.cybsh` triggers. A schedule runs arbitrary
    // scripts, so write/run methods demand Admin in `required_role`.
    "cron",
    // <<< /CYBERMANJU OS PUSH >>>
    // ANALYSIS 100%: device identity (§2B.1), server todos (§4.9), op-log
    // exchange (§2C) and LAN peers (§2B.2). Without these, `is_known_route`
    // 404s the families before auth runs.
    "devices",
    "todos",
    "oplog",
    "peers",
    "loose-groups",
    "metrics",
    "permissions",
    "readyz",
    "relay",
    "search",
    // Secrets keystore (Phase 3) — sealed values, master-passphrase gated.
    // Stays `Authenticated` (default): the blob is useless without the
    // passphrase; reveal writes an audit row.
    "secrets",
    "share-links",
    "shared",
    "sync",
    "trash",
    "users",
    "versions",
];

/// Does the router handle this path at all?
///
/// Consulted *before* the auth gate: an unknown path has to reach the 404 arm
/// at the bottom of `route_request`, otherwise every anonymous probe of a typo'd
/// or not-yet-implemented route answers `401` and the 404 is unreachable
/// without a token.
pub fn is_known_route(segments: &[&str]) -> bool {
    match segments {
        ["api"] => true,
        ["api", second, ..] => ROUTED_SEGMENTS.contains(second),
        _ => false,
    }
}

/// Route → required-role table.
///
/// Default is `Authenticated`: every route not listed here demands a valid
/// session. The admin-only set is deliberately narrow and matches the
/// published status-code matrix (user/role management, trash-empty,
/// share-admin, sync-config delete).
pub fn required_role(method: &str, segments: &[&str]) -> RequiredRole {
    match segments {
        // ── Public ───────────────────────────────────────────────
        ["api"] | ["api", "health"] => RequiredRole::Public,
        // <<< AGENT-4 OPS: probes are unauthenticated by design >>>
        ["api", "readyz"] | ["api", "metrics"] => RequiredRole::Public,
        ["api", "auth", "login"] | ["api", "users", "login"] => RequiredRole::Public,
        // First-run probe for the Docker/web login gate: tells any device
        // whether bootstrap registration is open without sending credentials.
        ["api", "auth", "status"] => RequiredRole::Public,
        // Bootstrap registration — the *handler* additionally enforces the
        // zero-users / env-flag gate and forces a non-admin role.
        ["api", "users", "register"] => RequiredRole::Public,
        // Token-gated public share links (and their byte stream).
        ["api", "shared", ..] => RequiredRole::Public,
        // The OAuth callback arrives as a browser redirect without an
        // Authorization header; the `state` parameter is the credential.
        ["api", "sync", "oauth", _, "callback"] => RequiredRole::Public,

        // ── Admin ────────────────────────────────────────────────
        ["api", "users"] if method == "POST" => RequiredRole::Admin,
        ["api", "users", _] if method == "DELETE" => RequiredRole::Admin,
        ["api", "users", _, "role"] if method == "POST" => RequiredRole::Admin,
        ["api", "trash"] if method == "DELETE" => RequiredRole::Admin,
        // Keypairs are shared backend resources, not caller-owned rows.
        ["api", "encryption", "keys"] if method == "POST" => RequiredRole::Admin,
        ["api", "share-links"] if method == "GET" => RequiredRole::Admin,
        ["api", "share-links", _] if method == "DELETE" => RequiredRole::Admin,
        ["api", "sync", "configs", _] if method == "DELETE" => RequiredRole::Admin,
        // MCP servers spawn processes — attaching/detaching is privileged.
        ["api", "agent", "configs", _, "mcp"] if method == "POST" => RequiredRole::Admin,
        ["api", "agent", "configs", _, "mcp", _] if method == "DELETE" => RequiredRole::Admin,
        // A schedule runs arbitrary `.cybsh` scripts — mutating one (or
        // firing it now) is privileged, same posture as MCP attach.
        ["api", "cron"] if method == "POST" => RequiredRole::Admin,
        ["api", "cron", _] if method == "PUT" || method == "DELETE" => RequiredRole::Admin,
        ["api", "cron", _, _] if method == "POST" => RequiredRole::Admin,
        // Vault snapshot export — the image holds sealed secrets and
        // transcripts, so reading it is privileged even over GET.
        ["api", "backup", "export"] => RequiredRole::Admin,
        // Secrets keystore — create/update/delete and the plaintext reveal
        // are dangerous-op posture (Admin); list/meta stay Authenticated.
        ["api", "secrets"] if method == "POST" => RequiredRole::Admin,
        ["api", "secrets", _] if method == "PUT" || method == "DELETE" => RequiredRole::Admin,
        ["api", "secrets", _, "reveal"] => RequiredRole::Admin,
        // Device identity (§2B.1): enrollment is Authenticated (any
        // signed-in user enrolls their own device); removal and revocation
        // are Admin — a revoked row fails closed everywhere.
        ["api", "devices", _] if method == "DELETE" => RequiredRole::Admin,
        ["api", "devices", _, "revoke"] => RequiredRole::Admin,
        // Sneakernet bundles (§2B.4): export walks the whole op-log and
        // import merges foreign rows — Admin, like backup export.
        ["api", "sync", "bundle", _] => RequiredRole::Admin,

        // ── Everything else: any authenticated session ───────────
        _ => RequiredRole::Authenticated,
    }
}

/// Authorize verified `claims` against a route requirement for one HTTP
/// method.
///
/// Returns `Ok(())` when allowed, otherwise a short machine-readable reason
/// that the caller turns into an HTTP 403.
///
/// P0-2 viewer rule: `role == "viewer"` is read-only — any write method
/// (`POST`/`PUT`/`DELETE`/`PATCH`) is denied even when the route itself only
/// demands `Authenticated`. Admins and `user`s are unaffected.
pub fn authorize(
    claims: &Claims,
    required: RequiredRole,
    method: &str,
) -> Result<(), &'static str> {
    if claims.is_expired() {
        return Err("token expired");
    }
    if !required.is_satisfied_by(&claims.role) {
        return Err("insufficient role");
    }
    if claims.role == "viewer" && is_write_method(method) {
        return Err("viewer role is read-only");
    }
    Ok(())
}

// ─── Guard: body size ───────────────────────────────────────────────

/// Reject request bodies larger than [`crate::MAX_BODY_SIZE`].
///
/// ```text
/// let content_length = match enforce_body_limit(content_length) {
///     Ok(n) => n,
///     Err((status, msg)) => return write_error(status, msg),
/// };
/// ```
pub fn enforce_body_limit(content_length: usize) -> Result<usize, (u16, &'static str)> {
    if content_length > MAX_BODY_SIZE {
        Err((413, "Request body too large"))
    } else {
        Ok(content_length)
    }
}

// ─── Guard: CORS origin allowlist ───────────────────────────────────

/// Return the origin unchanged when it is on the allowlist, `None` otherwise.
///
/// Never reflect an arbitrary `Origin` — that is the Docker bug this fixes.
pub fn cors_origin_allowed(origin: Option<&str>) -> Option<String> {
    origin
        .map(str::trim)
        .filter(|o| !o.is_empty() && ALLOWED_ORIGINS.contains(o))
        .map(|o| o.to_string())
}

/// Build the `Access-Control-*` block for an allowed origin (empty string
/// when the origin is absent or not allowed). Never reflects an arbitrary
/// origin — that is the Docker bug this fixes.
pub fn cors_response_headers(origin: Option<&str>) -> String {
    match cors_origin_allowed(origin) {
        Some(o) => format!(
            "Access-Control-Allow-Origin: {o}\r\n\
             Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
             Access-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\n"
        ),
        None => String::new(),
    }
}

// ─── Guard: rate limiting ───────────────────────────────────────────

/// Fixed-window per-IP rate limiter. Returns `true` when the request may
/// proceed, `false` when the caller must answer 429.
///
/// Lock poisoning fails **closed** for the poisoned map only: the guard
/// returns `false` so a wedged limiter never silently disables itself.
pub fn enforce_rate_limit(
    rate_limits: &Mutex<HashMap<String, (u32, Instant)>>,
    client_ip: &str,
) -> bool {
    let mut limits = match rate_limits.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    let now = Instant::now();

    // Drop windows older than 2x the period so the map cannot grow unbounded.
    limits.retain(|_, (_, ts)| now.duration_since(*ts).as_secs() < RATE_LIMIT_WINDOW_SECS * 2);

    let entry = limits.entry(client_ip.to_string()).or_insert((0, now));
    if now.duration_since(entry.1).as_secs() >= RATE_LIMIT_WINDOW_SECS {
        *entry = (0, now);
    }

    entry.0 += 1;
    entry.0 <= RATE_LIMIT_MAX
}

// ─── Guard: security headers ───────────────────────────────────────

/// Content-Security-Policy for the SPA + API responses.
pub const CSP: &str = "default-src 'self'; \
    script-src 'self'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data: blob:; \
    media-src 'self' blob:; \
    font-src 'self' data:; \
    connect-src 'self'; \
    worker-src 'self' blob:; \
    object-src 'none'; \
    base-uri 'self'; \
    form-action 'self'; \
    frame-ancestors 'none'";

/// Header block appended to every HTTP response this server produces.
///
/// Includes `X-Content-Type-Options`, `Referrer-Policy`, `X-Frame-Options`
/// and the CSP. HSTS is intentionally **not** here — it must be emitted by
/// the TLS terminating proxy (see `docs/SECURITY.md`).
pub fn security_headers() -> String {
    format!(
        "X-Content-Type-Options: nosniff\r\n\
         Referrer-Policy: no-referrer\r\n\
         X-Frame-Options: DENY\r\n\
         Content-Security-Policy: {CSP}\r\n\
         Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=()\r\n"
    )
}

// ─── Input validation ───────────────────────────────────────────────

fn has_control_chars(v: &str) -> bool {
    v.chars().any(|c| c.is_control())
}

/// Username: 1..=64 chars, `[A-Za-z0-9._-]`, no leading `.`/`-`.
pub fn validate_username(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() {
        return Err("Username is required".to_string());
    }
    if v.len() > MAX_USERNAME_LEN {
        return Err(format!(
            "Username must be at most {MAX_USERNAME_LEN} characters"
        ));
    }
    if has_control_chars(v) {
        return Err("Username must not contain control characters".to_string());
    }
    if !v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err("Username may only contain letters, digits, '.', '_' and '-'".to_string());
    }
    if v.starts_with('.') || v.starts_with('-') {
        return Err("Username must not start with '.' or '-'".to_string());
    }
    Ok(())
}

/// Display name: optional, 1..=128 printable chars.
pub fn validate_display_name(v: &str) -> Result<(), String> {
    if v.is_empty() {
        return Ok(());
    }
    if v.chars().count() > MAX_DISPLAY_NAME_LEN {
        return Err(format!(
            "Display name must be at most {MAX_DISPLAY_NAME_LEN} characters"
        ));
    }
    if has_control_chars(v) {
        return Err("Display name must not contain control characters".to_string());
    }
    Ok(())
}

/// Password length bounds (Argon2 cost is paid per byte).
pub fn validate_password(v: &str) -> Result<(), String> {
    if v.len() < MIN_PASSWORD_LEN {
        return Err(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters"
        ));
    }
    if v.len() > MAX_PASSWORD_LEN {
        return Err(format!(
            "Password must be at most {MAX_PASSWORD_LEN} characters"
        ));
    }
    Ok(())
}

/// Role must be one of [`VALID_ROLES`].
pub fn validate_role(v: &str) -> Result<(), String> {
    if VALID_ROLES.contains(&v) {
        Ok(())
    } else {
        Err(format!(
            "Invalid role: {v}. Must be one of {}",
            VALID_ROLES.join(", ")
        ))
    }
}

/// Generic identifier: non-empty, <= 64 chars, no path separators or control
/// characters. Used for ids that are interpolated into paths or table keys.
pub fn validate_id(v: &str) -> Result<(), String> {
    if v.is_empty() {
        return Err("Identifier is required".to_string());
    }
    if v.len() > 64 {
        return Err("Identifier is too long".to_string());
    }
    if has_control_chars(v) || v.contains('/') || v.contains('\\') || v.contains('\0') {
        return Err("Identifier contains invalid characters".to_string());
    }
    Ok(())
}

/// Share token: URL-safe base64 (`[A-Za-z0-9_-]`), `1..=128` chars.
///
/// The real tokens are `URL_SAFE_NO_PAD` encodings of 32 random bytes (43
/// chars); the length bound is loose so older/shorter tokens keep working —
/// the value is rejected before it reaches a database lookup either way.
pub fn validate_share_token(v: &str) -> Result<(), String> {
    if v.is_empty() || v.len() > 128 {
        return Err("Invalid share token".to_string());
    }
    if !v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid share token".to_string());
    }
    Ok(())
}

/// Clamp a caller-supplied `limit` into `1..=max`, falling back to `default`.
pub fn clamp_limit(value: Option<usize>, default: usize, max: usize) -> usize {
    match value {
        Some(0) => 1,
        Some(v) => v.min(max),
        None => default,
    }
}

/// Clamp a caller-supplied `offset`; unparsable/negative values become 0 and
/// the value is capped so a scan can never be requested as `usize::MAX`.
pub fn clamp_offset(value: Option<usize>) -> usize {
    value.unwrap_or(0).min(1_000_000)
}

// ─── Auth state: revocation, OAuth handshake, login backoff ─────────

/// Seconds since the unix epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One in-flight OAuth authorization-code exchange.
#[derive(Debug, Clone)]
pub struct PendingOAuth {
    /// Provider slug (`google` / `github` / `gitlab`).
    pub provider: String,
    /// Sync configuration the resulting credentials belong to.
    pub config_id: String,
    /// PKCE code verifier to redeem at the token endpoint.
    pub code_verifier: String,
    /// Registered redirect URI the provider will send the user back to.
    pub redirect_uri: String,
    /// Unix time the flow started — pending states expire.
    pub created_at: u64,
}

/// Per-account failed-login bookkeeping.
#[derive(Debug, Clone, Default)]
struct LoginBackoff {
    failures: u32,
    locked_until: u64,
}

/// Shared, per-dashboard authentication state:
/// revoked JWT ids, in-flight OAuth handshakes and login backoff.
#[derive(Default)]
pub struct AuthState {
    /// `jti` → token expiry (seconds). Presence means "logged out".
    revoked: Mutex<HashMap<String, u64>>,
    /// OAuth `state` → pending PKCE flow.
    oauth_pending: Mutex<HashMap<String, PendingOAuth>>,
    /// Login key (username or IP) → backoff counters.
    backoff: Mutex<HashMap<String, LoginBackoff>>,
}

/// Failures before the first lockout.
const LOGIN_FAILURES_BEFORE_LOCKOUT: u32 = 5;
/// Lockout duration after the fifth failure (doubles per extra failure).
const LOGIN_LOCKOUT_BASE_SECS: u64 = 30;
/// Cap for the exponential lockout.
const LOGIN_LOCKOUT_MAX_SECS: u64 = 900;
/// Backoff bookkeeping older than this is discarded.
const LOGIN_BACKOFF_TTL_SECS: u64 = 3_600;

impl AuthState {
    /// Create an empty state container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Revoke a token id until `exp`.
    pub fn revoke(&self, jti: &str, exp: u64) {
        if let Ok(mut map) = self.revoked.lock() {
            map.retain(|_, e| *e > now_secs());
            map.insert(jti.to_string(), exp.max(now_secs()));
        }
    }

    /// Whether `jti` has been revoked (or its bookkeeping is poisoned).
    pub fn is_revoked(&self, jti: &str) -> bool {
        match self.revoked.lock() {
            Ok(map) => map.contains_key(jti),
            // Fail closed: a poisoned revocation list must not accept tokens.
            Err(_) => true,
        }
    }

    /// Number of currently revoked token ids (diagnostics / tests).
    pub fn revoked_count(&self) -> usize {
        self.revoked.lock().map(|m| m.len()).unwrap_or(0)
    }

    /// Park an OAuth flow under `state`.
    pub fn put_oauth_pending(&self, state: &str, pending: PendingOAuth) {
        if let Ok(mut map) = self.oauth_pending.lock() {
            let cutoff = now_secs().saturating_sub(600);
            map.retain(|_, p| p.created_at >= cutoff);
            map.insert(state.to_string(), pending);
        }
    }

    /// Consume an OAuth flow by its `state` (one-shot).
    pub fn take_oauth_pending(&self, state: &str) -> Option<PendingOAuth> {
        self.oauth_pending
            .lock()
            .ok()
            .and_then(|mut m| m.remove(state))
    }

    /// Seconds the login key is still locked out for (`0` = not locked).
    pub fn login_locked_secs(&self, key: &str) -> u64 {
        match self.backoff.lock() {
            Ok(map) => map
                .get(key)
                .map(|b| b.locked_until.saturating_sub(now_secs()))
                .unwrap_or(0),
            // Fail closed on a poisoned backoff table.
            Err(_) => LOGIN_LOCKOUT_MAX_SECS,
        }
    }

    /// Record a failed login; returns the current failure count.
    pub fn record_login_failure(&self, key: &str) -> u32 {
        let now = now_secs();
        match self.backoff.lock() {
            Ok(mut map) => {
                map.retain(|_, b| b.locked_until < now + LOGIN_BACKOFF_TTL_SECS);
                let entry = map.entry(key.to_string()).or_default();
                entry.failures += 1;
                if entry.failures >= LOGIN_FAILURES_BEFORE_LOCKOUT {
                    let step = entry.failures - LOGIN_FAILURES_BEFORE_LOCKOUT;
                    let secs = LOGIN_LOCKOUT_BASE_SECS
                        .saturating_mul(1u64 << step.min(8))
                        .min(LOGIN_LOCKOUT_MAX_SECS);
                    entry.locked_until = now + secs;
                }
                entry.failures
            }
            Err(_) => LOGIN_FAILURES_BEFORE_LOCKOUT,
        }
    }

    /// Clear the failure counter after a successful authentication.
    pub fn clear_login_failures(&self, key: &str) {
        if let Ok(mut map) = self.backoff.lock() {
            map.remove(key);
        }
    }
}

// ─── JWT secret sourcing ────────────────────────────────────────────

fn blake3_key(material: &[u8]) -> [u8; 32] {
    *blake3::hash(material).as_bytes()
}

/// Resolve the directory used for persisted secrets.
///
/// Order: `CYBERMANJU_DATA_DIR` → parent of `DB_PATH` → platform data dir.
/// On Android the first branch is the normal answer: `run()` in src-tauri
/// exports it once from the probed app-private files dir (multi-user safe),
/// so this never has to trust the unprobed hardcoded path below.
/// Returns `None` when nothing can be resolved (the caller then falls back
/// to a per-process random secret rather than writing into the CWD).
pub fn default_secret_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CYBERMANJU_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    if let Ok(p) = std::env::var("DB_PATH") {
        if let Some(parent) = Path::new(&p).parent() {
            if !parent.as_os_str().is_empty() {
                return Some(parent.to_path_buf());
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(d) = std::env::var("APPDATA") {
            return Some(PathBuf::from(d).join("CyberManjuOS"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(h) = std::env::var("HOME") {
            return Some(PathBuf::from(h).join("Library/Application Support/CyberManjuOS"));
        }
    }
    #[cfg(target_os = "android")]
    {
        // App-private files dir: always writable under scoped storage, no
        // permission needed. External dirs are intentionally NOT used.
        // Normally the startup probe already exported CYBERMANJU_DATA_DIR
        // (see the resolution order above); the hardcoded path is only the
        // pre-startup last resort — wrong under work profiles (where
        // /data/data is user 0's dir) and debug applicationId suffixes, so
        // the DB probe never trusts it without proving writability.
        if let Ok(d) = std::env::var("CYBERMANJU_ANDROID_FILES_DIR") {
            if !d.trim().is_empty() {
                return Some(PathBuf::from(d));
            }
        }
        return Some(PathBuf::from("/data/data/com.cybermanju.os/files"));
    }
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    {
        let base = std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".local/share"))
            });
        if let Some(base) = base {
            return Some(base.join("cybermanju-os"));
        }
    }
    // On Android the block above always returns (env dir or the hardcoded
    // app-private files dir), so no tail expression may follow there —
    // otherwise `cargo build --target aarch64-linux-android` warns
    // `unreachable expression`. Every other platform can fall through.
    #[cfg(not(target_os = "android"))]
    None
}

/// Fail-closed permission gate (mirrors the keystore): group/other-readable
/// secret files are refused instead of silently trusted.
fn secret_file_mode_ok(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(meta) if meta.permissions().mode() & 0o077 == 0 => true,
            Ok(_) => {
                warn_log(&format!(
                    "refusing world/group-readable secret file {} (fix with chmod 600)",
                    path.display()
                ));
                false
            }
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

/// Read `path` if it exists, trimming trailing whitespace.
fn read_secret_file(path: &Path) -> Option<String> {
    if !secret_file_mode_ok(path) {
        return None;
    }
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.chars().filter(|c| !c.is_whitespace()).collect::<String>())
}

/// Write `bytes` to `path` with owner-only permissions (0600), best effort.
fn write_secret_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Load (or create on first run) the JWT HMAC secret.
///
/// Resolution order:
///   1. `CYBERMANJU_JWT_SECRET` — 64 hex chars are used raw, anything else is
///      stretched with BLAKE3 to 32 bytes.
///   2. `<secret dir>/jwt_secret`, generated with 0600 on first run.
///   3. A per-process random secret (sessions then die with the process —
///      only happens when no data directory can be resolved).
pub fn load_or_create_jwt_secret() -> [u8; 32] {
    load_or_create_jwt_secret_in(default_secret_dir().as_deref())
}

/// Same as [`load_or_create_jwt_secret`] with an explicit directory hint
/// (the database's directory, when the caller knows it).
pub fn load_or_create_jwt_secret_in(dir_hint: Option<&Path>) -> [u8; 32] {
    if let Ok(env_secret) = std::env::var("CYBERMANJU_JWT_SECRET") {
        let trimmed = env_secret.trim();
        if trimmed.len() == 64 {
            if let Ok(bytes) = hex_decode(trimmed) {
                if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
                    return arr;
                }
            }
        }
        if !trimmed.is_empty() {
            warn_log("CYBERMANJU_JWT_SECRET set; deriving a 256-bit key from it");
            return blake3_key(trimmed.as_bytes());
        }
    }

    let dir = match std::env::var("CYBERMANJU_DATA_DIR") {
        Ok(d) if !d.trim().is_empty() => Some(PathBuf::from(d)),
        _ => dir_hint
            .map(|p| p.to_path_buf())
            .or_else(default_secret_dir),
    };

    let dir = match dir {
        Some(d) => d,
        None => {
            warn_log("no data directory for JWT secret; using a per-process random secret");
            let mut secret = [0u8; 32];
            rand_core::OsRng.fill_bytes(&mut secret);
            return secret;
        }
    };

    let path = dir.join("jwt_secret");
    // Present-but-loose secrets fail closed on a per-process secret WITHOUT
    // overwriting the file (rotating over it would silently invalidate every
    // session and destroy the evidence).
    if path.exists() && !secret_file_mode_ok(&path) {
        let mut secret = [0u8; 32];
        rand_core::OsRng.fill_bytes(&mut secret);
        return secret;
    }
    if let Some(existing) = read_secret_file(&path) {
        if let Ok(bytes) = hex_decode(&existing) {
            if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
                return arr;
            }
        }
        if existing.len() >= 32 {
            return blake3_key(existing.as_bytes());
        }
    }

    let mut secret = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut secret);
    let hex = hex_encode(&secret);
    if let Err(e) = write_secret_file(&path, &hex) {
        warn_log(&format!(
            "could not persist JWT secret to {}: {} — using a per-process secret",
            path.display(),
            e
        ));
        rand_core::OsRng.fill_bytes(&mut secret);
    } else {
        info_log(&format!("JWT secret persisted at {}", path.display()));
    }
    secret
}

/// Resolve the machine-local master passphrase used for keys at rest.
///
/// Delegates to [`cybermanju_crypto::keystore::master_passphrase`] — the
/// keystore is the single implementation so the passphrase file is created
/// and read in exactly one place (`CYBERMANJU_MASTER_PASSPHRASE` →
/// `<data dir>/master.passphrase`, 0600, generated on first use).
pub fn master_passphrase() -> Option<String> {
    cybermanju_crypto::keystore::master_passphrase()
}

// ─── hex helpers (no extra dependency) ──────────────────────────────

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex encoding.
pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Decode a hex string; returns an error on odd length or bad digits.
pub fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err("odd-length hex string".to_string());
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = hex_val(pair[0])?;
        let lo = hex_val(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_val(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("invalid hex digit: {}", c as char)),
    }
}

fn warn_log(msg: &str) {
    log::warn!("{}", msg);
}

fn info_log(msg: &str) {
    log::info!("{}", msg);
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn segs<'a>(v: &'a [&'a str]) -> Vec<&'a str> {
        v.to_vec()
    }

    #[test]
    fn public_routes_need_no_role() {
        for (m, s) in [
            ("GET", segs(&["api", "health"])),
            ("POST", segs(&["api", "auth", "login"])),
            ("GET", segs(&["api", "auth", "status"])),
            ("POST", segs(&["api", "users", "register"])),
            ("GET", segs(&["api", "shared", "tok"])),
            ("GET", segs(&["api", "shared", "tok", "content"])),
        ] {
            assert_eq!(required_role(m, &s), RequiredRole::Public, "{m} {:?}", s);
        }
    }

    #[test]
    fn admin_routes_are_gated() {
        for (m, s) in [
            ("POST", segs(&["api", "users"])),
            ("DELETE", segs(&["api", "users", "id"])),
            ("POST", segs(&["api", "users", "id", "role"])),
            ("DELETE", segs(&["api", "trash"])),
            ("GET", segs(&["api", "share-links"])),
            ("DELETE", segs(&["api", "share-links", "id"])),
            ("DELETE", segs(&["api", "sync", "configs", "id"])),
        ] {
            assert_eq!(required_role(m, &s), RequiredRole::Admin, "{m} {:?}", s);
        }
    }

    #[test]
    fn default_is_authenticated() {
        assert_eq!(
            required_role("DELETE", &segs(&["api", "files", "id"])),
            RequiredRole::Authenticated
        );
        assert_eq!(
            required_role("POST", &segs(&["api", "sync", "configs"])),
            RequiredRole::Authenticated
        );
    }

    #[test]
    fn authorize_gates_admin() {
        let admin = Claims {
            sub: "a".into(),
            user_id: "1".into(),
            role: "admin".into(),
            iat: 0,
            exp: now_secs() + 60,
            jti: "j".into(),
        };
        let member = Claims {
            role: "user".into(),
            ..admin.clone()
        };
        let viewer = Claims {
            role: "viewer".into(),
            ..admin.clone()
        };
        assert!(authorize(&admin, RequiredRole::Admin, "DELETE").is_ok());
        assert!(authorize(&member, RequiredRole::Admin, "DELETE").is_err());
        assert!(authorize(&member, RequiredRole::Authenticated, "POST").is_ok());

        // P0-2: viewer is read-only — GET passes, writes are denied.
        assert!(authorize(&viewer, RequiredRole::Authenticated, "GET").is_ok());
        assert!(authorize(&viewer, RequiredRole::Authenticated, "HEAD").is_ok());
        for method in ["POST", "PUT", "DELETE", "PATCH"] {
            assert!(
                authorize(&viewer, RequiredRole::Authenticated, method).is_err(),
                "viewer must not {method}"
            );
        }
        // Viewer still cannot reach admin routes even for reads.
        assert!(authorize(&viewer, RequiredRole::Admin, "GET").is_err());

        let expired = Claims {
            exp: now_secs().saturating_sub(5),
            ..admin.clone()
        };
        assert!(authorize(&expired, RequiredRole::Authenticated, "GET").is_err());
    }

    #[test]
    fn body_limit_guard() {
        assert!(enforce_body_limit(0).is_ok());
        assert!(enforce_body_limit(MAX_BODY_SIZE).is_ok());
        let err = enforce_body_limit(MAX_BODY_SIZE + 1).unwrap_err();
        assert_eq!(err.0, 413);
    }

    #[test]
    fn cors_allowlist_never_reflects() {
        assert_eq!(
            cors_origin_allowed(Some("http://localhost:3456")).as_deref(),
            Some("http://localhost:3456")
        );
        assert_eq!(cors_origin_allowed(Some("https://evil.example")), None);
        assert_eq!(cors_origin_allowed(Some("null")), None);
        assert_eq!(cors_origin_allowed(None), None);
    }

    #[test]
    fn rate_limit_blocks_after_window() {
        let limits = Mutex::new(HashMap::new());
        for _ in 0..crate::RATE_LIMIT_MAX {
            assert!(enforce_rate_limit(&limits, "9.9.9.9"));
        }
        assert!(!enforce_rate_limit(&limits, "9.9.9.9"));
        // A different client is unaffected.
        assert!(enforce_rate_limit(&limits, "8.8.8.8"));
    }

    #[test]
    fn validators_accept_and_reject() {
        assert!(validate_username("alice_2").is_ok());
        assert!(validate_username("").is_err());
        assert!(validate_username("a b").is_err());
        assert!(validate_username("../etc").is_err());
        assert!(validate_role("admin").is_ok());
        assert!(validate_role("root").is_err());
        assert!(validate_password("short").is_err());
        assert!(validate_password("longenough").is_ok());
        assert!(validate_id("../../x").is_err());
        assert!(validate_share_token("abc-DEF_123").is_ok());
        assert!(validate_share_token("a/b").is_err());
    }

    #[test]
    fn limits_are_clamped() {
        assert_eq!(clamp_limit(None, 10, 100), 10);
        assert_eq!(clamp_limit(Some(0), 10, 100), 1);
        assert_eq!(clamp_limit(Some(10_000), 10, 100), 100);
        assert_eq!(clamp_offset(None), 0);
        assert_eq!(clamp_offset(Some(usize::MAX)), 1_000_000);
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0u8, 1, 0xab, 0xff];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes.to_vec());
        assert!(hex_decode("zz").is_err());
    }

    #[test]
    fn backoff_locks_after_repeated_failures() {
        let state = AuthState::new();
        for _ in 0..LOGIN_FAILURES_BEFORE_LOCKOUT {
            state.record_login_failure("bob");
        }
        assert!(state.login_locked_secs("bob") > 0);
        assert_eq!(state.login_locked_secs("alice"), 0);
        state.clear_login_failures("bob");
        assert_eq!(state.login_locked_secs("bob"), 0);
    }

    #[test]
    fn revocation_is_one_shot_lookup() {
        let state = AuthState::new();
        state.revoke("jti-1", now_secs() + 60);
        assert!(state.is_revoked("jti-1"));
        assert!(!state.is_revoked("jti-2"));
    }

    #[test]
    fn oauth_pending_is_consumed_once() {
        let state = AuthState::new();
        state.put_oauth_pending(
            "s1",
            PendingOAuth {
                provider: "github".into(),
                config_id: "cfg-1".into(),
                code_verifier: "v".into(),
                redirect_uri: "http://127.0.0.1:3456/callback".into(),
                created_at: now_secs(),
            },
        );
        assert!(state.take_oauth_pending("s1").is_some());
        assert!(state.take_oauth_pending("s1").is_none());
    }

    #[test]
    fn jwt_secret_is_stable_across_calls() {
        let dir = std::env::temp_dir().join("cybermanju-jwt-test");
        let a = load_or_create_jwt_secret_in(Some(&dir));
        let b = load_or_create_jwt_secret_in(Some(&dir));
        assert_eq!(a, b);
    }

    #[test]
    fn security_headers_cover_the_basics() {
        let h = security_headers();
        assert!(h.contains("X-Content-Type-Options: nosniff"));
        assert!(h.contains("Referrer-Policy:"));
        assert!(h.contains("X-Frame-Options: DENY"));
        assert!(h.contains("Content-Security-Policy:"));
        // HSTS must come from the TLS proxy, never from plain HTTP.
        assert!(!h.contains("Strict-Transport-Security"));
    }
}
