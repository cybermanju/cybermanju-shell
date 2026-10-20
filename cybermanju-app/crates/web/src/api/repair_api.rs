// CyberManju OS — durability routes (AGENT-7)
//
// Pre-created and pre-hooked by the supervisor: `crates/web/src/lib.rs` calls
// `route()` right after the auth gate, and `repair`/`scrub`/`lease` are already
// in `security::ROUTED_SEGMENTS`. AGENT-7 implements the arms; nobody edits
// `lib.rs`, `security.rs` or `api/mod.rs`.
//
// Contract: return `Some(response)` for a path this family owns, `None` for
// anything else. Do NOT hold this call across long work (a scrub or repair run
// is a background task) — spawn it and return `202` with a task id.
//
// A request only ever holds `&Database` for its own lifetime, so every worker
// below runs on a `Snapshot` copied out under the lock and parks its results
// on `repair`'s write-behind queue; the next request that holds the lock
// (`flush_pending` at the top of this function) makes them durable.

use cybermanju_db::Database;
use cybermanju_sync::{gc, health, lease, manifest, repair, scrub};
use serde::Serialize;
use serde_json::{json, Value};

/// Dispatch `/api/repair/*`, `/api/scrub/*`, `/api/lease/*`.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    body: &str,
    origin: Option<&str>,
) -> Option<String> {
    if !matches!(path_segments, ["api", "repair" | "scrub" | "lease", ..]) {
        return None;
    }

    // Land anything the last background worker left behind before reading.
    if let Err(e) = repair::flush_pending(db) {
        log::warn!("write-behind flush failed: {}", e);
    }

    match path_segments {
        // ── Durability status ────────────────────────────────────────────
        ["api", "repair", "status"] if method == "GET" => Some(respond(repair::status(db), origin)),
        ["api", "repair", "tasks"] if method == "GET" => Some(respond(Ok(repair::tasks()), origin)),
        ["api", "repair", "health"] if method == "GET" => Some(respond(health::status(db), origin)),

        // ── Long work: spawn, answer 202 with a task id ──────────────────
        ["api", "repair", "run"] if method == "POST" => start_repair(db, body, origin),
        ["api", "repair", "rebuild"] if method == "POST" => start_rebuild(db, origin),
        ["api", "repair", "gc"] if method == "POST" => start_gc(db, body, origin),
        ["api", "scrub", "run"] if method == "POST" => start_scrub(db, origin),

        // ── Scrub history ────────────────────────────────────────────────
        ["api", "scrub", "runs"] if method == "GET" => {
            Some(respond(scrub::list_scrub_runs(db), origin))
        }

        // ── Volume lease (a single table row — fast, stays on this thread)
        ["api", "lease", "acquire"] if method == "POST" => Some(respond(
            parse_body(body).and_then(|req| {
                lease::acquire_lease_in(
                    db,
                    &str_field(&req, "holder").unwrap_or_else(lease::default_holder),
                    &str_field(&req, "scope").unwrap_or_else(|| lease::DEFAULT_SCOPE.to_string()),
                    int_field(&req, "ttlSecs").unwrap_or(lease::DEFAULT_LEASE_TTL_SECS),
                )
            }),
            origin,
        )),
        ["api", "lease", "release"] if method == "POST" => Some(respond(
            parse_body(body).and_then(|req| {
                lease::release_lease(
                    db,
                    &str_field(&req, "holder").unwrap_or_else(lease::default_holder),
                    &str_field(&req, "scope").unwrap_or_else(|| lease::DEFAULT_SCOPE.to_string()),
                )
            }),
            origin,
        )),
        ["api", "lease", "status"] if method == "GET" => Some(respond(
            lease::inspect_lease(db, lease::DEFAULT_SCOPE),
            origin,
        )),
        ["api", "lease", "status", scope] if method == "GET" => {
            Some(respond(lease::inspect_lease(db, scope), origin))
        }

        // Owned path, wrong method — still ours, so answer 405 rather than
        // letting the router claim it is unknown.
        ["api", "repair" | "scrub" | "lease", ..] => {
            Some(crate::json_error(405, "Method not allowed", origin))
        }

        _ => None,
    }
}

// ─── Background arms ────────────────────────────────────────────────────────

/// `POST /api/repair/run` — drain the finding queue and repair it off-thread.
fn start_repair(db: &Database, body: &str, origin: Option<&str>) -> Option<String> {
    let req = match parse_body(body) {
        Ok(req) => req,
        Err(e) => return Some(crate::json_error(400, &e, origin)),
    };
    let snapshot = match repair::snapshot(db) {
        Ok(snapshot) => snapshot,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    let mut findings = repair::drain_findings();
    for extra in req
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match serde_json::from_value::<repair::Finding>(extra.clone()) {
            Ok(finding) => findings.push(finding),
            Err(e) => {
                return Some(crate::json_error(
                    400,
                    &format!("integrity: bad finding in body: {}", e),
                    origin,
                ))
            }
        }
    }
    let count = findings.len();
    let detail = format!("{} finding(s)", count);
    let task = match spawn_task("repair", &detail, move |task_id| {
        let outcome = repair::repair_snapshot(&snapshot, &findings);
        let mut dropped = 0usize;
        for (file, manifest_json) in &outcome.updates {
            if !repair::queue_pending(repair::PendingOp::Manifest {
                file: file.clone(),
                manifest_json: manifest_json.clone(),
            }) {
                dropped += 1;
            }
        }
        for row in &outcome.rows {
            if !repair::queue_pending(repair::PendingOp::Repair(row.clone())) {
                dropped += 1;
            }
        }
        let (repaired, unrecoverable) = (outcome.repaired(), outcome.unrecoverable());
        repair::finish_task(
            task_id,
            if unrecoverable > 0 { "error" } else { "done" },
            Some(format!(
                "{} repaired, {} unrecoverable{}",
                repaired,
                unrecoverable,
                if dropped > 0 {
                    format!(", {dropped} results refused (write-behind queue full — retry)")
                } else {
                    String::new()
                }
            )),
        );
    }) {
        Ok(task) => task,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    Some(accepted(
        &task,
        "repair",
        json!({ "findings": count }),
        origin,
    ))
}

/// `POST /api/repair/rebuild` — fetch the catalog back from the providers.
fn start_rebuild(db: &Database, origin: Option<&str>) -> Option<String> {
    let configs = match db.list_sync_configs() {
        Ok(configs) => configs,
        Err(e) => return Some(crate::json_error(500, &e.to_string(), origin)),
    };
    let task = match spawn_task(
        "rebuild",
        &format!("{} provider(s)", configs.len()),
        move |task_id| match manifest::rebuild_from_remote(&configs) {
            Ok(report) => {
                let parked =
                    repair::queue_pending(repair::PendingOp::Rebuild(report.files.clone()));
                repair::finish_task(
                    task_id,
                    "done",
                    Some(
                        json!({
                            "files": report.files.len(),
                            "chunksSeen": report.chunks_seen,
                            "locatorsDropped": report.locators_dropped,
                            "providers": report.providers,
                            "warnings": report.warnings,
                            "queueFull": !parked,
                        })
                        .to_string(),
                    ),
                );
            }
            Err(e) => repair::finish_task(task_id, "error", Some(e)),
        },
    ) {
        Ok(task) => task,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    Some(accepted(&task, "rebuild", Value::Null, origin))
}

/// `POST /api/repair/gc` — sweep unreferenced chunks off-thread.
fn start_gc(db: &Database, body: &str, origin: Option<&str>) -> Option<String> {
    let req = match parse_body(body) {
        Ok(req) => req,
        Err(e) => return Some(crate::json_error(400, &e, origin)),
    };
    let dry_run = bool_field(&req, "dryRun").unwrap_or(false);
    let grace = int_field(&req, "graceSecs")
        .map(|s| s as i64)
        .unwrap_or(gc::GC_GRACE_SECS);
    let snapshot = match repair::snapshot(db) {
        Ok(snapshot) => snapshot,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    let counts = match gc::read_ref_counts(db) {
        Ok(counts) => counts,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    if !dry_run {
        // Refresh `chunk_refs` up front: the counts come from the same
        // snapshot the sweep sees, so ordering cannot change what is kept.
        if let Err(e) = gc::persist_ref_counts(db, &snapshot) {
            return Some(crate::json_error(500, &e, origin));
        }
    }
    let task = match spawn_task(
        "gc",
        &format!("{} file(s)", snapshot.records.len()),
        move |task_id| match gc::sweep(&snapshot, &snapshot.configs, &counts, dry_run, grace) {
            Ok(report) => repair::finish_task(
                task_id,
                "done",
                Some(
                    json!({
                        "checked": report.checked,
                        "kept": report.kept,
                        "deleted": report.deleted,
                        "bytesFreed": report.bytes_freed,
                        "dryRun": report.dry_run,
                        "providers": report.providers,
                        "warnings": report.warnings,
                    })
                    .to_string(),
                ),
            ),
            Err(e) => repair::finish_task(task_id, "error", Some(e)),
        },
    ) {
        Ok(task) => task,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    Some(accepted(&task, "gc", json!({ "dryRun": dry_run }), origin))
}

/// `POST /api/scrub/run` — one verification pass, queued for repair on the way.
fn start_scrub(db: &Database, origin: Option<&str>) -> Option<String> {
    let snapshot = match repair::snapshot(db) {
        Ok(snapshot) => snapshot,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    let detail = format!("{} striped file(s)", snapshot.records.len());
    let task = match spawn_task("scrub", &detail, move |task_id| {
        let run = scrub::scrub_snapshot(&snapshot);
        repair::queue_findings(run.findings.iter().map(|f| {
            repair::Finding::new(
                f.file_id.clone(),
                f.chunk_index,
                Some(f.config_id.clone()),
                match f.kind.as_str() {
                    "corrupt" => repair::FindingKind::Corrupt,
                    "missing" => repair::FindingKind::Missing,
                    _ => repair::FindingKind::Unverified,
                },
                format!("{}: {}", f.remote_path, f.detail),
            )
        }));
        repair::queue_pending(repair::PendingOp::Scrub(run.clone()));
        let health_parked = repair::queue_pending(repair::PendingOp::Health(health::all()));
        let state = if run.corrupt + run.missing > 0 || !health_parked {
            "error"
        } else {
            "done"
        };
        repair::finish_task(
            task_id,
            state,
            Some(format!(
                "{} checked, {} corrupt, {} missing{}",
                run.checked,
                run.corrupt,
                run.missing,
                if health_parked {
                    String::new()
                } else {
                    " — health rows refused (queue full, retry)".to_string()
                }
            )),
        );
    }) {
        Ok(task) => task,
        Err(e) => return Some(crate::json_error(500, &e, origin)),
    };
    Some(accepted(&task, "scrub", Value::Null, origin))
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Register a task, run `work` on a worker thread, hand back its id.
fn spawn_task<F>(kind: &str, detail: &str, work: F) -> Result<String, String>
where
    F: FnOnce(&str) + Send + 'static,
{
    let id = repair::begin_task(kind, detail);
    let task_id = id.clone();
    match std::thread::Builder::new()
        .name(format!("cybermanju-{}", kind))
        .spawn(move || work(&task_id))
    {
        Ok(_) => Ok(id),
        Err(e) => {
            repair::finish_task(&id, "error", Some(e.to_string()));
            Err(format!("could not start {} worker: {}", kind, e))
        }
    }
}

/// `202 Accepted` carrying the task id to poll (`GET /api/repair/status`).
fn accepted(task_id: &str, kind: &str, extra: Value, origin: Option<&str>) -> String {
    let body = json!({ "accepted": true, "taskId": task_id, "kind": kind, "detail": extra });
    crate::http_response(202, "application/json", &body.to_string(), origin)
}

/// Map a shared-API `Result` onto its status code: the documented error
/// prefixes decide (`conflict:` → 409, `unsupported:` → 501, data loss → 500).
fn respond<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("conflict:") {
                409
            } else if message.starts_with("unsupported:") {
                501
            } else if message.starts_with("integrity:") || message.starts_with("unrecoverable:") {
                500
            } else if message.to_lowercase().contains("not found") {
                404
            } else {
                400
            };
            crate::json_error(status, &message, origin)
        }
    }
}

/// An empty body is `{}`; anything unparseable is a client error.
fn parse_body(body: &str) -> Result<Value, String> {
    if body.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(body).map_err(|e| format!("integrity: body is not JSON: {}", e))
}

fn str_field(req: &Value, key: &str) -> Option<String> {
    req.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

fn int_field(req: &Value, key: &str) -> Option<u64> {
    req.get(key).and_then(Value::as_u64)
}

fn bool_field(req: &Value, key: &str) -> Option<bool> {
    req.get(key).and_then(Value::as_bool)
}
