// CyberManju OS — browser-native database for the WASM demo build
//
// This is a REAL redb database (`cybermanju.db`), not a mock: it uses the
// same table names (`cybermanju_db::Database::get_*_table()`) and the same
// JSON row shapes (`cybermanju_types`) as the server, so a file written here
// opens cleanly under the native backend later.
//
// Storage: `OpfsBackend` (`opfs_backend.rs`, durable, worker-only) with an
// in-memory fallback when OPFS is unavailable. Opening is async (OPFS
// handles resolve via Promises); every call after `db_open` is synchronous,
// which is exactly what redb's `StorageBackend` trait needs.
//
// `db_dispatch(op, args_json)` always returns an envelope:
// `{"ok":true,"data":…}` or `{"ok":false,"error":"prefix: detail"}` using
// the same `"prefix: detail"` contract as the server, so the terminal and
// the account manager render identically on all transports.
//
// Honest demo limits (documented, not hidden):
// - tokens/PATs live in the `sync_secrets` table in cleartext inside the
//   origin-private file. Same-origin private, but weaker than the server's
//   sealed credential store.
// - `users.authenticate` returns a random demo token; nothing gates on it —
//   the demo build is a single-user local vault.
// - disk attach/check are catalog operations: there is no sealed container
//   or fsck here, so they verify the catalog row, not bytes.

use cybermanju_db::Database as DbDefs;
use js_sys::Reflect;
use redb::{Database as RedbDatabase, ReadableTable, StorageBackend};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use argon2::{PasswordHasher, PasswordVerifier};

use crate::opfs_backend::OpfsBackend;

const DB_FILE: &str = "cybermanju.db";
const DEMO_VOLUME: &str = "wasm-demo-volume";
const MAX_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;

thread_local! {
    static DB: RefCell<Option<DbHandle>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, Default)]
struct MemoryBackend(std::sync::Arc<std::sync::RwLock<Vec<u8>>>);

impl MemoryBackend {
    fn new() -> Self {
        Self(std::sync::Arc::new(std::sync::RwLock::new(Vec::new())))
    }
}

impl StorageBackend for MemoryBackend {
    fn len(&self) -> Result<u64, std::io::Error> {
        self.0
            .read()
            .map(|g| g.len() as u64)
            .map_err(|_| std::io::Error::other("memory backend poisoned"))
    }

    fn read(&self, offset: u64, len: usize) -> Result<Vec<u8>, std::io::Error> {
        let guard = self
            .0
            .read()
            .map_err(|_| std::io::Error::other("memory backend poisoned"))?;
        let offset = usize::try_from(offset)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "offset OOR"))?;
        if offset + len <= guard.len() {
            Ok(guard[offset..offset + len].to_vec())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "read OOR",
            ))
        }
    }

    fn set_len(&self, len: u64) -> Result<(), std::io::Error> {
        let mut guard = self
            .0
            .write()
            .map_err(|_| std::io::Error::other("memory backend poisoned"))?;
        let len = usize::try_from(len)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "len OOR"))?;
        if guard.len() < len {
            guard.resize(len, 0);
        } else {
            guard.truncate(len);
        }
        Ok(())
    }

    fn sync_data(&self, _eventual: bool) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn write(&self, offset: u64, data: &[u8]) -> Result<(), std::io::Error> {
        let mut guard = self
            .0
            .write()
            .map_err(|_| std::io::Error::other("memory backend poisoned"))?;
        let offset = usize::try_from(offset)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "offset OOR"))?;
        if offset + data.len() <= guard.len() {
            guard[offset..offset + data.len()].copy_from_slice(data);
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "write OOR",
            ))
        }
    }
}

#[derive(Debug, Clone)]
enum Backend {
    Opfs(OpfsBackend),
    Memory(MemoryBackend),
}

impl StorageBackend for Backend {
    fn len(&self) -> Result<u64, std::io::Error> {
        match self {
            Backend::Opfs(b) => b.len(),
            Backend::Memory(b) => b.len(),
        }
    }

    fn read(&self, offset: u64, len: usize) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Backend::Opfs(b) => b.read(offset, len),
            Backend::Memory(b) => b.read(offset, len),
        }
    }

    fn set_len(&self, len: u64) -> Result<(), std::io::Error> {
        match self {
            Backend::Opfs(b) => b.set_len(len),
            Backend::Memory(b) => b.set_len(len),
        }
    }

    fn sync_data(&self, eventual: bool) -> Result<(), std::io::Error> {
        match self {
            Backend::Opfs(b) => b.sync_data(eventual),
            Backend::Memory(b) => b.sync_data(eventual),
        }
    }

    fn write(&self, offset: u64, data: &[u8]) -> Result<(), std::io::Error> {
        match self {
            Backend::Opfs(b) => b.write(offset, data),
            Backend::Memory(b) => b.write(offset, data),
        }
    }
}

struct DbHandle {
    db: RedbDatabase,
    /// Retained so `db_snapshot`/`db_restore` can read and rewrite the full
    /// file image. Both backends (OPFS-durable and in-memory) are snapshotable,
    /// so `.cybermanju` CREATE/EXPORT/IMPORT works even without OPFS.
    backend: Backend,
}

// ─── JS bridges ────────────────────────────────────────────────────────

fn js_global(name: &str) -> Result<JsValue, String> {
    Reflect::get(&js_sys::global(), &JsValue::from_str(name))
        .map_err(|e| format!("unavailable: global.{name} missing ({e:?})"))
}

fn call0(obj: &JsValue, name: &str) -> Result<JsValue, String> {
    let func: js_sys::Function = Reflect::get(obj, &JsValue::from_str(name))
        .map_err(|e| format!("unavailable: {name} missing ({e:?})"))?
        .dyn_into()
        .map_err(|_| format!("unavailable: {name} is not a function"))?;
    Reflect::apply(&func, obj, &js_sys::Array::new())
        .map_err(|e| format!("unavailable: {name} threw ({e:?})"))
}

fn new_id() -> String {
    // `crypto.randomUUID` exists in workers and on the main thread. The
    // Reflect probe below only ever runs in a browser (host unit tests go
    // straight to the counter, which is unique per process).
    #[cfg(target_arch = "wasm32")]
    if let Ok(crypto) = js_global("crypto") {
        if let Ok(func) = Reflect::get(&crypto, &JsValue::from_str("randomUUID")) {
            if let Ok(func) = func.dyn_into::<js_sys::Function>() {
                if let Ok(v) = Reflect::apply(&func, &crypto, &js_sys::Array::new()) {
                    if let Some(s) = v.as_string() {
                        return s;
                    }
                }
            }
        }
    }
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("id-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

async fn open_opfs_handle(file: &str) -> Result<JsValue, String> {
    let navigator = js_global("navigator")?;
    let storage = Reflect::get(&navigator, &JsValue::from_str("storage"))
        .map_err(|e| format!("unavailable: navigator.storage missing ({e:?})"))?;
    if storage.is_undefined() || storage.is_null() {
        return Err("unavailable: navigator.storage is missing (insecure context?)".to_string());
    }
    let dir = JsFuture::from(
        call0(&storage, "getDirectory")?
            .dyn_into::<js_sys::Promise>()
            .map_err(|_| "unavailable: getDirectory is not async".to_string())?,
    )
    .await
    .map_err(|e| format!("unavailable: OPFS directory failed ({e:?})"))?;
    let opts = js_sys::Object::new();
    Reflect::set(&opts, &JsValue::from_str("create"), &JsValue::TRUE)
        .map_err(|e| format!("unavailable: opts failed ({e:?})"))?;
    let argv = js_sys::Array::new();
    argv.push(&JsValue::from_str(file));
    argv.push(&opts.into());
    let get_fh: js_sys::Function = Reflect::get(&dir, &JsValue::from_str("getFileHandle"))
        .map_err(|e| format!("unavailable: getFileHandle missing ({e:?})"))?
        .dyn_into()
        .map_err(|_| "unavailable: getFileHandle is not a function".to_string())?;
    let fh = JsFuture::from(
        Reflect::apply(&get_fh, &dir, &argv)
            .map_err(|e| format!("unavailable: getFileHandle threw ({e:?})"))?
            .dyn_into::<js_sys::Promise>()
            .map_err(|_| "unavailable: getFileHandle is not async".to_string())?,
    )
    .await
    .map_err(|e| format!("unavailable: file handle failed ({e:?})"))?;
    let handle = JsFuture::from(
        call0(&fh, "createSyncAccessHandle")?
            .dyn_into::<js_sys::Promise>()
            .map_err(|_| {
                "unavailable: createSyncAccessHandle is not async — database must run in a Dedicated Worker".to_string()
            })?,
    )
    .await
    .map_err(|e| format!("unavailable: sync access handle failed ({e:?})"))?;
    Ok(handle)
}

fn open_all_tables(db: &RedbDatabase) -> Result<(), String> {
    let mut txn = db
        .begin_write()
        .map_err(|e| format!("network: begin_write failed: {e}"))?;
    // Two-phase commits on OPFS: `flush()` is not `fsync()` (see module
    // docs), so every write pays for the extra barrier.
    txn.set_two_phase_commit(true);
    {
        txn.open_table(DbDefs::get_files_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_accounts_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_collections_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_collection_items_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_face_groups_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_loose_groups_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_encryption_keys_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_locations_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_users_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_user_file_perms_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_sync_configs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_parent_index_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_trash_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_audit_log_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_file_versions_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_share_links_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_sync_files_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_sync_runs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_sync_secrets_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_disks_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_volumes_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_block_map_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_scrub_runs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_repairs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_chunk_refs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_leases_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_provider_health_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_compute_tasks_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_shell_history_table())
            .map_err(|e| e.to_string())?;
        // Keep this list in lockstep with the native `Database::new`
        // (`crates/db/src/database.rs`): a read txn errors
        // `Table '<name>' does not exist` for any table skipped here, which
        // surfaces as a UI toast on every poll (e.g. `cron.list` on a fresh
        // browser DB failed before `schedules` was added).
        txn.open_table(DbDefs::get_agent_configs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_agent_sessions_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_agent_memories_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_schedules_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_schedule_runs_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_kv_table())
            .map_err(|e| e.to_string())?;
        txn.open_table(DbDefs::get_secrets_table())
            .map_err(|e| e.to_string())?;
    }
    txn.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Open (or create) the demo database. Resolves OPFS in this context;
/// falls back to a process-local in-memory database. The in-memory image is
/// kept in a shared (`Arc`) backend so `db_snapshot`/`db_restore` — and with
/// them `.cybermanju` CREATE/EXPORT/IMPORT — work on both backends.
#[wasm_bindgen]
pub async fn db_open() -> Result<JsValue, JsValue> {
    let (db, backend_name, backend) = match open_opfs_handle(DB_FILE).await {
        Ok(handle) => {
            let backend = OpfsBackend::new(handle, DB_FILE);
            let db = redb::Builder::new()
                .create_with_backend(Backend::Opfs(backend.clone()))
                .map_err(|e| format!("network: redb open failed: {e}"))?;
            (db, "opfs", Backend::Opfs(backend))
        }
        Err(detail) => {
            let backend = MemoryBackend::new();
            let db = redb::Builder::new()
                .create_with_backend(Backend::Memory(backend.clone()))
                .map_err(|e| format!("network: in-memory open failed: {e}"))?;
            let _ = detail;
            (db, "memory", Backend::Memory(backend))
        }
    };
    if let Err(e) = open_all_tables(&db) {
        return Ok(JsValue::from_str(&envelope_json(
            false,
            &serde_json::Value::Null,
            Some(e),
        )));
    }
    DB.with(|cell| {
        *cell.borrow_mut() = Some(DbHandle { db, backend });
    });
    Ok(JsValue::from_str(&envelope_json(
        true,
        &serde_json::json!({ "backend": backend_name, "file": DB_FILE }),
        None,
    )))
}

pub(crate) fn envelope_json(ok: bool, data: &serde_json::Value, error: Option<String>) -> String {
    let mut map = serde_json::Map::new();
    map.insert("ok".to_string(), serde_json::Value::Bool(ok));
    if ok {
        map.insert("data".to_string(), data.clone());
    } else if let Some(e) = error {
        map.insert("error".to_string(), serde_json::Value::String(e));
    }
    serde_json::Value::Object(map).to_string()
}

// ─── table helpers ───────────────────────────────────────────────────

fn with_db<T>(f: impl FnOnce(&RedbDatabase) -> Result<T, String>) -> Result<T, String> {
    DB.with(|cell| {
        let borrow = cell.borrow();
        let handle = borrow
            .as_ref()
            .ok_or_else(|| "unavailable: database is not open (call db_open first)".to_string())?;
        f(&handle.db)
    })
}

fn read_all(
    table: redb::TableDefinition<'static, &'static str, &'static str>,
) -> Result<Vec<(String, String)>, String> {
    with_db(|db| {
        let txn = db.begin_read().map_err(|e| e.to_string())?;
        let t = txn.open_table(table).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for entry in t.iter().map_err(|e| e.to_string())? {
            let (k, v) = entry.map_err(|e| e.to_string())?;
            out.push((k.value().to_string(), v.value().to_string()));
        }
        Ok(out)
    })
}

fn read_one(
    table: redb::TableDefinition<'static, &'static str, &'static str>,
    key: &str,
) -> Result<Option<String>, String> {
    with_db(|db| {
        let txn = db.begin_read().map_err(|e| e.to_string())?;
        let t = txn.open_table(table).map_err(|e| e.to_string())?;
        Ok(t.get(key)
            .map_err(|e| e.to_string())?
            .map(|v| v.value().to_string()))
    })
}

fn write_one(
    table: redb::TableDefinition<'static, &'static str, &'static str>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    with_db(|db| {
        let mut txn = db.begin_write().map_err(|e| e.to_string())?;
        txn.set_two_phase_commit(true);
        {
            let mut t = txn.open_table(table).map_err(|e| e.to_string())?;
            t.insert(key, value).map_err(|e| e.to_string())?;
        }
        txn.commit().map_err(|e| e.to_string())?;
        Ok(())
    })
}

fn delete_one(
    table: redb::TableDefinition<'static, &'static str, &'static str>,
    key: &str,
) -> Result<bool, String> {
    with_db(|db| {
        let mut txn = db.begin_write().map_err(|e| e.to_string())?;
        txn.set_two_phase_commit(true);
        let removed = {
            let mut t = txn.open_table(table).map_err(|e| e.to_string())?;
            let removed = t.remove(key).map_err(|e| e.to_string())?.is_some();
            removed
        };
        txn.commit().map_err(|e| e.to_string())?;
        Ok(removed)
    })
}

fn arg<'a>(args: &'a serde_json::Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("invalid: missing string argument '{name}'"))
}

fn opt_arg(args: &serde_json::Value, name: &str) -> Option<String> {
    args.get(name).and_then(|v| {
        if v.is_null() {
            None
        } else if let Some(s) = v.as_str() {
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        } else {
            None
        }
    })
}

/// One kv entry: ≤ 64 MiB (fits a photo, stays far below wasm OOM),
/// namespace-per-key (`secret:`, `config:`, `content:<id>`, `volume:<path>`).
const MAX_KV_BYTES: usize = 64 * 1024 * 1024;

fn kv_key(args: &serde_json::Value) -> Result<String, String> {
    let key = arg(args, "key")?.trim().to_string();
    if key.is_empty() {
        return Err("invalid: key is required".to_string());
    }
    if key.len() > 512 {
        return Err("invalid: key must be at most 512 characters".to_string());
    }
    if key.chars().any(|c| c.is_control()) {
        return Err("invalid: key must not contain control characters".to_string());
    }
    Ok(key)
}

// ─── validation (mirrors crates/web/src/security.rs bounds) ──────────

fn validate_username(v: &str) -> Result<(), String> {
    let v = v.trim();
    if v.is_empty() {
        return Err("Username is required".to_string());
    }
    if v.len() > 64 {
        return Err("Username must be at most 64 characters".to_string());
    }
    if v.chars().any(|c| c.is_control()) {
        return Err("Username must not contain control characters".to_string());
    }
    if !v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err("Username may only contain letters, digits, '.', '_' and '-'".to_string());
    }
    if v.starts_with('.') || v.starts_with('-') {
        return Err("Username must not start with '.' or '-'".to_string());
    }
    Ok(())
}

fn validate_password(v: &str) -> Result<(), String> {
    if v.len() < 8 {
        return Err("Password must be at least 8 characters".to_string());
    }
    if v.len() > 1024 {
        return Err("Password must be at most 1024 characters".to_string());
    }
    Ok(())
}

fn argon2id() -> Result<argon2::Argon2<'static>, String> {
    let params = argon2::Params::new(19456, 2, 1, Some(32))
        .map_err(|e| format!("unavailable: argon2 params rejected ({e})"))?;
    Ok(argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    ))
}

// ─── dispatcher ──────────────────────────────────────────────────────

/// One JSON call in, one envelope out. `args_json` is an object; mutating
/// ops accept `now` (ISO-8601, supplied by the worker — no clock dep).
#[wasm_bindgen]
pub fn db_dispatch(op: &str, args_json: &str) -> String {
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
    let now = args
        .get("now")
        .and_then(|v| v.as_str())
        .unwrap_or("1970-01-01T00:00:00Z");
    let result: Result<serde_json::Value, String> = (|| match op {
        // ── accounts ──
        "accounts.list" => Ok(serde_json::Value::Array(
            read_all(DbDefs::get_accounts_table())?
                .into_iter()
                .filter_map(|(_, v)| serde_json::from_str(&v).ok())
                .collect(),
        )),
        "accounts.create" => {
            let name = arg(&args, "name")?.trim().to_string();
            if name.is_empty() {
                return Err("invalid: account name is required".to_string());
            }
            let account_type = args
                .get("accountType")
                .and_then(|v| v.as_str())
                .unwrap_or("local")
                .to_string();
            let rows = read_all(DbDefs::get_accounts_table())?;
            let id = new_id();
            let account = serde_json::json!({
                "id": id,
                "name": name,
                "accountType": account_type,
                "path": opt_arg(&args, "path").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "color": opt_arg(&args, "color").unwrap_or_else(|| "#6366f1".to_string()),
                "isActive": rows.is_empty(),
                "createdAt": now,
                "updatedAt": now,
            });
            write_one(
                DbDefs::get_accounts_table(),
                account["id"].as_str().unwrap_or(""),
                &account.to_string(),
            )?;
            Ok(account)
        }
        "accounts.switch" => {
            let target = arg(&args, "accountId")?.to_string();
            let rows = read_all(DbDefs::get_accounts_table())?;
            if !rows.iter().any(|(k, _)| k == &target) {
                return Err(format!("not_found: account {target}"));
            }
            let mut switched = serde_json::Value::Null;
            for (key, raw) in rows {
                if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    let active = key == target;
                    v["isActive"] = serde_json::Value::Bool(active);
                    v["updatedAt"] = serde_json::Value::String(now.to_string());
                    write_one(DbDefs::get_accounts_table(), &key, &v.to_string())?;
                    if active {
                        switched = v;
                    }
                }
            }
            Ok(switched)
        }
        "accounts.delete" => {
            let target = arg(&args, "accountId")?.to_string();
            let raw = read_one(DbDefs::get_accounts_table(), &target)?
                .ok_or_else(|| format!("not_found: account {target}"))?;
            let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            if v.get("isActive").and_then(|b| b.as_bool()).unwrap_or(false) {
                return Err(
                    "Cannot delete the active account. Switch to another account first."
                        .to_string(),
                );
            }
            Ok(serde_json::Value::Bool(delete_one(
                DbDefs::get_accounts_table(),
                &target,
            )?))
        }
        // ── sync configs (+ side-table secrets, like the server) ──
        "sync.list" => {
            let mut out = Vec::new();
            for (_, v) in read_all(DbDefs::get_sync_configs_table())? {
                if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&v) {
                    out.push(cfg);
                }
            }
            Ok(serde_json::Value::Array(out))
        }
        "sync.save" => {
            let mut cfg = args
                .get("config")
                .cloned()
                .ok_or_else(|| "invalid: missing 'config' object".to_string())?;
            let id = cfg
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = if id.is_empty() { new_id() } else { id };
            cfg["id"] = serde_json::Value::String(id.clone());
            let incoming_token = cfg
                .get("token")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            // The row never carries the secret (mirrors `skip_serializing`).
            if let Some(map) = cfg.as_object_mut() {
                map.remove("token");
            }
            write_one(DbDefs::get_sync_configs_table(), &id, &cfg.to_string())?;
            // Secret semantics mirror the server (`save_config`): an absent
            // token leaves the stored secret untouched (rename-only saves),
            // an explicit empty token clears it (OAuth revoke).
            if let Some(token) = incoming_token {
                if token.trim().is_empty() {
                    let _ = delete_one(DbDefs::get_sync_secrets_table(), &id);
                } else {
                    write_one(DbDefs::get_sync_secrets_table(), &id, token.trim())?;
                }
            }
            Ok(cfg)
        }
        "sync.delete" => {
            let id = arg(&args, "configId")?.to_string();
            if !delete_one(DbDefs::get_sync_configs_table(), &id)? {
                return Err(format!("not_found: sync config {id}"));
            }
            let _ = delete_one(DbDefs::get_sync_secrets_table(), &id);
            Ok(serde_json::Value::Bool(true))
        }
        "sync.secret" => Ok(
            read_one(DbDefs::get_sync_secrets_table(), arg(&args, "configId")?)?
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        ),
        // ── scheduler (cron) ──
        // Pure persistence: the browser owns validation + next-fire
        // computation via the TS twin (`src/utils/schedule.ts`), and the
        // store-level tick runs due rows through the exec dispatcher. No
        // background thread exists in WASM — this matches the plan.
        "cron.list" => {
            let mut out = Vec::new();
            for (_, v) in read_all(DbDefs::get_schedules_table())? {
                if let Ok(row) = serde_json::from_str::<serde_json::Value>(&v) {
                    out.push(row);
                }
            }
            Ok(serde_json::Value::Array(out))
        }
        "cron.save" => {
            let mut row = args
                .get("row")
                .cloned()
                .ok_or_else(|| "invalid: missing 'row' object".to_string())?;
            let id = row
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = if id.is_empty() { new_id() } else { id };
            row["id"] = serde_json::Value::String(id.clone());
            let path = row
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !path.to_lowercase().ends_with(".cybsh") {
                return Err(format!(
                    "invalid: schedule path must be a .cybsh script (got `{path}`)"
                ));
            }
            if row
                .get("expr")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .is_empty()
            {
                return Err("invalid: schedule expression is required".to_string());
            }
            write_one(DbDefs::get_schedules_table(), &id, &row.to_string())?;
            Ok(row)
        }
        "cron.delete" => {
            let id = arg(&args, "id")?.to_string();
            if !delete_one(DbDefs::get_schedules_table(), &id)? {
                return Err(format!("not_found: schedule {id}"));
            }
            Ok(serde_json::Value::Bool(true))
        }
        "cron.history" => {
            let id = arg(&args, "id")?.to_string();
            let mut out = Vec::new();
            for (_, v) in read_all(DbDefs::get_schedule_runs_table())? {
                if let Ok(run) = serde_json::from_str::<serde_json::Value>(&v) {
                    if run.get("scheduleId").and_then(|x| x.as_str()) == Some(id.as_str()) {
                        out.push(run);
                    }
                }
            }
            // Newest first (mirror the server's `finished_at` sort).
            out.sort_by(|a, b| {
                let ka = a.get("finishedAt").and_then(|x| x.as_str()).unwrap_or("");
                let kb = b.get("finishedAt").and_then(|x| x.as_str()).unwrap_or("");
                kb.cmp(ka)
            });
            out.truncate(20);
            Ok(serde_json::Value::Array(out))
        }
        "cron.runRecord" => {
            // The browser tick appends a fire result (it executes the script
            // client-side; there is no daemon thread to record for it).
            let run = args
                .get("run")
                .cloned()
                .ok_or_else(|| "invalid: missing 'run' object".to_string())?;
            let run_id = run
                .get("runId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let run_id = if run_id.is_empty() { new_id() } else { run_id };
            write_one(DbDefs::get_schedule_runs_table(), &run_id, &run.to_string())?;
            Ok(run)
        }
        // ── secrets keystore (Phase 3) ──
        // The session vault passphrase is an argument on create/update/
        // reveal — never stored. List/get strip `value_sealed` the same
        // way the native `SecretMeta` projection does.
        "secrets.list" => {
            let mut out = Vec::new();
            for (_, v) in read_all(DbDefs::get_secrets_table())? {
                if let Ok(mut row) = serde_json::from_str::<serde_json::Value>(&v) {
                    if let Some(map) = row.as_object_mut() {
                        map.remove("valueSealed");
                    }
                    out.push(row);
                }
            }
            // Title sort, mirror the native list.
            out.sort_by_key(|r| {
                r.get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_lowercase()
            });
            Ok(serde_json::Value::Array(out))
        }
        "secrets.get" => {
            let id = arg(&args, "id")?.to_string();
            let raw = read_one(DbDefs::get_secrets_table(), &id)?
                .ok_or_else(|| format!("not_found: no secret `{id}`"))?;
            let mut row: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| format!("integrity: bad row: {e}"))?;
            if let Some(map) = row.as_object_mut() {
                map.remove("valueSealed");
            }
            Ok(row)
        }
        "secrets.create" | "secrets.update" => {
            let mut row = args
                .get("row")
                .cloned()
                .ok_or_else(|| "invalid: missing 'row' object".to_string())?;
            let id = row
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = if id.is_empty() { new_id() } else { id };
            row["id"] = serde_json::Value::String(id.clone());
            let title = row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if title.is_empty() {
                return Err("invalid: secret title is required".to_string());
            }
            row["title"] = serde_json::Value::String(title);
            // Re-seal when a plaintext `value` is present; otherwise keep
            // the existing sealed blob (update semantics).
            if let Some(plaintext) = args.get("value").and_then(|v| v.as_str()) {
                let passphrase = opt_arg(&args, "passphrase").unwrap_or_default();
                if passphrase.is_empty() {
                    return Err(
                        "unsupported: no vault passphrase — unlock the session first".to_string(),
                    );
                }
                if plaintext.trim().is_empty() {
                    row["valueSealed"] = serde_json::Value::String(String::new());
                    row["hasValue"] = serde_json::Value::Bool(false);
                } else {
                    let sealed = crate::crypto::seal_blob_str(&passphrase, plaintext)
                        .map_err(|e| format!("integrity: failed to seal secret: {e}"))?;
                    row["valueSealed"] = serde_json::Value::String(sealed);
                    row["hasValue"] = serde_json::Value::Bool(true);
                }
            } else if op == "secrets.create" {
                row["valueSealed"] = serde_json::Value::String(String::new());
                row["hasValue"] = serde_json::Value::Bool(false);
            } else if row.get("valueSealed").is_none() {
                // Partial update without a value on a row that never had one.
                row["valueSealed"] = serde_json::Value::String(String::new());
                row["hasValue"] = serde_json::Value::Bool(false);
            }
            // Timestamps mirror the native `now()`; fill defaults when
            // absent (the caller may pass `now` for deterministic tests).
            for field in ["createdAt", "updatedAt"] {
                if row
                    .get(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .is_empty()
                {
                    row[field] = serde_json::Value::String(now.to_string());
                }
            }
            write_one(DbDefs::get_secrets_table(), &id, &row.to_string())?;
            if let Some(map) = row.as_object_mut() {
                map.remove("valueSealed");
            }
            Ok(row)
        }
        "secrets.delete" => {
            let id = arg(&args, "id")?.to_string();
            if !delete_one(DbDefs::get_secrets_table(), &id)? {
                return Err(format!("not_found: no secret `{id}`"));
            }
            Ok(serde_json::Value::Bool(true))
        }
        "secrets.reveal" => {
            let id = arg(&args, "id")?.to_string();
            let raw = read_one(DbDefs::get_secrets_table(), &id)?
                .ok_or_else(|| format!("not_found: no secret `{id}`"))?;
            let row: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| format!("integrity: bad row: {e}"))?;
            let sealed = row
                .get("valueSealed")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if sealed.is_empty() {
                return Err(format!("not_found: secret `{id}` has no stored value"));
            }
            let passphrase = opt_arg(&args, "passphrase").unwrap_or_default();
            if passphrase.is_empty() {
                return Err(
                    "unsupported: no vault passphrase — unlock the session first".to_string(),
                );
            }
            let value = crate::crypto::open_blob_str(&passphrase, sealed)
                .map_err(|e| format!("integrity: failed to open sealed secret: {e}"))?;
            Ok(serde_json::json!({ "value": value }))
        }
        // ── users ──
        "users.list" => {
            let mut out = Vec::new();
            for (_, v) in read_all(DbDefs::get_users_table())? {
                if let Ok(mut u) = serde_json::from_str::<serde_json::Value>(&v) {
                    if let Some(map) = u.as_object_mut() {
                        map.remove("passwordHash");
                    }
                    out.push(u);
                }
            }
            Ok(serde_json::Value::Array(out))
        }
        "users.register" => {
            let username = arg(&args, "username")?.trim().to_string();
            let password = arg(&args, "password")?.to_string();
            validate_username(&username)?;
            validate_password(&password)?;
            for (_, v) in read_all(DbDefs::get_users_table())? {
                if let Ok(u) = serde_json::from_str::<serde_json::Value>(&v) {
                    if u.get("username").and_then(|n| n.as_str()) == Some(&username) {
                        return Err(format!("Username '{username}' already exists"));
                    }
                }
            }
            let requested = opt_arg(&args, "role").unwrap_or_default();
            let empty = read_all(DbDefs::get_users_table())?.is_empty();
            let role = if ["admin", "user", "viewer"].contains(&requested.as_str()) {
                requested
            } else if empty {
                "admin".to_string()
            } else {
                "user".to_string()
            };
            let salt = argon2::password_hash::SaltString::generate(
                &mut argon2::password_hash::rand_core::OsRng,
            );
            let hash = argon2id()?
                .hash_password(password.as_bytes(), &salt)
                .map_err(|e| format!("unavailable: argon2 failed ({e})"))?
                .to_string();
            let id = new_id();
            let mut user = serde_json::json!({
                "id": id,
                "username": username,
                "passwordHash": hash,
                "displayName": opt_arg(&args, "displayName").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "role": role,
                "isActive": true,
                "createdAt": now,
                "updatedAt": now,
            });
            write_one(DbDefs::get_users_table(), &id, &user.to_string())?;
            if let Some(map) = user.as_object_mut() {
                map.remove("passwordHash");
            }
            Ok(user)
        }
        "users.delete" => {
            let id = arg(&args, "userId")?.to_string();
            if !delete_one(DbDefs::get_users_table(), &id)? {
                return Err(format!("not_found: user {id}"));
            }
            Ok(serde_json::Value::Bool(true))
        }
        "users.set_role" => {
            let id = arg(&args, "userId")?.to_string();
            let role = arg(&args, "role")?.to_string();
            if !["admin", "user", "viewer"].contains(&role.as_str()) {
                return Err(format!("invalid: unknown role '{role}'"));
            }
            let raw = read_one(DbDefs::get_users_table(), &id)?
                .ok_or_else(|| format!("not_found: user {id}"))?;
            let mut user: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            user["role"] = serde_json::Value::String(role);
            user["updatedAt"] = serde_json::Value::String(now.to_string());
            write_one(DbDefs::get_users_table(), &id, &user.to_string())?;
            if let Some(map) = user.as_object_mut() {
                map.remove("passwordHash");
            }
            Ok(user)
        }
        "users.authenticate" => {
            let username = arg(&args, "username")?.trim().to_string();
            let password = arg(&args, "password")?.to_string();
            let mut found: Option<serde_json::Value> = None;
            for (_, v) in read_all(DbDefs::get_users_table())? {
                if let Ok(u) = serde_json::from_str::<serde_json::Value>(&v) {
                    if u.get("username").and_then(|n| n.as_str()) == Some(username.as_str()) {
                        found = Some(u);
                        break;
                    }
                }
            }
            let user = found.ok_or_else(|| "Invalid credentials".to_string())?;
            let stored = user
                .get("passwordHash")
                .and_then(|h| h.as_str())
                .ok_or_else(|| "Invalid credentials".to_string())?;
            let parsed = argon2::password_hash::PasswordHash::new(stored)
                .map_err(|_| "Invalid credentials".to_string())?;
            argon2id()?
                .verify_password(password.as_bytes(), &parsed)
                .map_err(|_| "Invalid credentials".to_string())?;
            Ok(serde_json::json!({
                "userId": user.get("id"),
                "username": user.get("username"),
                "role": user.get("role"),
                "displayName": user.get("displayName"),
                "token": format!("demo-{}", new_id()),
            }))
        }
        // ── disks + merged volume ──
        "disks.list" => Ok(serde_json::Value::Array(
            read_all(DbDefs::get_disks_table())?
                .into_iter()
                .filter_map(|(_, v)| serde_json::from_str(&v).ok())
                .collect(),
        )),
        "disks.create" => {
            let config_id = arg(&args, "configId")?.to_string();
            // Parity with `cybermanju-disk::create`: the sealed container is
            // passphrase-derived, so an empty passphrase must fail here too
            // instead of creating a disk on Pages that AppImage then refuses
            // to attach.
            let passphrase = args
                .get("passphrase")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if passphrase.is_empty() {
                return Err("unsupported: a disk passphrase must not be empty".to_string());
            }
            let size = args
                .get("sizeBytes")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "invalid: missing sizeBytes".to_string())?;
            if size < 1024 * 1024 {
                return Err("unsupported: demo disk size must be ≥ 1 MiB".to_string());
            }
            let count = read_all(DbDefs::get_disks_table())?.len() as u64;
            let id = new_id();
            let provider = opt_arg(&args, "provider").unwrap_or_else(|| {
                // Default to the bound provider's backend so the disk card
                // reads "github", not "local".
                read_one(DbDefs::get_sync_configs_table(), &config_id)
                    .ok()
                    .flatten()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .and_then(|cfg| {
                        cfg.get("backendType")
                            .and_then(|b| b.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "local".to_string())
            });
            let disk = serde_json::json!({
                "id": id,
                "name": opt_arg(&args, "name").unwrap_or_else(|| format!("disk-{}", count + 1)),
                "provider": provider,
                "configId": config_id,
                "volumeUuid": DEMO_VOLUME,
                "capacityBytes": size,
                "blockSize": 4096,
                "usedBytes": 0,
                "placementOrder": count,
                "state": "attached",
                "health": "ok",
                "containerPath": format!("opfs:{DB_FILE}#/disks/{id}"),
                "blocksWrittenSinceCheckpoint": 0,
                "createdAt": now,
                "updatedAt": now,
            });
            write_one(DbDefs::get_disks_table(), &id, &disk.to_string())?;
            Ok(disk)
        }
        "disks.attach" => {
            let id = arg(&args, "id")?.to_string();
            let mut disk = get_disk(&id)?;
            disk["state"] = serde_json::Value::String("attached".to_string());
            disk["updatedAt"] = serde_json::Value::String(now.to_string());
            write_one(DbDefs::get_disks_table(), &id, &disk.to_string())?;
            Ok(disk)
        }
        "disks.detach" => {
            let id = arg(&args, "id")?.to_string();
            let mut disk = get_disk(&id)?;
            disk["state"] = serde_json::Value::String("detached".to_string());
            disk["updatedAt"] = serde_json::Value::String(now.to_string());
            write_one(DbDefs::get_disks_table(), &id, &disk.to_string())?;
            Ok(disk)
        }
        "disks.resize" => {
            let id = arg(&args, "id")?.to_string();
            let size = args
                .get("sizeBytes")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "invalid: missing sizeBytes".to_string())?;
            let mut disk = get_disk(&id)?;
            let used = disk.get("usedBytes").and_then(|v| v.as_u64()).unwrap_or(0);
            if size < used {
                return Err(format!(
                    "unsupported: size {size} is below {used} used bytes"
                ));
            }
            disk["capacityBytes"] = serde_json::Value::Number(size.into());
            disk["updatedAt"] = serde_json::Value::String(now.to_string());
            write_one(DbDefs::get_disks_table(), &id, &disk.to_string())?;
            Ok(disk)
        }
        "disks.check" => {
            let id = arg(&args, "id")?.to_string();
            let disk = get_disk(&id)?;
            Ok(serde_json::json!({
                "ok": true,
                "diskId": id,
                "state": disk.get("state"),
                "health": disk.get("health"),
                "detail": "demo check: catalog row present and well-formed",
            }))
        }
        "disks.key-holder" => {
            let id = arg(&args, "id")?.to_string();
            let rows = read_all(DbDefs::get_disks_table())?;
            if !rows.iter().any(|(k, _)| k == &id) {
                return Err(format!("not_found: disk '{id}' not found"));
            }
            let mut holder: Option<serde_json::Value> = None;
            for (key, value) in rows {
                if let Ok(mut disk) = serde_json::from_str::<serde_json::Value>(&value) {
                    let is_holder = key == id;
                    disk["holdsKeys"] = serde_json::Value::Bool(is_holder);
                    disk["updatedAt"] = serde_json::Value::String(now.to_string());
                    write_one(DbDefs::get_disks_table(), &key, &disk.to_string())?;
                    if is_holder {
                        holder = Some(disk);
                    }
                }
            }
            holder.ok_or_else(|| format!("not_found: disk '{id}' not found"))
        }
        "volume.df" => {
            let mut total = 0u64;
            let mut used = 0u64;
            let mut count = 0u64;
            let mut attached = Vec::new();
            for (_, v) in read_all(DbDefs::get_disks_table())? {
                if let Ok(d) = serde_json::from_str::<serde_json::Value>(&v) {
                    if d.get("state").and_then(|s| s.as_str()) == Some("attached") {
                        total = total.saturating_add(
                            d.get("capacityBytes").and_then(|n| n.as_u64()).unwrap_or(0),
                        );
                        used = used.saturating_add(
                            d.get("usedBytes").and_then(|n| n.as_u64()).unwrap_or(0),
                        );
                        count += 1;
                        attached.push(serde_json::json!({
                            "id": d.get("id"),
                            "name": d.get("name"),
                            "provider": d.get("provider"),
                            "capacityBytes": d.get("capacityBytes"),
                            "state": d.get("state"),
                            "health": d.get("health"),
                            "createdAt": d.get("createdAt"),
                            "usedBytes": d.get("usedBytes"),
                            "compute": 1,
                        }));
                    }
                }
            }
            Ok(serde_json::json!({
                "totalBytes": total,
                "usedBytes": used,
                "freeBytes": total.saturating_sub(used),
                "root": "/",
                "diskCount": count,
                "attachedBytes": total,
                "scratchBytes": 0,
                "disks": attached,
            }))
        }
        // ── kv (secrets, config, file content, volume mirror) ──
        "kv.get" => {
            let key = kv_key(&args)?;
            match read_one(DbDefs::get_kv_table(), &key)? {
                Some(value) => Ok(serde_json::json!({
                    "key": key,
                    "value": value,
                    "bytes": value.len(),
                })),
                None => Ok(serde_json::Value::Null),
            }
        }
        "kv.set" => {
            let key = kv_key(&args)?;
            let value = args
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "invalid: missing string argument 'value'".to_string())?;
            if value.len() > MAX_KV_BYTES {
                return Err(format!(
                    "invalid: value is {} bytes (limit {MAX_KV_BYTES})",
                    value.len()
                ));
            }
            write_one(DbDefs::get_kv_table(), &key, value)?;
            Ok(serde_json::json!({ "key": key, "bytes": value.len() }))
        }
        "kv.delete" => Ok(serde_json::Value::Bool(delete_one(
            DbDefs::get_kv_table(),
            &kv_key(&args)?,
        )?)),
        // Keys + sizes only — fetching a secret list must not stream every
        // secret body through the worker bridge; use `kv.get` for values.
        "kv.list" => {
            let prefix = opt_arg(&args, "prefix").unwrap_or_default();
            let mut out = Vec::new();
            for (k, v) in read_all(DbDefs::get_kv_table())? {
                if k.starts_with(&prefix) {
                    out.push(serde_json::json!({ "key": k, "bytes": v.len() }));
                }
            }
            Ok(serde_json::Value::Array(out))
        }

        // ── files (metadata only in the demo) ──
        "files.list" => Ok(serde_json::Value::Array(
            read_all(DbDefs::get_files_table())?
                .into_iter()
                .filter_map(|(_, v)| serde_json::from_str(&v).ok())
                .collect(),
        )),
        "files.get" => {
            let id = arg(&args, "fileId")?.to_string();
            read_one(DbDefs::get_files_table(), &id)?
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .ok_or_else(|| format!("not_found: file {id}"))
        }
        "files.create_folder" => {
            let name = arg(&args, "name")?.trim().to_string();
            if name.is_empty() {
                return Err("invalid: folder name is required".to_string());
            }
            let id = new_id();
            let node = serde_json::json!({
                "id": id,
                "name": name,
                "fileType": "folder",
                "parentId": opt_arg(&args, "parentId").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "sizeBytes": 0,
                "mimeType": serde_json::Value::Null,
                "hashBlake3": serde_json::Value::Null,
                "encrypted": false,
                "encryptionAlgorithm": serde_json::Value::Null,
                "compressionLayers": [],
                "thumbnailPath": serde_json::Value::Null,
                "contextData": serde_json::Value::Null,
                "tags": [],
                "collectionIds": [],
                "faceGroupIds": [],
                "looseGroupIds": [],
                "gpsLat": serde_json::Value::Null,
                "gpsLon": serde_json::Value::Null,
                "createdAt": now,
                "modifiedAt": now,
            });
            write_one(DbDefs::get_files_table(), &id, &node.to_string())?;
            Ok(node)
        }
        "files.create" => {
            let name = arg(&args, "name")?.trim().to_string();
            if name.is_empty() {
                return Err("invalid: file name is required".to_string());
            }
            let content = opt_arg(&args, "content");
            let content_bytes = content.as_deref().map(str::len).unwrap_or(0);
            if content_bytes > MAX_KV_BYTES {
                return Err(format!(
                    "invalid: content is {content_bytes} bytes (limit {MAX_KV_BYTES})"
                ));
            }
            let id = new_id();
            let declared = args
                .get("sizeBytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                .min(u64::from(u32::MAX)) as usize;
            let size_bytes = content_bytes.max(declared);
            let node = serde_json::json!({
                "id": id,
                "name": name,
                "fileType": opt_arg(&args, "fileType").unwrap_or_else(|| "file".to_string()),
                "parentId": opt_arg(&args, "parentId").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "sizeBytes": size_bytes,
                "mimeType": opt_arg(&args, "mimeType").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "hashBlake3": opt_arg(&args, "hashBlake3").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "encrypted": args.get("encrypted").and_then(|v| v.as_bool()).unwrap_or(false),
                "encryptionAlgorithm": opt_arg(&args, "encryptionAlgorithm").map(serde_json::Value::String).unwrap_or(serde_json::Value::Null),
                "compressionLayers": args.get("compressionLayers").cloned().unwrap_or_else(|| serde_json::json!([])),
                "thumbnailPath": serde_json::Value::Null,
                "contextData": args.get("contextData").cloned().unwrap_or(serde_json::Value::Null),
                "tags": args.get("tags").cloned().unwrap_or_else(|| serde_json::json!([])),
                "collectionIds": serde_json::json!([]),
                "faceGroupIds": serde_json::json!([]),
                "looseGroupIds": serde_json::json!([]),
                "gpsLat": args.get("gpsLat").cloned().unwrap_or(serde_json::Value::Null),
                "gpsLon": args.get("gpsLon").cloned().unwrap_or(serde_json::Value::Null),
                "createdAt": now,
                "modifiedAt": now,
            });
            write_one(DbDefs::get_files_table(), &id, &node.to_string())?;
            if let Some(body) = content {
                write_one(DbDefs::get_kv_table(), &format!("content:{id}"), &body)?;
            }
            Ok(node)
        }
        "files.patch" => {
            let id = arg(&args, "fileId")?.to_string();
            let raw = read_one(DbDefs::get_files_table(), &id)?
                .ok_or_else(|| format!("not_found: file {id}"))?;
            let mut node: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let patch = args
                .get("patch")
                .and_then(|v| v.as_object())
                .ok_or_else(|| "invalid: missing 'patch' object".to_string())?;
            // Whitelist: identity, size, encryption and tagging travel — ids,
            // timestamps and type never do.
            const PATCHABLE: [&str; 15] = [
                "name",
                "parentId",
                "sizeBytes",
                "mimeType",
                "hashBlake3",
                "encrypted",
                "encryptionAlgorithm",
                "compressionLayers",
                "tags",
                "contextData",
                "thumbnailPath",
                "collectionIds",
                "gpsLat",
                "gpsLon",
                "fileType",
            ];
            let mut applied = 0usize;
            for (k, v) in patch {
                if PATCHABLE.contains(&k.as_str()) {
                    node[k.as_str()] = v.clone();
                    applied += 1;
                }
            }
            if applied == 0 {
                return Err("invalid: patch contains no supported fields".to_string());
            }
            if let Some(name) = node.get("name").and_then(|v| v.as_str()) {
                let name = name.trim();
                if name.is_empty() {
                    return Err("invalid: file name must not be empty".to_string());
                }
                node["name"] = serde_json::Value::String(name.to_string());
            }
            node["modifiedAt"] = serde_json::Value::String(now.to_string());
            write_one(DbDefs::get_files_table(), &id, &node.to_string())?;
            Ok(node)
        }
        "files.rename" => {
            let id = arg(&args, "fileId")?.to_string();
            let name = arg(&args, "newName")?.trim().to_string();
            let raw = read_one(DbDefs::get_files_table(), &id)?
                .ok_or_else(|| format!("not_found: file {id}"))?;
            let mut node: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            node["name"] = serde_json::Value::String(name);
            node["modifiedAt"] = serde_json::Value::String(now.to_string());
            write_one(DbDefs::get_files_table(), &id, &node.to_string())?;
            Ok(node)
        }
        "files.delete" => {
            let id = arg(&args, "fileId")?.to_string();
            if !delete_one(DbDefs::get_files_table(), &id)? {
                return Err(format!("not_found: file {id}"));
            }
            Ok(serde_json::Value::Bool(true))
        }
        _ => Err(format!("unsupported: unknown demo-db op '{op}'")),
    })();
    match result {
        Ok(data) => envelope_json(true, &data, None),
        Err(e) => envelope_json(false, &serde_json::Value::Null, Some(e)),
    }
}

fn err_envelope(error: &str) -> String {
    envelope_json(false, &serde_json::Value::Null, Some(error.to_string()))
}

/// Whole-file image of the open database, for crash-recovery snapshots and
/// `.cybermanju` CREATE/EXPORT. Works on both backends: OPFS (durable) and
/// the shared in-memory backend (session image, still a real redb file).
#[wasm_bindgen]
pub fn db_snapshot() -> Result<JsValue, JsValue> {
    let bytes: Vec<u8> = DB
        .with(|cell| {
            let borrow = cell.borrow();
            let handle = borrow.as_ref().ok_or_else(|| {
                "unavailable: database is not open (call db_open first)".to_string()
            })?;
            let backend = &handle.backend;
            let len = backend
                .len()
                .map_err(|e| format!("network: snapshot size failed: {e}"))?;
            if len > MAX_SNAPSHOT_BYTES as u64 {
                return Err("too_large: database image exceeds 256 MiB snapshot cap".to_string());
            }
            if len == 0 {
                return Err("unavailable: database image is empty (call db_open first)".to_string());
            }
            backend
                .read(0, len as usize)
                .map_err(|e| format!("network: snapshot read failed: {e}"))
        })
        .map_err(|e| JsValue::from_str(&e))?;
    let view = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    view.copy_from(&bytes);
    Ok(view.into())
}

/// Validate an image in an isolated in-memory redb before replacing the live
/// database. This prevents arbitrary legacy/raw files from truncating OPFS.
fn validate_snapshot_image(data: &[u8]) -> Result<(), String> {
    if data.is_empty() {
        return Err("invalid: refusing to restore an empty image".to_string());
    }
    if data.len() > MAX_SNAPSHOT_BYTES {
        return Err("too_large: image exceeds 256 MiB snapshot cap".to_string());
    }
    let staged = Backend::Memory(MemoryBackend::new());
    staged
        .set_len(data.len() as u64)
        .map_err(|e| format!("invalid: cannot stage snapshot image: {e}"))?;
    staged
        .write(0, data)
        .map_err(|e| format!("invalid: cannot stage snapshot image: {e}"))?;
    let db = redb::Builder::new()
        .create_with_backend(staged)
        .map_err(|e| format!("invalid: backup is not a valid redb database image: {e}"))?;
    open_all_tables(&db)
        .map_err(|e| format!("invalid: backup redb tables cannot be opened: {e}"))?;
    Ok(())
}

/// Replace the database file with a validated snapshot image and reopen. The
/// open handle is reused (dropped redb state first), so no new OPFS acquisition
/// — and no second sync handle on the same file — is needed. Works on both
/// the OPFS and the in-memory backend, so IMPORT works everywhere.
#[wasm_bindgen]
pub fn db_restore(data: &[u8]) -> String {
    if let Err(e) = validate_snapshot_image(data) {
        return err_envelope(&e);
    }
    let backend = DB.with(|cell| cell.borrow_mut().take().map(|h| h.backend));
    let backend = match backend {
        Some(b) => b,
        _ => {
            return err_envelope("unavailable: restore needs an open database (call db_open first)")
        }
    };
    let reopened: Result<RedbDatabase, String> = (|| {
        backend
            .set_len(0)
            .map_err(|e| format!("network: restore truncate failed: {e}"))?;
        backend
            .write(0, data)
            .map_err(|e| format!("network: restore write failed: {e}"))?;
        backend
            .sync_data(false)
            .map_err(|e| format!("network: restore flush failed: {e}"))?;
        let db = redb::Builder::new()
            .create_with_backend(backend.clone())
            .map_err(|e| format!("network: restored database failed to open: {e}"))?;
        open_all_tables(&db)?;
        Ok(db)
    })();
    match reopened {
        Ok(db) => {
            DB.with(|cell| *cell.borrow_mut() = Some(DbHandle { db, backend }));
            envelope_json(
                true,
                &serde_json::json!({ "restoredBytes": data.len() }),
                None,
            )
        }
        Err(e) => {
            // Restore failed after the handle was taken — reopen the (possibly
            // truncated) backend so the vault stays usable instead of stuck
            // in "not open". Best-effort: surface the original error.
            if let Ok(db) = redb::Builder::new().create_with_backend(backend.clone()) {
                DB.with(|cell| {
                    *cell.borrow_mut() = Some(DbHandle { db, backend });
                });
            }
            err_envelope(&e)
        }
    }
}

fn get_disk(id: &str) -> Result<serde_json::Value, String> {
    read_one(DbDefs::get_disks_table(), id)?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .ok_or_else(|| format!("not_found: disk {id}"))
}

#[cfg(test)]
mod open_tables_tests {
    use super::*;

    /// Regression: `open_all_tables` must create every table any op reads.
    /// `cron.list` on a fresh browser DB used to fail with
    /// `Table 'schedules' does not exist` (read txns don't create tables),
    /// which the store surfaced as a `Failed to fetch schedules` toast on
    /// every poll. Same coverage for the agent tables, which the native
    /// `Database::new` has always created.
    #[test]
    fn open_all_tables_creates_cron_and_agent_tables() {
        let db = redb::Builder::new()
            .create_with_backend(Backend::Memory(MemoryBackend::new()))
            .expect("in-memory open");
        open_all_tables(&db).expect("open_all_tables");
        let tx = db.begin_read().expect("read txn");
        for table in [
            DbDefs::get_schedules_table(),
            DbDefs::get_schedule_runs_table(),
            DbDefs::get_agent_configs_table(),
            DbDefs::get_agent_sessions_table(),
            DbDefs::get_agent_memories_table(),
        ] {
            tx.open_table(table)
                .expect("open_all_tables must create the table");
        }
    }

    #[test]
    fn restore_preflight_rejects_arbitrary_bytes() {
        let err = validate_snapshot_image(b"not a redb snapshot").expect_err("must refuse");
        assert!(err.starts_with("invalid:"), "got: {err}");
    }

    #[test]
    fn restore_preflight_accepts_a_valid_redb_image() {
        let backend = Backend::Memory(MemoryBackend::new());
        let db = redb::Builder::new()
            .create_with_backend(backend.clone())
            .expect("open in-memory database");
        open_all_tables(&db).expect("create schema tables");
        drop(db);
        let len = backend.len().expect("image size");
        let image = backend.read(0, len as usize).expect("read image");

        validate_snapshot_image(&image).expect("valid image passes isolated preflight");
    }
}
