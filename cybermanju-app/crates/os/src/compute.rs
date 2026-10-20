//! Compute fan-out (AGENT-8 item 7 / MISSING.md E1, E2): **the more providers
//! you connect, the more processing you have**.
//!
//! [`workers`] scores the pool — local `rayon` slots **plus** one slice per
//! connected provider — and [`run`] builds a rayon pool of exactly that size,
//! so attaching provider B measurably increases the parallelism of the next
//! job (that is the Tier-2 acceptance test).
//!
//! Provider slots come from the *real* `Capabilities::compute` value each
//! backend reports (`cybermanju-types::sync::Capabilities`, AGENT-8 item 8).
//! The backends are constructed through their I/O-free `new(…)` constructors:
//! going via `create_backend` would call `oauth::resolve_token`, which can hit
//! the network just to answer "how many workers do I have?".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use cybermanju_db::Database;
use cybermanju_search::{DocumentParams, SearchIndex, SearchRequest};
use cybermanju_types::sync::{Capabilities, StorageBackend, SyncBackendType};
use rayon::prelude::*;
use serde::Serialize;

use crate::api::Kernel;
use crate::task::{TaskState, TaskTable};

/// Hard ceiling on the pool a single job may build. Provider rows are data,
/// not a licence to fork a million threads.
const MAX_POOL_THREADS: usize = 256;

/// Per-provider ceiling, applied before the pool is sized.
const MAX_SLOTS_PER_PROVIDER: u32 = 64;

/// Files up to this size get their text content read for `index`.
const INDEX_CONTENT_LIMIT: u64 = 1 << 20;

/// The workloads this scheduler can actually run — all of them do real work
/// against real bytes (no simulated progress).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub name: &'static str,
    pub description: &'static str,
    /// Volume path argument the job expects.
    pub takes_path: bool,
}

/// `jobs` — the catalogue.
pub fn available_jobs() -> Vec<JobInfo> {
    vec![
        JobInfo {
            name: "compress",
            description: "triple-compress every file under a path and report savings",
            takes_path: true,
        },
        JobInfo {
            name: "hash",
            description: "BLAKE3 every file under a path",
            takes_path: true,
        },
        JobInfo {
            name: "index",
            description: "re-index file names and text content for `search`",
            takes_path: true,
        },
    ]
}

/// One connected provider's contribution to the pool.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderWorker {
    pub id: String,
    /// `config` (a sync config) or `disk` (an attached `.cybermanju` disk).
    pub source: String,
    pub backend: String,
    /// Concurrent slots this provider adds (`Capabilities::compute`).
    pub slots: u32,
}

/// The fan-out pool: local `rayon` slots + provider slots.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workers {
    /// `rayon::current_num_threads()` — local CPU slots.
    pub local_threads: usize,
    /// Sum of every provider's `Capabilities::compute`.
    pub provider_slots: usize,
    /// `local_threads + provider_slots` — the pool a job is built with.
    pub total: usize,
    pub providers: Vec<ProviderWorker>,
}

/// `Capabilities` for a backend type without touching the network.
pub fn capabilities_for(backend_type: &SyncBackendType) -> Capabilities {
    let backend: Box<dyn StorageBackend> = match backend_type {
        SyncBackendType::Local => Box::new(cybermanju_sync::backends::LocalBackend::new(
            "/cybermanju-unused",
        )),
        SyncBackendType::GitHub => Box::new(cybermanju_sync::backends::GitHubBackend::new(
            "", "", "main",
        )),
        SyncBackendType::GitLab => Box::new(cybermanju_sync::backends::GitLabBackend::new(
            "", "", "main", None,
        )),
        SyncBackendType::GoogleDrive => {
            Box::new(cybermanju_sync::backends::GoogleDriveBackend::new("", None))
        }
    };
    backend.capabilities()
}

/// Score the pool. Providers are deduplicated: a disk bound to an enabled sync
/// config adds its slots to that config instead of counting twice, while any
/// other attached disk stands alone as a provider.
pub fn workers(db: Option<&Database>) -> Workers {
    let local_threads = rayon::current_num_threads();
    let mut providers: BTreeMap<String, ProviderWorker> = BTreeMap::new();

    if let Some(db) = db {
        if let Ok(configs) = db.list_sync_configs() {
            for config in configs.into_iter().filter(|c| c.enabled) {
                let caps = capabilities_for(&config.backend_type);
                providers.insert(
                    config.id.clone(),
                    ProviderWorker {
                        id: config.id.clone(),
                        source: "config".to_string(),
                        backend: config.backend_type.to_string(),
                        slots: caps.compute.min(MAX_SLOTS_PER_PROVIDER),
                    },
                );
            }
        }
        if let Ok(disks) = crate::api::list_disks(db) {
            for disk in disks.into_iter().filter(|d| d.attached()) {
                // A disk row without a capability record still contributes one
                // slot: a newly connected provider adds processing power.
                let slots = if disk.compute == 0 {
                    1
                } else {
                    disk.compute.min(MAX_SLOTS_PER_PROVIDER)
                };
                if !disk.provider.is_empty() && providers.contains_key(&disk.provider) {
                    // Same provider seen through its disk: merge, never double
                    // count — one provider, one slice of the pool.
                    if let Some(entry) = providers.get_mut(&disk.provider) {
                        entry.slots = entry.slots.max(slots);
                    }
                    continue;
                }
                let id = disk.id.clone();
                providers.insert(
                    id.clone(),
                    ProviderWorker {
                        id,
                        source: "disk".to_string(),
                        backend: disk.provider.clone(),
                        slots,
                    },
                );
            }
        }
    }

    let provider_slots: usize = providers.values().map(|p| p.slots as usize).sum();
    let total = (local_threads + provider_slots).clamp(1, MAX_POOL_THREADS);
    Workers {
        local_threads,
        provider_slots,
        total,
        providers: providers.into_values().collect(),
    }
}

/// One completed (or partly completed) job.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobReport {
    /// The `compute_tasks` row this job ran as.
    pub task_id: u32,
    pub kind: String,
    pub path: String,
    /// Files handed to the pool.
    pub items: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
    /// `bytes_in - bytes_out` (negative when a file did not shrink).
    pub saved_bytes: i64,
    pub elapsed_ms: u64,
    /// Pool the job actually ran on.
    pub workers: Workers,
    pub cancelled: bool,
    /// Sample output (first few lines) for the shell.
    pub lines: Vec<String>,
}

/// Run a workload across the fan-out pool.
///
/// The job registers itself in the task table, so `ps`/`top` see it and
/// `kill <id>` cancels it between items.
pub fn run(db: Option<&Database>, kind: &str, path: &str) -> Result<JobReport, String> {
    let snapshot = workers(db);
    run_scheduled(&snapshot, kind, path)
}

/// Same as [`run`] but with the pool snapshot supplied by the caller — used
/// when a background thread schedules the job and cannot carry the
/// `&Database` with it.
pub fn run_scheduled(snapshot: &Workers, kind: &str, path: &str) -> Result<JobReport, String> {
    let names: Vec<&str> = available_jobs().iter().map(|j| j.name).collect();
    if !names.contains(&kind) {
        return Err(crate::shell::did_you_mean("unknown job", kind, &names));
    }
    if kind == "index" {
        return run_index(snapshot, path);
    }

    let kernel = Kernel::global();
    let root = kernel.resolve(path)?;
    if !root.exists() {
        return Err(format!("not_found: {path}"));
    }
    let files = collect_files(&root);
    if files.is_empty() {
        return Err(format!("not_found: {path} has nothing to process"));
    }

    let snapshot = snapshot.clone();
    let pool = build_pool(snapshot.total)?;
    let started = Instant::now();
    let total_items = files.len();
    let done = AtomicUsize::new(0);

    let (task_id, cancel) =
        TaskTable::global().spawn("compute", &format!("compute run {kind} {path}"), "local");
    let cancel: &Arc<std::sync::atomic::AtomicBool> = &cancel;

    type Item = (u64, Option<u64>, String);
    let outcomes: Vec<Option<Result<Item, String>>> = pool.install(|| {
        files
            .par_iter()
            .map(|file| {
                if cancel.load(Ordering::SeqCst) {
                    return None;
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                TaskTable::global().progress(task_id, n as f64 / total_items as f64, n as u64);
                Some(process_one(kind, file))
            })
            .collect()
    });

    let cancelled = cancel.load(Ordering::SeqCst);
    let mut bytes_in = 0u64;
    let mut bytes_out = 0u64;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut lines: Vec<String> = Vec::new();
    for outcome in outcomes.into_iter().flatten() {
        match outcome {
            Ok((input, output, line)) => {
                succeeded += 1;
                bytes_in += input;
                if let Some(out) = output {
                    bytes_out += out;
                }
                if lines.len() < 8 {
                    lines.push(line);
                }
            }
            Err(e) => {
                failed += 1;
                if lines.len() < 8 {
                    lines.push(e);
                }
            }
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let saved = bytes_in as i64 - bytes_out as i64;
    finish_task(task_id, cancelled, succeeded, failed);

    lines.push(format!(
        "{succeeded}/{total_items} files · {bytes_in} → {bytes_out} bytes · {} · {} workers",
        if saved >= 0 {
            format!("{saved} saved")
        } else {
            format!("{} grown", -saved)
        },
        snapshot.total
    ));

    Ok(JobReport {
        task_id,
        kind: kind.to_string(),
        path: path.to_string(),
        items: total_items,
        succeeded,
        failed,
        bytes_in,
        bytes_out,
        saved_bytes: saved,
        elapsed_ms,
        workers: snapshot,
        cancelled,
        lines,
    })
}

/// `search` re-index: read/extract in parallel (the expensive half), then one
/// serial tantivy batch commit (the writer owns that part).
fn run_index(snapshot: &Workers, path: &str) -> Result<JobReport, String> {
    let kernel = Kernel::global();
    let root = kernel.resolve(path)?;
    if !root.exists() {
        return Err(format!("not_found: {path}"));
    }
    let files = collect_files(&root);
    if files.is_empty() {
        return Err(format!("not_found: {path} has nothing to index"));
    }

    let snapshot = snapshot.clone();
    let pool = build_pool(snapshot.total)?;
    let started = Instant::now();
    let total = files.len();
    let done = AtomicUsize::new(0);

    let (task_id, cancel) =
        TaskTable::global().spawn("index", &format!("compute run index {path}"), "local");
    let cancel: &Arc<std::sync::atomic::AtomicBool> = &cancel;

    struct Extracted {
        file_id: String,
        name: String,
        ext: String,
        content: String,
        created: String,
        size: u64,
    }

    let extracted: Vec<Option<Extracted>> = pool.install(|| {
        files
            .par_iter()
            .map(|file| {
                if cancel.load(Ordering::SeqCst) {
                    return None;
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                TaskTable::global().progress(task_id, n as f64 / total as f64, n as u64);
                let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
                Some(Extracted {
                    file_id: kernel.display(file),
                    name: file
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    ext: file
                        .extension()
                        .map(|e| e.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "bin".to_string()),
                    content: read_text(file, size),
                    created: created_rfc3339(file),
                    size,
                })
            })
            .collect()
    });

    let cancelled = cancel.load(Ordering::SeqCst);
    let docs: Vec<Extracted> = extracted.into_iter().flatten().collect();
    let mut bytes_in = 0u64;
    for doc in &docs {
        bytes_in += doc.size;
    }
    let count = docs.len();
    let mut lines = Vec::new();

    if !cancelled && count > 0 {
        let index = search_index()?;
        let no_tags: Vec<String> = Vec::new();
        let params: Vec<DocumentParams<'_>> = docs
            .iter()
            .map(|d| DocumentParams {
                file_id: &d.file_id,
                file_name: &d.name,
                content_text: &d.content,
                tags: &no_tags,
                file_type: &d.ext,
                is_encrypted: false,
                has_geo: false,
                created_at: &d.created,
                blake3_hash: None,
            })
            .collect();
        let docs_total = params.len();
        index
            .add_document_batch(params)
            .map_err(|e| format!("io error: cannot index: {e}"))?;
        index
            .commit()
            .map_err(|e| format!("io error: cannot commit index: {e}"))?;
        let indexed = index
            .doc_count()
            .map_err(|e| format!("io error: cannot read index: {e}"))?;
        lines.push(format!(
            "indexed {docs_total} files · index holds {indexed} docs"
        ));
        for doc in docs.iter().take(6) {
            lines.push(format!(
                "{} · {} chars",
                doc.file_id,
                doc.content.chars().count()
            ));
        }
    }

    let elapsed_ms = started.elapsed().as_millis() as u64;
    let failed = 0usize;
    let succeeded = count;
    finish_task(task_id, cancelled, succeeded, failed);
    lines.push(format!(
        "{count}/{total} files re-indexed · {bytes_in} bytes · {} workers",
        snapshot.total
    ));

    Ok(JobReport {
        task_id,
        kind: "index".to_string(),
        path: path.to_string(),
        items: total,
        succeeded,
        failed,
        bytes_in,
        bytes_out: bytes_in,
        saved_bytes: 0,
        elapsed_ms,
        workers: snapshot,
        cancelled,
        lines,
    })
}

fn build_pool(total: usize) -> Result<rayon::ThreadPool, String> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(total)
        .build()
        .map_err(|e| format!("io error: cannot build worker pool: {e}"))
}

fn finish_task(task_id: u32, cancelled: bool, succeeded: usize, failed: usize) {
    if cancelled {
        TaskTable::global().finish(task_id, TaskState::Killed, None);
    } else if failed > 0 && succeeded == 0 {
        TaskTable::global().finish(
            task_id,
            TaskState::Failed,
            Some(format!("{failed} items failed")),
        );
    } else {
        TaskTable::global().update(task_id, |t| t.progress = 1.0);
        TaskTable::global().finish(task_id, TaskState::Done, None);
    }
}

/// One unit of work: `(bytes_in, bytes_out, line)`.
fn process_one(kind: &str, file: &Path) -> Result<(u64, Option<u64>, String), String> {
    let name = file.display().to_string();
    let meta = std::fs::metadata(file).map_err(|e| format!("io error: {name}: {e}"))?;
    let size = meta.len();
    match kind {
        "compress" => {
            let data = std::fs::read(file).map_err(|e| format!("io error: {name}: {e}"))?;
            let (compressed, stats) = cybermanju_compression::TripleCompressor::new()
                .compress_triple(&data)
                .map_err(|e| format!("io error: {name}: {e}"))?;
            let percent = if size == 0 {
                0.0
            } else {
                compressed.len() as f64 / size as f64 * 100.0
            };
            let _ = stats;
            Ok((
                size,
                Some(compressed.len() as u64),
                format!("{name} {size} → {} bytes ({percent:.0}%)", compressed.len()),
            ))
        }
        "hash" => {
            let data = std::fs::read(file).map_err(|e| format!("io error: {name}: {e}"))?;
            let digest = cybermanju_compression::TripleCompressor::blake3_hash(&data);
            Ok((size, None, format!("{name} {digest}")))
        }
        other => Err(format!("unsupported: job '{other}' is not implemented")),
    }
}

fn read_text(file: &Path, size: u64) -> String {
    if size > INDEX_CONTENT_LIMIT || !is_probably_text(file) {
        return String::new();
    }
    std::fs::read(file)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

fn is_probably_text(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => {
            let sample = &bytes[..bytes.len().min(4096)];
            !sample.contains(&0) && std::str::from_utf8(sample).is_ok()
        }
        Err(_) => false,
    }
}

fn created_rfc3339(path: &Path) -> String {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|t| t.to_rfc3339())
        .unwrap_or_default()
}

/// Every regular file under `root`, sorted for deterministic output.
fn collect_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => stack.push(path),
                Ok(meta) if meta.is_file() => out.push(path),
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// The volume's search index, created on first use.
pub fn search_index() -> Result<&'static SearchIndex, String> {
    static INDEX: OnceLock<SearchIndex> = OnceLock::new();
    if let Some(index) = INDEX.get() {
        return Ok(index);
    }
    let dir = cybermanju_crypto::keystore::data_dir()
        .unwrap_or_else(|| PathBuf::from(".cybermanju"))
        .join("shell-search-index");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("io error: cannot create index dir {}: {e}", dir.display()))?;
    let path = dir.to_string_lossy().into_owned();
    let index =
        SearchIndex::new(&path).map_err(|e| format!("io error: cannot open search index: {e}"))?;
    match INDEX.set(index) {
        Ok(()) => Ok(INDEX.get().expect("just set")),
        Err(_) => Ok(INDEX.get().expect("set by another thread")),
    }
}

/// `search <query>` — BM25 over the shell's volume index.
pub fn search(query: &str, limit: usize) -> Result<Vec<cybermanju_search::SearchResult>, String> {
    let index = search_index()?;
    let count = index
        .doc_count()
        .map_err(|e| format!("io error: cannot read index: {e}"))?;
    if count == 0 {
        return Err("no index: run `compute run index /` first".to_string());
    }
    index
        .search(&SearchRequest {
            query: query.to_string(),
            limit: Some(limit),
            offset: None,
        })
        .map_err(|e| format!("search failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_backend_reports_a_compute_slot_count() {
        let all = [
            SyncBackendType::Local,
            SyncBackendType::GitHub,
            SyncBackendType::GitLab,
            SyncBackendType::GoogleDrive,
        ];
        for ty in all {
            let caps = capabilities_for(&ty);
            assert!(caps.compute >= 1, "backend {ty} reports no compute slots");
        }
    }

    #[test]
    fn attaching_a_provider_grows_the_pool() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cyb-workers.db");
        let db = Database::new(path.to_str().expect("utf8")).expect("db");

        let before = workers(Some(&db));
        assert_eq!(before.provider_slots, 0, "no providers connected yet");

        crate::api::put_disk(&db, &disk("disk-a", "local", 2)).expect("put disk a");
        let one = workers(Some(&db));
        assert_eq!(one.provider_slots, 2);
        assert!(one.total > before.total, "pool grew with provider A");

        crate::api::put_disk(&db, &disk("disk-b", "github", 1)).expect("put disk b");
        let two = workers(Some(&db));
        assert!(
            two.total > one.total,
            "attaching provider B must increase parallelism ({} → {})",
            one.total,
            two.total
        );

        // Detaching takes the slots away again — the model is live, not a
        // counter that only ever goes up.
        let mut rec = crate::api::get_disk(&db, "disk-b")
            .expect("get disk b")
            .expect("disk b exists");
        rec.state = "detached".to_string();
        crate::api::put_disk(&db, &rec).expect("detach");
        let after = workers(Some(&db));
        assert!(after.total < two.total, "detaching shrinks the pool");
        assert_eq!(after.provider_slots, 2);
    }

    #[test]
    fn unknown_jobs_are_rejected_with_a_suggestion() {
        let err = run(None, "compres", "/").expect_err("typo must fail");
        assert!(err.contains("compress"), "suggests the real job: {err}");
        assert!(err.starts_with("unknown job:"), "got {err}");
    }

    #[test]
    fn hash_job_runs_real_work_over_a_real_file() {
        let volume = crate::testutil::volume_dir();
        // Scoped to a private sub-directory: the volume root is shared with
        // the shell tests, which create and delete their own entries.
        let scope = volume.join("hash-scope");
        std::fs::create_dir_all(&scope).expect("scope dir");
        for i in 0..6 {
            std::fs::write(
                scope.join(format!("fanout-{i}.txt")),
                format!("payload {i}"),
            )
            .expect("write");
        }

        let report = run(None, "hash", "/hash-scope").expect("job");
        assert!(report.items >= 6, "indexed the scratch volume");
        assert_eq!(report.failed, 0, "{:?}", report.lines);
        assert!(report.bytes_in > 0);
        assert!(report.workers.total >= 1);
        assert!(!report.cancelled);
        assert!(report.lines.iter().any(|l| l.contains("workers")));
        let task = TaskTable::global().get(report.task_id).expect("task");
        assert_eq!(task.state, TaskState::Done);
        assert_eq!(task.kind, "compute");
    }

    fn disk(id: &str, provider: &str, compute: u32) -> crate::api::DiskRecord {
        crate::api::DiskRecord {
            id: id.to_string(),
            name: id.to_string(),
            provider: provider.to_string(),
            capacity_bytes: 1 << 20,
            state: "attached".to_string(),
            health: "ok".to_string(),
            created_at: String::new(),
            used_bytes: 0,
            compute,
        }
    }
}
