// CyberManju OS — cron/scheduler routes (Phase 1).
//
// Contract: `route()` returns `Some(response)` for `/api/cron/*` and `None`
// for anything else. Wired in `lib.rs` beside the disk/os route families
// and registered in `security::ROUTED_SEGMENTS` ("cron").
//
// The store logic lives in `cybermanju_os::scheduler` (single source of
// truth shared with the Tauri commands and the `cron` shell verb). This
// module only maps HTTP → those helpers.
//
// Roles: `required_role` already gates create/update/delete/run to Admin
// (a schedule runs arbitrary `.cybsh` scripts — same posture as MCP
// attach). List/history stay `Authenticated`.

use cybermanju_db::Database;
use cybermanju_os::scheduler;
use cybermanju_types::schedule::{ScheduleRow, ScheduleRun};
use serde::{Deserialize, Serialize};

/// Render a `Result` the way this crate does elsewhere: 404 on `not_found:`,
/// 400 otherwise (invalid expression, missing file).
fn respond<T: Serialize>(result: Result<T, String>, origin: Option<&str>) -> String {
    match result {
        Ok(value) => crate::json_ok(&value, origin),
        Err(message) => {
            let status = if message.starts_with("not_found:") {
                404
            } else {
                400
            };
            crate::json_error(status, &message, origin)
        }
    }
}

fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|err| format!("Invalid JSON: {}", err))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRequest {
    /// Optional existing id. Supplied → upsert that row (the shape
    /// `cron_save(row)` takes over IPC); empty/absent → create fresh.
    #[serde(default)]
    id: Option<String>,
    /// Volume path of the `.cybsh` script.
    path: String,
    /// Cron expression (`"30 2 * * *"`) or interval (`"every 10m"`).
    expr: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    /// Fire once on daemon start even if the first cron fire is later.
    #[serde(default)]
    run_on_boot: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRequest {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    expr: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    run_on_boot: Option<bool>,
}

fn new_row(req: &CreateRequest) -> ScheduleRow {
    ScheduleRow {
        id: req
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("sched-{}", uuid::Uuid::new_v4())),
        path: req.path.clone(),
        expr: req.expr.clone(),
        enabled: req.enabled.unwrap_or(true),
        description: req.description.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        last_fired_at: None,
        next_fire_at: None,
        last_run_id: None,
        run_on_boot: req.run_on_boot,
    }
}

/// Create (no/empty `id`) or update (existing `id`) a schedule. An update
/// keeps the row's history — `created_at`/`last_fired_at`/`last_run_id`
/// belong to the schedule, not to the last write of its definition.
fn upsert(db: &Database, req: &CreateRequest) -> Result<ScheduleRow, String> {
    let mut row = new_row(req);
    if let Some(id) = req.id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(existing) = scheduler::get(db, id)? {
            row.created_at = existing.created_at;
            row.last_fired_at = existing.last_fired_at;
            row.last_run_id = existing.last_run_id;
        }
    }
    scheduler::save(db, row)
}

/// Dispatch `/api/cron/*`.
pub fn route(
    db: &Database,
    method: &str,
    path_segments: &[&str],
    body: &str,
    origin: Option<&str>,
) -> Option<String> {
    let response = match path_segments {
        ["api", "cron"] if method == "GET" => respond(scheduler::list(db), origin),

        ["api", "cron"] if method == "POST" => match parse::<CreateRequest>(body) {
            Ok(req) => respond(upsert(db, &req), origin),
            Err(e) => respond::<ScheduleRow>(Err(e), origin),
        },

        // NOTE: `POST /api/cron/{id}/run` is handled lockless in lib.rs
        // before the request lock is taken (the script executes
        // synchronously); it never reaches this family.
        ["api", "cron", id, "enable"] if method == "POST" => {
            respond(scheduler::set_enabled(db, id, true), origin)
        }

        ["api", "cron", id, "disable"] if method == "POST" => {
            respond(scheduler::set_enabled(db, id, false), origin)
        }

        ["api", "cron", id, "runs"] if method == "GET" => {
            respond(scheduler::history(db, id, 20), origin)
        }

        ["api", "cron", id] if method == "PUT" => match parse::<UpdateRequest>(body) {
            Ok(req) => respond(update(db, id, &req), origin),
            Err(e) => respond::<ScheduleRow>(Err(e), origin),
        },

        ["api", "cron", id] if method == "DELETE" => match scheduler::remove(db, id) {
            Ok(true) => respond(Ok(serde_json::json!({ "removed": true })), origin),
            Ok(false) => {
                respond::<serde_json::Value>(Err(format!("not_found: no schedule `{id}`")), origin)
            }
            Err(e) => respond::<serde_json::Value>(Err(e), origin),
        },

        _ => return None,
    };
    Some(response)
}

fn update(db: &Database, id: &str, req: &UpdateRequest) -> Result<ScheduleRow, String> {
    let mut row =
        scheduler::get(db, id)?.ok_or_else(|| format!("not_found: no schedule `{id}`"))?;
    if let Some(path) = &req.path {
        if !path.to_lowercase().ends_with(".cybsh") {
            return Err(format!(
                "invalid: schedule path must be a .cybsh script (got `{path}`)"
            ));
        }
        row.path = path.clone();
    }
    if let Some(expr) = &req.expr {
        row.expr = expr.clone();
    }
    if let Some(description) = &req.description {
        row.description = Some(description.clone());
    }
    if let Some(enabled) = req.enabled {
        row.enabled = enabled;
    }
    if let Some(run_on_boot) = req.run_on_boot {
        row.run_on_boot = run_on_boot;
    }
    scheduler::save(db, row)
}

// Re-exported for callers that imported these from here historically.
pub use scheduler::{history, list, remove, save, set_enabled};

/// Run-history type re-exported so the Tauri wrapper's signature stays
/// stable without importing `cybermanju_types` directly.
pub type CronRun = ScheduleRun;
