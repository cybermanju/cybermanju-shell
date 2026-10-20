// CyberManju OS — Task (crates/os/src/task.rs).
//! The process table (AGENT-8 item 6 / MISSING.md F2): every long-running
//! activity in the OS — a sync run, a scrub, a repair, a GC pass, a compute
//! job, a shell pipeline — is a **task** with `id, kind, state, progress,
//! started_at, bytes, provider`.
//!
//! `ps` lists the table, `top` adds live system stats, `kill` flips the task's
//! cancellation handle (the same `Arc<AtomicBool>` cooperative-cancel token the
//! rest of the app already uses) and marks the row `killed`.
//!
//! The table is in memory — it has to be, `ps` must not block on I/O — and is
//! mirrored into the pre-declared `compute_tasks` table whenever a caller
//! hands us a `&Database`, so the UI can still show history after a restart.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cybermanju_db::Database;
use redb::ReadableTable;
use serde::{Deserialize, Serialize};

/// Rows kept before the oldest finished task is dropped.
pub const MAX_TASKS: usize = 200;

/// Tick rate Linux accounting assumes (`sysconf(_SC_CLK_TCK)` default).
const CLOCK_TICKS_PER_SEC: f64 = 100.0;

/// Whether `/proc` exposes the stats `top` reads. Linux and Android both
/// mount procfs (a Termux build sets `target_os = "android"`, not `"linux"`);
/// anywhere else the readers degrade to `source: "unavailable"` zeros.
const PROC_STATS: bool = cfg!(any(target_os = "linux", target_os = "android"));

/// Lifecycle of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Pending,
    Running,
    Done,
    Failed,
    Killed,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Running => "running",
            TaskState::Done => "done",
            TaskState::Failed => "failed",
            TaskState::Killed => "killed",
        }
    }

    pub fn parse(raw: &str) -> TaskState {
        match raw {
            "pending" => TaskState::Pending,
            "done" => TaskState::Done,
            "failed" => TaskState::Failed,
            "killed" => TaskState::Killed,
            _ => TaskState::Running,
        }
    }

    /// Terminal states are never re-opened and are what `ps` sorts last.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Done | TaskState::Failed | TaskState::Killed
        )
    }
}

/// One row of the process table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: u32,
    /// `sync` · `scrub` · `repair` · `gc` · `compute` · `shell` · `index`.
    pub kind: String,
    /// Human label — usually the command line.
    pub name: String,
    pub state: TaskState,
    /// 0.0 …= 1.0.
    pub progress: f64,
    pub started_at: String,
    #[serde(default)]
    pub ended_at: Option<String>,
    /// Bytes processed so far.
    pub bytes: u64,
    /// Provider/config id, or `local`.
    pub provider: String,
    #[serde(default)]
    pub error: Option<String>,
}

impl Task {
    /// Age in milliseconds since `started_at`, best effort.
    pub fn age_ms(&self) -> u64 {
        parse_rfc3339_ms(&self.started_at)
            .and_then(|start| {
                chrono::Utc::now()
                    .timestamp_millis()
                    .checked_sub(start as i64)
            })
            .map(|ms| ms.max(0) as u64)
            .unwrap_or(0)
    }
}

/// `ps(1)` output.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PsSnapshot {
    pub tasks: Vec<Task>,
    pub counts: TaskCounts,
    /// Cancelled since process start — `kill` reports from here.
    pub pid: u64,
    pub uptime_ms: u64,
}

/// Row counts by state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCounts {
    pub pending: usize,
    pub running: usize,
    pub done: usize,
    pub failed: usize,
    pub killed: usize,
    pub total: usize,
}

/// `/proc/loadavg` — 1/5/15 minute load averages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadAvg {
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    /// `"proc"` or `"unavailable"` (non-Linux / sandboxed).
    pub source: String,
}

/// Process memory, read from `/proc/self/statm` (pages → bytes).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemInfo {
    /// Resident set size of this process.
    pub rss_bytes: u64,
    /// Shared pages of this process.
    pub shared_bytes: u64,
    /// Host memory, when readable.
    pub total_bytes: u64,
    pub available_bytes: u64,
}

/// `top(1)` header: live system stats + the process table.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopSnapshot {
    pub uptime_ms: u64,
    pub load: LoadAvg,
    pub mem: MemInfo,
    /// Whole-process CPU utilisation since the previous `top` call (0..=100).
    pub cpu_percent: f64,
    pub counts: TaskCounts,
    pub tasks: Vec<Task>,
}

struct Entry {
    task: Task,
    cancel: Arc<AtomicBool>,
}

/// The process table.
pub struct TaskTable {
    next_id: AtomicU32,
    entries: Mutex<HashMap<u32, Entry>>,
    started: Mutex<std::time::Instant>,
    loaded: Mutex<bool>,
    last_cpu: Mutex<Option<(std::time::Instant, f64)>>,
}

impl TaskTable {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
            entries: Mutex::new(HashMap::new()),
            started: Mutex::new(std::time::Instant::now()),
            loaded: Mutex::new(false),
            last_cpu: Mutex::new(None),
        }
    }

    /// Process-wide table behind `ps`/`top`/`kill`.
    pub fn global() -> &'static TaskTable {
        static TABLE: OnceLock<TaskTable> = OnceLock::new();
        TABLE.get_or_init(TaskTable::new)
    }

    // ── lifecycle ───────────────────────────────────────────────────────

    /// Register a task and return its id. The returned `Arc<AtomicBool>` is
    /// the cancellation handle workers poll.
    pub fn spawn(&self, kind: &str, name: &str, provider: &str) -> (u32, Arc<AtomicBool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        let task = Task {
            id,
            kind: kind.to_string(),
            name: name.to_string(),
            state: TaskState::Running,
            progress: 0.0,
            started_at: chrono::Utc::now().to_rfc3339(),
            ended_at: None,
            bytes: 0,
            provider: provider.to_string(),
            error: None,
        };
        self.insert(Entry {
            task,
            cancel: Arc::clone(&cancel),
        });
        (id, cancel)
    }

    /// Register a task that starts `pending` (queued behind a worker slot).
    pub fn spawn_pending(&self, kind: &str, name: &str, provider: &str) -> u32 {
        let (id, _) = self.spawn(kind, name, provider);
        self.update(id, |t| t.state = TaskState::Pending);
        id
    }

    fn insert(&self, entry: Entry) {
        let mut map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        map.insert(entry.task.id, entry);
        let mut finished: Vec<u32> = map
            .values()
            .filter(|e| e.task.state.is_terminal())
            .map(|e| e.task.id)
            .collect();
        if map.len() > MAX_TASKS {
            // Drop the oldest terminal rows first; live rows are never evicted.
            finished.sort_unstable();
            while map.len() > MAX_TASKS && !finished.is_empty() {
                map.remove(&finished.remove(0));
            }
        }
    }

    /// Read one task.
    pub fn get(&self, id: u32) -> Option<Task> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .map(|e| e.task.clone())
    }

    /// The cancellation handle for a task (what `kill` flips).
    pub fn cancel_handle(&self, id: u32) -> Option<Arc<AtomicBool>> {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .map(|e| Arc::clone(&e.cancel))
    }

    /// Mutable access to a row — the only way tasks report progress.
    pub fn update<F: FnOnce(&mut Task)>(&self, id: u32, f: F) -> Option<Task> {
        let mut map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let entry = map.get_mut(&id)?;
        f(&mut entry.task);
        Some(entry.task.clone())
    }

    /// Report progress: `progress` (0..=1) and cumulative `bytes`.
    pub fn progress(&self, id: u32, progress: f64, bytes: u64) {
        self.update(id, |t| {
            t.progress = progress.clamp(0.0, 1.0);
            t.bytes = bytes;
        });
    }

    /// Move a task to a terminal state. Idempotent: a killed task stays killed.
    pub fn finish(&self, id: u32, state: TaskState, error: Option<String>) -> Option<Task> {
        let mut map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let entry = map.get_mut(&id)?;
        if entry.task.state.is_terminal() {
            return Some(entry.task.clone());
        }
        entry.task.state = state;
        entry.task.ended_at = Some(chrono::Utc::now().to_rfc3339());
        entry.task.error = error;
        Some(entry.task.clone())
    }

    /// `kill <id>` — flip the cancellation handle and mark the row `killed`.
    pub fn kill(&self, id: u32) -> Result<Task, String> {
        let mut map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let entry = map
            .get_mut(&id)
            .ok_or_else(|| format!("not found: task {id}"))?;
        if entry.task.state.is_terminal() {
            return Err(format!(
                "invalid: task {id} is already {}",
                entry.task.state.as_str()
            ));
        }
        entry.cancel.store(true, Ordering::SeqCst);
        entry.task.state = TaskState::Killed;
        entry.task.ended_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(entry.task.clone())
    }

    /// True once `kill` (or an external cancel) fired for this task.
    pub fn is_cancelled(&self, id: u32) -> bool {
        self.entries
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .map(|e| e.cancel.load(Ordering::SeqCst))
            .unwrap_or(true)
    }

    /// Drop every finished row. Returns how many went away.
    pub fn clear_finished(&self) -> usize {
        let mut map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let before = map.len();
        map.retain(|_, e| !e.task.state.is_terminal());
        before - map.len()
    }

    // ── views ───────────────────────────────────────────────────────────

    /// All rows, live first then newest-finished, each group newest first.
    pub fn list(&self) -> Vec<Task> {
        let map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let mut rows: Vec<Task> = map.values().map(|e| e.task.clone()).collect();
        drop(map);
        rows.sort_by(|a, b| {
            a.state
                .is_terminal()
                .cmp(&b.state.is_terminal())
                .then(b.started_at.cmp(&a.started_at))
                .then(b.id.cmp(&a.id))
        });
        rows
    }

    /// State counts.
    pub fn counts(&self) -> TaskCounts {
        let map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let mut counts = TaskCounts {
            total: map.len(),
            ..TaskCounts::default()
        };
        for e in map.values() {
            match e.task.state {
                TaskState::Pending => counts.pending += 1,
                TaskState::Running => counts.running += 1,
                TaskState::Done => counts.done += 1,
                TaskState::Failed => counts.failed += 1,
                TaskState::Killed => counts.killed += 1,
            }
        }
        counts
    }

    /// `ps(1)`.
    pub fn ps(&self) -> PsSnapshot {
        let uptime_ms = self
            .started
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .elapsed()
            .as_millis() as u64;
        PsSnapshot {
            tasks: self.list(),
            counts: self.counts(),
            pid: std::process::id() as u64,
            uptime_ms,
        }
    }

    // ── `top` system stats ──────────────────────────────────────────────

    /// `top(1)` — live system stats over the process table. Reads
    /// `/proc/loadavg`, `/proc/self/statm`, `/proc/self/stat` and
    /// `/proc/meminfo`; each source degrades to zeros with its `source`
    /// marked `unavailable` rather than inventing numbers.
    pub fn top(&self) -> TopSnapshot {
        let uptime_ms = self
            .started
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .elapsed()
            .as_millis() as u64;
        TopSnapshot {
            uptime_ms,
            load: read_loadavg(),
            mem: read_meminfo(),
            cpu_percent: self.process_cpu_percent(),
            counts: self.counts(),
            tasks: self.list(),
        }
    }

    /// CPU used by this process since the previous call, as 0..=100 of one
    /// core. First call reports 0 (there is no delta to measure yet).
    fn process_cpu_percent(&self) -> f64 {
        let Some(jiffies) = read_process_cpu_jiffies() else {
            return 0.0;
        };
        let mut last = self.last_cpu.lock().unwrap_or_else(|p| p.into_inner());
        let now = std::time::Instant::now();
        let pct = match *last {
            Some((prev_at, prev_j)) => {
                let dt = now.duration_since(prev_at).as_secs_f64();
                let dj = jiffies - prev_j;
                if dt > 0.0 && dj >= 0.0 {
                    ((dj / CLOCK_TICKS_PER_SEC) / dt * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            None => 0.0,
        };
        *last = Some((now, jiffies));
        pct
    }

    // ── persistence ─────────────────────────────────────────────────────

    /// Mirror the table into `compute_tasks`. Returns rows written.
    pub fn persist(&self, db: &Database) -> Result<usize, String> {
        let rows = self.list();
        let count = rows.len();
        let tx = db.begin_write().map_err(|e| e.to_string())?;
        {
            let mut table = tx
                .open_table(Database::get_compute_tasks_table())
                .map_err(|e| e.to_string())?;
            for row in &rows {
                let key = format!("{:08}", row.id);
                let json = serde_json::to_string(row).map_err(|e| e.to_string())?;
                table
                    .insert(key.as_str(), json.as_str())
                    .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(count)
    }

    /// Restore rows from `compute_tasks` (once per process). Live rows win
    /// over stored ones so a restart does not resurrect a finished task.
    pub fn load(&self, db: &Database) -> Result<usize, String> {
        {
            let mut loaded = self.loaded.lock().unwrap_or_else(|p| p.into_inner());
            if *loaded {
                return Ok(0);
            }
            *loaded = true;
        }
        let tx = db.begin_read().map_err(|e| e.to_string())?;
        let table = tx
            .open_table(Database::get_compute_tasks_table())
            .map_err(|e| e.to_string())?;
        let mut restored = Vec::new();
        for entry in table.iter().map_err(|e| e.to_string())? {
            let (key, value) = entry.map_err(|e| e.to_string())?;
            let id: u32 = key.value().parse().unwrap_or(0);
            if id == 0 {
                continue;
            }
            if let Ok(task) = serde_json::from_str::<Task>(value.value()) {
                restored.push((id, task));
            }
        }
        drop(table);
        drop(tx);

        let mut map = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let mut restored_count = 0;
        for (id, task) in restored {
            if map.contains_key(&id) {
                continue;
            }
            map.insert(
                id,
                Entry {
                    task,
                    cancel: Arc::new(AtomicBool::new(false)),
                },
            );
            restored_count += 1;
            self.next_id
                .fetch_max(id.wrapping_add(1), Ordering::Relaxed);
        }
        Ok(restored_count)
    }

    /// `load` + `persist` in one call, for handlers that only have a request
    /// coming in with a `&Database`.
    pub fn refresh(&self, db: &Database) {
        if let Err(e) = self.load(db) {
            log::debug!("could not restore compute_tasks: {e}");
        }
        if let Err(e) = self.persist(db) {
            log::debug!("could not persist compute_tasks: {e}");
        }
    }
}

/// Convenience: `ps` + restore/persist when a database is in hand.
///
/// Repair/scrub/GC rows are merged in, so this is the one process table —
/// the shell, `GET /api/os/ps` and the task panel all read it.
pub fn ps(db: Option<&Database>) -> PsSnapshot {
    if let Some(db) = db {
        TaskTable::global().refresh(db);
    }
    let mut snap = TaskTable::global().ps();
    let (tasks, counts) = merged_tasks(snap.tasks);
    snap.tasks = tasks;
    snap.counts = counts;
    snap
}

/// Convenience: `top` + restore/persist when a database is in hand.
///
/// Same merged table as `ps`, plus the live system stats.
pub fn top(db: Option<&Database>) -> TopSnapshot {
    if let Some(db) = db {
        TaskTable::global().refresh(db);
    }
    let mut snap = TaskTable::global().top();
    let (tasks, counts) = merged_tasks(snap.tasks);
    snap.tasks = tasks;
    snap.counts = counts;
    snap
}

/// Convenience: `kill <id>`.
pub fn kill(id: u32) -> Result<Task, String> {
    if id >= REPAIR_ID_BASE {
        return Err(
            "unsupported: repair/scrub/gc tasks run detached — track them with \
             `GET /api/repair/tasks` or `repair` in cybsh; they finish on their own"
                .to_string(),
        );
    }
    TaskTable::global().kill(id)
}

// ─── repair/scrub/GC bridge (AGENT-7 items 2+10) ────────────────────────────

/// First id issued to a mirrored repair/scrub/GC row. OS rows start at 1 and
/// the table holds at most `MAX_TASKS` live rows, so the two spaces never
/// collide. `kill` refuses these ids (see above): the workers are detached
/// threads with no cancellation handle, and pretending otherwise would be a
/// fake success.
pub const REPAIR_ID_BASE: u32 = 2_000_000_000;

/// FNV-1a over the repair task's string id, into `0..range`.
fn fnv1a_32(text: &str, range: u32) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash % range.max(1)
}

fn repair_state_of(raw: &str) -> TaskState {
    match raw {
        "done" => TaskState::Done,
        "error" => TaskState::Failed,
        "killed" => TaskState::Killed,
        "pending" => TaskState::Pending,
        _ => TaskState::Running,
    }
}

/// Repair/scrub/GC runs as `ps`/`top` rows.
///
/// `cybermanju_sync::repair` keeps its own lightweight registry (string
/// ids, no cancel handles) while the OS table owns numeric ids and
/// cancellation. Rather than merging the registries — which would couple
/// every repair worker to the OS table — `ps`/`top` read both and present
/// one process table. Rows are derived on the fly (never persisted into
/// `compute_tasks`, never killable) with stable synthetic ids, so a
/// repeated `ps` shows the same row for the same run.
pub fn repair_rows() -> Vec<Task> {
    let live: Vec<u32> = TaskTable::global().list().iter().map(|t| t.id).collect();
    let mut rows = Vec::new();
    for info in cybermanju_sync::repair::tasks() {
        let mut id = REPAIR_ID_BASE + fnv1a_32(&info.id, 100_000_000);
        while live.contains(&id) || rows.iter().any(|t: &Task| t.id == id) {
            id = id.wrapping_add(1);
        }
        let state = repair_state_of(&info.state);
        let name = info.detail.clone().unwrap_or_else(|| info.kind.clone());
        rows.push(Task {
            id,
            kind: info.kind.clone(),
            name,
            state,
            progress: info.progress.clamp(0.0, 1.0),
            started_at: info.started_at.clone(),
            ended_at: info.finished_at.clone(),
            bytes: 0,
            provider: "repair".to_string(),
            error: if state == TaskState::Failed {
                info.detail.clone()
            } else {
                None
            },
        });
    }
    rows.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.id.cmp(&a.id)));
    rows
}

/// `ps` + repair rows: the single process table every surface reads.
fn merged_tasks(base: Vec<Task>) -> (Vec<Task>, TaskCounts) {
    let mut tasks = base;
    tasks.extend(repair_rows());
    let mut counts = TaskCounts {
        total: tasks.len(),
        ..TaskCounts::default()
    };
    for t in &tasks {
        match t.state {
            TaskState::Pending => counts.pending += 1,
            TaskState::Running => counts.running += 1,
            TaskState::Done => counts.done += 1,
            TaskState::Failed => counts.failed += 1,
            TaskState::Killed => counts.killed += 1,
        }
    }
    // Live rows first, newest first — same order as `TaskTable::list`.
    tasks.sort_by(|a, b| {
        a.state
            .is_terminal()
            .cmp(&b.state.is_terminal())
            .then(b.started_at.cmp(&a.started_at))
            .then(b.id.cmp(&a.id))
    });
    (tasks, counts)
}

/// RFC3339 → epoch millis (best effort).
fn parse_rfc3339_ms(raw: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.timestamp_millis().max(0) as u64)
}

/// `/proc/loadavg`.
pub fn read_loadavg() -> LoadAvg {
    if PROC_STATS {
        if let Ok(raw) = std::fs::read_to_string("/proc/loadavg") {
            let mut it = raw.split_whitespace();
            let (l1, l5, l15) = (it.next(), it.next(), it.next());
            if let (Some(a), Some(b), Some(c)) = (l1, l5, l15) {
                if let (Ok(a), Ok(b), Ok(c)) =
                    (a.parse::<f64>(), b.parse::<f64>(), c.parse::<f64>())
                {
                    return LoadAvg {
                        load1: a,
                        load5: b,
                        load15: c,
                        source: "proc".to_string(),
                    };
                }
            }
        }
    }
    LoadAvg {
        source: "unavailable".to_string(),
        ..LoadAvg::default()
    }
}

/// `/proc/self/statm` + `/proc/meminfo`.
pub fn read_meminfo() -> MemInfo {
    let mut info = MemInfo::default();
    if PROC_STATS {
        let page = 4096u64;
        if let Ok(raw) = std::fs::read_to_string("/proc/self/statm") {
            let mut it = raw.split_whitespace();
            if let (Some(total_pages), Some(resident), Some(shared)) =
                (it.next(), it.next(), it.next())
            {
                if let (Ok(_total), Ok(r), Ok(s)) = (
                    total_pages.parse::<u64>(),
                    resident.parse::<u64>(),
                    shared.parse::<u64>(),
                ) {
                    info.rss_bytes = r * page;
                    info.shared_bytes = s * page;
                }
            }
        }
        if let Ok(raw) = std::fs::read_to_string("/proc/meminfo") {
            for line in raw.lines() {
                let mut parts = line.split(':');
                let key = parts.next().unwrap_or_default();
                let value = parts.next().unwrap_or_default().trim();
                let kb: u64 = value
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                match key {
                    "MemTotal" => info.total_bytes = kb * 1024,
                    "MemAvailable" => info.available_bytes = kb * 1024,
                    _ => {}
                }
            }
        }
    }
    info
}

/// utime+stime of this process in clock ticks.
fn read_process_cpu_jiffies() -> Option<f64> {
    if !PROC_STATS {
        return None;
    }
    let raw = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The comm field can contain spaces/parens — everything after the last
    // ')' is positional.
    let rest = raw.rsplit(')').next()?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After `) state`, field 14 (utime) is index 11 here and 15 (stime) 12.
    let utime: f64 = fields.get(11)?.parse().ok()?;
    let stime: f64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> TaskTable {
        TaskTable::new()
    }

    #[test]
    fn spawn_list_finish_and_counts() {
        let t = fresh();
        let (a, _) = t.spawn("compute", "thumbnail batch", "local");
        let (b, _) = t.spawn("sync", "sync start --config gdrive", "gdrive");
        let rows = t.list();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.state == TaskState::Running));

        t.finish(a, TaskState::Done, None).expect("finish");
        let counts = t.counts();
        assert_eq!((counts.running, counts.done, counts.total), (1, 1, 2));
        assert!(t.get(a).unwrap().ended_at.is_some());
        // Live rows sort before terminal ones.
        assert_eq!(t.list()[0].id, b);
        assert_ne!(a, b);
    }

    #[test]
    fn kill_cancels_the_handle_and_refuses_finished_tasks() {
        let t = fresh();
        let (id, token) = t.spawn("sync", "sync start", "gdrive");
        assert!(!token.load(Ordering::SeqCst));
        let killed = t.kill(id).expect("kill");
        assert_eq!(killed.state, TaskState::Killed);
        assert!(token.load(Ordering::SeqCst), "cancel handle flipped");
        assert!(t.is_cancelled(id));
        let err = t.kill(id).expect_err("second kill");
        assert!(err.starts_with("invalid:"), "got {err}");
        assert!(t.kill(9_999).is_err());
    }

    #[test]
    fn progress_bytes_and_clear_finished() {
        let t = fresh();
        let (id, _) = t.spawn("gc", "gc pass", "local");
        t.progress(id, 0.25, 1_024);
        t.progress(id, 2.5, 4_096); // clamped
        let task = t.get(id).unwrap();
        assert!((task.progress - 1.0).abs() < f64::EPSILON);
        assert_eq!(task.bytes, 4_096);
        t.finish(id, TaskState::Done, None);
        assert_eq!(t.clear_finished(), 1);
        assert_eq!(t.counts().total, 0);
        assert!(t.get(id).is_none());
    }

    #[test]
    fn old_terminal_rows_are_pruned_but_live_rows_survive() {
        let t = fresh();
        let live = t.spawn("compute", "long job", "local").0;
        for i in 0..(MAX_TASKS + 5) {
            let (id, _) = t.spawn("shell", &format!("cmd {i}"), "local");
            t.finish(id, TaskState::Done, None);
        }
        {
            let map = t.entries.lock().unwrap();
            assert!(map.len() <= MAX_TASKS, "pruned to MAX_TASKS");
            assert!(map.contains_key(&live), "live row kept");
        }
        assert!(t.get(live).is_some());
    }

    #[test]
    fn repair_tasks_appear_in_ps_but_are_not_killable() {
        let id = cybermanju_sync::repair::begin_task("repair", "testing bridge");
        let rows = repair_rows();
        let row = rows
            .iter()
            .find(|t| t.kind == "repair")
            .expect("mirrored row");
        assert!(row.id >= REPAIR_ID_BASE, "synthetic id: {}", row.id);
        assert_eq!(row.provider, "repair");
        assert_eq!(row.state, TaskState::Running);

        let snap = fresh();
        let (tasks, counts) = merged_tasks(snap.list());
        assert!(tasks.iter().any(|t| t.id >= REPAIR_ID_BASE));
        assert_eq!(counts.total, tasks.len());

        let err = kill(row.id).expect_err("detached rows cannot be killed");
        assert!(err.starts_with("unsupported:"), "got {err}");

        cybermanju_sync::repair::finish_task(&id, "done", Some("ok".to_string()));
        let rows = repair_rows();
        // Finished rows linger until the next begin_task prunes them.
        if let Some(row) = rows.iter().find(|t| t.provider == "repair") {
            assert!(row.state.is_terminal() || row.state == TaskState::Running);
        }
    }

    #[test]
    fn rows_round_trip_through_compute_tasks() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cyb-test.db");
        let db = Database::new(path.to_str().expect("utf8")).expect("db");

        let t = fresh();
        let (id, _) = t.spawn("index", "search reindex", "local");
        t.progress(id, 0.5, 77);
        t.finish(id, TaskState::Done, None);
        assert_eq!(t.persist(&db).expect("persist"), 1);

        // A second table (as after a restart) restores the row.
        let restored = fresh();
        assert_eq!(restored.load(&db).expect("load"), 1);
        let task = restored.get(id).expect("restored");
        assert_eq!(task.kind, "index");
        assert_eq!(task.state, TaskState::Done);
        assert_eq!(task.bytes, 77);
        // Loading again is a no-op.
        assert_eq!(restored.load(&db).expect("load again"), 0);
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn top_reads_real_system_stats() {
        let t = fresh();
        let top = t.top();
        // `/proc/loadavg` is world-readable on desktop Linux; Android's
        // hidepid denies it, and then the number is reported as unavailable
        // rather than invented.
        match top.load.source.as_str() {
            "proc" => assert!(
                top.load.load1 >= 0.0 && top.load.load15 >= 0.0,
                "load averages parsed"
            ),
            "unavailable" => {
                assert_eq!(top.load.load1, 0.0);
                assert_eq!(top.load.load15, 0.0);
            }
            other => panic!("unknown load source: {other}"),
        }
        assert!(top.mem.rss_bytes > 0, "statm is readable");
        assert!(top.mem.total_bytes > 0, "meminfo is readable");
        // First sample has no delta; the second one does.
        assert_eq!(top.cpu_percent, 0.0);
        std::thread::sleep(std::time::Duration::from_millis(120));
        let _ = t.top();
        std::thread::sleep(std::time::Duration::from_millis(120));
        let second = t.top();
        assert!(second.cpu_percent >= 0.0 && second.cpu_percent <= 100.0);
    }
}
