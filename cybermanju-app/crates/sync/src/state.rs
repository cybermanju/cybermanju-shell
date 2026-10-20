// CyberManju OS — Shared Sync State
// Live progress + cancellation flag shared between the sync pipeline and
// whichever front-end is driving it (Tauri IPC, REST or WASM).
//
// <<< AGENT-2 RUN REGISTRY (item 5): every run owns its own `SyncState`,
// so two runs can never clobber each other's progress or cancel flag.
// `RunRegistry` tracks live + recent runs by id — the REST job API and the
// `sync_runs` history both hang off it.

use cybermanju_types::sync::{SyncProgress, SyncResult, SyncStatus};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub struct SyncState {
    pub progress: Mutex<SyncProgress>,
    pub cancel_flag: AtomicBool,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            progress: Mutex::new(SyncProgress {
                total_files: 0,
                processed_files: 0,
                current_file: None,
                status: SyncStatus::Idle,
                bytes_uploaded: 0,
                errors: Vec::new(),
                started_at: None,
                estimated_remaining_seconds: None,
            }),
            cancel_flag: AtomicBool::new(false),
        }
    }

    /// Run `f` against the progress struct, recovering from a poisoned lock
    /// so a panicking worker can never wedge every later status read.
    fn with_progress<F: FnOnce(&mut SyncProgress)>(&self, f: F) {
        match self.progress.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    /// Snapshot of the current progress, with an ETA derived from elapsed time.
    pub fn snapshot(&self) -> SyncProgress {
        let progress = match self.progress.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };

        let elapsed = progress
            .started_at
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_seconds() as f64)
            .unwrap_or(0.0);

        let estimated_remaining_seconds =
            if progress.processed_files > 0 && progress.total_files > 0 {
                let avg_per_file = elapsed / progress.processed_files as f64;
                Some(avg_per_file * (progress.total_files - progress.processed_files) as f64)
            } else {
                None
            };

        SyncProgress {
            estimated_remaining_seconds,
            ..progress
        }
    }

    /// Reset progress for a new run of `total` files.
    ///
    /// Deliberately does **not** touch the cancel flag: clearing it here was
    /// how a cancel used to be silently un-cancelled (a second run's reset
    /// flipped the first run's flag back to false). The flag belongs to the
    /// run, and runs get fresh states via [`RunRegistry::begin`].
    pub fn reset(&self, total: u32) {
        self.with_progress(|p| {
            p.total_files = total;
            p.processed_files = 0;
            p.current_file = None;
            p.status = SyncStatus::Scanning;
            p.bytes_uploaded = 0;
            p.errors.clear();
            p.started_at = None;
            p.estimated_remaining_seconds = None;
        });
    }

    /// Arm a state for a run that does not exist yet: clears any cancel
    /// left by the *previous* run of this (shared) state. Safe by ordering —
    /// callers do this **before** registering the run, so no cancel can
    /// possibly target the run being armed; `RunRegistry::begin` refuses to
    /// arm a state that still has an active run, which is what used to make
    /// concurrent runs clobber each other.
    pub fn prepare_run(&self) {
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    pub fn set_total(&self, total: u32) {
        self.with_progress(|p| p.total_files = total);
    }

    pub fn set_status(&self, status: SyncStatus) {
        self.with_progress(|p| p.status = status);
    }

    pub fn set_current(&self, current: Option<String>) {
        self.with_progress(|p| p.current_file = current);
    }

    pub fn set_started_at(&self, started_at: Option<String>) {
        self.with_progress(|p| p.started_at = started_at);
    }

    pub fn set_processed(&self, processed: u32) {
        self.with_progress(|p| p.processed_files = processed);
    }

    pub fn inc_processed(&self) {
        self.with_progress(|p| p.processed_files = p.processed_files.saturating_add(1));
    }

    pub fn add_bytes(&self, bytes: u64) {
        self.with_progress(|p| p.bytes_uploaded = p.bytes_uploaded.saturating_add(bytes));
    }

    pub fn add_error(&self, error: String) {
        self.with_progress(|p| p.errors.push(error));
    }

    /// Request cancellation of the running sync.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
        self.with_progress(|p| {
            p.status = SyncStatus::Cancelled;
            p.current_file = None;
        });
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }
}

impl Default for SyncState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Runs & registry ─────────────────────────────────────────────────

/// Terminal bookkeeping for one run: when it finished and what came out.
#[derive(Debug, Clone)]
pub struct SyncRunOutcome {
    pub finished_at: String,
    pub result: Option<SyncResult>,
    pub error: Option<String>,
}

/// One registered sync run: its id, the config it drives and the progress
/// state every poller of this run reads.
pub struct SyncRun {
    pub run_id: String,
    pub config_id: String,
    /// JWT `user_id` that started the run (`"system"` for the auto-sync
    /// scheduler, `"local"` for the trusted Tauri path). Cancel/poll entry
    /// points check it (P0-10).
    pub owner_id: String,
    pub state: Arc<SyncState>,
    pub started_at: String,
    outcome: Mutex<Option<SyncRunOutcome>>,
}

impl SyncRun {
    pub fn is_finished(&self) -> bool {
        self.outcome
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_some()
    }

    pub fn outcome(&self) -> Option<SyncRunOutcome> {
        self.outcome
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Request cancellation of this run only — scoped, never global.
    pub fn cancel(&self) {
        self.state.cancel();
    }
}

/// How many finished runs stay in memory. Older lookups fall through to the
/// `sync_runs` table, which keeps its own 20-row history.
const MAX_LIVE_RUNS: usize = 16;

/// Unpredictable run ids (P0-10): 128 bits from the OS RNG, hex-encoded.
/// The previous `run-<timestamp>-<seq>` shape was guessable across users.
fn new_run_id() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 16];
    rand_core::OsRng.fill_bytes(&mut bytes);
    let mut out = String::with_capacity(36);
    out.push_str("run-");
    for b in bytes {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

struct RegistryInner {
    runs: HashMap<String, Arc<SyncRun>>,
    order: VecDeque<String>,
    latest: Option<String>,
    /// Recently evicted ids, oldest first (P0-21): an evicted poll answers
    /// `expired:`, never `not_found:` — the job existed, its window passed.
    /// Bounded (64); restarts lose it, but completed runs persist in the
    /// `sync_runs` history table, so post-restart polls still resolve.
    expired: VecDeque<String>,
}

/// How many evicted ids are remembered for `expired:` answers.
const MAX_EXPIRED_IDS: usize = 64;

/// Process-wide registry of sync runs (item 5 / item 14).
///
/// A singleton is deliberate: the desktop app, the embedded web dashboard
/// and the Docker server all live in one process, and the previous design —
/// one un-scoped `SyncState` mutated by every run — is exactly what let two
/// runs clobber each other.
pub struct RunRegistry {
    inner: Mutex<RegistryInner>,
}

impl RunRegistry {
    fn new() -> Self {
        Self {
            inner: Mutex::new(RegistryInner {
                runs: HashMap::new(),
                order: VecDeque::new(),
                latest: None,
                expired: VecDeque::new(),
            }),
        }
    }

    /// The process-wide registry.
    pub fn global() -> &'static RunRegistry {
        static GLOBAL: OnceLock<RunRegistry> = OnceLock::new();
        GLOBAL.get_or_init(RunRegistry::new)
    }

    /// Register a new run driven by `state`.
    ///
    /// Ordering matters (see [`SyncState::prepare_run`]): this refuses a
    /// state that already has an **active** run — the old un-cancel bug —
    /// and only then arms and resets it, so a cancel can never be cleared
    /// out from under a run that already exists.
    ///
    /// Trusted-local entry point: the run is owned by `"local"`. REST
    /// callers must use [`RunRegistry::begin_owned`] with the JWT `user_id`.
    pub fn begin(
        &self,
        config_id: &str,
        state: Arc<SyncState>,
        total: u32,
    ) -> Result<Arc<SyncRun>, String> {
        self.begin_owned(config_id, state, total, "local")
    }

    /// Owner-bound registration (P0-10): the run id is unpredictable
    /// ([`new_run_id`]) and the owner is stored for every later
    /// cancel/poll check.
    pub fn begin_owned(
        &self,
        config_id: &str,
        state: Arc<SyncState>,
        total: u32,
        owner_id: &str,
    ) -> Result<Arc<SyncRun>, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let active_on_state = inner
            .runs
            .values()
            .any(|run| Arc::ptr_eq(&run.state, &state) && !run.is_finished());
        if active_on_state {
            return Err("a sync run is already in progress".to_string());
        }

        state.prepare_run();
        state.reset(total);

        let run_id = new_run_id();
        let run = Arc::new(SyncRun {
            run_id: run_id.clone(),
            config_id: config_id.to_string(),
            owner_id: owner_id.to_string(),
            state,
            started_at: chrono::Utc::now().to_rfc3339(),
            outcome: Mutex::new(None),
        });
        inner.runs.insert(run_id.clone(), Arc::clone(&run));
        inner.order.push_back(run_id.clone());
        inner.latest = Some(run_id);
        self.evict(&mut inner);
        Ok(run)
    }

    pub fn get(&self, run_id: &str) -> Option<Arc<SyncRun>> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.runs.get(run_id).cloned()
    }

    /// Owner-bound lookup (P0-10): `None` for unknown ids AND for runs
    /// owned by someone else (indistinguishable on purpose). Empty
    /// `requester` is the trusted local path and sees everything.
    pub fn get_for(&self, run_id: &str, requester: &str, is_admin: bool) -> Option<Arc<SyncRun>> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .runs
            .get(run_id)
            .cloned()
            .filter(|run| requester.is_empty() || is_admin || run.owner_id == requester)
    }

    /// Most recently started run (finished or not).
    pub fn latest(&self) -> Option<Arc<SyncRun>> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .latest
            .as_ref()
            .and_then(|id| inner.runs.get(id))
            .cloned()
    }

    /// Most recently started run owned by `requester` (P0-10): what an
    /// owner-scoped "cancel latest" targets. Empty `requester` (trusted
    /// local) or `is_admin` keeps the legacy global-latest behaviour.
    pub fn latest_for(&self, requester: &str, is_admin: bool) -> Option<Arc<SyncRun>> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if requester.is_empty() || is_admin {
            return inner
                .latest
                .as_ref()
                .and_then(|id| inner.runs.get(id))
                .cloned();
        }
        inner
            .order
            .iter()
            .rev()
            .filter_map(|id| inner.runs.get(id))
            .find(|run| run.owner_id == requester)
            .cloned()
    }

    /// Mark a run finished. Idempotent — first terminal state wins.
    pub fn finish(&self, run: &Arc<SyncRun>, result: Option<SyncResult>, error: Option<String>) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        {
            let mut outcome = run.outcome.lock().unwrap_or_else(|p| p.into_inner());
            if outcome.is_none() {
                *outcome = Some(SyncRunOutcome {
                    finished_at: chrono::Utc::now().to_rfc3339(),
                    result,
                    error,
                });
            }
        }
        self.evict(&mut inner);
    }

    /// Cancel one run by id, or the latest run when `run_id` is `None`.
    ///
    /// Returns `false` only for an unknown id — "nothing to cancel" is a
    /// success (the REST route and its idempotency test rely on that).
    ///
    /// Trusted-local entry point (no owner check). REST callers must use
    /// [`RunRegistry::cancel_as`].
    pub fn cancel(&self, run_id: Option<&str>) -> bool {
        self.cancel_as(run_id, "", true)
    }

    /// Owner-bound cancel (P0-10): an explicit id cancels only the caller's
    /// own run (admins: any run) — strangers get `false` (unknown-id shape,
    /// no oracle). `None` cancels the caller's latest own run (admins keep
    /// the legacy global-latest); "nothing running" is success.
    pub fn cancel_as(&self, run_id: Option<&str>, requester: &str, is_admin: bool) -> bool {
        let target = {
            let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
            match run_id {
                Some(id) => match inner.runs.get(id) {
                    Some(run) if requester.is_empty() || is_admin || run.owner_id == requester => {
                        Some(Arc::clone(run))
                    }
                    _ => return false,
                },
                None => {
                    if requester.is_empty() || is_admin {
                        inner
                            .latest
                            .as_ref()
                            .and_then(|id| inner.runs.get(id))
                            .cloned()
                    } else {
                        inner
                            .order
                            .iter()
                            .rev()
                            .filter_map(|id| inner.runs.get(id))
                            .find(|run| run.owner_id == requester)
                            .cloned()
                    }
                }
            }
        };
        if let Some(run) = target {
            if !run.is_finished() {
                run.cancel();
            }
        }
        true
    }

    /// Drop finished runs past [`MAX_LIVE_RUNS`], never the latest one.
    fn evict(&self, inner: &mut RegistryInner) {
        while inner.order.len() > MAX_LIVE_RUNS {
            let evictable = inner.order.iter().position(|id| {
                Some(id) != inner.latest.as_ref()
                    && inner
                        .runs
                        .get(id)
                        .map(|run| run.is_finished())
                        .unwrap_or(true)
            });
            match evictable {
                Some(idx) => {
                    if let Some(id) = inner.order.remove(idx) {
                        inner.runs.remove(&id);
                        inner.expired.push_back(id);
                        while inner.expired.len() > MAX_EXPIRED_IDS {
                            inner.expired.pop_front();
                        }
                    }
                }
                None => break,
            }
        }
    }

    /// Whether `run_id` left through eviction (P0-21): pollers translate
    /// this to `expired:`, not `not_found:`.
    pub fn was_evicted(&self, run_id: &str) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.expired.iter().any(|id| id == run_id)
    }
}
