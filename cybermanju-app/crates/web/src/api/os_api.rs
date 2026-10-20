// CyberManju OS — OS routes (AGENT-8)
//
// Pre-created and pre-hooked by the supervisor: `crates/web/src/lib.rs` calls
// `route()` right after the auth gate, and `os` is already in
// `security::ROUTED_SEGMENTS`. AGENT-8 implements the arms; nobody edits
// `lib.rs`, `security.rs` or `api/mod.rs`.
//
// Contract: return `Some(response)` for a path this family owns, `None` for
// anything else. `POST /api/os/exec` (the `cybsh` terminal) is the important
// one — it must be authenticated like every other route.

use cybermanju_db::Database;
use cybermanju_os::api::{Kernel, OpenFlags};
use serde::Deserialize;
use serde::Serialize;
use std::sync::{Arc, RwLock};

/// Render a `Result` the way this crate does everywhere else: 404 when the
/// message reports a missing entity, 501 when the capability does not exist
/// on this platform, 400 otherwise (`not a directory: …`, `disk full: …`).
fn respond<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("not found:")
                || message.contains("not found:")
                || message.starts_with("no such")
            {
                404
            } else if message.starts_with("unsupported:") {
                501
            } else {
                400
            };
            crate::json_error(status, &message, origin)
        }
    }
}

/// `400` for a body this route cannot even parse.
fn bad_request(message: String, origin: Option<&str>) -> String {
    crate::json_error(400, &message, origin)
}

/// Every request body is JSON; this is the house error for one that is not.
fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|err| format!("Invalid JSON: {}", err))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecRequest {
    /// One shell line, exactly as the user typed it (`echo hi | cat`).
    line: String,
}

/// `/api/os/exec` result. A command that *runs* and reports a failure is a
/// 200 with `ok: false` — the terminal shows the message, it does not fail the
/// HTTP request. Only a body we cannot parse is a 400.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecResult {
    ok: bool,
    line: String,
    output: String,
    /// Present when `ok` is false, for clients that only read `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    prompt: &'static str,
}

/// `du(1)` for one path.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuResult {
    path: String,
    bytes: u64,
    files: u64,
}

/// `PUT /api/os/write` body — the editor save path (P1-6). Same envelope as
/// the wasm `write` arm (`{"ok":true,"output":"wrote …"}`) so every transport
/// answers one shape.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteRequest {
    path: String,
    content: String,
}

/// Single-write cap, mirroring `MAX_WRITE_BYTES` in `crates/os-wasm/src/os.rs`
/// (shared 1 MiB contract, P1-17).
const MAX_WRITE_BYTES: usize = 1024 * 1024;

/// Write text through the volume kernel: containment-checked `resolve`,
/// admission-controlled `write` (honest `disk_full:`), fd always closed.
fn write_volume_text(path: &str, content: &str) -> Result<usize, String> {
    if path.trim().is_empty() {
        return Err("invalid: path is required".to_string());
    }
    let kernel = Kernel::global();
    let fd = kernel.open(path, OpenFlags::create())?;
    let bytes = content.as_bytes();
    let mut written = 0usize;
    let result = (|| {
        while written < bytes.len() {
            let n = kernel.write(fd, &bytes[written..])?;
            if n == 0 {
                return Err("io error: write returned 0 bytes".to_string());
            }
            written += n;
        }
        Ok(written)
    })();
    let _ = kernel.close(fd);
    result
}

/// One `key=value` lookup on the raw query string (`path=…&…`), percent-decoded.
fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(pct_decode(v));
            }
        }
    }
    None
}

/// Percent-decode a path segment or query value into bytes, then UTF-8.
/// Malformed `%` runs survive verbatim; `+` is left alone (only query
/// *values* use `+`-for-space, and volume paths may legitimately contain `+`).
fn pct_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut raw: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(&h), Some(&l)) = (bytes.get(i + 1), bytes.get(i + 2)) {
                if let (Some(h), Some(l)) = (hex_val(h), hex_val(l)) {
                    raw.push(h << 4 | l);
                    i += 3;
                    continue;
                }
            }
        }
        raw.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `sync start …` inside `POST /api/os/exec`, run for real.
///
/// The normal `os_api::route` path holds the request database lock, under
/// which `start_job` would deadlock (it takes its own short read lock to
/// load the config). So `route_request` calls this **before** the lock is
/// taken: a line that parses as a single `sync start` becomes a detached
/// job via `start_job` — exactly like `POST /api/sync/start` — and answers
/// the terminal's `ExecResult` shape (`ok: true` + `{jobId}` output).
/// Anything else returns `None` and flows to the locked path untouched.
///
/// P0-10: `claims` come from the JWT gate — the job is owned by the
/// caller, and the line's file ids are owner-checked (P0-1) before the run
/// registers. Public callers (`None`) can only reach this on routes that
/// already passed the auth gate, so `None` denies the file check.
pub fn try_sync_start_exec(
    shared: &Arc<RwLock<Database>>,
    body: &str,
    origin: Option<&str>,
    claims: Option<&crate::security::Claims>,
) -> Option<String> {
    let req: ExecRequest = serde_json::from_str(body).ok()?;
    let start = cybermanju_os::shell::parse_sync_start(&req.line)?;
    let line = req.line.clone();

    // Resolve the config id with a short-lived read; the guard is dropped
    // before `start_job` takes its own lock.
    let config_id = match start.config_id {
        Some(id) => id,
        None => {
            let guard = shared.read().ok()?;
            let configs = crate::api::sync_api::list_configs(&guard).ok()?;
            drop(guard);
            configs.into_iter().find(|c| c.enabled).map(|c| c.id)?
        }
    };

    // P0-10 owner + P0-1 object gate, both before the run registers.
    let (owner_id, _) = match claims {
        Some(c) => (c.user_id.clone(), c.role == "admin"),
        None => (String::new(), false),
    };
    if let Some(claims) = claims {
        let guard = shared.read().ok()?;
        if let Err(e) =
            crate::api::files::check_access_many(&guard, claims, &start.file_ids, "read")
        {
            return Some(crate::json_ok(
                &ExecResult {
                    ok: false,
                    line,
                    output: e,
                    error: None,
                    prompt: cybermanju_os::PROMPT,
                },
                origin,
            ));
        }
    }

    let output =
        match crate::api::sync_api::start_job_for(shared, &config_id, start.file_ids, &owner_id) {
            Ok(job) => format!(
                "started sync job {} (config {}) · {}/{} files — poll with `sync status`",
                job.job_id, job.config_id, job.progress.processed_files, job.progress.total_files,
            ),
            Err(message) => {
                return Some(crate::json_ok(
                    &ExecResult {
                        ok: false,
                        line,
                        output: message.clone(),
                        error: None,
                        prompt: cybermanju_os::PROMPT,
                    },
                    origin,
                ));
            }
        };
    Some(crate::json_ok(
        &ExecResult {
            ok: true,
            line,
            output,
            error: None,
            prompt: cybermanju_os::PROMPT,
        },
        origin,
    ))
}

/// Dispatch `/api/os/*` (exec, complete, stat, ls, du, df, ps, top, jobs,
/// workers, write).
///
/// Volume paths arrive two ways: legacy segments (`/api/os/ls/projects/2026`
/// reads `/projects/2026`) and the `?path=` query form the frontend builds
/// with `encodeURIComponent` (P1-7 — spaces, unicode and `..` survive). Both
/// are percent-decoded; the query form wins only when no segments are given.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    query: &str,
    body: &str,
    origin: Option<&str>,
) -> Option<String> {
    let rest: &[&str] = match path_segments {
        ["api", "os"] => &[],
        ["api", "os", rest @ ..] => rest,
        _ => return None,
    };
    if rest.is_empty() {
        return Some(crate::json_error(404, "not found: /api/os", origin));
    }
    let head = rest[0];
    // Path arguments arrive as segments; a volume path is re-joined so
    // `/api/os/ls/projects/2026` reads `/projects/2026`.
    let path = if rest.len() < 2 {
        query_param(query, "path")
            .or_else(|| {
                (head == "complete")
                    .then(|| query_param(query, "prefix"))
                    .flatten()
            })
            .unwrap_or_else(|| "/".to_string())
    } else {
        pct_decode(&format!("/{}", rest[1..].join("/")))
    };

    let response = match (head, method) {
        // ─── cybsh ──────────────────────────────────────────────────────
        ("exec", "POST") => match parse::<ExecRequest>(body) {
            Ok(request) => {
                let line = request.line.clone();
                let (ok, output) = match cybermanju_os::execute(&line, Some(db)) {
                    Ok(output) => (true, output),
                    Err(message) => (false, message),
                };
                crate::json_ok(
                    &ExecResult {
                        ok,
                        line,
                        output,
                        error: None,
                        prompt: cybermanju_os::PROMPT,
                    },
                    origin,
                )
            }
            Err(err) => bad_request(err, origin),
        },

        // Tab completion for the terminal's prompt.
        ("complete", "GET") => respond(Ok(cybermanju_os::completions(&path[1..])), origin),

        // ─── filesystem ────────────────────────────────────────────────
        ("stat", "GET") => respond(Kernel::global().stat(&path), origin),
        ("ls", "GET") => respond(Kernel::global().readdir(&path), origin),
        ("du", "GET") => match Kernel::global().du(&path) {
            Ok((bytes, files)) => crate::json_ok(&DuResult { path, bytes, files }, origin),
            Err(message) => crate::json_error(404, &message, origin),
        },
        ("df", "GET") => crate::json_ok(&Kernel::global().df(Some(db)), origin),

        // ─── task table / fan-out ───────────────────────────────────────
        ("ps", "GET") => crate::json_ok(&cybermanju_os::ps(Some(db)), origin),
        ("top", "GET") => crate::json_ok(&cybermanju_os::top(Some(db)), origin),
        ("workers", "GET") => crate::json_ok(&cybermanju_os::workers(Some(db)), origin),
        ("jobs", "GET") => crate::json_ok(&cybermanju_os::available_jobs(), origin),

        // ─── volume write (editor save path, P1-6) ────────────────────────
        ("write", "PUT") => match parse::<WriteRequest>(body) {
            Ok(request) => {
                if request.content.len() > MAX_WRITE_BYTES {
                    crate::json_error(
                        413,
                        &format!(
                            "too_large: content is {} bytes, volume write limit is {}",
                            request.content.len(),
                            MAX_WRITE_BYTES
                        ),
                        origin,
                    )
                } else {
                    match write_volume_text(&request.path, &request.content) {
                        Ok(bytes) => crate::json_ok(
                            &serde_json::json!({
                                "ok": true,
                                "output": format!("wrote {} ({} bytes)", request.path, bytes),
                                "path": request.path,
                                "bytes": bytes,
                            }),
                            origin,
                        ),
                        Err(message)
                            if message.starts_with("too_large:")
                                || message.starts_with("disk_full:") =>
                        {
                            let status = if message.starts_with("too_large:") {
                                413
                            } else {
                                507
                            };
                            crate::json_error(status, &message, origin)
                        }
                        Err(message) => respond::<serde_json::Value>(Err(message), origin),
                    }
                }
            }
            Err(err) => bad_request(err, origin),
        },

        // Known endpoint, wrong verb — say so instead of falling through to
        // the generic 404.
        (_, _) if KNOWN.contains(&head) => crate::json_error(
            405,
            &format!("method not allowed: {method} /api/os/{head}"),
            origin,
        ),
        _ => crate::json_error(404, &format!("not found: /api/os/{head}"), origin),
    };
    Some(response)
}

/// Endpoints this family owns, for the 405 arm above.
const KNOWN: &[&str] = &[
    "exec", "complete", "stat", "ls", "du", "df", "ps", "top", "workers", "jobs", "write",
];
