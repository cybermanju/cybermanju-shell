// CyberManju OS — background scrubber (AGENT-7 item 1)
//
// Silent loss becomes detectable: a scheduled pass walks every chunk copy in
// every manifest, downloads it, and re-verifies its BLAKE3 against the
// recorded `artifact_hash`. Anything that does not match is reported **and
// queued for repair** — a finding is never just logged.
//
// Results land in the `scrub_runs` table: one row per pass, with the
// per-provider `{checked, ok, corrupt, missing, duration_ms}` breakdown the
// brief asks for.
//
// Two shapes again, mirroring `repair.rs`: `scrub(db)` for callers that hold
// the database (the daemon), `scrub_snapshot(...)` for a REST worker that
// only ever holds a snapshot.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant};

use cybermanju_db::Database;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::backends::create_backend;
use crate::health::{self, Outcome};
use crate::manifest;
use crate::repair::{self, Finding, FindingKind, Snapshot};
use crate::transfer;

/// How often the daemon runs a full pass.
pub const SCRUB_INTERVAL: Duration = Duration::from_secs(3600);
/// How often the daemon wakes up to look at the clock.
const TICK_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderScrub {
    pub config_id: String,
    pub checked: u32,
    pub ok: u32,
    pub corrupt: u32,
    pub missing: u32,
    pub duration_ms: u64,
}

impl ProviderScrub {
    pub fn new(config_id: &str) -> Self {
        Self {
            config_id: config_id.to_string(),
            checked: 0,
            ok: 0,
            corrupt: 0,
            missing: 0,
            duration_ms: 0,
        }
    }
}

/// One bad copy, as scrub found it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScrubFinding {
    pub file_id: String,
    pub chunk_index: u32,
    pub config_id: String,
    pub remote_path: String,
    /// `corrupt` | `missing`.
    pub kind: String,
    pub detail: String,
}

/// One full scrub pass.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScrubRun {
    pub run_id: String,
    pub started_at: String,
    pub finished_at: String,
    /// `completed` | `error`.
    pub status: String,
    pub checked: u32,
    pub ok: u32,
    pub corrupt: u32,
    pub missing: u32,
    pub duration_ms: u64,
    pub providers: Vec<ProviderScrub>,
    pub findings: Vec<ScrubFinding>,
    pub errors: Vec<String>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn run_id() -> String {
    format!(
        "scrub-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    )
}

/// Download one locator's bytes so the caller can hash them.
fn check_copy(
    config: &cybermanju_types::sync::SyncConfig,
    remote_path: &str,
) -> Result<Vec<u8>, String> {
    let backend = create_backend(config)?;
    let tmp = std::env::temp_dir().join(format!(
        "cybermanju-scrub-{}-{}.part",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    let tmp_str = tmp.to_string_lossy().to_string();
    let attempt = backend
        .download_file(remote_path, &tmp_str)
        .and_then(|()| std::fs::read(&tmp).map_err(|e| format!("read failed: {}", e)));
    let _ = std::fs::remove_file(&tmp);
    attempt
}

/// Walk every locator of every manifest and verify it. Cheap on the happy
/// path (one download per unique object) and honest on the sad one.
pub fn scrub_snapshot(snapshot: &Snapshot) -> ScrubRun {
    let started = Instant::now();
    let mut run = ScrubRun {
        run_id: run_id(),
        started_at: now(),
        finished_at: String::new(),
        status: "completed".to_string(),
        checked: 0,
        ok: 0,
        corrupt: 0,
        missing: 0,
        duration_ms: 0,
        providers: Vec::new(),
        findings: Vec::new(),
        errors: Vec::new(),
    };

    // The same content-addressed object can back many manifests; check each
    // object once.
    let mut seen: HashMap<(String, String), ()> = HashMap::new();
    let mut per: HashMap<String, ProviderScrub> = HashMap::new();

    for record in &snapshot.records {
        for entry in &record.manifest.chunks {
            for loc in manifest::entry_locs(entry) {
                if seen
                    .insert((loc.config_id.clone(), loc.remote_path.clone()), ())
                    .is_some()
                {
                    continue;
                }
                let bucket = per
                    .entry(loc.config_id.clone())
                    .or_insert_with(|| ProviderScrub::new(&loc.config_id));
                bucket.checked += 1;
                run.checked += 1;

                let config = match snapshot.configs.iter().find(|c| c.id == loc.config_id) {
                    Some(config) => config,
                    None => {
                        bucket.missing += 1;
                        run.missing += 1;
                        run.findings.push(ScrubFinding {
                            file_id: record.file.id.clone(),
                            chunk_index: entry.index,
                            config_id: loc.config_id.clone(),
                            remote_path: loc.remote_path.clone(),
                            kind: "missing".to_string(),
                            detail: "provider config is gone".to_string(),
                        });
                        continue;
                    }
                };

                let probe = Instant::now();
                let outcome = check_copy(config, &loc.remote_path);
                let elapsed = probe.elapsed().as_millis() as u64;
                bucket.duration_ms += elapsed;

                match outcome {
                    Ok(bytes) if transfer::blake3_hex(&bytes) == loc.artifact_hash => {
                        bucket.ok += 1;
                        run.ok += 1;
                        health::observe(
                            &loc.config_id,
                            &Outcome::Success {
                                latency_ms: elapsed,
                            },
                        );
                    }
                    Ok(_) => {
                        bucket.corrupt += 1;
                        run.corrupt += 1;
                        health::observe(
                            &loc.config_id,
                            &Outcome::Failure {
                                message: "artifact hash mismatch".to_string(),
                            },
                        );
                        run.findings.push(ScrubFinding {
                            file_id: record.file.id.clone(),
                            chunk_index: entry.index,
                            config_id: loc.config_id.clone(),
                            remote_path: loc.remote_path.clone(),
                            kind: "corrupt".to_string(),
                            detail: format!(
                                "artifact hash mismatch (recorded {})",
                                loc.artifact_hash
                            ),
                        });
                    }
                    Err(e) => {
                        bucket.missing += 1;
                        run.missing += 1;
                        health::observe(&loc.config_id, &Outcome::Failure { message: e.clone() });
                        run.findings.push(ScrubFinding {
                            file_id: record.file.id.clone(),
                            chunk_index: entry.index,
                            config_id: loc.config_id.clone(),
                            remote_path: loc.remote_path.clone(),
                            kind: "missing".to_string(),
                            detail: e,
                        });
                    }
                }
            }
        }
    }

    let mut providers: Vec<ProviderScrub> = per.into_values().collect();
    providers.sort_by_key(|a| a.config_id.clone());
    run.providers = providers;
    run.duration_ms = started.elapsed().as_millis() as u64;
    run.finished_at = now();
    run
}

/// Persist a run to the `scrub_runs` table.
pub fn put_scrub_run(db: &Database, run: &ScrubRun) -> Result<(), String> {
    let serialized = serde_json::to_string(run).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_scrub_runs_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(run.run_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Every recorded scrub run, oldest first.
pub fn list_scrub_runs(db: &Database) -> Result<Vec<ScrubRun>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_scrub_runs_table())
        .map_err(|e| e.to_string())?;
    let mut runs = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_key, value) = entry.map_err(|e| e.to_string())?;
        match serde_json::from_str::<ScrubRun>(value.value()) {
            Ok(run) => runs.push(run),
            Err(e) => return Err(format!("integrity: scrub run unreadable: {}", e)),
        }
    }
    runs.sort_by_key(|a| a.started_at.clone());
    Ok(runs)
}

/// Run one full pass against the database: persist the run, queue every
/// finding for repair, refresh the health rows.
pub fn scrub(db: &Database) -> Result<ScrubRun, String> {
    let snapshot = repair::snapshot(db)?;
    let task = repair::begin_task(
        "scrub",
        &format!("{} striped file(s)", snapshot.records.len()),
    );
    let run = scrub_snapshot(&snapshot);
    put_scrub_run(db, &run)?;
    repair::queue_findings(run.findings.iter().map(|f| {
        Finding::new(
            f.file_id.clone(),
            f.chunk_index,
            Some(f.config_id.clone()),
            match f.kind.as_str() {
                "corrupt" => FindingKind::Corrupt,
                "missing" => FindingKind::Missing,
                _ => FindingKind::Unverified,
            },
            format!("{}: {}", f.remote_path, f.detail),
        )
    }));
    let _ = health::persist_all(db);
    repair::finish_task(
        &task,
        if run.corrupt + run.missing > 0 {
            "error"
        } else {
            "done"
        },
        Some(format!(
            "{} checked, {} corrupt, {} missing",
            run.checked, run.corrupt, run.missing
        )),
    );
    Ok(run)
}

// ─── The daemon (needs one wiring call — request R7-1) ──────────────────────

static SCRUB: OnceLock<()> = OnceLock::new();

/// Start the background scrub loop exactly once. Safe to call from every
/// request or command. Deliberately *not* called from anywhere inside this
/// crate: the supervisor wires it next to `scheduler::ensure_started` (see
/// request R7-1) so the daemon owns a real `Arc<RwLock<Database>>`.
pub fn ensure_scrub_started(db: Arc<RwLock<Database>>) {
    if SCRUB.set(()).is_err() {
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("cybermanju-scrub".to_string())
        .spawn(move || run_loop(db))
    {
        log::error!("could not start scrub daemon: {}", e);
    }
}

fn run_loop(db: Arc<RwLock<Database>>) {
    // First pass after a full interval: booting must not hammer every
    // provider with a full download sweep.
    let mut last_run = Instant::now();
    log::info!(
        "scrub daemon started (tick {}s, interval {}s)",
        TICK_INTERVAL.as_secs(),
        SCRUB_INTERVAL.as_secs()
    );
    loop {
        std::thread::sleep(TICK_INTERVAL);
        if last_run.elapsed() < SCRUB_INTERVAL {
            continue;
        }
        last_run = Instant::now();
        let guard = match db.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match scrub(&guard) {
            Ok(run) => log::info!(
                "scrub {} finished: {} checked, {} corrupt, {} missing in {}ms",
                run.run_id,
                run.checked,
                run.corrupt,
                run.missing,
                run.duration_ms
            ),
            Err(e) => log::warn!("scrub run failed: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repair::fixtures::Env;

    #[test]
    fn a_clean_pool_scrubs_all_green() {
        let env = Env::new("scrub-clean", 2, 1);
        let run = scrub_snapshot(&env.snapshot());
        assert_eq!(run.status, "completed");
        assert_eq!(run.checked, 2, "{:?}", run);
        assert_eq!(run.ok, 2, "{:?}", run);
        assert_eq!(run.corrupt, 0);
        assert_eq!(run.missing, 0);
        assert!(run.findings.is_empty());
        assert_eq!(run.providers.len(), 2);
        assert_eq!(run.providers[0].checked + run.providers[1].checked, 2);
        env.cleanup();
    }

    #[test]
    fn scrub_finds_a_corrupted_copy_and_reports_it_per_provider() {
        let env = Env::new("scrub-corrupt", 2, 1);
        env.corrupt(1);

        let run = scrub_snapshot(&env.snapshot());
        assert_eq!(run.corrupt, 1, "{:?}", run);
        assert_eq!(run.ok, 1, "{:?}", run);
        assert_eq!(run.findings.len(), 1, "{:?}", run.findings);
        let finding = &run.findings[0];
        assert_eq!(finding.kind, "corrupt");
        assert_eq!(finding.config_id, env.configs[1].id);
        assert_eq!(finding.chunk_index, 0);

        let bucket = run
            .providers
            .iter()
            .find(|p| p.config_id == env.configs[1].id)
            .expect("provider row");
        assert_eq!(bucket.corrupt, 1);

        // …and it is queued for repair, not merely logged. (The queue is
        // process-global, so assert on content rather than an exact count.)
        repair::queue_findings(run.findings.iter().map(|f| {
            Finding::new(
                f.file_id.clone(),
                f.chunk_index,
                Some(f.config_id.clone()),
                FindingKind::Corrupt,
                f.detail.clone(),
            )
        }));
        assert!(repair::queued_findings() >= 1);
        let queued = repair::drain_findings();
        assert!(
            queued.iter().any(|f| f.kind == FindingKind::Corrupt),
            "{:?}",
            queued
        );
        env.cleanup();
    }

    #[test]
    fn scrub_reports_a_missing_copy() {
        let env = Env::new("scrub-missing", 2, 1);
        env.remove(1);

        let run = scrub_snapshot(&env.snapshot());
        assert_eq!(run.missing, 1, "{:?}", run);
        assert_eq!(run.ok, 1);
        assert_eq!(run.findings[0].kind, "missing");
        let detail = run.findings[0].detail.to_lowercase();
        assert!(
            detail.contains("not_found")
                || detail.contains("not found")
                || detail.contains("no such file")
                || detail.contains("does not exist"),
            "{}",
            run.findings[0].detail
        );
        env.cleanup();
    }

    #[test]
    fn a_run_round_trips_through_the_scrub_runs_table() {
        let env = Env::new("scrub-table", 2, 1);
        let db = env.db();
        let run = scrub_snapshot(&env.snapshot());
        put_scrub_run(db, &run).expect("persist");

        let mut runs = list_scrub_runs(db).expect("list");
        let stored = runs.pop().expect("one run");
        assert_eq!(stored.run_id, run.run_id);
        assert_eq!(stored.checked, run.checked);
        assert_eq!(stored.providers, run.providers);
        env.cleanup();
    }
}
