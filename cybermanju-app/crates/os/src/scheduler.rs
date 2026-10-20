// CyberManju OS — cron daemon: fires due `.cybsh` schedules.
//
// Pattern: identical to `cybermanju_sync::scheduler` — a `OnceLock` + a
// named thread woken every 60 s. Wired in **both** transport entry points
// (`web/lib.rs` and `tauri/commands/sync.rs`); a single wiring leaves the
// other transport dark (the `ensure_scrub_started` lesson — it is dead
// code today because it was never called).
//
// Execution reuses the script interpreter (`shell::execute("run <path>")`):
// no fourth dialect, no host shell. Runaway scripts are bounded by the
// interpreter's own step/size caps (`MAX_SOURCE_BYTES`, step limit), so the
// daemon never joins a worker with a wall-clock timeout and never leaks a
// stuck thread.

use std::sync::{Arc, OnceLock, RwLock};

use chrono::{DateTime, Utc};
use cybermanju_db::Database;
use cybermanju_types::schedule::{ScheduleRow, ScheduleRun};
use log::{error, info, warn};

use crate::schedule::ScheduleSpec;

/// How often the scheduler wakes to look for due rows.
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Output kept per run in `schedule_runs.output_tail`.
const OUTPUT_TAIL_LIMIT: usize = 8 * 1024;

static SCHEDULER: OnceLock<()> = OnceLock::new();

/// Start the background cron thread exactly once. Safe to call from every
/// request or command — all calls after the first are no-ops.
pub fn ensure_started(db: Arc<RwLock<Database>>) {
    if SCHEDULER.set(()).is_err() {
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("cybermanju-cron".to_string())
        .spawn(move || run_loop(db))
    {
        error!("could not start cron scheduler: {}", e);
    }
}

fn run_loop(db: Arc<RwLock<Database>>) {
    info!("cron scheduler started (tick {}s)", TICK_INTERVAL.as_secs());
    loop {
        std::thread::sleep(TICK_INTERVAL);
        if let Err(e) = tick(&db) {
            warn!("cron tick failed: {}", e);
        }
    }
}

fn tick(db: &RwLock<Database>) -> Result<(), String> {
    let now = Utc::now();
    let due: Vec<ScheduleRow> = {
        let guard = db.read().map_err(|e| e.to_string())?;
        guard
            .list_schedules()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.enabled)
            .filter(|s| is_due(s, now))
            .collect()
    };
    if due.is_empty() {
        return Ok(());
    }
    for row in due {
        fire(db, row, now)?;
    }
    Ok(())
}

fn is_due(row: &ScheduleRow, now: DateTime<Utc>) -> bool {
    if row.last_fired_at.is_none() && row.run_on_boot {
        return true;
    }
    match row.next_fire_at.as_deref() {
        Some(ts) => match parse_ts(ts) {
            Ok(t) => t <= now,
            Err(_) => false,
        },
        None => false,
    }
}

fn parse_ts(ts: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(ts)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| e.to_string())
}

/// Next fire after `now` for an expression, or `None` for a bad expr.
fn next_fire(expr: &str, now: DateTime<Utc>) -> Option<String> {
    match crate::schedule::ScheduleSpec::parse(expr) {
        Ok(spec) => spec.next_after(now).map(|t| t.to_rfc3339()),
        Err(e) => {
            warn!("cron: bad expr `{expr}` on a schedule: {e}");
            None
        }
    }
}

/// Recompute `next_fire_at` after an enable/edit. Exported so the REST
/// create/update handlers and the Tauri `cron_save` command stay thin.
pub fn recompute_next(row: &mut ScheduleRow, now: DateTime<Utc>) {
    row.next_fire_at = next_fire(&row.expr, now);
}

fn fire(db: &RwLock<Database>, mut row: ScheduleRow, now: DateTime<Utc>) -> Result<(), String> {
    // Advance last/next **before** running: a crashing script must not
    // re-fire every tick. `run_on_boot` is one-shot.
    row.last_fired_at = Some(now.to_rfc3339());
    row.run_on_boot = false;
    row.next_fire_at = next_fire(&row.expr, now);
    let run_id = format!("sched-{}", uuid::Uuid::new_v4());
    row.last_run_id = Some(run_id.clone());
    let handle = {
        let guard = db.read().map_err(|e| e.to_string())?;
        guard.save_schedule(&row).map_err(|e| e.to_string())?;
        // Clone the redb handle so the script runs outside the app lock.
        guard.clone()
    };

    let started = Utc::now();
    let (status, output_tail) = match execute_script(&handle, &row.path) {
        Ok(out) => ("ok".to_string(), Some(tail(&out))),
        Err(msg) => ("error".to_string(), Some(tail(&msg))),
    };
    let run = ScheduleRun {
        run_id,
        schedule_id: row.id.clone(),
        started_at: started.to_rfc3339(),
        finished_at: Utc::now().to_rfc3339(),
        status,
        output_tail,
    };
    {
        let guard = db.read().map_err(|e| e.to_string())?;
        guard.save_schedule_run(&run).map_err(|e| e.to_string())?;
    }
    info!("cron: schedule '{}' fired ({})", row.id, run.status);
    Ok(())
}

/// Run one schedule's script through the interpreter. The Database handle
/// is cloned (shares the redb handle) so the caller's app lock is not held
/// across the run — same lock-free discipline as the sync pipeline.
fn execute_script(handle: &Database, path: &str) -> Result<String, String> {
    let owned = handle.clone();
    crate::shell::execute(&format!("run {path}"), Some(&owned))
}

fn tail(s: &str) -> String {
    let t = s.trim_end();
    if t.len() <= OUTPUT_TAIL_LIMIT {
        t.to_string()
    } else {
        let mut end = OUTPUT_TAIL_LIMIT;
        while !t.is_char_boundary(end) {
            end -= 1;
        }
        t[..end].to_string()
    }
}

/// One-shot: fire a schedule immediately (the `cron run <id>` verb and
/// `POST /api/cron/{id}/run`). Returns the persisted run on success.
///
/// Takes a plain `&Database` (not the app lock) so the REST/Tauri callers —
/// which already hold the request lock — can reuse their handle without
/// deadlocking: the script runs on a cloned redb handle, outside any lock.
pub fn run_now(handle: &Database, id: &str) -> Result<ScheduleRun, String> {
    let mut row = handle
        .get_schedule(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: no schedule `{id}`"))?;
    let now = Utc::now();
    row.last_fired_at = Some(now.to_rfc3339());
    row.run_on_boot = false;
    row.next_fire_at = next_fire(&row.expr, now);
    let run_id = format!("sched-{}", uuid::Uuid::new_v4());
    row.last_run_id = Some(run_id.clone());
    handle.save_schedule(&row).map_err(|e| e.to_string())?;
    let started = Utc::now();
    let (status, output_tail) = match execute_script(handle, &row.path) {
        Ok(out) => ("ok".to_string(), Some(tail(&out))),
        Err(msg) => ("error".to_string(), Some(tail(&msg))),
    };
    let run = ScheduleRun {
        run_id,
        schedule_id: row.id.clone(),
        started_at: started.to_rfc3339(),
        finished_at: Utc::now().to_rfc3339(),
        status,
        output_tail,
    };
    handle.save_schedule_run(&run).map_err(|e| e.to_string())?;
    Ok(run)
}

// ─── store CRUD (single source of truth for REST + Tauri + cybsh verb) ──
// These live here, not in `web::cron_api`, so the `cron` shell verb (in this
// crate) and the Tauri commands can share them without `os` depending on `web`.

/// Every schedule row, oldest `created_at` first.
pub fn list(db: &Database) -> Result<Vec<ScheduleRow>, String> {
    db.list_schedules().map_err(|e| e.to_string())
}

/// One schedule by id.
pub fn get(db: &Database, id: &str) -> Result<Option<ScheduleRow>, String> {
    db.get_schedule(id).map_err(|e| e.to_string())
}

/// Validate + persist a row, recomputing `next_fire_at`. Used by create and
/// by the `cron add` verb (which pre-fills the row).
pub fn save(db: &Database, mut row: ScheduleRow) -> Result<ScheduleRow, String> {
    ScheduleSpec::parse(&row.expr)?;
    if !row.path.to_lowercase().ends_with(".cybsh") {
        return Err(format!(
            "invalid: schedule path must be a .cybsh script (got `{}`)",
            row.path
        ));
    }
    if row.id.trim().is_empty() {
        row.id = format!("sched-{}", uuid::Uuid::new_v4());
    }
    recompute_next(&mut row, Utc::now());
    db.save_schedule(&row).map_err(|e| e.to_string())?;
    Ok(row)
}

/// Remove a schedule. `false` = nothing matched.
pub fn remove(db: &Database, id: &str) -> Result<bool, String> {
    db.remove_schedule(id).map_err(|e| e.to_string())
}

/// Enable or disable. An enable recomputes `next_fire_at` from now so a row
/// that sat disabled does not fire instantly for a stale next.
pub fn set_enabled(db: &Database, id: &str, enabled: bool) -> Result<ScheduleRow, String> {
    let mut row = db
        .get_schedule(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("not_found: no schedule `{id}`"))?;
    row.enabled = enabled;
    if enabled {
        recompute_next(&mut row, Utc::now());
    }
    db.save_schedule(&row).map_err(|e| e.to_string())?;
    Ok(row)
}

/// Fire history for one schedule, newest first.
pub fn history(db: &Database, id: &str, limit: usize) -> Result<Vec<ScheduleRun>, String> {
    db.list_schedule_runs(id, limit).map_err(|e| e.to_string())
}
