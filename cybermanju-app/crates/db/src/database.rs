// CyberManju OS — Database (crates/db/src/database.rs).
// Role: redb store (cybermanju.db): tables + sessions/memories.
// Exports: SYNC_RUN_HISTORY_LIMIT, Database, new, begin_read, begin_write, get_files_table, get_accounts_table, get_collections_table.
// Integrates: anyhow, cybermanju_types, base64, rand_core.
// Contracts: signals: AGENT-.
use anyhow::Result;
use cybermanju_types::schema::{
    AuditEntry, DeviceRow, FileNode, FileVersion, OpRow, ShareLink, TodoRow, TrashItem,
};
use cybermanju_types::sync::{SyncFile, SyncRunRecord};
use redb::{
    Database as RedbDatabase, ReadTransaction, ReadableTable, TableDefinition, WriteTransaction,
};

const MAX_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
const FILES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("files");
const ACCOUNTS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("accounts");
const COLLECTIONS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("collections");
const COLLECTION_ITEMS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("collection_items");
const FACE_GROUPS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("face_groups");
const LOOSE_GROUPS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("loose_groups");
const ENCRYPTION_KEYS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("encryption_keys");
const LOCATIONS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("locations");
const USERS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("users");
const USER_FILE_PERMS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("user_file_perms");
const SYNC_CONFIGS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("sync_configs");
const PARENT_INDEX_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("parent_index");
const TRASH_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("trash");
const AUDIT_LOG_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("audit_log");
const FILE_VERSIONS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("file_versions");
const SHARE_LINKS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("share_links");
// <<< AGENT-2 SYNC PERSISTENCE: remote locators, run history, config
// secrets and the schema marker. See AGENT-2.md items 2, 14. >>>
const SYNC_FILES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("sync_files");
const SYNC_RUNS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("sync_runs");
const SYNC_SECRETS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("sync_secrets");
const SCHEMA_VERSION_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("schema_version");
// <<< CYBERMANJU OS PUSH: tables declared up front by the supervisor so the
// three parallel agents never edit this file. AGENT-6 = disks/volumes/block_map,
// AGENT-7 = scrub_runs/repairs/chunk_refs/leases/provider_health,
// AGENT-8 = compute_tasks/shell_history. Values are JSON strings, same as
// every other table here. See MISSING.md §"Pre-wired before launch". >>>
const DISKS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("disks");
const VOLUMES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("volumes");
const BLOCK_MAP_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("block_map");
const SCRUB_RUNS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("scrub_runs");
const REPAIRS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("repairs");
const CHUNK_REFS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("chunk_refs");
const LEASES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("leases");
const PROVIDER_HEALTH_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("provider_health");
const COMPUTE_TASKS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("compute_tasks");
const SHELL_HISTORY_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("shell_history");
// <<< AI AGENT: configs (no key material — keys live in sync_secrets as
// `agent:key:<config_id>`) and session transcripts. Values are JSON strings,
// same convention as every other table here. >>>
const AGENT_CONFIGS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("agent_configs");
const AGENT_SESSIONS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("agent_sessions");
// <<< SEMANTIC MEMORY: long-term memories (curated text + embedding vector
// as JSON rows). New table = backward compatible: existing DB files gain it
// on open, like every table above. >>>
const AGENT_MEMORIES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("agent_memories");
// Scheduler: recurring `.cybsh` triggers + fire history (Phase 1).
const SCHEDULES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("schedules");
const SCHEDULE_RUNS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("schedule_runs");
// Secrets keystore (Phase 3): password-manager rows. Values are sealed
// (`seal:v1`) under the per-transport vault passphrase; the table stores
// only the sealed blob + metadata. List responses never serialize the blob.
const SECRETS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("secrets");
// Device identity registry (§2B.1): one row per device (`id` = device id),
// carrying the display name + ML-DSA public key. The private half never
// leaves the device; revoked rows stay for history but fail closed.
const DEVICES_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("devices");
// Server-side todos (§4.9 agent bridge): same shape as the browser TodoItem
// plus owner/source, keyed by todo id. Agent tools and the Archive TASKS tab
// share one list per owner.
const TODOS_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("todos");
// Op-log (§2C/P3-2): every mutation appends one JSON row keyed by
// zero-padded `seq` (local append order). The next-seq counter lives in kv
// (`oplog:next_seq`) and advances inside the same write txn as the append,
// so seq assignment is atomic with the row it numbers.
const OPLOG_TABLE: TableDefinition<'static, &'static str, &'static str> =
    TableDefinition::new("oplog");
// Generic key/value blob: vault secrets, app config, shell file content and
// any other session state the browser build keeps inside `.cybermanju`.
// Keys are namespaced by convention (`secret:`, `config:`, `content:`,
// `volume:`), values are raw UTF-8 strings.
const KV_TABLE: TableDefinition<'static, &'static str, &'static str> = TableDefinition::new("kv");

/// Rows kept in `sync_runs` — enough for a UI history page, few enough that
/// the prune scan stays trivial.
pub const SYNC_RUN_HISTORY_LIMIT: usize = 20;

/// Clone shares the underlying redb handle (its own internal locking), so a
/// short-lived clone lets long I/O (verified moves) run without holding the
/// request `RwLock` — same lock-free discipline as the sync pipeline.
#[derive(Clone)]
pub struct Database {
    db: std::sync::Arc<RedbDatabase>,
    /// Single-opener guard (P0-22/P3-2 close-out): one `cybermanju.db` has ONE
    /// opener per path. `Arc`-shared so clones never release it early; the
    /// lockfile is removed when the last clone drops.
    opener_lock: std::sync::Arc<OpenerLock>,
}

/// Single-opener lockfile (`<db-path>.lock`, std-only, no new deps).
///
/// redb serializes storage txns but cannot stop two *processes* (desktop +
/// Docker on the same `/data`) from each opening the file and split-braining
/// the vault. This guard makes the second opener fail closed with
/// `conflict:` — sync across openers happens via op-log/LAN/bundle, never via
/// a shared file. Crash-stale locks recover honestly: a lock whose PID is
/// dead (Linux `/proc` check) is removed and retried once; a live PID — or a
/// platform without `/proc` — stays `conflict:` with the `rm` recovery path.
struct OpenerLock {
    lock_path: std::path::PathBuf,
}

impl OpenerLock {
    fn acquire(db_path: &str) -> anyhow::Result<std::sync::Arc<Self>> {
        let lock_path = std::path::PathBuf::from(format!("{db_path}.lock"));
        match Self::try_create(&lock_path) {
            Ok(()) => Ok(std::sync::Arc::new(Self { lock_path })),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // A lockfile is already there: live opener fails closed,
                // dead (crashed) opener is reaped once, then retried.
                if pid_live(&lock_path) {
                    return Err(anyhow::anyhow!(
                        "conflict: database '{}' is already open by another process (single-opener design — run desktop XOR Docker on the same /data, sync via op-log/LAN/bundle; if no opener is running, remove '{}' after verifying that)",
                        db_path,
                        lock_path.display()
                    ));
                }
                let _ = std::fs::remove_file(&lock_path);
                Self::try_create(&lock_path).map_err(|_| {
                    anyhow::anyhow!(
                        "conflict: database '{}' is already open by another process (lost the lock race — sync via op-log/LAN/bundle)",
                        db_path
                    )
                })?;
                Ok(std::sync::Arc::new(Self { lock_path }))
            }
            Err(e) => Err(anyhow::anyhow!(
                "cannot create opener lock '{}': {e}",
                lock_path.display()
            )),
        }
    }

    fn try_create(lock_path: &std::path::Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)?;
        writeln!(f, "{}", std::process::id())?;
        Ok(())
    }
}

impl Drop for OpenerLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

/// `true` when the PID recorded in `lock_path` is (or may be) still running.
/// Own PID always counts as live (same-process double-open is the same
/// split-brain). On Linux a missing `/proc/<pid>` proves death; platforms
/// without `/proc` fail closed (live) with the documented `rm` recovery.
fn pid_live(lock_path: &std::path::Path) -> bool {
    let content = std::fs::read_to_string(lock_path).unwrap_or_default();
    let pid: u32 = match content.trim().parse() {
        Ok(pid) => pid,
        // Foreign content: never guess — fail closed.
        Err(_) => return true,
    };
    if pid == std::process::id() {
        return true;
    }
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        true
    }
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        // redb creates the FILE but never its parent directories. Create them
        // so callers can pass `<data-dir>/cybermanju.db` on a fresh install
        // (desktop AppImage/Flatpak, Docker) instead of failing with ENOENT.
        let p = std::path::Path::new(path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    anyhow::anyhow!("cannot create database directory {}: {e}", parent.display())
                })?;
            }
        }
        // Single-opener first (P0-22): the second opener fails closed with
        // `conflict:` here, before redb is ever touched. The guard is stored
        // on `Self` so the lockfile lives exactly as long as the database.
        let opener_lock = OpenerLock::acquire(path)?;
        let db = RedbDatabase::create(path)?;
        let write_txn = db.begin_write()?;
        {
            write_txn.open_table(FILES_TABLE)?;
            write_txn.open_table(ACCOUNTS_TABLE)?;
            write_txn.open_table(COLLECTIONS_TABLE)?;
            write_txn.open_table(COLLECTION_ITEMS_TABLE)?;
            write_txn.open_table(FACE_GROUPS_TABLE)?;
            write_txn.open_table(LOOSE_GROUPS_TABLE)?;
            write_txn.open_table(ENCRYPTION_KEYS_TABLE)?;
            write_txn.open_table(LOCATIONS_TABLE)?;
            write_txn.open_table(USERS_TABLE)?;
            write_txn.open_table(USER_FILE_PERMS_TABLE)?;
            write_txn.open_table(SYNC_CONFIGS_TABLE)?;
            write_txn.open_table(PARENT_INDEX_TABLE)?;
            write_txn.open_table(TRASH_TABLE)?;
            write_txn.open_table(AUDIT_LOG_TABLE)?;
            write_txn.open_table(FILE_VERSIONS_TABLE)?;
            write_txn.open_table(SHARE_LINKS_TABLE)?;
            // <<< AGENT-2 SYNC PERSISTENCE >>>
            write_txn.open_table(SYNC_FILES_TABLE)?;
            write_txn.open_table(SYNC_RUNS_TABLE)?;
            write_txn.open_table(SYNC_SECRETS_TABLE)?;
            // <<< CYBERMANJU OS PUSH: open the new tables too >>>
            write_txn.open_table(DISKS_TABLE)?;
            write_txn.open_table(VOLUMES_TABLE)?;
            write_txn.open_table(BLOCK_MAP_TABLE)?;
            write_txn.open_table(SCRUB_RUNS_TABLE)?;
            write_txn.open_table(REPAIRS_TABLE)?;
            write_txn.open_table(CHUNK_REFS_TABLE)?;
            write_txn.open_table(LEASES_TABLE)?;
            write_txn.open_table(PROVIDER_HEALTH_TABLE)?;
            write_txn.open_table(COMPUTE_TASKS_TABLE)?;
            write_txn.open_table(SHELL_HISTORY_TABLE)?;
            // <<< AI AGENT tables >>>
            write_txn.open_table(AGENT_CONFIGS_TABLE)?;
            write_txn.open_table(AGENT_SESSIONS_TABLE)?;
            write_txn.open_table(AGENT_MEMORIES_TABLE)?;
            // Scheduler + secrets keystore (Phase 1/3).
            write_txn.open_table(SCHEDULES_TABLE)?;
            write_txn.open_table(SCHEDULE_RUNS_TABLE)?;
            write_txn.open_table(SECRETS_TABLE)?;
            // Device identity + agent todos + op-log (ANALYSIS 100% pass).
            write_txn.open_table(DEVICES_TABLE)?;
            write_txn.open_table(TODOS_TABLE)?;
            write_txn.open_table(OPLOG_TABLE)?;
            // Generic kv blob (secrets, config, content) — same table the
            // browser build writes so a `.cybermanju` image opens here too.
            write_txn.open_table(KV_TABLE)?;
            {
                let mut schema = write_txn.open_table(SCHEMA_VERSION_TABLE)?;
                if schema.get("schema")?.is_none() {
                    schema.insert("schema", "1")?;
                }
            }
        }
        write_txn.commit()?;
        Ok(Self {
            db: std::sync::Arc::new(db),
            opener_lock,
        })
    }

    /// Path of this database's single-opener lockfile (`<db-path>.lock`).
    /// Exposed so health probes and the P0-22 regression test can assert the
    /// guard without reaching into internals.
    pub fn opener_lock_path(&self) -> &std::path::Path {
        &self.opener_lock.lock_path
    }

    pub fn begin_read(&self) -> Result<ReadTransaction> {
        Ok(self.db.begin_read()?)
    }

    pub fn begin_write(&self) -> Result<WriteTransaction> {
        Ok(self.db.begin_write()?)
    }

    pub fn get_files_table() -> TableDefinition<'static, &'static str, &'static str> {
        FILES_TABLE
    }
    pub fn get_accounts_table() -> TableDefinition<'static, &'static str, &'static str> {
        ACCOUNTS_TABLE
    }
    pub fn get_collections_table() -> TableDefinition<'static, &'static str, &'static str> {
        COLLECTIONS_TABLE
    }
    pub fn get_collection_items_table() -> TableDefinition<'static, &'static str, &'static str> {
        COLLECTION_ITEMS_TABLE
    }
    pub fn get_face_groups_table() -> TableDefinition<'static, &'static str, &'static str> {
        FACE_GROUPS_TABLE
    }
    pub fn get_loose_groups_table() -> TableDefinition<'static, &'static str, &'static str> {
        LOOSE_GROUPS_TABLE
    }
    pub fn get_encryption_keys_table() -> TableDefinition<'static, &'static str, &'static str> {
        ENCRYPTION_KEYS_TABLE
    }
    pub fn get_locations_table() -> TableDefinition<'static, &'static str, &'static str> {
        LOCATIONS_TABLE
    }
    pub fn get_users_table() -> TableDefinition<'static, &'static str, &'static str> {
        USERS_TABLE
    }
    pub fn get_user_file_perms_table() -> TableDefinition<'static, &'static str, &'static str> {
        USER_FILE_PERMS_TABLE
    }
    pub fn get_sync_configs_table() -> TableDefinition<'static, &'static str, &'static str> {
        SYNC_CONFIGS_TABLE
    }
    pub fn get_parent_index_table() -> TableDefinition<'static, &'static str, &'static str> {
        PARENT_INDEX_TABLE
    }
    pub fn get_trash_table() -> TableDefinition<'static, &'static str, &'static str> {
        TRASH_TABLE
    }
    pub fn get_audit_log_table() -> TableDefinition<'static, &'static str, &'static str> {
        AUDIT_LOG_TABLE
    }
    pub fn get_file_versions_table() -> TableDefinition<'static, &'static str, &'static str> {
        FILE_VERSIONS_TABLE
    }
    pub fn get_share_links_table() -> TableDefinition<'static, &'static str, &'static str> {
        SHARE_LINKS_TABLE
    }
    // <<< AGENT-2 SYNC PERSISTENCE >>>
    pub fn get_sync_files_table() -> TableDefinition<'static, &'static str, &'static str> {
        SYNC_FILES_TABLE
    }
    pub fn get_sync_runs_table() -> TableDefinition<'static, &'static str, &'static str> {
        SYNC_RUNS_TABLE
    }
    pub fn get_sync_secrets_table() -> TableDefinition<'static, &'static str, &'static str> {
        SYNC_SECRETS_TABLE
    }
    // <<< CYBERMANJU OS PUSH: accessors for the pre-declared OS tables >>>
    /// AGENT-6 — one `.cybermanju` disk record per provider binding.
    pub fn get_disks_table() -> TableDefinition<'static, &'static str, &'static str> {
        DISKS_TABLE
    }
    /// AGENT-6 — the merged logical volume definition.
    pub fn get_volumes_table() -> TableDefinition<'static, &'static str, &'static str> {
        VOLUMES_TABLE
    }
    /// AGENT-6 — LBA → chunk placement rows.
    pub fn get_block_map_table() -> TableDefinition<'static, &'static str, &'static str> {
        BLOCK_MAP_TABLE
    }
    /// AGENT-7 — one row per scrub pass (per provider).
    pub fn get_scrub_runs_table() -> TableDefinition<'static, &'static str, &'static str> {
        SCRUB_RUNS_TABLE
    }
    /// AGENT-7 — repair findings and their outcome.
    pub fn get_repairs_table() -> TableDefinition<'static, &'static str, &'static str> {
        REPAIRS_TABLE
    }
    /// AGENT-7 — chunk hash → refcount/manifest references (GC authority).
    pub fn get_chunk_refs_table() -> TableDefinition<'static, &'static str, &'static str> {
        CHUNK_REFS_TABLE
    }
    /// AGENT-7 — single-writer volume leases.
    pub fn get_leases_table() -> TableDefinition<'static, &'static str, &'static str> {
        LEASES_TABLE
    }
    /// AGENT-7 — per-provider health score and quarantine state.
    pub fn get_provider_health_table() -> TableDefinition<'static, &'static str, &'static str> {
        PROVIDER_HEALTH_TABLE
    }
    /// AGENT-8 — the process table behind `ps` / `top` / `kill`.
    pub fn get_compute_tasks_table() -> TableDefinition<'static, &'static str, &'static str> {
        COMPUTE_TASKS_TABLE
    }
    /// Scheduler — recurring `.cybsh` triggers.
    pub fn get_schedules_table() -> TableDefinition<'static, &'static str, &'static str> {
        SCHEDULES_TABLE
    }
    /// Scheduler — fire history (pruned).
    pub fn get_schedule_runs_table() -> TableDefinition<'static, &'static str, &'static str> {
        SCHEDULE_RUNS_TABLE
    }
    /// AGENT-8 — `cybsh` command history.
    pub fn get_shell_history_table() -> TableDefinition<'static, &'static str, &'static str> {
        SHELL_HISTORY_TABLE
    }
    // <<< AI AGENT accessors >>>
    /// Saved agent configs (keyless rows — provider keys are side-tabled).
    pub fn get_agent_configs_table() -> TableDefinition<'static, &'static str, &'static str> {
        AGENT_CONFIGS_TABLE
    }
    /// Agent session transcripts.
    pub fn get_agent_sessions_table() -> TableDefinition<'static, &'static str, &'static str> {
        AGENT_SESSIONS_TABLE
    }
    /// Agent long-term memories (text + embedding vectors as JSON rows).
    pub fn get_agent_memories_table() -> TableDefinition<'static, &'static str, &'static str> {
        AGENT_MEMORIES_TABLE
    }

    /// Generic key/value blob (secrets, config, file content, volume mirror).
    pub fn get_kv_table() -> TableDefinition<'static, &'static str, &'static str> {
        KV_TABLE
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(KV_TABLE)?;
        Ok(table.get(key)?.map(|value| value.value().to_string()))
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(KV_TABLE)?;
            table.insert(key, value)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn kv_delete(&self, key: &str) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(KV_TABLE)?;
            let previous = table.remove(key)?;
            previous.is_some()
        };
        tx.commit()?;
        Ok(removed)
    }

    pub fn kv_list(&self, prefix: &str) -> Result<Vec<(String, usize)>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(KV_TABLE)?;
        let mut rows = Vec::new();
        for entry in table.iter()? {
            let (key, value) = entry?;
            let key = key.value().to_string();
            if key.starts_with(prefix) {
                rows.push((key, value.value().len()));
            }
        }
        Ok(rows)
    }

    /// Row key for a synced copy: one local file × one config.
    fn sync_file_key(file_id: &str, config_id: &str) -> String {
        format!("{}/{}", file_id, config_id)
    }

    /// Insert or replace the locator record for one synced copy.
    pub fn upsert_sync_file(&self, record: &SyncFile) -> Result<()> {
        let key = Self::sync_file_key(&record.id, record.config_id.as_deref().unwrap_or(""));
        let serialized = serde_json::to_string(record)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SYNC_FILES_TABLE)?;
            table.insert(key.as_str(), serialized.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Locator record for one (file, config) pair.
    pub fn get_sync_file(&self, file_id: &str, config_id: &str) -> Result<Option<SyncFile>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_FILES_TABLE)?;
        let key = Self::sync_file_key(file_id, config_id);
        match table.get(key.as_str())? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    /// Newest record uploaded to `remote_path` by `config_id` (linear scan —
    /// the table holds one row per synced file, never more than a library).
    pub fn find_sync_file_by_remote(
        &self,
        config_id: &str,
        remote_path: &str,
    ) -> Result<Option<SyncFile>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_FILES_TABLE)?;
        let mut found: Option<SyncFile> = None;
        for entry in table.iter()? {
            let (_, value) = entry?;
            let record: SyncFile = serde_json::from_str(value.value())?;
            if record.config_id.as_deref() == Some(config_id)
                && record.remote_path.as_deref() == Some(remote_path)
            {
                found = Some(record);
            }
        }
        Ok(found)
    }

    /// All locator records, optionally narrowed to one config.
    pub fn list_sync_files(&self, config_id: Option<&str>) -> Result<Vec<SyncFile>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_FILES_TABLE)?;
        let mut records = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let record: SyncFile = serde_json::from_str(value.value())?;
            if config_id.is_none() || record.config_id.as_deref() == config_id {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// Drop one locator record (used when the remote copy is deleted).
    pub fn remove_sync_file(&self, file_id: &str, config_id: &str) -> Result<bool> {
        let key = Self::sync_file_key(file_id, config_id);
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(SYNC_FILES_TABLE)?;
            let removed = table.remove(key.as_str())?.is_some();
            removed
        };
        tx.commit()?;
        Ok(removed)
    }

    /// Persist a finished run and prune the history to
    /// [`SYNC_RUN_HISTORY_LIMIT`] rows (oldest `finished_at` first).
    pub fn save_sync_run(&self, record: &SyncRunRecord) -> Result<()> {
        let serialized = serde_json::to_string(record)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SYNC_RUNS_TABLE)?;
            table.insert(record.run_id.as_str(), serialized.as_str())?;

            let mut rows: Vec<(String, String)> = Vec::new();
            for entry in table.iter()? {
                let (key, value) = entry?;
                let stored: SyncRunRecord = serde_json::from_str(value.value())?;
                rows.push((key.value().to_string(), stored.finished_at));
            }
            if rows.len() > SYNC_RUN_HISTORY_LIMIT {
                rows.sort_by_key(|a| a.1.clone());
                for (stale, _) in rows.iter().take(rows.len() - SYNC_RUN_HISTORY_LIMIT) {
                    table.remove(stale.as_str())?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// One finished run by id (survives restarts).
    pub fn get_sync_run(&self, run_id: &str) -> Result<Option<SyncRunRecord>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_RUNS_TABLE)?;
        match table.get(run_id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    /// Run history, newest `finished_at` first, capped at `limit`.
    pub fn list_sync_runs(&self, limit: usize) -> Result<Vec<SyncRunRecord>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_RUNS_TABLE)?;
        let mut rows = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            rows.push(serde_json::from_str::<SyncRunRecord>(value.value())?);
        }
        rows.sort_by_key(|a| std::cmp::Reverse(a.finished_at.clone()));
        rows.truncate(limit);
        Ok(rows)
    }

    // ─── Scheduler: recurring triggers + fire history (Phase 1) ─────────

    /// Every schedule row, oldest `created_at` first.
    pub fn list_schedules(&self) -> Result<Vec<cybermanju_types::schedule::ScheduleRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SCHEDULES_TABLE)?;
        let mut rows: Vec<cybermanju_types::schedule::ScheduleRow> = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            rows.push(serde_json::from_str(value.value())?);
        }
        rows.sort_by_key(|a| a.created_at.clone());
        Ok(rows)
    }

    pub fn get_schedule(
        &self,
        id: &str,
    ) -> Result<Option<cybermanju_types::schedule::ScheduleRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SCHEDULES_TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn save_schedule(&self, row: &cybermanju_types::schedule::ScheduleRow) -> Result<()> {
        let serialized = serde_json::to_string(row)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SCHEDULES_TABLE)?;
            table.insert(row.id.as_str(), serialized.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_schedule(&self, id: &str) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(SCHEDULES_TABLE)?;
            let removed = table.remove(id)?.is_some();
            removed
        };
        tx.commit()?;
        Ok(removed)
    }

    /// Persist a finished run and prune the history to
    /// [`cybermanju_types::schedule::SCHEDULE_RUN_HISTORY_LIMIT`] rows
    /// (oldest `finished_at` first).
    pub fn save_schedule_run(&self, run: &cybermanju_types::schedule::ScheduleRun) -> Result<()> {
        let limit = cybermanju_types::schedule::SCHEDULE_RUN_HISTORY_LIMIT;
        let serialized = serde_json::to_string(run)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SCHEDULE_RUNS_TABLE)?;
            table.insert(run.run_id.as_str(), serialized.as_str())?;

            let mut rows: Vec<(String, String)> = Vec::new();
            for entry in table.iter()? {
                let (key, value) = entry?;
                let stored: cybermanju_types::schedule::ScheduleRun =
                    serde_json::from_str(value.value())?;
                rows.push((key.value().to_string(), stored.finished_at));
            }
            if rows.len() > limit {
                rows.sort_by_key(|a| a.1.clone());
                for (stale, _) in rows.iter().take(rows.len() - limit) {
                    table.remove(stale.as_str())?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Run history for one schedule, newest `finished_at` first, capped.
    pub fn list_schedule_runs(
        &self,
        schedule_id: &str,
        limit: usize,
    ) -> Result<Vec<cybermanju_types::schedule::ScheduleRun>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SCHEDULE_RUNS_TABLE)?;
        let mut rows = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let run: cybermanju_types::schedule::ScheduleRun = serde_json::from_str(value.value())?;
            if run.schedule_id == schedule_id {
                rows.push(run);
            }
        }
        rows.sort_by_key(|a| std::cmp::Reverse(a.finished_at.clone()));
        rows.truncate(limit);
        Ok(rows)
    }

    // ─── Secrets keystore (Phase 3) ────────────────────────────────────
    /// Secrets table accessor (sealed password-manager rows).
    pub fn get_secrets_table() -> TableDefinition<'static, &'static str, &'static str> {
        SECRETS_TABLE
    }

    /// All stored secrets (sealed values included — the caller decides what
    /// to serialize; REST/Tauri list paths project through `SecretMeta`).
    pub fn list_secrets(&self) -> Result<Vec<cybermanju_types::secrets::SecretRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SECRETS_TABLE)?;
        let mut rows: Vec<cybermanju_types::secrets::SecretRow> = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            rows.push(serde_json::from_str(value.value())?);
        }
        rows.sort_by_key(|a| a.title.to_lowercase());
        Ok(rows)
    }

    pub fn get_secret(&self, id: &str) -> Result<Option<cybermanju_types::secrets::SecretRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SECRETS_TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn save_secret(&self, row: &cybermanju_types::secrets::SecretRow) -> Result<()> {
        let serialized = serde_json::to_string(row)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SECRETS_TABLE)?;
            table.insert(row.id.as_str(), serialized.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_secret(&self, id: &str) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(SECRETS_TABLE)?;
            let removed = table.remove(id)?.is_some();
            removed
        };
        tx.commit()?;
        Ok(removed)
    }

    // ─── Device identity registry (§2B.1) ─────────────────────────────
    // One row per device; the private key half never leaves the device.
    // Revoked rows stay for history but fail closed at every call site.

    pub fn get_devices_table() -> TableDefinition<'static, &'static str, &'static str> {
        DEVICES_TABLE
    }

    pub fn list_devices(&self) -> Result<Vec<DeviceRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(DEVICES_TABLE)?;
        let mut rows: Vec<DeviceRow> = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            rows.push(serde_json::from_str(value.value())?);
        }
        rows.sort_by_key(|a| a.name.to_lowercase());
        Ok(rows)
    }

    pub fn get_device(&self, id: &str) -> Result<Option<DeviceRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(DEVICES_TABLE)?;
        match table.get(id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn save_device(&self, row: &DeviceRow) -> Result<()> {
        let serialized = serde_json::to_string(row)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(DEVICES_TABLE)?;
            table.insert(row.id.as_str(), serialized.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_device(&self, id: &str) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(DEVICES_TABLE)?;
            let removed = table.remove(id)?.is_some();
            removed
        };
        tx.commit()?;
        Ok(removed)
    }

    /// Mark last-seen (heartbeat). Missing rows are `not_found:`, never
    /// auto-created — registration is the explicit entry point.
    pub fn touch_device(&self, id: &str, at: &str) -> Result<DeviceRow> {
        let mut row = self
            .get_device(id)?
            .ok_or_else(|| anyhow::anyhow!("not_found: no device '{id}'"))?;
        row.last_seen_at = at.to_string();
        self.save_device(&row)?;
        Ok(row)
    }

    // ─── Server-side todos (§4.9 agent bridge) ────────────────────────
    // One list per owner shared by the agent tools and the Archive TASKS tab.

    pub fn get_todos_table() -> TableDefinition<'static, &'static str, &'static str> {
        TODOS_TABLE
    }

    pub fn list_todos(&self, owner_id: Option<&str>) -> Result<Vec<TodoRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(TODOS_TABLE)?;
        let mut rows: Vec<TodoRow> = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let row: TodoRow = serde_json::from_str(value.value())?;
            if owner_id.map(|o| row.owner_id == o).unwrap_or(true) {
                rows.push(row);
            }
        }
        rows.sort_by_key(|a| a.created_at.clone());
        Ok(rows)
    }

    pub fn save_todo(&self, row: &TodoRow) -> Result<()> {
        if row.title.trim().is_empty() {
            anyhow::bail!("invalid: todo title is required");
        }
        let serialized = serde_json::to_string(row)?;
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(TODOS_TABLE)?;
            table.insert(row.id.as_str(), serialized.as_str())?;
        }
        tx.commit()?;
        // Op-log (§2C): logged here, not at the API layer — Tauri twins and
        // agent tools write todos straight through `db`, bypassing the REST
        // route, and every writer must append.
        let _ = self.record_mutation_op(
            "todo",
            None,
            serde_json::json!({ "id": row.id, "title": row.title, "done": row.done }),
        );
        Ok(())
    }

    pub fn remove_todo(&self, id: &str) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(TODOS_TABLE)?;
            let removed = table.remove(id)?.is_some();
            removed
        };
        tx.commit()?;
        if removed {
            let _ = self.record_mutation_op(
                "todo",
                None,
                serde_json::json!({ "id": id, "deleted": true }),
            );
        }
        Ok(removed)
    }

    // ─── Op-log (§2C/P3-2) ───────────────────────────────────────────
    // Append-only mutation log; devices exchange rows using the version
    // vectors `leases` already models. Keys are zero-padded seq so range
    // scans come out in append order.

    pub fn get_oplog_table() -> TableDefinition<'static, &'static str, &'static str> {
        OPLOG_TABLE
    }

    fn oplog_key(seq: u64) -> String {
        format!("{seq:020}")
    }

    /// Append one op; seq assignment is atomic with the row write (same
    /// txn as the `oplog:next_seq` counter bump). Returns the stored row.
    pub fn append_op(&self, mut row: OpRow) -> Result<OpRow> {
        if row.kind.trim().is_empty() {
            anyhow::bail!("invalid: op kind is required");
        }
        let tx = self.db.begin_write()?;
        let seq = {
            let stored_next = {
                let kv = tx.open_table(KV_TABLE)?;
                let entry = kv.get("oplog:next_seq")?;
                entry.map(|value| value.value().to_string())
            };
            match stored_next {
                Some(value) => value.parse::<u64>().map_err(|e| {
                    anyhow::anyhow!("integrity: invalid op-log sequence counter: {e}")
                })?,
                None => {
                    // Older databases may have rows but no counter. Continue
                    // after the last stored row instead of overwriting seq 0.
                    let table = tx.open_table(OPLOG_TABLE)?;
                    let mut next = 0u64;
                    for entry in table.iter()? {
                        let (_, value) = entry?;
                        let row: OpRow = serde_json::from_str(value.value())?;
                        let after = row.seq.checked_add(1).ok_or_else(|| {
                            anyhow::anyhow!("too_large: op-log sequence exhausted")
                        })?;
                        next = next.max(after);
                    }
                    next
                }
            }
        };
        let following = seq
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("too_large: op-log sequence exhausted"))?;
        {
            let mut kv = tx.open_table(KV_TABLE)?;
            kv.insert("oplog:next_seq", following.to_string().as_str())?;
        }
        row.seq = seq;
        if row.op_id.trim().is_empty() {
            row.op_id = format!("{}-{seq}", row.device_id);
        }
        let serialized = serde_json::to_string(&row)?;
        {
            let mut table = tx.open_table(OPLOG_TABLE)?;
            table.insert(Self::oplog_key(seq).as_str(), serialized.as_str())?;
        }
        tx.commit()?;
        Ok(row)
    }

    /// Rows with `seq >= since`, oldest first. `since` is the next-sequence
    /// cursor (0 reads the first row); `limit` caps the page (0 = uncapped).
    pub fn list_ops_since(&self, since: u64, limit: usize) -> Result<Vec<OpRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(OPLOG_TABLE)?;
        let mut rows: Vec<OpRow> = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let row: OpRow = serde_json::from_str(value.value())?;
            if row.seq >= since {
                rows.push(row);
            }
            if limit > 0 && rows.len() >= limit {
                break;
            }
        }
        rows.sort_by_key(|r| r.seq);
        Ok(rows)
    }

    /// Owner-scoped op-log page. Filtering happens before applying `limit`,
    /// so unrelated tenants neither leak rows nor starve this owner's page.
    pub fn list_ops_since_for_devices(
        &self,
        since: u64,
        limit: usize,
        device_ids: &std::collections::HashSet<String>,
    ) -> Result<Vec<OpRow>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(OPLOG_TABLE)?;
        let mut rows: Vec<OpRow> = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let row: OpRow = serde_json::from_str(value.value())?;
            if row.seq >= since && device_ids.contains(&row.device_id) {
                rows.push(row);
                if limit > 0 && rows.len() >= limit {
                    break;
                }
            }
        }
        Ok(rows)
    }

    /// Next sequence cursor (0 when the log is empty).
    pub fn oplog_head(&self) -> Result<u64> {
        let tx = self.db.begin_read()?;
        let stored_next = {
            let kv = tx.open_table(KV_TABLE)?;
            let entry = kv.get("oplog:next_seq")?;
            entry.map(|value| value.value().to_string())
        };
        if let Some(value) = stored_next {
            return value
                .parse::<u64>()
                .map_err(|e| anyhow::anyhow!("integrity: invalid op-log sequence counter: {e}"));
        }
        // Compatibility with databases written before the counter existed.
        let table = tx.open_table(OPLOG_TABLE)?;
        let mut next = 0u64;
        for entry in table.iter()? {
            let (_, value) = entry?;
            let row: OpRow = serde_json::from_str(value.value())?;
            let after = row
                .seq
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("too_large: op-log sequence exhausted"))?;
            next = next.max(after);
        }
        Ok(next)
    }

    /// Merge foreign rows: unknown `op_id`s append (new local seq), known
    /// ids are idempotent no-ops. Returns (appended, skipped).
    pub fn merge_ops(&self, foreign: &[OpRow]) -> Result<(usize, usize)> {
        let known: std::collections::HashSet<String> = self
            .list_ops_since(0, 0)?
            .into_iter()
            .map(|r| r.op_id)
            .collect();
        let mut appended = 0usize;
        let mut skipped = 0usize;
        for op in foreign {
            if known.contains(&op.op_id) {
                skipped += 1;
                continue;
            }
            let mut row = op.clone();
            row.seq = 0;
            self.append_op(row)?;
            appended += 1;
        }
        Ok((appended, skipped))
    }

    /// Candidate conflicts: two ops touching the same `file_id` from
    /// different devices where neither base vector dominates the other.
    /// Pure scan over stored rows — the conflict UI renders these.
    fn op_conflicts_from_rows(rows: &[OpRow]) -> Vec<(OpRow, OpRow)> {
        let mut out = Vec::new();
        for (i, a) in rows.iter().enumerate() {
            let Some(fid) = a.file_id.as_deref() else {
                continue;
            };
            for b in rows.iter().skip(i + 1) {
                if b.file_id.as_deref() != Some(fid) || a.device_id == b.device_id {
                    continue;
                }
                let a_dom = dominates(&a.bases, &b.bases);
                let b_dom = dominates(&b.bases, &a.bases);
                if !a_dom && !b_dom {
                    out.push((a.clone(), b.clone()));
                }
            }
        }
        out
    }

    pub fn op_conflicts(&self) -> Result<Vec<(OpRow, OpRow)>> {
        let rows = self.list_ops_since(0, 0)?;
        Ok(Self::op_conflicts_from_rows(&rows))
    }

    /// Conflict scan over only the supplied devices, keeping tenant data out
    /// of both the response and the quadratic comparison set.
    pub fn op_conflicts_for_devices(
        &self,
        device_ids: &std::collections::HashSet<String>,
    ) -> Result<Vec<(OpRow, OpRow)>> {
        let rows = self.list_ops_since_for_devices(0, 0, device_ids)?;
        Ok(Self::op_conflicts_from_rows(&rows))
    }

    /// Local writer identity for op-log rows: explicit override first, then
    /// the host — the same rule as `sync::lease::default_holder`, duplicated
    /// here so `db` stays dependency-free.
    pub fn local_device_id() -> String {
        if let Ok(id) = std::env::var("CYBERMANJU_DEVICE") {
            if !id.trim().is_empty() {
                return id;
            }
        }
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| format!("device-{}", std::process::id()))
    }

    /// Mutation hook (§2C rollout): every file/todo/collection mutation
    /// appends one op so offline queue-and-replay and the conflict UI see
    /// it. Best-effort by design — the op-log is derived data and must never
    /// break the mutation itself (append failures return `None`; the caller
    /// keeps its `let _ =` and moves on).
    pub fn record_mutation_op(
        &self,
        kind: &str,
        file_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Option<OpRow> {
        if kind.trim().is_empty() {
            return None;
        }
        let row = OpRow {
            seq: 0,
            op_id: String::new(),
            device_id: Self::local_device_id(),
            created_at: chrono::Utc::now().to_rfc3339(),
            kind: kind.to_string(),
            file_id: file_id.map(|s| s.to_string()),
            payload,
            bases: Default::default(),
        };
        self.append_op(row).ok()
    }

    /// Look one op up by its global id (resolution validates both forks).
    pub fn get_op_by_id(&self, op_id: &str) -> Result<Option<OpRow>> {
        for row in self.list_ops_since(0, 0)? {
            if row.op_id == op_id {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }

    // ─── Persistent revocations (P0-5) ──────────────────────────────
    // Logout must survive restarts and replica hand-offs: revoked JTIs live
    // in kv (`revoked:<jti>` → exp epoch secs) next to the rows, not just in
    // the dashboard's memory. `AuthState` hydrates from here at boot and
    // every `revoke()` persists here too. Single-file redb has one opener,
    // so this is the shared revocation list for every transport on the box.

    fn revoked_key(jti: &str) -> String {
        format!("revoked:{jti}")
    }

    /// Persist one revocation (idempotent; keeps the farthest expiry).
    pub fn save_revocation(&self, jti: &str, exp: u64) -> Result<()> {
        if jti.trim().is_empty() {
            anyhow::bail!("invalid: jti is required");
        }
        let tx = self.db.begin_write()?;
        {
            let mut kv = tx.open_table(KV_TABLE)?;
            let key = Self::revoked_key(jti);
            let keep = match kv.get(key.as_str())? {
                Some(v) => v.value().parse::<u64>().unwrap_or(0).max(exp),
                None => exp,
            };
            kv.insert(key.as_str(), keep.to_string().as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Live revocations (`exp` still in the future) for boot hydration.
    pub fn list_live_revocations(&self, now_secs: u64) -> Result<Vec<(String, u64)>> {
        let tx = self.db.begin_read()?;
        let kv = tx.open_table(KV_TABLE)?;
        let mut out = Vec::new();
        for entry in kv.iter()? {
            let (key, value) = entry?;
            let key = key.value();
            let Some(jti) = key.strip_prefix("revoked:") else {
                continue;
            };
            if let Ok(exp) = value.value().parse::<u64>() {
                if exp > now_secs {
                    out.push((jti.to_string(), exp));
                }
            }
        }
        Ok(out)
    }

    // ─── Config secrets (AGENT-3 request: token lives beside the row) ──
    // `SyncConfig.token` is `skip_serializing`, so the JSON row never holds
    // it. The side table does — written by `save_config`, merged back by
    // `get_config`/`list_configs`, removed by `delete_config`.

    pub fn put_sync_secret(&self, config_id: &str, secret: &str) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SYNC_SECRETS_TABLE)?;
            table.insert(config_id, secret)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_sync_secret(&self, config_id: &str) -> Result<Option<String>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_SECRETS_TABLE)?;
        Ok(table.get(config_id)?.map(|v| v.value().to_string()))
    }

    pub fn remove_sync_secret(&self, config_id: &str) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut table = tx.open_table(SYNC_SECRETS_TABLE)?;
            let removed = table.remove(config_id)?.is_some();
            removed
        };
        tx.commit()?;
        Ok(removed)
    }

    /// Every saved sync config with its secret merged back in (the row JSON
    /// never carries `token`). Used by the pipeline's striped placement,
    /// which must reach several providers in one run.
    pub fn list_sync_configs(&self) -> Result<Vec<cybermanju_types::sync::SyncConfig>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SYNC_CONFIGS_TABLE)?;
        let mut configs = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            let mut config: cybermanju_types::sync::SyncConfig =
                serde_json::from_str(value.value())?;
            if config.token.is_none() {
                config.token = self.get_sync_secret(&config.id)?;
            }
            configs.push(config);
        }
        Ok(configs)
    }

    /// Ids of every live file. Trashed files live in their own table, so
    /// this is exactly the set an auto-sync scan should walk (item 11).
    pub fn list_file_ids(&self) -> Result<Vec<String>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(FILES_TABLE)?;
        let mut ids = Vec::new();
        for entry in table.iter()? {
            let (key, _) = entry?;
            ids.push(key.value().to_string());
        }
        Ok(ids)
    }

    /// Persisted schema marker (1 = first sync-persistence schema).
    pub fn schema_version(&self) -> Result<u64> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SCHEMA_VERSION_TABLE)?;
        match table.get("schema")? {
            Some(v) => Ok(v.value().parse::<u64>().unwrap_or(0)),
            None => Ok(0),
        }
    }

    pub fn log_audit(
        &self,
        action: &str,
        entity_type: &str,
        entity_id: &str,
        user_id: Option<&str>,
        details: Option<serde_json::Value>,
    ) -> Result<()> {
        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            action: action.to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            user_id: user_id.map(|s| s.to_string()),
            details,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(AUDIT_LOG_TABLE)?;
            table.insert(entry.id.as_str(), serde_json::to_string(&entry)?.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn trash_file(
        &self,
        file_id: &str,
        file_node: &FileNode,
        deleted_by: Option<&str>,
    ) -> Result<()> {
        let trash_item = TrashItem {
            id: file_id.to_string(),
            original_file: file_node.clone(),
            deleted_at: chrono::Utc::now().to_rfc3339(),
            deleted_by: deleted_by.map(|s| s.to_string()),
            restore_path: file_node.parent_id.clone(),
        };
        let tx = self.db.begin_write()?;
        {
            let mut trash_table = tx.open_table(TRASH_TABLE)?;
            trash_table.insert(file_id, serde_json::to_string(&trash_item)?.as_str())?;
            let mut files_table = tx.open_table(FILES_TABLE)?;
            files_table.remove(file_id)?;
            if let Some(parent_id) = file_node.parent_id.as_deref() {
                let mut parent_table = tx.open_table(PARENT_INDEX_TABLE)?;
                let existing: Vec<String> = parent_table
                    .get(parent_id)?
                    .and_then(|value| serde_json::from_str(value.value()).ok())
                    .unwrap_or_default();
                let mut ids = existing;
                ids.retain(|id| id != file_id);
                if ids.is_empty() {
                    parent_table.remove(parent_id)?;
                } else {
                    parent_table.insert(parent_id, serde_json::to_string(&ids)?.as_str())?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn restore_from_trash(&self, file_id: &str) -> Result<Option<TrashItem>> {
        let tx = self.db.begin_write()?;
        let result = {
            let trash_table = tx.open_table(TRASH_TABLE)?;
            let found: Option<TrashItem> = trash_table
                .get(file_id)?
                .and_then(|v| serde_json::from_str::<TrashItem>(v.value()).ok());
            found
        };
        if let Some(ref item) = result {
            let mut files_table = tx.open_table(FILES_TABLE)?;
            let serialized = serde_json::to_string(&item.original_file)?;
            files_table.insert(file_id, serialized.as_str())?;
            if let Some(parent_id) = item.original_file.parent_id.as_deref() {
                let mut parent_table = tx.open_table(PARENT_INDEX_TABLE)?;
                let existing: Vec<String> = parent_table
                    .get(parent_id)?
                    .and_then(|value| serde_json::from_str(value.value()).ok())
                    .unwrap_or_default();
                let mut ids = existing;
                if !ids.contains(&file_id.to_string()) {
                    ids.push(file_id.to_string());
                }
                parent_table.insert(parent_id, serde_json::to_string(&ids)?.as_str())?;
            }
            let mut trash_table = tx.open_table(TRASH_TABLE)?;
            trash_table.remove(file_id)?;
        }
        tx.commit()?;
        Ok(result)
    }

    pub fn list_trash(&self) -> Result<Vec<TrashItem>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(TRASH_TABLE)?;
        let mut items = Vec::new();
        for entry in table.iter()? {
            let (_, value) = entry?;
            if let Ok(item) = serde_json::from_str::<TrashItem>(value.value()) {
                items.push(item);
            }
        }
        Ok(items)
    }

    pub fn empty_trash(&self) -> Result<u32> {
        let tx = self.db.begin_write()?;
        let mut count = 0u32;
        {
            let trash_table = tx.open_table(TRASH_TABLE)?;
            let keys: Vec<String> = trash_table
                .iter()?
                .filter_map(|e| e.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            drop(trash_table);
            let mut trash_table = tx.open_table(TRASH_TABLE)?;
            for key in keys {
                trash_table.remove(key.as_str())?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn create_file_version(
        &self,
        file_node: &FileNode,
        snapshot_data: Option<&str>,
    ) -> Result<FileVersion> {
        let tx = self.db.begin_write()?;
        let version = {
            let versions_table = tx.open_table(FILE_VERSIONS_TABLE)?;
            let existing: Vec<FileVersion> = versions_table
                .iter()?
                .filter_map(|e| e.ok())
                .filter(|(k, _)| k.value().starts_with(&format!("{}/", file_node.id)))
                .filter_map(|(_, v)| serde_json::from_str::<FileVersion>(v.value()).ok())
                .collect();
            let next_ver = existing.iter().map(|v| v.version_number).max().unwrap_or(0) + 1;
            FileVersion {
                id: format!("{}/v{}", file_node.id, next_ver),
                file_id: file_node.id.clone(),
                version_number: next_ver,
                hash_blake3: file_node.hash_blake3.clone(),
                size_bytes: file_node.size_bytes,
                snapshot_data: snapshot_data.map(|s| s.to_string()),
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        };
        {
            let mut versions_table = tx.open_table(FILE_VERSIONS_TABLE)?;
            versions_table.insert(
                version.id.as_str(),
                serde_json::to_string(&version)?.as_str(),
            )?;
        }
        tx.commit()?;
        Ok(version)
    }

    pub fn list_file_versions(&self, file_id: &str) -> Result<Vec<FileVersion>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(FILE_VERSIONS_TABLE)?;
        let versions: Vec<FileVersion> = table
            .iter()?
            .filter_map(|e| e.ok())
            .filter(|(k, _)| k.value().starts_with(&format!("{}/", file_id)))
            .filter_map(|(_, v)| serde_json::from_str::<FileVersion>(v.value()).ok())
            .collect();
        Ok(versions)
    }

    pub fn revert_file_version(
        &self,
        file_id: &str,
        version_id: &str,
    ) -> Result<Option<FileVersion>> {
        let tx = self.db.begin_write()?;
        let version = {
            let versions_table = tx.open_table(FILE_VERSIONS_TABLE)?;
            let found: Option<FileVersion> = versions_table
                .get(version_id)?
                .and_then(|v| serde_json::from_str::<FileVersion>(v.value()).ok());
            found
        };
        if let Some(ref ver) = version {
            let mut files_table = tx.open_table(FILES_TABLE)?;
            let existing_node: Option<FileNode> = files_table
                .get(file_id)?
                .and_then(|v| serde_json::from_str::<FileNode>(v.value()).ok());
            if let Some(mut file_node) = existing_node {
                file_node.hash_blake3 = ver.hash_blake3.clone();
                file_node.size_bytes = ver.size_bytes;
                file_node.modified_at = chrono::Utc::now().to_rfc3339();
                files_table.insert(file_id, serde_json::to_string(&file_node)?.as_str())?;
            }
        }
        tx.commit()?;
        Ok(version)
    }

    pub fn create_share_link(&self, file_id: &str, expires_in_hours: u64) -> Result<ShareLink> {
        use base64::Engine;
        use rand_core::RngCore;
        let mut token_bytes = [0u8; 32];
        rand_core::OsRng.fill_bytes(&mut token_bytes);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);

        let now = chrono::Utc::now();
        let expires_at = if expires_in_hours > 0 {
            (now + chrono::Duration::hours(expires_in_hours as i64)).to_rfc3339()
        } else {
            (now + chrono::Duration::days(365)).to_rfc3339()
        };

        let link = ShareLink {
            id: uuid::Uuid::new_v4().to_string(),
            file_id: file_id.to_string(),
            token: token.clone(),
            expires_at,
            created_at: now.to_rfc3339(),
            url: None,
        };

        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(SHARE_LINKS_TABLE)?;
            table.insert(link.id.as_str(), serde_json::to_string(&link)?.as_str())?;
        }
        tx.commit()?;
        Ok(link)
    }

    pub fn get_share_link_by_token(&self, token: &str) -> Result<Option<ShareLink>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(SHARE_LINKS_TABLE)?;
        for entry in table.iter()? {
            let (_, value) = entry?;
            let link: ShareLink = serde_json::from_str(value.value())?;
            if link.token == token {
                return Ok(Some(link));
            }
        }
        Ok(None)
    }

    pub fn get_file_node(&self, file_id: &str) -> Result<Option<FileNode>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(FILES_TABLE)?;
        match table.get(file_id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn add_to_parent_index(&self, file_id: &str, parent_id: &str) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(PARENT_INDEX_TABLE)?;
            let existing: Vec<String> = table
                .get(parent_id)?
                .and_then(|v| serde_json::from_str(v.value()).ok())
                .unwrap_or_default();
            let mut ids = existing;
            if !ids.contains(&file_id.to_string()) {
                ids.push(file_id.to_string());
            }
            table.insert(parent_id, serde_json::to_string(&ids)?.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_from_parent_index(&self, file_id: &str, parent_id: &str) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut table = tx.open_table(PARENT_INDEX_TABLE)?;
            let existing: Vec<String> = table
                .get(parent_id)?
                .and_then(|v| serde_json::from_str(v.value()).ok())
                .unwrap_or_default();
            let mut ids = existing;
            ids.retain(|id| id != file_id);
            if ids.is_empty() {
                table.remove(parent_id)?;
            } else {
                table.insert(parent_id, serde_json::to_string(&ids)?.as_str())?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_by_parent(&self, parent_id: &str) -> Result<Vec<String>> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(PARENT_INDEX_TABLE)?;
        let value = table.get(parent_id)?;
        match value {
            Some(v) => {
                let ids: Vec<String> = serde_json::from_str(v.value())?;
                Ok(ids)
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn insert_file_with_index(
        &self,
        file_id: &str,
        serialized: &str,
        parent_id: Option<&str>,
    ) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut files_table = tx.open_table(FILES_TABLE)?;
            files_table.insert(file_id, serialized)?;
            if let Some(pid) = parent_id {
                let mut index_table = tx.open_table(PARENT_INDEX_TABLE)?;
                let existing: Vec<String> = index_table
                    .get(pid)?
                    .and_then(|v| serde_json::from_str(v.value()).ok())
                    .unwrap_or_default();
                let mut ids = existing;
                if !ids.contains(&file_id.to_string()) {
                    ids.push(file_id.to_string());
                }
                index_table.insert(pid, serde_json::to_string(&ids)?.as_str())?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_file_with_index(&self, file_id: &str, parent_id: Option<&str>) -> Result<bool> {
        let tx = self.db.begin_write()?;
        let removed = {
            let mut files_table = tx.open_table(FILES_TABLE)?;
            let result = files_table.remove(file_id)?.is_some();
            result
        };
        if removed {
            if let Some(pid) = parent_id {
                let mut index_table = tx.open_table(PARENT_INDEX_TABLE)?;
                let existing: Vec<String> = index_table
                    .get(pid)?
                    .and_then(|v| serde_json::from_str(v.value()).ok())
                    .unwrap_or_default();
                let mut ids = existing;
                ids.retain(|id| id != file_id);
                if ids.is_empty() {
                    index_table.remove(pid)?;
                } else {
                    index_table.insert(pid, serde_json::to_string(&ids)?.as_str())?;
                }
            }
        }
        tx.commit()?;
        Ok(removed)
    }

    pub fn move_file_with_index(
        &self,
        file_id: &str,
        serialized: &str,
        old_parent_id: Option<&str>,
        new_parent_id: &str,
    ) -> Result<()> {
        let tx = self.db.begin_write()?;
        {
            let mut files_table = tx.open_table(FILES_TABLE)?;
            files_table.insert(file_id, serialized)?;
            if let Some(old_pid) = old_parent_id {
                let mut index_table = tx.open_table(PARENT_INDEX_TABLE)?;
                let existing: Vec<String> = index_table
                    .get(old_pid)?
                    .and_then(|v| serde_json::from_str(v.value()).ok())
                    .unwrap_or_default();
                let mut ids = existing;
                ids.retain(|id| id != file_id);
                if ids.is_empty() {
                    index_table.remove(old_pid)?;
                } else {
                    index_table.insert(old_pid, serde_json::to_string(&ids)?.as_str())?;
                }
            }
            let mut index_table = tx.open_table(PARENT_INDEX_TABLE)?;
            let existing: Vec<String> = index_table
                .get(new_parent_id)?
                .and_then(|v| serde_json::from_str(v.value()).ok())
                .unwrap_or_default();
            let mut ids = existing;
            if !ids.contains(&file_id.to_string()) {
                ids.push(file_id.to_string());
            }
            index_table.insert(new_parent_id, serde_json::to_string(&ids)?.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Export a compact, consistent point-in-time image of the complete database.
    ///
    /// redb read transactions provide a stable view while concurrent writes
    /// continue. Copying the logical tables into a fresh database avoids
    /// copying a live/mmap'd file and preserves every table in the schema.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>> {
        self.snapshot_bytes_with_limit(MAX_SNAPSHOT_BYTES)
    }

    /// Export a snapshot only when its finished file is within `max_bytes`.
    /// Check metadata before `read` so an oversized database is not materialized
    /// as an unbounded `Vec` for REST, Tauri, shell or WASM transport.
    pub fn snapshot_bytes_with_limit(&self, max_bytes: usize) -> Result<Vec<u8>> {
        let path =
            std::env::temp_dir().join(format!("cybermanju-snapshot-{}.db", uuid::Uuid::new_v4()));

        let result = (|| -> Result<Vec<u8>> {
            let snapshot_db = RedbDatabase::create(&path)?;
            {
                let read_tx = self.db.begin_read()?;
                let write_tx = snapshot_db.begin_write()?;

                macro_rules! copy_table {
                    ($table:ident) => {{
                        let source = read_tx.open_table($table)?;
                        let mut destination = write_tx.open_table($table)?;
                        for entry in source.iter()? {
                            let (key, value) = entry?;
                            destination.insert(key.value(), value.value())?;
                        }
                    }};
                }

                copy_table!(FILES_TABLE);
                copy_table!(ACCOUNTS_TABLE);
                copy_table!(COLLECTIONS_TABLE);
                copy_table!(COLLECTION_ITEMS_TABLE);
                copy_table!(FACE_GROUPS_TABLE);
                copy_table!(LOOSE_GROUPS_TABLE);
                copy_table!(ENCRYPTION_KEYS_TABLE);
                copy_table!(LOCATIONS_TABLE);
                copy_table!(USERS_TABLE);
                copy_table!(USER_FILE_PERMS_TABLE);
                copy_table!(SYNC_CONFIGS_TABLE);
                copy_table!(PARENT_INDEX_TABLE);
                copy_table!(TRASH_TABLE);
                copy_table!(AUDIT_LOG_TABLE);
                copy_table!(FILE_VERSIONS_TABLE);
                copy_table!(SHARE_LINKS_TABLE);
                copy_table!(SYNC_FILES_TABLE);
                copy_table!(SYNC_RUNS_TABLE);
                copy_table!(SYNC_SECRETS_TABLE);
                copy_table!(SCHEMA_VERSION_TABLE);
                copy_table!(DISKS_TABLE);
                copy_table!(VOLUMES_TABLE);
                copy_table!(BLOCK_MAP_TABLE);
                copy_table!(SCRUB_RUNS_TABLE);
                copy_table!(REPAIRS_TABLE);
                copy_table!(CHUNK_REFS_TABLE);
                copy_table!(LEASES_TABLE);
                copy_table!(PROVIDER_HEALTH_TABLE);
                copy_table!(COMPUTE_TASKS_TABLE);
                copy_table!(SHELL_HISTORY_TABLE);
                copy_table!(AGENT_CONFIGS_TABLE);
                copy_table!(AGENT_SESSIONS_TABLE);
                copy_table!(AGENT_MEMORIES_TABLE);
                copy_table!(KV_TABLE);
                copy_table!(SCHEDULES_TABLE);
                copy_table!(SCHEDULE_RUNS_TABLE);
                copy_table!(SECRETS_TABLE);

                write_tx.commit()?;
            }
            drop(snapshot_db);

            let snapshot_bytes = std::fs::metadata(&path)?.len();
            if snapshot_bytes > max_bytes as u64 {
                anyhow::bail!(
                    "too_large: database snapshot is {snapshot_bytes} bytes, exceeding the {max_bytes}-byte cap"
                );
            }
            let bytes = std::fs::read(&path)?;
            Ok(bytes)
        })();

        let _ = std::fs::remove_file(&path);
        result
    }
}

/// Version-vector dominance for the op-log conflict scan: `a` dominates
/// `b` when every counter `b` has seen is <= in `a`, and `a` is strictly
/// ahead somewhere (a device `a` knows that `b` never saw counts too).
fn dominates(
    a: &std::collections::BTreeMap<String, u64>,
    b: &std::collections::BTreeMap<String, u64>,
) -> bool {
    let mut ahead = false;
    for (device, counter) in b {
        let mine = a.get(device).copied().unwrap_or(0);
        if mine < *counter {
            return false;
        }
        if mine > *counter {
            ahead = true;
        }
    }
    for device in a.keys() {
        if !b.contains_key(device) && a[device] > 0 {
            ahead = true;
        }
    }
    ahead
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    #[test]
    fn snapshot_bytes_preserves_rows_and_opens_as_a_database() {
        let source_path = std::env::temp_dir().join(format!(
            "cybermanju-snapshot-source-{}.db",
            uuid::Uuid::new_v4()
        ));
        let snapshot_path = std::env::temp_dir().join(format!(
            "cybermanju-snapshot-copy-{}.db",
            uuid::Uuid::new_v4()
        ));
        let source = Database::new(source_path.to_str().unwrap()).unwrap();
        {
            let tx = source.db.begin_write().unwrap();
            {
                let mut table = tx.open_table(KV_TABLE).unwrap();
                table.insert("mirror:test", "snapshot-value").unwrap();
            }
            tx.commit().unwrap();
        }

        let bytes = source.snapshot_bytes().unwrap();
        std::fs::write(&snapshot_path, &bytes).unwrap();
        let snapshot = Database::new(snapshot_path.to_str().unwrap()).unwrap();
        let read_tx = snapshot.db.begin_read().unwrap();
        let table = read_tx.open_table(KV_TABLE).unwrap();
        assert_eq!(
            table.get("mirror:test").unwrap().unwrap().value(),
            "snapshot-value"
        );

        drop(table);
        drop(read_tx);
        drop(snapshot);
        drop(source);
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(snapshot_path);
    }

    #[test]
    fn snapshot_bytes_refuses_a_small_limit() {
        let path = std::env::temp_dir().join(format!(
            "cybermanju-snapshot-limit-{}.db",
            uuid::Uuid::new_v4()
        ));
        let db = Database::new(path.to_str().unwrap()).expect("source database");
        let err = db
            .snapshot_bytes_with_limit(1)
            .expect_err("database snapshot exceeds one byte");
        assert!(err.to_string().starts_with("too_large:"), "got: {err}");
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn kv_methods_round_trip_list_prefix_and_delete() {
        let path =
            std::env::temp_dir().join(format!("cybermanju-kv-methods-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(path.to_str().unwrap()).unwrap();
        db.kv_set("volume:/notes.md", "Olá, vault").unwrap();
        db.kv_set("config:theme", "dark").unwrap();

        assert_eq!(
            db.kv_get("volume:/notes.md").unwrap().as_deref(),
            Some("Olá, vault")
        );
        assert_eq!(db.kv_get("missing").unwrap(), None);
        assert_eq!(
            db.kv_list("volume:").unwrap(),
            vec![("volume:/notes.md".to_string(), "Olá, vault".len())]
        );
        assert!(db.kv_delete("volume:/notes.md").unwrap());
        assert!(!db.kv_delete("volume:/notes.md").unwrap());
        assert_eq!(db.kv_get("volume:/notes.md").unwrap(), None);

        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn single_opener_second_open_conflicts_and_release_reopens() {
        // P0-22 close-out: one db path has ONE opener; the second fails
        // closed with `conflict:`, and dropping the first releases the lock.
        let path = std::env::temp_dir().join(format!(
            "cybermanju-single-opener-{}.db",
            uuid::Uuid::new_v4()
        ));
        let path_str = path.to_str().unwrap();
        let db = Database::new(path_str).unwrap();
        assert!(
            db.opener_lock_path().exists(),
            "lockfile must exist while open"
        );
        let err = Database::new(path_str)
            .err()
            .expect("second opener must fail")
            .to_string();
        assert!(
            err.starts_with("conflict:"),
            "second opener must fail closed, got: {err}"
        );
        drop(db);
        assert!(
            !std::path::Path::new(&format!("{path_str}.lock")).exists(),
            "lockfile must release on close"
        );
        let _reopened = Database::new(path_str).unwrap();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{path_str}.lock"));
    }
}

#[cfg(test)]
mod registry_oplog_tests {
    use super::*;
    use cybermanju_types::schema::{DeviceRow, OpRow, TodoRow};

    fn test_db(name: &str) -> (Database, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("cybermanju-{name}-{}.db", uuid::Uuid::new_v4()));
        let db = Database::new(path.to_str().unwrap()).unwrap();
        (db, path)
    }

    fn device(id: &str) -> DeviceRow {
        DeviceRow {
            id: id.to_string(),
            name: format!("{id} laptop"),
            owner_id: "test-owner".to_string(),
            public_key: String::new(),
            key_algorithm: "ml-dsa-65".to_string(),
            created_at: "2026-10-11T00:00:00Z".to_string(),
            last_seen_at: "2026-10-11T00:00:00Z".to_string(),
            revoked: false,
        }
    }

    #[test]
    fn devices_register_touch_and_revoke() {
        let (db, path) = test_db("devices");
        assert!(db.get_device("d1").unwrap().is_none());
        db.save_device(&device("d1")).unwrap();
        assert_eq!(db.list_devices().unwrap().len(), 1);
        let touched = db.touch_device("d1", "2026-10-12T00:00:00Z").unwrap();
        assert_eq!(touched.last_seen_at, "2026-10-12T00:00:00Z");
        assert!(db.touch_device("ghost", "2026-10-12T00:00:00Z").is_err());
        let mut revoked = device("d1");
        revoked.revoked = true;
        db.save_device(&revoked).unwrap();
        assert!(db.get_device("d1").unwrap().unwrap().revoked);
        assert!(db.remove_device("d1").unwrap());
        assert!(!db.remove_device("d1").unwrap());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn todos_save_list_scope_and_delete() {
        let (db, path) = test_db("todos");
        let row = TodoRow {
            id: "t1".to_string(),
            title: "Buy milk".to_string(),
            done: false,
            created_at: "2026-10-11T00:00:00Z".to_string(),
            done_at: None,
            owner_id: "u1".to_string(),
            source: "local".to_string(),
        };
        db.save_todo(&row).unwrap();
        assert_eq!(db.list_todos(None).unwrap().len(), 1);
        assert_eq!(db.list_todos(Some("u1")).unwrap().len(), 1);
        assert!(db.list_todos(Some("u2")).unwrap().is_empty());
        let bad = TodoRow {
            title: "   ".to_string(),
            ..row.clone()
        };
        assert!(db.save_todo(&bad).is_err());
        assert!(db.remove_todo("t1").unwrap());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    fn op(device: &str, kind: &str, file: Option<&str>) -> OpRow {
        OpRow {
            seq: 0,
            op_id: String::new(),
            device_id: device.to_string(),
            created_at: "2026-10-11T00:00:00Z".to_string(),
            kind: kind.to_string(),
            file_id: file.map(str::to_string),
            payload: serde_json::json!({}),
            bases: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn oplog_appends_with_atomic_seq_and_merges_idempotently() {
        let (db, path) = test_db("oplog");
        let a = db.append_op(op("d1", "tag", Some("f1"))).unwrap();
        let b = db.append_op(op("d1", "star", Some("f1"))).unwrap();
        assert_eq!((a.seq, b.seq), (0, 1));
        assert_eq!(a.op_id, "d1-0");
        assert!(db
            .append_op(OpRow {
                kind: String::new(),
                ..op("d1", "", None)
            })
            .is_err());
        assert_eq!(db.oplog_head().unwrap(), 2);
        assert_eq!(db.list_ops_since(1, 0).unwrap().len(), 1);
        // Merge: same op_id skips, foreign op appends with a fresh local seq.
        let (appended, skipped) = db
            .merge_ops(&[a.clone(), op("d2", "tag", Some("f1"))])
            .unwrap();
        assert_eq!((appended, skipped), (1, 1));
        assert_eq!(db.oplog_head().unwrap(), 3);
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn oplog_conflict_scan_flags_forks_only() {
        let (db, path) = test_db("conflicts");
        // Same file, both devices advanced independently → conflict.
        db.append_op(op("d1", "rename", Some("f9"))).unwrap();
        db.append_op(op("d2", "rename", Some("f9"))).unwrap();
        // Different files → never a conflict.
        db.append_op(op("d1", "tag", Some("other"))).unwrap();
        let conflicts = db.op_conflicts().unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].0.file_id.as_deref(), Some("f9"));
        let only_d1 = ["d1".to_string()].into_iter().collect();
        assert!(db.op_conflicts_for_devices(&only_d1).unwrap().is_empty());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn owner_scoped_oplog_filters_before_applying_page_limit() {
        let (db, path) = test_db("oplog-owner-page");
        let first = db.append_op(op("owner-a", "tag", Some("f1"))).unwrap();
        db.append_op(op("owner-b", "rename", Some("secret-file")))
            .unwrap();
        let third = db.append_op(op("owner-a", "star", Some("f2"))).unwrap();
        let owner_a = ["owner-a".to_string()].into_iter().collect();
        let page = db.list_ops_since_for_devices(0, 2, &owner_a).unwrap();
        assert_eq!(
            page.iter().map(|op| op.seq).collect::<Vec<_>>(),
            vec![first.seq, third.seq]
        );
        assert!(page.iter().all(|op| op.device_id == "owner-a"));
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
