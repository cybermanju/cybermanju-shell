// CyberManju OS — repair, provider relocation & catalog (AGENT-7 items 2 + 4)
//
// Scrub reports findings; this module fixes them:
//
//   * **replicated chunk** → rebuild the damaged copy from a verifiable one
//     (same content-addressed path, possibly on a different provider);
//   * **erasure-coded chunk** → if every copy is gone but `k` shards survive,
//     decode, re-encode and re-upload the missing shards;
//   * **neither** → the chunk is `unrecoverable:` and the row in `repairs`
//     says so — surfaced, never dropped;
//   * **provider loss** (disabled / 401 / quota) → every locator on it is
//     re-placed onto a healthy provider, chunk by chunk.
//
// Two execution shapes share one core:
//
//   * `repair(db, …)` when the caller *does* hold the database (daemon, tests);
//   * `repair_snapshot(…)` when it does not (a REST worker only ever gets
//     `&Database` for the life of the request) — the worker pushes its
//     results onto the write-behind queue and `flush_pending` lands them on
//     the next request that holds the lock.
//
// The catalog side (item 4 / MISSING C4) lives here too: `publish_catalog`
// writes the manifest set to ≥2 providers, and `rebuild_from_remote` restores
// the whole catalog after the local redb file is gone.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use cybermanju_db::Database;
use cybermanju_types::sync::{SyncConfig, SyncFile};
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

use crate::backends::create_backend;
use crate::health::{self, HealthStatus, Outcome};
use crate::manifest::{self, ChunkEntry, ChunkLoc, ChunkManifest};
use crate::transfer;

// ─── Findings ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FindingKind {
    /// The artifact could not be downloaded from its provider.
    Missing,
    /// The artifact downloaded but its BLAKE3 does not match.
    Corrupt,
    /// The provider itself is gone (disabled / 401 / quota).
    ProviderLost,
    /// Something else asked for this chunk to be looked at.
    Unverified,
}

/// One thing that is wrong with one chunk, from scrub, a provider outage or
/// an operator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub file_id: String,
    pub chunk_index: u32,
    /// The implicated provider; `None` means "the chunk as a whole".
    pub config_id: Option<String>,
    pub kind: FindingKind,
    pub detail: String,
}

impl Finding {
    pub fn new(
        file_id: impl Into<String>,
        chunk_index: u32,
        config_id: Option<String>,
        kind: FindingKind,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            file_id: file_id.into(),
            chunk_index,
            config_id,
            kind,
            detail: detail.into(),
        }
    }
}

static FINDINGS: OnceLock<Mutex<Vec<Finding>>> = OnceLock::new();

/// Upper bound for queued scrub findings (P0-23): findings are regenerable
/// (scrub re-runs), so overflow drops the oldest instead of growing without
/// bound while an idle server never drains them.
const MAX_FINDINGS: usize = 2000;

fn finding_queue() -> &'static Mutex<Vec<Finding>> {
    FINDINGS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Queue a finding for the next repair pass (scrub calls this).
pub fn queue_finding(finding: Finding) {
    if let Ok(mut queue) = finding_queue().lock() {
        queue.push(finding);
        while queue.len() > MAX_FINDINGS {
            queue.remove(0);
        }
    }
}

/// Queue many findings at once.
pub fn queue_findings(findings: impl IntoIterator<Item = Finding>) {
    if let Ok(mut queue) = finding_queue().lock() {
        queue.extend(findings);
        while queue.len() > MAX_FINDINGS {
            queue.remove(0);
        }
    }
}

/// Take everything queued so far.
pub fn drain_findings() -> Vec<Finding> {
    finding_queue()
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default()
}

/// How many findings are waiting for a repair pass.
pub fn queued_findings() -> usize {
    finding_queue().lock().map(|q| q.len()).unwrap_or(0)
}

// ─── Task handles (item 2: observable by AGENT-8's ps/top) ─────────────────

/// A repair/scrub/GC run as `ps`/`top` sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub detail: Option<String>,
    /// `0.0..=1.0`.
    pub progress: f64,
    pub started_at: String,
    pub finished_at: Option<String>,
}

static TASKS: OnceLock<Mutex<Vec<TaskInfo>>> = OnceLock::new();
static NEXT_TASK: AtomicU64 = AtomicU64::new(1);

fn task_list() -> &'static Mutex<Vec<TaskInfo>> {
    TASKS.get_or_init(|| Mutex::new(Vec::new()))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Register a running task and return its id.
pub fn begin_task(kind: &str, detail: &str) -> String {
    let id = format!(
        "{}-{}-{}",
        kind,
        std::process::id(),
        NEXT_TASK.fetch_add(1, Ordering::Relaxed)
    );
    let task = TaskInfo {
        id: id.clone(),
        kind: kind.to_string(),
        state: "running".to_string(),
        detail: Some(detail.to_string()),
        progress: 0.0,
        started_at: now(),
        finished_at: None,
    };
    if let Ok(mut tasks) = task_list().lock() {
        tasks.retain(|t| t.state == "running");
        tasks.push(task);
    }
    id
}

/// Update a task's progress (`0.0..=1.0`).
pub fn progress_task(id: &str, progress: f64, detail: Option<String>) {
    if let Ok(mut tasks) = task_list().lock() {
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.progress = progress.clamp(0.0, 1.0);
            if detail.is_some() {
                task.detail = detail;
            }
        }
    }
}

/// Close a task with a terminal state (`done` / `error`).
pub fn finish_task(id: &str, state: &str, detail: Option<String>) {
    if let Ok(mut tasks) = task_list().lock() {
        if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
            task.state = state.to_string();
            task.progress = 1.0;
            task.finished_at = Some(now());
            if detail.is_some() {
                task.detail = detail;
            }
        }
    }
}

/// Every task currently known, finished ones included (capped). This is the
/// hook AGENT-8's `ps`/`top` reads — see request R7-3.
pub fn tasks() -> Vec<TaskInfo> {
    task_list()
        .lock()
        .map(|tasks| tasks.clone())
        .unwrap_or_default()
}

// ─── Write-behind queue ─────────────────────────────────────────────────────

/// Work a background worker could not write itself (it does not hold the
/// database). Flushed by whoever does.
#[derive(Debug, Clone)]
pub enum PendingOp {
    /// A manifest whose locators were repaired.
    Manifest {
        file: SyncFile,
        manifest_json: String,
    },
    /// A finished repair row.
    Repair(RepairRow),
    /// A finished scrub run.
    Scrub(crate::scrub::ScrubRun),
    /// Health rows to persist.
    Health(Vec<health::ProviderHealth>),
    /// Records recovered from the providers by a background rebuild.
    Rebuild(Vec<SyncFile>),
}

static PENDING: OnceLock<Mutex<Vec<PendingOp>>> = OnceLock::new();

/// Upper bound for parked write-behind ops (P0-23). Overflow refuses with
/// `false` (backpressure — the caller surfaces `too_large:`) instead of
/// holding an idle server's repairs in RAM forever or silently dropping
/// durability results. Flushes drain per request, so sustained overflow
/// means the server is wedged, not busy.
const MAX_PENDING_OPS: usize = 10_000;

fn pending_ops() -> &'static Mutex<Vec<PendingOp>> {
    PENDING.get_or_init(|| Mutex::new(Vec::new()))
}

/// Park an op for the next holder of the database lock. Returns `false`
/// when the queue is full (caller must surface the refusal, not drop it).
pub fn queue_pending(op: PendingOp) -> bool {
    if let Ok(mut queue) = pending_ops().lock() {
        if queue.len() >= MAX_PENDING_OPS {
            return false;
        }
        queue.push(op);
        return true;
    }
    false
}

/// How many ops are waiting to be written.
pub fn pending_len() -> usize {
    pending_ops().lock().map(|q| q.len()).unwrap_or(0)
}

/// Write every parked op. Returns how many landed.
///
/// Call this at the top of any request that holds the database write lock
/// (every POST does), so results produced by background workers become
/// durable before they are read back out. Ops that fail to write (a lock
/// conflict, a full disk) are parked again rather than dropped.
pub fn flush_pending(db: &Database) -> Result<usize, String> {
    let ops = match pending_ops().lock() {
        Ok(mut queue) => std::mem::take(&mut *queue),
        Err(_) => return Ok(0),
    };
    let mut landed = 0usize;
    for (i, op) in ops.iter().enumerate() {
        match write_op(db, op) {
            Ok(()) => landed += 1,
            Err(e) => {
                // Everything from here on (including this one) goes back on
                // the queue — a background worker's result must not vanish.
                if let Ok(mut queue) = pending_ops().lock() {
                    queue.extend(ops[i..].iter().cloned());
                }
                return Err(format!(
                    "pending durability write failed after {} ops: {}",
                    landed, e
                ));
            }
        }
    }
    Ok(landed)
}

fn write_op(db: &Database, op: &PendingOp) -> Result<(), String> {
    match op {
        PendingOp::Manifest {
            file,
            manifest_json,
        } => {
            let mut file = file.clone();
            file.manifest_ref = Some(manifest_json.clone());
            db.upsert_sync_file(&file).map_err(|e| e.to_string())
        }
        PendingOp::Repair(row) => put_repairs_row(db, row),
        PendingOp::Scrub(run) => crate::scrub::put_scrub_run(db, run),
        PendingOp::Health(rows) => {
            for row in rows {
                health::persist(db, row)?;
            }
            Ok(())
        }
        PendingOp::Rebuild(records) => {
            write_rebuild_records(db, records)?;
            Ok(())
        }
    }
}

// ─── Snapshot ───────────────────────────────────────────────────────────────

/// One file plus its parsed manifest.
#[derive(Debug, Clone)]
pub struct Record {
    pub file: SyncFile,
    pub manifest: ChunkManifest,
}

/// Everything a background worker needs, copied out under the lock: the
/// striped records and the provider configs.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub records: Vec<Record>,
    pub configs: Vec<SyncConfig>,
}

impl Snapshot {
    pub fn record(&self, file_id: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.file.id == file_id)
    }

    /// Every `(config, path)` locator pair referenced by any manifest — the
    /// GC's and eviction's referenced set (per provider, because two files
    /// can share one content-addressed object on different providers).
    pub fn referenced_pairs(&self) -> BTreeSet<(String, String)> {
        let mut out = BTreeSet::new();
        for record in &self.records {
            for entry in &record.manifest.chunks {
                for loc in manifest::entry_locs(entry) {
                    out.insert((loc.config_id.clone(), loc.remote_path.clone()));
                }
            }
        }
        out
    }

    /// Same, but grouped by provider: `config_id → set of paths`.
    pub fn referenced_by_provider(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (config_id, path) in self.referenced_pairs() {
            out.entry(config_id).or_default().insert(path);
        }
        out
    }
}

/// Copy the striped records and configs out of the database.
pub fn snapshot(db: &Database) -> Result<Snapshot, String> {
    let configs = db.list_sync_configs().map_err(|e| e.to_string())?;
    let rows = db.list_sync_files(None).map_err(|e| e.to_string())?;
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    for file in rows {
        let json = match file.manifest_ref.clone() {
            Some(json) => json,
            None => continue,
        };
        if !seen.insert(file.id.clone()) {
            // Several rows can exist (one per config); the manifest lives on
            // the striping config's row.
            continue;
        }
        match serde_json::from_str::<ChunkManifest>(&json) {
            Ok(manifest) => records.push(Record { file, manifest }),
            Err(e) => {
                return Err(format!(
                    "integrity: manifest for '{}' is unreadable: {}",
                    file.id, e
                ))
            }
        }
    }
    Ok(Snapshot { records, configs })
}

// ─── Repair rows ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepairRow {
    pub repair_id: String,
    pub file_id: String,
    pub chunk_index: u32,
    /// `copy` (replicated), `erasure` (RS), `provider` (relocation).
    pub kind: String,
    pub status: String,
    pub method: String,
    pub sources: Vec<String>,
    pub targets: Vec<String>,
    pub bytes: u64,
    pub error: Option<String>,
    pub finished_at: String,
}

/// What one repair pass produced.
#[derive(Debug, Clone, Default)]
pub struct RepairOutcome {
    pub rows: Vec<RepairRow>,
    /// Records whose manifest changed and must be written back.
    pub updates: Vec<(SyncFile, String)>,
}

impl RepairOutcome {
    pub fn repaired(&self) -> usize {
        self.rows.iter().filter(|r| r.status == "repaired").count()
    }

    pub fn unrecoverable(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.status == "unrecoverable")
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.rows.iter().filter(|r| r.status == "skipped").count()
    }
}

fn repair_id(file_id: &str, chunk_index: u32) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!(
        "rep-{}-{}-{}",
        file_id,
        chunk_index,
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn put_repairs_row(db: &Database, row: &RepairRow) -> Result<(), String> {
    let serialized = serde_json::to_string(row).map_err(|e| e.to_string())?;
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_repairs_table())
            .map_err(|e| e.to_string())?;
        table
            .insert(row.repair_id.as_str(), serialized.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Every recorded repair, newest id last.
pub fn list_repairs(db: &Database) -> Result<Vec<RepairRow>, String> {
    list_repairs_paged(db, 0)
}

/// Newest unbounded growth guard (P0-23): `limit` caps collected rows
/// (`0` = uncapped, legacy callers only). Iteration stops early — a 100k-row
/// table no longer clones itself into every status poll.
pub fn list_repairs_paged(db: &Database, limit: usize) -> Result<Vec<RepairRow>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_repairs_table())
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (_key, value) = entry.map_err(|e| e.to_string())?;
        match serde_json::from_str::<RepairRow>(value.value()) {
            Ok(row) => rows.push(row),
            Err(e) => return Err(format!("integrity: repair row unreadable: {}", e)),
        }
        if limit > 0 && rows.len() >= limit {
            break;
        }
    }
    Ok(rows)
}

// ─── Bytes helpers ──────────────────────────────────────────────────────────

fn temp_path(tag: &str) -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir().join(format!(
        "cybermanju-repair-{}-{}-{}.part",
        tag,
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn download(config: &SyncConfig, remote_path: &str, tag: &str) -> Result<Vec<u8>, String> {
    let backend = create_backend(config)?;
    let tmp = temp_path(tag);
    let tmp_str = tmp.to_string_lossy().to_string();
    let attempt = backend
        .download_file(remote_path, &tmp_str)
        .and_then(|()| std::fs::read(&tmp).map_err(|e| format!("read failed: {}", e)));
    let _ = std::fs::remove_file(&tmp);
    attempt
}

pub(crate) fn upload(
    config: &SyncConfig,
    remote_path: &str,
    bytes: &[u8],
    tag: &str,
) -> Result<(), String> {
    let backend = create_backend(config)?;
    let tmp = temp_path(tag);
    std::fs::write(&tmp, bytes).map_err(|e| format!("write failed: {}", e))?;
    let tmp_str = tmp.to_string_lossy().to_string();
    let attempt = backend.upload_file(&tmp_str, remote_path);
    let _ = std::fs::remove_file(&tmp);
    attempt.map(|_| ())
}

/// Download a locator and verify it against its recorded artifact hash.
pub(crate) fn fetch_verified(config: &SyncConfig, loc: &ChunkLoc) -> Result<Vec<u8>, String> {
    let bytes = download(config, &loc.remote_path, "src")?;
    if transfer::blake3_hex(&bytes) != loc.artifact_hash {
        return Err(format!(
            "integrity: copy on '{}' failed its artifact hash check",
            config.id
        ));
    }
    Ok(bytes)
}

/// Pick the provider a rebuilt copy should land on: the original one when it
/// is still eligible, otherwise the best eligible provider that does not
/// already hold another copy of this chunk.
fn place_on(
    configs: &[SyncConfig],
    preferred: &str,
    occupied: &[String],
) -> Result<SyncConfig, String> {
    let usable = |c: &SyncConfig| {
        c.enabled
            && health::get(&c.id).status != HealthStatus::Quarantined
            && !occupied.iter().any(|id| id == &c.id)
    };
    for config in configs {
        if config.id == preferred && usable(config) {
            return Ok(config.clone());
        }
    }
    for config in health::eligible(configs) {
        if !occupied.iter().any(|id| id == &config.id) {
            return Ok(config);
        }
    }
    Err(format!(
        "no eligible provider left to place a copy (occupied: {})",
        if occupied.is_empty() {
            "none".to_string()
        } else {
            occupied.join(", ")
        }
    ))
}

// ─── The core ───────────────────────────────────────────────────────────────

/// The result of rebuilding one chunk.
#[derive(Debug, Clone, Default)]
pub struct ChunkRepair {
    pub locs: Vec<ChunkLoc>,
    pub sources: Vec<String>,
    pub written: Vec<String>,
    pub bytes: u64,
    /// `replica` | `erasure`.
    pub method: &'static str,
    /// `repaired` | `skipped` | `unrecoverable`.
    pub status: &'static str,
    pub error: Option<String>,
}

/// Rebuild the locators listed in `targets` for one chunk.
///
/// * replicated: fetch one verifiable copy, re-upload it to each target;
/// * erasure: fetch `k` verifiable shards, decode, re-encode, re-upload only
///   the missing/failed shards;
/// * no verifiable source at all → `unrecoverable`.
///
/// A chunk that keeps at least one healthy copy but has nowhere eligible to
/// put the rebuilt one is `skipped` (not unrecoverable) — it still reads.
pub fn repair_chunk(
    configs: &[SyncConfig],
    entry: &ChunkEntry,
    parity: u8,
    targets: &[String],
) -> ChunkRepair {
    let locs: Vec<ChunkLoc> = manifest::entry_locs(entry).into_iter().cloned().collect();
    let all_targets = targets.is_empty();
    let is_target = |loc: &ChunkLoc| all_targets || targets.iter().any(|t| t == &loc.config_id);

    let occupied: Vec<String> = locs
        .iter()
        .filter(|loc| !is_target(loc))
        .map(|loc| loc.config_id.clone())
        .collect();

    let config_of = |id: &str| configs.iter().find(|c| c.id == id).cloned();

    // Sources first: healthy locs that are *not* being rebuilt, then the
    // targets themselves (a degraded provider may still serve reads).
    let order: Vec<usize> = (0..locs.len())
        .filter(|i| !is_target(&locs[*i]))
        .chain((0..locs.len()).filter(|i| is_target(&locs[*i])))
        .collect();

    let mut sources: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    let artifact: Option<Vec<u8>> = match manifest::redundancy_for(locs.len(), parity) {
        manifest::Redundancy::Replica { .. } => {
            let mut found = None;
            for i in &order {
                let loc = &locs[*i];
                let config = match config_of(&loc.config_id) {
                    Some(config) => config,
                    None => {
                        errors.push(format!("config '{}' is gone", loc.config_id));
                        continue;
                    }
                };
                match fetch_verified(&config, loc) {
                    Ok(bytes) => {
                        health::observe(&loc.config_id, &Outcome::Success { latency_ms: 0 });
                        sources.push(loc.config_id.clone());
                        found = Some(bytes);
                        break;
                    }
                    Err(e) => {
                        health::observe(&loc.config_id, &Outcome::Failure { message: e.clone() });
                        errors.push(format!("{}: {}", loc.config_id, e));
                    }
                }
            }
            found
        }
        manifest::Redundancy::Erasure { data, parity: m } => {
            let need = data as usize;
            let mut present: Vec<(u8, Vec<u8>)> = Vec::with_capacity(need);
            for i in &order {
                if present.len() >= need {
                    break;
                }
                let loc = &locs[*i];
                let config = match config_of(&loc.config_id) {
                    Some(config) => config,
                    None => {
                        errors.push(format!("config '{}' is gone", loc.config_id));
                        continue;
                    }
                };
                match fetch_verified(&config, loc) {
                    Ok(bytes) => {
                        health::observe(&loc.config_id, &Outcome::Success { latency_ms: 0 });
                        sources.push(loc.config_id.clone());
                        present.push((*i as u8, bytes));
                    }
                    Err(e) => {
                        health::observe(&loc.config_id, &Outcome::Failure { message: e.clone() });
                        errors.push(format!("{}: {}", loc.config_id, e));
                    }
                }
            }
            if present.len() < need {
                return ChunkRepair {
                    locs,
                    sources,
                    method: "erasure",
                    status: "unrecoverable",
                    error: Some(format!(
                        "unrecoverable: chunk {} needs {} erasure shards, {} usable ({})",
                        entry.index,
                        need,
                        present.len(),
                        errors.join("; ")
                    )),
                    ..ChunkRepair::default()
                };
            }
            present.sort_by_key(|(i, _)| *i);
            match manifest::decode_shards(data, m, &present) {
                Ok(artifact) => Some(artifact),
                Err(e) => {
                    return ChunkRepair {
                        locs,
                        sources,
                        method: "erasure",
                        status: "unrecoverable",
                        error: Some(format!(
                            "unrecoverable: chunk {} could not be decoded: {}",
                            entry.index, e
                        )),
                        ..ChunkRepair::default()
                    }
                }
            }
        }
    };

    let artifact = match artifact {
        Some(artifact) => artifact,
        None => {
            return ChunkRepair {
                locs,
                sources,
                method: "replica",
                status: "unrecoverable",
                error: Some(format!(
                    "unrecoverable: chunk {} has no verifiable copy ({})",
                    entry.index,
                    errors.join("; ")
                )),
                ..ChunkRepair::default()
            }
        }
    };

    // Rebuild every target (all of them when `targets` was empty).
    let mut out = ChunkRepair {
        method: match manifest::redundancy_for(locs.len(), parity) {
            manifest::Redundancy::Erasure { .. } => "erasure",
            manifest::Redundancy::Replica { .. } => "replica",
        },
        status: "repaired",
        ..ChunkRepair::default()
    };
    let mut new_locs = locs.clone();
    let mut any_written = false;

    for i in 0..locs.len() {
        if !is_target(&locs[i]) {
            continue;
        }
        let original = &locs[i];
        let planned = match place_on(configs, &original.config_id, &occupied) {
            Ok(config) => config,
            Err(e) => {
                out.status = if any_written { "repaired" } else { "skipped" };
                out.error = Some(format!(
                    "unsupported: chunk {} copy on '{}' could not be re-placed: {}",
                    entry.index, original.config_id, e
                ));
                out.locs = new_locs;
                out.bytes = artifact.len() as u64 * out.written.len() as u64;
                return out;
            }
        };

        // Erasure chunks are sharded at content-addressed shard paths; a
        // replicated chunk keeps its original chunk path.
        let (remote_path, payload) = match manifest::redundancy_for(locs.len(), parity) {
            manifest::Redundancy::Erasure { data, parity: m } => {
                let shard_index = i as u8;
                let shards = match manifest::encode_shards(&artifact, data, m) {
                    Ok(shards) => shards,
                    Err(e) => {
                        out.status = "unrecoverable";
                        out.error = Some(format!(
                            "unrecoverable: chunk {} could not be re-encoded: {}",
                            entry.index, e
                        ));
                        out.locs = new_locs;
                        return out;
                    }
                };
                let shard = shards
                    .get(shard_index as usize)
                    .cloned()
                    .ok_or_else(|| format!("shard {} out of range", shard_index));
                match shard {
                    Ok(shard) => (manifest::shard_remote_path(&entry.hash, shard_index), shard),
                    Err(e) => {
                        out.status = "unrecoverable";
                        out.error = Some(e);
                        out.locs = new_locs;
                        return out;
                    }
                }
            }
            manifest::Redundancy::Replica { .. } => {
                (original.remote_path.clone(), artifact.clone())
            }
        };

        match upload(&planned, &remote_path, &payload, "dst") {
            Ok(()) => {
                health::observe(&planned.id, &Outcome::Success { latency_ms: 0 });
                new_locs[i] = ChunkLoc {
                    config_id: planned.id.clone(),
                    remote_path,
                    artifact_hash: transfer::blake3_hex(&payload),
                };
                out.written.push(planned.id.clone());
                any_written = true;
            }
            Err(e) => {
                health::observe(&planned.id, &Outcome::Failure { message: e.clone() });
                errors.push(format!("upload to '{}' failed: {}", planned.id, e));
            }
        }
    }

    out.locs = new_locs;
    out.bytes = out.written.len() as u64 * artifact.len() as u64;
    if !any_written {
        out.status = "skipped";
        out.error = Some(format!(
            "unsupported: nothing could be written for chunk {} ({})",
            entry.index,
            errors.join("; ")
        ));
    } else if out.written.len() < out.locs.iter().filter(|l| is_target(l)).count() {
        out.error = Some(format!(
            "partially repaired: {} of {} copies written ({})",
            out.written.len(),
            out.locs.iter().filter(|l| is_target(l)).count(),
            errors.join("; ")
        ));
    }
    out
}

/// Run a whole pass over a snapshot: one group of findings per chunk.
pub fn repair_snapshot(snapshot: &Snapshot, findings: &[Finding]) -> RepairOutcome {
    // (file, chunk) → target config ids + the most serious kind seen.
    let mut groups: BTreeMap<(String, u32), (BTreeSet<String>, FindingKind, String)> =
        BTreeMap::new();
    for finding in findings {
        let key = (finding.file_id.clone(), finding.chunk_index);
        let slot = groups
            .entry(key)
            .or_insert_with(|| (BTreeSet::new(), finding.kind, finding.detail.clone()));
        if let Some(config_id) = &finding.config_id {
            slot.0.insert(config_id.clone());
        }
        // Provider loss is the most actionable kind: keep it as the label.
        if finding.kind == FindingKind::ProviderLost {
            slot.1 = FindingKind::ProviderLost;
            slot.2 = finding.detail.clone();
        }
    }

    let mut outcome = RepairOutcome::default();
    for ((file_id, chunk_index), (targets, kind, detail)) in groups {
        let row_kind = match kind {
            FindingKind::ProviderLost => "provider",
            _ => "copy",
        };
        let recorded_targets: Vec<String> = targets.iter().cloned().collect();

        let record = match snapshot.record(&file_id) {
            Some(record) => record,
            None => {
                let error = format!("integrity: no manifest for file '{}' ({})", file_id, detail);
                outcome.rows.push(RepairRow {
                    repair_id: repair_id(&file_id, chunk_index),
                    file_id,
                    chunk_index,
                    kind: row_kind.to_string(),
                    status: "skipped".to_string(),
                    method: "none".to_string(),
                    sources: vec![],
                    targets: recorded_targets,
                    bytes: 0,
                    error: Some(error),
                    finished_at: now(),
                });
                continue;
            }
        };
        let entry = match record.manifest.chunks.get(chunk_index as usize) {
            Some(entry) => entry,
            None => {
                outcome.rows.push(RepairRow {
                    repair_id: repair_id(&file_id, chunk_index),
                    file_id,
                    chunk_index,
                    kind: row_kind.to_string(),
                    status: "skipped".to_string(),
                    method: "none".to_string(),
                    sources: vec![],
                    targets: recorded_targets,
                    bytes: 0,
                    error: Some(format!(
                        "integrity: manifest has no chunk {} ({})",
                        chunk_index, detail
                    )),
                    finished_at: now(),
                });
                continue;
            }
        };

        let parity = record.manifest.parity;
        let repair = repair_chunk(&snapshot.configs, entry, parity, &recorded_targets);
        let changed = repair.locs
            != manifest::entry_locs(entry)
                .into_iter()
                .cloned()
                .collect::<Vec<ChunkLoc>>();

        if changed {
            let mut manifest = record.manifest.clone();
            let slot = &mut manifest.chunks[chunk_index as usize];
            slot.primary = repair.locs[0].clone();
            slot.replicas = repair.locs[1..].to_vec();
            match serde_json::to_string(&manifest) {
                Ok(json) => outcome.updates.push((record.file.clone(), json)),
                Err(e) => outcome.rows.push(RepairRow {
                    repair_id: repair_id(&file_id, chunk_index),
                    file_id: file_id.clone(),
                    chunk_index,
                    kind: row_kind.to_string(),
                    status: "skipped".to_string(),
                    method: "none".to_string(),
                    sources: vec![],
                    targets: recorded_targets.clone(),
                    bytes: 0,
                    error: Some(format!("integrity: manifest re-encode failed: {}", e)),
                    finished_at: now(),
                }),
            }
        }

        let method = repair.method.to_string();
        let status = repair.status.to_string();
        let error = repair.error.clone();
        outcome.rows.push(RepairRow {
            repair_id: repair_id(&file_id, chunk_index),
            file_id,
            chunk_index,
            kind: row_kind.to_string(),
            status,
            method,
            sources: repair.sources,
            targets: recorded_targets,
            bytes: repair.bytes,
            error,
            finished_at: now(),
        });
    }
    outcome
}

/// Repair from the database: drain the queue, add `findings`, write results.
pub fn repair(db: &Database, findings: Vec<Finding>) -> Result<RepairOutcome, String> {
    let mut all = drain_findings();
    all.extend(findings);
    let snapshot = snapshot(db)?;
    let task = begin_task("repair", &format!("{} finding(s)", all.len()));
    let outcome = repair_snapshot(&snapshot, &all);
    let (repaired, unrecoverable) = (outcome.repaired(), outcome.unrecoverable());
    apply_outcome(db, &outcome)?;
    finish_task(
        &task,
        if unrecoverable > 0 { "error" } else { "done" },
        Some(format!(
            "{} repaired, {} unrecoverable",
            repaired, unrecoverable
        )),
    );
    Ok(outcome)
}

/// Write a repair outcome back to the database (manifests + `repairs` rows).
pub fn apply_outcome(db: &Database, outcome: &RepairOutcome) -> Result<(), String> {
    for (file, json) in &outcome.updates {
        let mut file = file.clone();
        file.manifest_ref = Some(json.clone());
        db.upsert_sync_file(&file).map_err(|e| e.to_string())?;
    }
    for row in &outcome.rows {
        put_repairs_row(db, row)?;
    }
    Ok(())
}

// ─── Provider loss (item 2) ─────────────────────────────────────────────────

/// Re-place every locator that lived on `dead_config_id`.
pub fn relocate_records(snapshot: &Snapshot, dead_config_id: &str) -> RepairOutcome {
    let findings: Vec<Finding> = snapshot
        .records
        .iter()
        .flat_map(|record| {
            record
                .manifest
                .chunks
                .iter()
                .filter(|entry| {
                    manifest::entry_locs(entry)
                        .iter()
                        .any(|loc| loc.config_id == dead_config_id)
                })
                .map(|entry| {
                    Finding::new(
                        record.file.id.clone(),
                        entry.index,
                        Some(dead_config_id.to_string()),
                        FindingKind::ProviderLost,
                        format!("provider '{}' is gone", dead_config_id),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    repair_snapshot(snapshot, &findings)
}

/// Provider-loss relocation plus a write-back.
pub fn relocate_provider(db: &Database, dead_config_id: &str) -> Result<RepairOutcome, String> {
    let snapshot = snapshot(db)?;
    let task = begin_task("repair", &format!("relocate provider '{}'", dead_config_id));
    let outcome = relocate_records(&snapshot, dead_config_id);
    let (repaired, unrecoverable) = (outcome.repaired(), outcome.unrecoverable());
    apply_outcome(db, &outcome)?;
    finish_task(
        &task,
        if unrecoverable > 0 { "error" } else { "done" },
        Some(format!(
            "{} repaired, {} unrecoverable",
            repaired, unrecoverable
        )),
    );
    Ok(outcome)
}

// ─── Reed–Solomon restripe (item 3's production path) ───────────────────────

/// What a restripe did to one file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestripeReport {
    pub file_id: String,
    pub parity: u8,
    pub data: u8,
    pub chunks: usize,
    pub shards_written: usize,
    pub bytes: u64,
    pub providers: Vec<String>,
}

/// Convert a replicated file to Reed–Solomon with the given parity.
///
/// The upload pipeline keeps writing whole copies (its code is another
/// agent's), so this is the only production path that creates erasure-coded
/// chunks — see request R7-2 for hooking it up.
pub fn restripe_records(
    snapshot: &Snapshot,
    file_id: &str,
    parity: u8,
) -> Result<(RestripeReport, Option<(SyncFile, String)>), String> {
    let record = snapshot
        .record(file_id)
        .ok_or_else(|| format!("integrity: no manifest for file '{}'", file_id))?;
    if parity == 0 {
        return Err("unsupported: parity 0 cannot be erasure coded".to_string());
    }

    // RS needs at least `parity + 2` distinct providers (k = n - m ≥ 2 keeps
    // the layout stable under the derivation rule in `manifest`).
    let eligible = health::eligible(&snapshot.configs);
    let want = parity as usize + 2;
    if eligible.len() < want {
        return Err(format!(
            "unsupported: Reed–Solomon with parity {} needs {} eligible providers, {} are",
            parity,
            want,
            eligible.len()
        ));
    }

    let mut manifest = record.manifest.clone();
    // Changing parity under an entry that is already erasure coded would
    // make every reader derive the wrong `k`/`m` for it — refuse rather than
    // silently mis-decode later.
    let old_parity = record.manifest.parity;
    if old_parity != parity
        && manifest
            .chunks
            .iter()
            .any(|e| manifest::redundancy_for(1 + e.replicas.len(), old_parity).is_erasure())
    {
        return Err(format!(
            "unsupported: manifest is already erasure coded at parity {}; \
             changing to {} needs a re-encode pass",
            old_parity, parity
        ));
    }
    let mut providers: Vec<String> = Vec::new();
    let mut shards_written = 0usize;
    let mut bytes = 0u64;
    let mut data_shards = 0u8;

    for entry in &mut manifest.chunks {
        let total = 1 + entry.replicas.len();
        if manifest::redundancy_for(total, old_parity).is_erasure() {
            // Already erasure coded (same parity — checked above).
            continue;
        }

        // Any surviving copy is a valid source for the artifact.
        let mut artifact: Option<Vec<u8>> = None;
        let mut last_err = String::new();
        for loc in manifest::entry_locs(entry) {
            let config = match snapshot.configs.iter().find(|c| c.id == loc.config_id) {
                Some(config) => config,
                None => continue,
            };
            match fetch_verified(config, loc) {
                Ok(bytes_) => {
                    artifact = Some(bytes_);
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        let artifact = artifact.ok_or_else(|| {
            format!(
                "unrecoverable: chunk {} has no verifiable copy to restripe ({})",
                entry.index, last_err
            )
        })?;

        let n = eligible.len().min(total.max(want));
        let data = (n - parity as usize) as u8;
        data_shards = data;
        let shards = manifest::encode_shards(&artifact, data, parity)?;
        let mut locs = Vec::with_capacity(n);
        for (i, shard) in shards.iter().enumerate() {
            let config = &eligible[i];
            let remote = manifest::shard_remote_path(&entry.hash, i as u8);
            upload(config, &remote, shard, "rs")?;
            health::observe(&config.id, &Outcome::Success { latency_ms: 0 });
            locs.push(ChunkLoc {
                config_id: config.id.clone(),
                remote_path: remote,
                artifact_hash: transfer::blake3_hex(shard),
            });
            providers.push(config.id.clone());
            shards_written += 1;
            bytes += shard.len() as u64;
        }
        entry.primary = locs[0].clone();
        entry.replicas = locs[1..].to_vec();
    }

    manifest.parity = parity;
    let json = serde_json::to_string(&manifest).map_err(|e| e.to_string())?;
    let report = RestripeReport {
        file_id: file_id.to_string(),
        parity,
        data: data_shards,
        chunks: manifest.chunks.len(),
        shards_written,
        bytes,
        providers: {
            providers.sort();
            providers.dedup();
            providers
        },
    };
    Ok((report, Some((record.file.clone(), json))))
}

/// Restripe one file and write the manifest back.
pub fn restripe(db: &Database, file_id: &str, parity: u8) -> Result<RestripeReport, String> {
    let snapshot = snapshot(db)?;
    let task = begin_task(
        "repair",
        &format!("restripe '{}' to parity {}", file_id, parity),
    );
    let (report, update) = restripe_records(&snapshot, file_id, parity)?;
    if let Some((file, json)) = update {
        let mut file = file;
        file.manifest_ref = Some(json);
        db.upsert_sync_file(&file).map_err(|e| e.to_string())?;
    }
    finish_task(
        &task,
        "done",
        Some(format!("{} shards", report.shards_written)),
    );
    Ok(report)
}

// ─── Catalog (item 4) ───────────────────────────────────────────────────────

/// What the last catalog publish did, for `GET /api/repair/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    pub object_path: Option<String>,
    pub providers: Vec<String>,
    pub entries: usize,
    pub replicas_ok: bool,
    pub warnings: Vec<String>,
}

/// Write the current catalog to every enabled provider (≥2 for durability).
pub fn publish_catalog(db: &Database) -> Result<CatalogStatus, String> {
    let records = db.list_sync_files(None).map_err(|e| e.to_string())?;
    let configs = db.list_sync_configs().map_err(|e| e.to_string())?;
    let node = crate::lease::default_holder();
    let report = manifest::publish_catalog(&records, &configs, &node)?;
    Ok(CatalogStatus {
        object_path: Some(report.object_path),
        providers: report.providers.clone(),
        entries: report.entries,
        replicas_ok: report.providers.len() >= 2,
        warnings: report.warnings,
    })
}

/// What a rebuild from remote restored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildReport {
    pub files: usize,
    pub file_nodes_written: usize,
    pub chunks_seen: usize,
    pub locators_dropped: usize,
    pub providers: Vec<String>,
    pub warnings: Vec<String>,
}

/// Rebuild the catalog from providers after the local redb file is gone
/// (this is the C4 test: delete the DB, run this, read the data back).
pub fn rebuild_from_remote(db: &Database) -> Result<RebuildReport, String> {
    let configs = db.list_sync_configs().map_err(|e| e.to_string())?;
    let rebuild = manifest::rebuild_from_remote(&configs)?;
    let written = write_rebuild_records(db, &rebuild.files)?;

    Ok(RebuildReport {
        files: rebuild.files.len(),
        file_nodes_written: written,
        chunks_seen: rebuild.chunks_seen,
        locators_dropped: rebuild.locators_dropped,
        providers: rebuild.providers,
        warnings: rebuild.warnings,
    })
}

/// Land rebuilt records: the sync-file row plus the file node that makes the
/// file visible to the rest of the OS. Parent folders are reconstructed from
/// `original_path` (P0-24) — a rebuild used to flatten every recovered file
/// to the root. Folder ids are deterministic (`rebuild-dir-<blake3-16>` of
/// the full dir path), so retries reuse the same folders instead of
/// duplicating them. Returns how many file *nodes* had to be created.
fn write_rebuild_records(db: &Database, records: &[SyncFile]) -> Result<usize, String> {
    let mut written = 0usize;
    for record in records {
        db.upsert_sync_file(record).map_err(|e| e.to_string())?;
        if db
            .get_file_node(&record.id)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            let parent_id = ensure_rebuild_parents(db, &record.original_path)?;
            let name = std::path::Path::new(&record.original_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| record.id.clone());
            let node = cybermanju_types::schema::FileNode {
                id: record.id.clone(),
                name,
                file_type: "file".to_string(),
                parent_id: None,
                size_bytes: record.size_bytes,
                mime_type: None,
                hash_blake3: record.hash_blake3.clone(),
                encrypted: record.encrypted.unwrap_or(false),
                encryption_algorithm: None,
                compression_layers: Vec::new(),
                thumbnail_path: None,
                context_data: None,
                tags: Vec::new(),
                collection_ids: Vec::new(),
                face_group_ids: Vec::new(),
                loose_group_ids: Vec::new(),
                gps_lat: None,
                gps_lon: None,
                created_at: record.synced_at.clone().unwrap_or_else(now),
                modified_at: record.synced_at.clone().unwrap_or_else(now),
            };
            let serialized = serde_json::to_string(&node).map_err(|e| e.to_string())?;
            db.insert_file_with_index(&record.id, &serialized, parent_id.as_deref())
                .map_err(|e| e.to_string())?;
            written += 1;
        }
    }
    Ok(written)
}

/// Recreate the folder chain for a rebuilt `original_path`, deepest first.
/// Returns the deepest folder id (the file's parent). Deterministic ids
/// make retries idempotent; existing folders (or files) are reused, never
/// duplicated. Single-component paths (bare filenames) stay root-level.
fn ensure_rebuild_parents(db: &Database, original_path: &str) -> Result<Option<String>, String> {
    let path = std::path::Path::new(original_path);
    let mut dirs: Vec<String> = Vec::new();
    let mut cursor = path.parent();
    while let Some(dir) = cursor {
        let text = dir.to_string_lossy().to_string();
        if text.is_empty() || text == "/" || text == "." {
            break;
        }
        dirs.push(text);
        cursor = dir.parent();
    }
    if dirs.is_empty() {
        return Ok(None);
    }
    dirs.reverse();
    let now = chrono::Utc::now().to_rfc3339();
    let mut parent_id: Option<String> = None;
    // `dirs` already holds full root-first ancestor paths (`/a`, `/a/b`) —
    // each becomes one folder row, linked to the previous id.
    for dir in dirs {
        let id = format!("rebuild-dir-{}", &crate::blake3_hex(dir.as_bytes())[..16]);
        if db.get_file_node(&id).map_err(|e| e.to_string())?.is_none() {
            let name = std::path::Path::new(&dir)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.clone());
            let node = cybermanju_types::schema::FileNode {
                id: id.clone(),
                name,
                file_type: "folder".to_string(),
                parent_id: parent_id.clone(),
                size_bytes: 0,
                mime_type: None,
                hash_blake3: None,
                encrypted: false,
                encryption_algorithm: None,
                compression_layers: Vec::new(),
                thumbnail_path: None,
                context_data: None,
                tags: Vec::new(),
                collection_ids: Vec::new(),
                face_group_ids: Vec::new(),
                loose_group_ids: Vec::new(),
                gps_lat: None,
                gps_lon: None,
                created_at: now.clone(),
                modified_at: now.clone(),
            };
            let serialized = serde_json::to_string(&node).map_err(|e| e.to_string())?;
            db.insert_file_with_index(&id, &serialized, parent_id.as_deref())
                .map_err(|e| e.to_string())?;
        }
        parent_id = Some(id);
    }
    Ok(parent_id)
}

// ─── Status (item 9's payload) ──────────────────────────────────────────────

/// Everything `GET /api/repair/status` reports.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairStatus {
    pub tasks: Vec<TaskInfo>,
    pub queued_findings: usize,
    pub pending_ops: usize,
    pub repairs: Vec<RepairRow>,
    pub repaired: usize,
    pub unrecoverable: usize,
    pub skipped: usize,
    pub last_scrub: Option<crate::scrub::ScrubRun>,
    pub health: Vec<health::ProviderHealth>,
    /// `true` when `repairs` was cut at the requested page (P0-23) — counts
    /// below still cover the collected page only.
    #[serde(default)]
    pub repairs_capped: bool,
}

/// Default page for the status poll (P0-23): enough for the UI, small
/// enough that a huge repairs table never clones itself per poll.
pub const REPAIR_STATUS_DEFAULT_LIMIT: usize = 200;
/// Hard ceiling for `?limit=` on the status poll.
pub const REPAIR_STATUS_MAX_LIMIT: usize = 2000;

/// Aggregate the durability surfaces for the REST layer.
pub fn status(db: &Database) -> Result<RepairStatus, String> {
    status_paged(db, REPAIR_STATUS_DEFAULT_LIMIT)
}

/// Paged aggregate (P0-23): `limit` caps collected repair rows (`0` =
/// uncapped). Counts cover the collected page; `repairs_capped` says so.
pub fn status_paged(db: &Database, limit: usize) -> Result<RepairStatus, String> {
    let limit = if limit == 0 {
        0
    } else {
        limit.min(REPAIR_STATUS_MAX_LIMIT)
    };
    let repairs = list_repairs_paged(db, limit)?;
    let capped = limit > 0 && repairs.len() >= limit;
    let (repaired, unrecoverable, skipped) = (
        repairs.iter().filter(|r| r.status == "repaired").count(),
        repairs
            .iter()
            .filter(|r| r.status == "unrecoverable")
            .count(),
        repairs.iter().filter(|r| r.status == "skipped").count(),
    );
    let mut scrub_runs = crate::scrub::list_scrub_runs(db)?;
    scrub_runs.reverse();
    Ok(RepairStatus {
        tasks: tasks(),
        queued_findings: queued_findings(),
        pending_ops: pending_len(),
        repairs,
        repaired,
        unrecoverable,
        skipped,
        last_scrub: scrub_runs.into_iter().next(),
        health: health::status(db)?,
        repairs_capped: capped,
    })
}

// ─── Test fixtures ──────────────────────────────────────────────────────────

/// A throwaway pool: N local providers, one striped file, its manifest and a
/// database holding the record. Shared by the repair/scrub/GC tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::PathBuf;

    use cybermanju_types::sync::SyncStatus;

    use super::*;
    use crate::manifest::{ChunkManifest, CHUNK_SIZE, MANIFEST_VERSION};

    fn nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    pub struct Env {
        pub root: PathBuf,
        db: Database,
        pub configs: Vec<SyncConfig>,
        pub plaintext: Vec<u8>,
        pub record: SyncFile,
        pub manifest: ChunkManifest,
        pub hash: String,
    }

    impl Env {
        /// `providers` local backends + one replicated file whose chunk has
        /// `1 + min(parity, providers-1)` copies — the layout today's upload
        /// pipeline writes.
        pub fn new(tag: &str, providers: usize, parity: u8) -> Self {
            let root = std::env::temp_dir().join(format!(
                "cybermanju-env-{}-{}-{}",
                tag,
                std::process::id(),
                nanos()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("mkdir");

            let mut configs = Vec::new();
            for i in 0..providers {
                let dir = root.join(format!("provider-{}", i));
                std::fs::create_dir_all(&dir).expect("mkdir provider");
                let config: SyncConfig = serde_json::from_value(serde_json::json!({
                    "id": format!("{}-p{}", tag, i),
                    "backendType": "local",
                    "enabled": true,
                    "basePath": dir.to_string_lossy(),
                    "parity": parity,
                    "encryptBeforeUpload": false,
                    "autoSync": false,
                    "compressBeforeUpload": false,
                    "createPreviews": false,
                    "deleteRawAfterSync": false,
                    "maxConcurrentUploads": 4,
                }))
                .expect("config");
                configs.push(config);
            }

            let plaintext: Vec<u8> = (0..131_072u32).map(|i| (i % 251) as u8).collect();
            let hash = transfer::blake3_hex(&plaintext);
            let copies = 1 + (parity as usize).min(providers.saturating_sub(1));

            let remote = manifest::chunk_remote_path(&hash);
            let artifact_hash = transfer::blake3_hex(&plaintext);
            let mut locs = Vec::new();
            for (i, config) in configs.iter().enumerate().take(copies) {
                let abs = root.join(format!("provider-{}", i)).join(&remote);
                std::fs::create_dir_all(abs.parent().expect("parent")).expect("mkdir chunks");
                std::fs::write(&abs, &plaintext).expect("write artifact");
                locs.push(ChunkLoc {
                    config_id: config.id.clone(),
                    remote_path: remote.clone(),
                    artifact_hash: artifact_hash.clone(),
                });
            }

            let entry = ChunkEntry {
                index: 0,
                hash: hash.clone(),
                size: plaintext.len() as u64,
                primary: locs[0].clone(),
                replicas: locs[1..].to_vec(),
            };
            let manifest = ChunkManifest {
                version: MANIFEST_VERSION,
                file_hash: hash.clone(),
                chunk_size: CHUNK_SIZE,
                total_size: plaintext.len() as u64,
                chunks: vec![entry],
                parity,
            };
            let manifest_json = serde_json::to_string(&manifest).expect("manifest json");
            let record = SyncFile {
                id: format!("{}-file", tag),
                config_id: Some(configs[0].id.clone()),
                home_config_id: Some(configs[0].id.clone()),
                original_path: "/pool/volume/test.bin".to_string(),
                compressed_path: None,
                preview_path: None,
                remote_url: None,
                remote_path: None,
                size_bytes: plaintext.len() as u64,
                compressed_size_bytes: None,
                hash_blake3: Some(hash.clone()),
                artifact_hash: None,
                manifest_ref: Some(manifest_json),
                last_verified_at: None,
                key_handle: None,
                compressed: Some(false),
                encrypted: Some(false),
                backend_type: cybermanju_types::sync::SyncBackendType::Local,
                synced_at: Some(chrono::Utc::now().to_rfc3339()),
                status: SyncStatus::Completed,
                error_message: None,
            };

            let db_path = root.join("test.redb");
            let db = Database::new(db_path.to_str().expect("db path")).expect("db");
            db.upsert_sync_file(&record).expect("upsert");
            // The configs have to exist in the database too: every consumer
            // (placement, GC, health) reads them from `list_sync_configs`.
            Self::write_configs(&db, &configs).expect("configs");

            Env {
                root,
                db,
                configs,
                plaintext,
                record,
                manifest,
                hash,
            }
        }

        pub fn db(&self) -> &Database {
            &self.db
        }

        /// The snapshot as it stands in memory (pre-repair).
        pub fn snapshot(&self) -> Snapshot {
            Snapshot {
                records: vec![Record {
                    file: self.record.clone(),
                    manifest: self.manifest.clone(),
                }],
                configs: self.configs.clone(),
            }
        }

        /// The snapshot as the database now holds it (post-repair).
        pub fn reload(&self) -> Snapshot {
            snapshot(&self.db).expect("snapshot")
        }

        pub fn object_path(&self, provider: usize) -> PathBuf {
            self.root
                .join(format!("provider-{}", provider))
                .join(manifest::chunk_remote_path(&self.hash))
        }

        pub fn shard_path(&self, provider: usize, shard: u8) -> PathBuf {
            self.root
                .join(format!("provider-{}", provider))
                .join(manifest::shard_remote_path(&self.hash, shard))
        }

        /// Flip the bytes of provider `i`'s copy so its BLAKE3 no longer
        /// matches the recorded artifact hash.
        pub fn corrupt(&self, provider: usize) {
            let path = self.object_path(provider);
            let mut bytes = std::fs::read(&path).expect("read artifact");
            bytes.push(0xA5);
            std::fs::write(&path, bytes).expect("corrupt artifact");
        }

        /// Delete provider `i`'s copy outright.
        pub fn remove(&self, provider: usize) {
            std::fs::remove_file(self.object_path(provider)).expect("remove artifact");
        }

        /// Delete one erasure shard (after a restripe, the chunk path on a
        /// provider may not exist — the shards do).
        pub fn remove_shard(&self, provider: usize, shard: u8) {
            std::fs::remove_file(self.shard_path(provider, shard)).expect("remove shard");
        }

        /// Write configs into any database (needed when a test starts from a
        /// fresh, empty redb — the C4 rebuild path).
        pub fn write_configs(db: &Database, configs: &[SyncConfig]) -> Result<(), String> {
            let tx = db.begin_write().map_err(|e| e.to_string())?;
            {
                let mut table = tx
                    .open_table(Database::get_sync_configs_table())
                    .map_err(|e| e.to_string())?;
                for config in configs {
                    let json = serde_json::to_string(config).map_err(|e| e.to_string())?;
                    table
                        .insert(config.id.as_str(), json.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }
            tx.commit().map_err(|e| e.to_string())
        }

        /// Drop an unreferenced object into a provider (GC bait).
        pub fn write_orphan(&self, provider: usize, name: &str, bytes: &[u8]) {
            let dir = self.root.join(format!("provider-{}", provider));
            let abs = dir.join(name);
            std::fs::create_dir_all(abs.parent().expect("parent")).expect("mkdir");
            std::fs::write(abs, bytes).expect("write orphan");
        }

        /// Restore from `snapshot` and assert the bytes come back identical.
        pub fn restore_ok(&self, snap: &Snapshot, tag: &str) {
            let dest = self.root.join(format!("restored-{}.bin", tag));
            let dest_str = dest.to_string_lossy().to_string();
            manifest::restore(&snap.records[0].manifest, &self.configs, &dest_str)
                .expect("restore");
            let back = std::fs::read(&dest).expect("read restored");
            assert_eq!(back, self.plaintext, "read-back must be byte-identical");
        }

        pub fn cleanup(self) {
            let root = self.root.clone();
            drop(self);
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::Env;
    use super::*;
    use crate::manifest::ChunkManifest;

    fn finding_for(env: &Env, kind: FindingKind, config: usize) -> Finding {
        Finding::new(
            env.record.id.clone(),
            0,
            Some(env.configs[config].id.clone()),
            kind,
            "test finding",
        )
    }

    #[test]
    fn a_corrupt_copy_is_rebuilt_from_its_twin() {
        let env = Env::new("repair-corrupt", 2, 1);
        env.corrupt(1);

        // Scrub finds it…
        let run = crate::scrub::scrub_snapshot(&env.snapshot());
        assert_eq!(run.corrupt, 1, "{:?}", run);
        let findings: Vec<Finding> = run
            .findings
            .iter()
            .map(|f| {
                Finding::new(
                    f.file_id.clone(),
                    f.chunk_index,
                    Some(f.config_id.clone()),
                    FindingKind::Corrupt,
                    f.detail.clone(),
                )
            })
            .collect();

        // …repair fixes it.
        let outcome = repair(env.db(), findings).expect("repair");
        assert_eq!(outcome.repaired(), 1, "{:?}", outcome.rows);
        assert_eq!(outcome.unrecoverable(), 0);

        let snap = env.reload();
        let rebuilt = std::fs::read(env.object_path(1)).expect("read rebuilt copy");
        assert_eq!(
            transfer::blake3_hex(&rebuilt),
            snap.records[0].manifest.chunks[0].replicas[0].artifact_hash,
            "rebuilt copy verifies against its recorded hash"
        );
        env.restore_ok(&snap, "repair-corrupt");

        assert!(list_repairs(env.db())
            .expect("repairs table")
            .iter()
            .any(|r| r.status == "repaired"));
        env.cleanup();
    }

    #[test]
    fn an_erasure_chunk_is_regenerated_from_surviving_shards() {
        let env = Env::new("repair-rs", 3, 1);
        restripe(env.db(), &env.record.id, 1).expect("restripe");
        let snap = env.reload();
        assert!(
            manifest::entry_redundancy(&snap.records[0].manifest.chunks[0], 1).is_erasure(),
            "restripe actually coded the chunk"
        );

        // Lose the parity shard (shard 2 sits on provider 2).
        env.remove_shard(2, 2);
        let outcome =
            repair(env.db(), vec![finding_for(&env, FindingKind::Missing, 2)]).expect("repair");
        assert_eq!(outcome.repaired(), 1, "{:?}", outcome.rows);
        assert!(
            env.shard_path(2, 2).exists(),
            "the missing shard was regenerated"
        );
        env.restore_ok(&env.reload(), "repair-rs");
        env.cleanup();
    }

    #[test]
    fn a_chunk_with_no_surviving_copy_is_reported_unrecoverable() {
        let env = Env::new("repair-gone", 1, 0);
        env.remove(0);

        let outcome =
            repair(env.db(), vec![finding_for(&env, FindingKind::Missing, 0)]).expect("repair");
        assert_eq!(outcome.unrecoverable(), 1, "{:?}", outcome.rows);
        let error = outcome.rows[0].error.as_deref().unwrap_or("");
        assert!(error.starts_with("unrecoverable:"), "{}", error);
        // …and it is durable, not just returned.
        assert!(list_repairs(env.db())
            .expect("repairs table")
            .iter()
            .any(|r| r.status == "unrecoverable"));
        env.cleanup();
    }

    #[test]
    fn provider_loss_relocates_every_chunk_that_lived_only_there() {
        let env = Env::new("repair-loss", 3, 1);
        let mut snapshot = env.snapshot();
        snapshot.configs[0].enabled = false;

        let outcome = relocate_records(&snapshot, &env.configs[0].id);
        assert_eq!(outcome.repaired(), 1, "{:?}", outcome.rows);
        assert_eq!(outcome.updates.len(), 1);

        let manifest: ChunkManifest =
            serde_json::from_str(&outcome.updates[0].1).expect("manifest json");
        let holders: Vec<String> = manifest::entry_locs(&manifest.chunks[0])
            .iter()
            .map(|l| l.config_id.clone())
            .collect();
        assert!(!holders.contains(&env.configs[0].id), "{:?}", holders);
        assert!(holders.contains(&env.configs[2].id), "moved to idle p2");
        assert!(
            env.object_path(2).exists(),
            "bytes landed on the new provider"
        );

        // The data still reads back (the dead provider's locator is gone).
        let snap = Snapshot {
            records: vec![Record {
                file: outcome.updates[0].0.clone(),
                manifest,
            }],
            configs: env.configs.clone(),
        };
        env.restore_ok(&snap, "provider-loss");
        env.cleanup();
    }

    #[test]
    fn restriping_makes_a_file_erasure_coded_and_it_still_restores() {
        let env = Env::new("repair-restripe", 3, 1);
        let report = restripe(env.db(), &env.record.id, 1).expect("restripe");
        assert_eq!(report.shards_written, 3, "{:?}", report);
        assert_eq!(report.data, 2);
        assert_eq!(report.parity, 1);

        let snap = env.reload();
        let entry = &snap.records[0].manifest.chunks[0];
        assert!(manifest::entry_redundancy(entry, 1).is_erasure());
        for shard in 0..3u8 {
            assert!(
                env.shard_path(shard as usize, shard).exists(),
                "shard {}",
                shard
            );
        }
        env.restore_ok(&snap, "restripe");

        // Two providers cannot hold a stable RS layout — refuse, don't guess.
        let small = Env::new("restripe-2", 2, 1);
        let err = restripe(small.db(), &small.record.id, 1).expect_err("unsupported");
        assert!(err.starts_with("unsupported:"), "{}", err);
        env.cleanup();
        small.cleanup();
    }

    #[test]
    fn the_catalog_survives_deleting_the_local_database() {
        let env = Env::new("catalog", 2, 1);
        let status = publish_catalog(env.db()).expect("publish");
        assert!(status.replicas_ok, "{:?}", status);
        assert_eq!(status.providers.len(), 2, "{:?}", status);
        assert_eq!(status.entries, 1);

        // The C4 test: the local redb file is gone — brand-new empty db.
        let fresh_path = env.root.join("fresh.redb");
        let fresh = Database::new(fresh_path.to_str().expect("path")).expect("fresh db");
        Env::write_configs(&fresh, &env.configs).expect("configs");

        let report = rebuild_from_remote(&fresh).expect("rebuild");
        assert_eq!(report.files, 1, "{:?}", report);
        assert_eq!(report.chunks_seen, 2, "{:?}", report);
        assert!(
            fresh.get_file_node(&env.record.id).expect("node").is_some(),
            "file nodes are restored too"
        );

        let snap = snapshot(&fresh).expect("snapshot");
        assert_eq!(snap.records.len(), 1);
        env.restore_ok(&snap, "catalog");
        env.cleanup();
    }

    #[test]
    fn pending_ops_land_when_someone_holds_the_lock() {
        let env = Env::new("pending", 1, 0);
        let row = RepairRow {
            repair_id: "rep-pending-test".to_string(),
            file_id: env.record.id.clone(),
            chunk_index: 7,
            kind: "copy".to_string(),
            status: "repaired".to_string(),
            method: "replica".to_string(),
            sources: vec![env.configs[0].id.clone()],
            targets: vec![],
            bytes: 42,
            error: None,
            finished_at: now(),
        };
        queue_pending(PendingOp::Repair(row));
        flush_pending(env.db()).expect("flush");
        assert!(list_repairs(env.db())
            .expect("repairs")
            .iter()
            .any(|r| r.repair_id == "rep-pending-test"));
        env.cleanup();
    }

    #[test]
    fn tasks_are_visible_to_ps_and_top() {
        let id = begin_task("repair", "testing");
        progress_task(&id, 0.5, None);
        assert!(tasks().iter().any(|t| t.id == id && t.state == "running"));
        finish_task(&id, "done", Some("ok".to_string()));
        let task = tasks().into_iter().find(|t| t.id == id).expect("task");
        assert_eq!(task.state, "done");
        assert_eq!(task.progress, 1.0);
        assert!(task.finished_at.is_some());
    }

    #[test]
    fn a_finding_for_an_unknown_manifest_is_skipped_not_swallowed() {
        let env = Env::new("repair-unknown", 1, 0);
        let snapshot = env.snapshot();
        let outcome = repair_snapshot(
            &snapshot,
            &[Finding::new(
                "no-such-file",
                0,
                None,
                FindingKind::Corrupt,
                "x",
            )],
        );
        assert_eq!(outcome.skipped(), 1);
        let error = outcome.rows[0].error.as_deref().unwrap_or("");
        assert!(error.starts_with("integrity:"), "{}", error);
        env.cleanup();
    }
}
