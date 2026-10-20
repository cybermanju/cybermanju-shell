// CyberManju OS — Standalone Web Server (Docker)
//
// Unified HTTP server that:
// 1. Serves compiled Vue frontend as static files (binary-safe)
// 2. Routes /api/* to the shared cybermanju-web REST API
//
// Environment variables:
//   PORT               — listening port (default: 3456)
//   DB_PATH            — path to redb database (default: /data/cybermanju.db)
//   SEARCH_INDEX_PATH  — Tantivy index directory (default: <dir of DB_PATH>/tantivy_index)
//   STATIC_DIR         — path to frontend dist files (default: ./static)
//   RUST_LOG           — log level (default: info)

use cybermanju_db::Database;
use cybermanju_search::SearchIndex;
use cybermanju_web::{handle_request, http_response, serve_static_file, WebDashboard};
use log::{error, info, warn};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Set by the SIGTERM/SIGINT handler; the accept loop polls it.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Shared application state passed to each connection handler
struct AppState {
    static_dir: PathBuf,
    db_path: String,
    dashboard: WebDashboard,
    // <<< AGENT-4 OPS: per-IP rate limits — same primitive the desktop server uses >>>
    rate_limits: Mutex<HashMap<String, (u32, Instant)>>,
    // <<< AGENT-4 OPS: connections currently being served, drained on shutdown >>>
    in_flight: AtomicUsize,
}

/// Signal handler: only flips an AtomicBool, so it is async-signal-safe.
extern "C" fn on_termination_signal(_sig: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

// <<< AGENT-4 OPS: graceful shutdown (item 13) >>>
//
// SIGTERM (docker stop) or SIGINT stops the accept loop, drains in-flight
// requests and then closes the database. Without a handler the default action
// would kill the process immediately mid-request.
fn install_signal_handlers() {
    // SAFETY: `on_termination_signal` touches no more than an AtomicBool.
    unsafe {
        libc::signal(
            libc::SIGTERM,
            on_termination_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            on_termination_signal as *const () as libc::sighandler_t,
        );
    }
}

fn handle_connection(state: &AppState, mut stream: TcpStream) {
    // Same 256-slot cap as the desktop transport: `in_flight` is
    // incremented before spawn, so reaching the cap here means shedding
    // this connection with 503 instead of growing threads unbounded.
    // SSE tails hold a slot for up to 5 min each — the cap is ample
    // because every stream is time-bounded (see `handle_sse_connection`).
    if state.in_flight.load(Ordering::SeqCst)
        > cybermanju_web::security::MAX_CONCURRENT_CONNECTIONS as usize
    {
        let resp = http_response(
            503,
            "application/json",
            r#"{"error":true,"status":503,"message":"Too many connections"}"#,
            None,
        );
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(30))).ok();

    let client_ip = stream
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    // <<< AGENT-4 OPS: rate limit (item 7) — `security::enforce_rate_limit`
    // through the shared wrapper, so Docker and the desktop transport use the
    // same fixed window, the same fail-closed poisoning rule and the same
    // per-IP accounting. >>>
    if !cybermanju_web::check_rate_limit(&state.rate_limits, &client_ip) {
        let resp = http_response(
            429,
            "application/json",
            r#"{"error":true,"status":429,"message":"Rate limit exceeded"}"#,
            None,
        );
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    let mut reader = BufReader::new(&stream);

    // Read request line (capped — an unbounded line is a memory DoS).
    let mut request_line = String::new();
    {
        use std::io::Read as _;
        let mut capped =
            (&mut reader).take(cybermanju_web::security::MAX_REQUEST_LINE_BYTES as u64 + 2);
        if capped.read_line(&mut request_line).is_err() {
            let resp = http_response(
                400,
                "application/json",
                r#"{"error":true,"status":400,"message":"Malformed request line"}"#,
                None,
            );
            let _ = stream.write_all(resp.as_bytes());
            return;
        }
    }
    if request_line.len() > cybermanju_web::security::MAX_REQUEST_LINE_BYTES {
        let resp = http_response(
            400,
            "application/json",
            r#"{"error":true,"status":400,"message":"Request line too long"}"#,
            None,
        );
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    let request_line = request_line.trim().to_string();

    // Parse method and path from "GET /path HTTP/1.1"
    let parts: Vec<&str> = request_line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        let resp = http_response(
            400,
            "application/json",
            r#"{"error":true,"status":400,"message":"Malformed request line"}"#,
            None,
        );
        let _ = stream.write_all(resp.as_bytes());
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers to determine Content-Length, Authorization, and Origin.
    // Capped like the desktop transport: at most 100 header lines, each
    // at most 8 KiB, auth at most 4 KiB — an uncapped header loop is a
    // memory DoS at 256 concurrent connections.
    let mut content_length: usize = 0;
    let mut auth_header: Option<String> = None;
    let mut origin_header: Option<String> = None;
    let mut header_lines: usize = 0;
    loop {
        if header_lines >= cybermanju_web::security::MAX_HEADER_LINES {
            let resp = http_response(
                400,
                "application/json",
                r#"{"error":true,"status":400,"message":"Too many headers"}"#,
                None,
            );
            let _ = stream.write_all(resp.as_bytes());
            return;
        }
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
            break;
        }
        if line.len() > cybermanju_web::security::MAX_HEADER_LINE_BYTES {
            let resp = http_response(
                400,
                "application/json",
                r#"{"error":true,"status":400,"message":"Header line too long"}"#,
                None,
            );
            let _ = stream.write_all(resp.as_bytes());
            return;
        }
        header_lines += 1;
        let line_trimmed = line.trim().to_lowercase();
        if line_trimmed.starts_with("content-length:") {
            content_length = line_trimmed
                .strip_prefix("content-length:")
                .unwrap_or_default()
                .trim()
                .parse()
                .unwrap_or(0);
        } else if line_trimmed.starts_with("authorization:") {
            let val = line.trim();
            if val.len() > 14 + cybermanju_web::security::MAX_AUTH_HEADER_BYTES {
                let resp = http_response(
                    401,
                    "application/json",
                    r#"{"error":true,"status":401,"message":"Authorization header too long"}"#,
                    None,
                );
                let _ = stream.write_all(resp.as_bytes());
                return;
            }
            auth_header = val
                .strip_prefix("Authorization:")
                .or_else(|| val.strip_prefix("authorization:"))
                .map(|s| s.trim().to_string());
        } else if line_trimmed.starts_with("origin:") {
            let val = line.trim();
            origin_header = val
                .strip_prefix("Origin:")
                .or_else(|| val.strip_prefix("origin:"))
                .map(|s| s.trim().to_string());
        }
    }

    // <<< AGENT-4 OPS: body size cap (item 7) — `security::enforce_body_limit`
    // rejects before a single body byte is allocated. >>>
    if let Err((status, message)) = cybermanju_web::security::enforce_body_limit(content_length) {
        let body = format!(
            r#"{{"error":true,"status":{},"message":"{}"}}"#,
            status, message
        );
        let resp = http_response(status, "application/json", &body, None);
        let _ = stream.write_all(resp.as_bytes());
        return;
    }

    // Read body if present
    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        if std::io::Read::read_exact(&mut reader, &mut buf).is_err() {
            body = String::new();
        } else if let Ok(s) = String::from_utf8(buf) {
            body = s;
        }
    }

    // <<< AGENT-4 OPS: CORS allowlist (item 7). The UI is served from this
    // same origin, so it needs no CORS header at all; only allowlisted
    // localhost origins get one. The origin is *filtered*, never reflected —
    // and `http_response` re-checks it, so a forwarded header cannot smuggle
    // a permissive Access-Control-Allow-Origin. >>>
    let origin = cybermanju_web::security::cors_origin_allowed(origin_header.as_deref());

    // SSE job tail serves the identical stream as the desktop transport.
    // The shared handler clears the write timeout, heartbeats, and caps
    // the stream at 5 min. Drop the buffered reader first: it borrows
    // the socket and the handler takes it by value.
    if let Some(job_id) = cybermanju_web::is_sse_events_path(method, path) {
        drop(reader);
        cybermanju_web::handle_sse_connection(
            &state.dashboard,
            stream,
            &job_id,
            auth_header.as_deref(),
            origin.as_deref(),
        );
        return;
    }

    // Route: /api/* → shared REST handlers, everything else → static files
    if path.starts_with("/api/") || path == "/api" {
        let response = handle_request(
            &state.dashboard,
            &state.dashboard.db,
            method,
            path,
            &body,
            auth_header.as_deref(),
            origin.as_deref(),
        );
        let _ = stream.write_all(response.as_bytes());
    } else if method == "OPTIONS" {
        // CORS preflight for static assets
        let resp = http_response(204, "text/plain", "", origin.as_deref());
        let _ = stream.write_all(resp.as_bytes());
    } else if method == "GET" || method == "HEAD" {
        // Serve static files (binary-safe, writes directly to stream)
        serve_static_file(&mut stream, &state.static_dir, path);
    } else {
        let resp = http_response(
            405,
            "application/json",
            r#"{"error":"Method Not Allowed"}"#,
            None,
        );
        let _ = stream.write_all(resp.as_bytes());
    }
}

fn main() {
    env_logger::Builder::from_env("RUST_LOG").init();
    install_signal_handlers();

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3456".to_string())
        .parse()
        .unwrap_or(3456);

    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "/data/cybermanju.db".to_string());

    let static_dir: PathBuf = std::env::var("STATIC_DIR")
        .unwrap_or_else(|_| "./static".to_string())
        .into();

    // The Tantivy index lives on the same volume as the database so both
    // survive a container recreate and a backup covers them together.
    let index_path: String = std::env::var("SEARCH_INDEX_PATH").unwrap_or_else(|_| {
        Path::new(&db_path)
            .parent()
            .unwrap_or_else(|| Path::new("/data"))
            .join("tantivy_index")
            .to_string_lossy()
            .into_owned()
    });

    // Ensure database directory exists
    if let Some(parent) = Path::new(&db_path).parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            error!("Failed to create database directory {:?}: {}", parent, e);
            std::process::exit(1);
        }
    }

    // Verify static directory exists
    if !static_dir.exists() {
        error!("Static directory does not exist: {}", static_dir.display());
        std::process::exit(1);
    }

    // Open the database exactly once and share it with the REST handlers.
    let db = match Database::new(&db_path) {
        Ok(db) => Arc::new(RwLock::new(db)),
        Err(e) => {
            error!("Failed to open database {}: {}", db_path, e);
            std::process::exit(1);
        }
    };

    // <<< AGENT-4 OPS: real search index (item 4) — without this
    // WebDashboard::search_index stays None and /api/search silently degrades
    // to a substring scan of the files table.
    if let Err(e) = fs::create_dir_all(&index_path) {
        error!(
            "Failed to create search index directory {}: {}",
            index_path, e
        );
        std::process::exit(1);
    }
    let search_index = match SearchIndex::new(&index_path) {
        Ok(index) => index,
        Err(e) => {
            error!("Failed to open search index {}: {}", index_path, e);
            std::process::exit(1);
        }
    };

    let mut dashboard = WebDashboard::new_shared_with_bind_addr(port, Arc::clone(&db), "0.0.0.0");
    dashboard.set_search_index(Arc::new(RwLock::new(search_index)));
    // The dashboard now owns the only remaining database handle.
    drop(db);

    let state = Arc::new(AppState {
        static_dir,
        db_path: db_path.clone(),
        dashboard,
        rate_limits: Mutex::new(HashMap::new()),
        in_flight: AtomicUsize::new(0),
    });
    // The shared SSE handler polls `dashboard.running` as its liveness
    // signal; this transport never calls `WebDashboard::start` (it runs
    // its own accept loop), so arm it here and drop it after the accept
    // loop exits — otherwise every Docker SSE stream would end on its
    // first tick.
    state.dashboard.running.store(true, Ordering::SeqCst);

    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind on port {}: {}", port, e);
            std::process::exit(1);
        }
    };
    // Non-blocking accept so the loop can notice SIGTERM within ~50ms.
    listener.set_nonblocking(true).ok();

    info!("═══════════════════════════════════════════════════════");
    info!("  CyberManju OS — Web Server");
    info!("  Listening on http://{}", addr);
    info!("  API:        http://localhost:{}/api/health", port);
    info!("  Database:   {}", state.db_path);
    info!("  Search:     {}", index_path);
    info!("  Static:     {}", state.static_dir.display());
    info!("═══════════════════════════════════════════════════════");

    // Serve requests in a thread-per-connection model (same as cybermanju-web)
    while !SHUTDOWN.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                state.in_flight.fetch_add(1, Ordering::SeqCst);
                std::thread::spawn(move || {
                    handle_connection(&state, stream);
                    state.in_flight.fetch_sub(1, Ordering::SeqCst);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                warn!("Accept error: {}", e);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    // End SSE tails at the next 500 ms poll tick before draining.
    state.dashboard.running.store(false, Ordering::SeqCst);
    // <<< AGENT-4 OPS: drain → flush → exit (item 13) >>>
    info!(
        "Shutdown requested — draining {} in-flight request(s)",
        state.in_flight.load(Ordering::SeqCst)
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while state.in_flight.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let stuck = state.in_flight.load(Ordering::SeqCst);
    if stuck > 0 {
        warn!(
            "Shutdown deadline reached with {} request(s) in flight",
            stuck
        );
    }

    // redb commits durably at transaction time; dropping the last handle
    // closes the file cleanly (the dashboard owns that handle now).
    drop(state);
    info!("Database closed — shutdown complete");
}
