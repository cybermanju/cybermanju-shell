// CyberManju OS — auto-sync scheduler (AGENT-2 item 11)
//
// `SyncConfig.auto_sync` used to be a field nothing read. This is the
// reader: a background thread that wakes once a minute and, for every
// **enabled + autoSync** config that is due, runs the normal pipeline over
// the full live file set — same registry, same run ids, same `sync_runs`
// history as a manual run.
//
// Deliberate limits (documented, not accidental):
//   * interval is a constant (10 min per config) — no new config field, so
//     no other agent's struct literals break;
//   * auto-sync never stomps a manual run: if any run is active it waits;
//   * the first tick runs immediately (opting in should do something);
//   * a config that is disabled or has `autoSync: false` is never touched.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use cybermanju_compression::TripleCompressor;
use cybermanju_db::Database;
use cybermanju_types::sync::{SyncRunRecord, SyncStatus};
use log::{error, info, warn};

use crate::pipeline::SyncPipeline;
use crate::state::{RunRegistry, SyncState};

/// Minimum gap between two automatic runs of the same config.
pub const AUTO_SYNC_INTERVAL: Duration = Duration::from_secs(600);

/// How often the scheduler wakes to look for due configs.
const TICK_INTERVAL: Duration = Duration::from_secs(60);

static SCHEDULER: OnceLock<()> = OnceLock::new();

/// Start the background auto-sync thread exactly once. Safe to call from
/// every request or command — all calls after the first are no-ops.
pub fn ensure_started(db: Arc<RwLock<Database>>) {
    if SCHEDULER.set(()).is_err() {
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("cybermanju-auto-sync".to_string())
        .spawn(move || run_loop(db))
    {
        error!("could not start auto-sync scheduler: {}", e);
    }
}

fn run_loop(db: Arc<RwLock<Database>>) {
    info!(
        "auto-sync scheduler started (tick {}s, interval {}s)",
        TICK_INTERVAL.as_secs(),
        AUTO_SYNC_INTERVAL.as_secs()
    );
    // Thread-local: when the last config ran, keyed by config id.
    let mut last_run: HashMap<String, Instant> = HashMap::new();
    loop {
        std::thread::sleep(TICK_INTERVAL);
        if let Err(e) = tick(&db, &mut last_run) {
            warn!("auto-sync tick failed: {}", e);
        }
    }
}

fn tick(db: &RwLock<Database>, last_run: &mut HashMap<String, Instant>) -> Result<(), String> {
    let registry = RunRegistry::global();

    // Never stomp a manual run: auto-sync waits its turn.
    if let Some(latest) = registry.latest() {
        if !latest.is_finished() {
            return Ok(());
        }
    }

    let due: Vec<_> = {
        let guard = db.read().map_err(|e| e.to_string())?;
        guard
            .list_sync_configs()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|c| c.enabled && c.auto_sync)
            .filter(|c| {
                last_run
                    .get(&c.id)
                    .map(|at| at.elapsed() >= AUTO_SYNC_INTERVAL)
                    .unwrap_or(true)
            })
            .collect()
    };
    if due.is_empty() {
        return Ok(());
    }

    let file_ids = {
        let guard = db.read().map_err(|e| e.to_string())?;
        guard.list_file_ids().map_err(|e| e.to_string())?
    };
    if file_ids.is_empty() {
        return Ok(());
    }

    for config in due {
        // A manual run may have started while we were preparing.
        if let Some(latest) = registry.latest() {
            if !latest.is_finished() {
                return Ok(());
            }
        }

        last_run.insert(config.id.clone(), Instant::now());
        let total = file_ids.len() as u32;
        let run =
            match registry.begin_owned(&config.id, Arc::new(SyncState::new()), total, "system") {
                Ok(run) => run,
                Err(_) => return Ok(()),
            };
        info!(
            "auto-sync run {} started for config '{}'",
            run.run_id, config.id
        );

        let pipeline = SyncPipeline::new(config, Arc::clone(&run.state));
        let compression = TripleCompressor::new();
        let outcome = pipeline.sync_all(file_ids.clone(), db, &compression);

        // Terminal bookkeeping mirrors `sync_api::execute_run` — pollers
        // must never be left on a status that never becomes terminal.
        let (result, run_error) = match outcome {
            Ok(result) => {
                if run.state.is_cancelled() {
                    run.state.set_status(SyncStatus::Cancelled);
                } else {
                    run.state.set_status(SyncStatus::Completed);
                    run.state.set_current(None);
                    let total = run.state.snapshot().total_files;
                    run.state.set_processed(total);
                }
                (Some(result), None)
            }
            Err(e) => {
                run.state.set_status(SyncStatus::Error);
                run.state.add_error(e.clone());
                (None, Some(e))
            }
        };
        registry.finish(&run, result.clone(), run_error);

        let progress = run.state.snapshot();
        let record = SyncRunRecord {
            run_id: run.run_id.clone(),
            config_id: run.config_id.clone(),
            owner_id: Some(run.owner_id.clone()),
            started_at: run.started_at.clone(),
            finished_at: run
                .outcome()
                .map(|o| o.finished_at)
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            status: progress.status.clone(),
            files_synced: result.as_ref().map(|r| r.files_synced).unwrap_or(0),
            bytes_uploaded: result.as_ref().map(|r| r.bytes_uploaded).unwrap_or(0),
            errors: progress.errors.clone(),
            progress,
            result,
        };
        {
            let guard = db.read().map_err(|e| e.to_string())?;
            if let Err(e) = guard.save_sync_run(&record) {
                error!("could not persist auto-sync run {}: {}", record.run_id, e);
            }
        }
        info!(
            "auto-sync run {} finished ({:?}, {} files)",
            record.run_id, record.status, record.files_synced
        );
    }
    Ok(())
}
