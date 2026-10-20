// CyberManju OS — Api (crates/os/src/api.rs).
//! The typed syscall boundary (AGENT-8 item 4 / MISSING.md F3).
//!
//! Everything the shell, the REST routes and future apps do to storage goes
//! through this surface: `open · read · write · seek · close · stat · unlink
//! · readdir · mkdir · mount · df`. Nothing above this module touches a
//! provider or a storage crate directly for I/O.
//!
//! The namespace is **one merged volume**: a directory tree rooted at
//! [`Kernel::root`] whose logical capacity is [`DEFAULT_CAPACITY_BYTES`] of
//! local scratch **plus every attached `.cybermanju` disk in the `disks`
//! table** (AGENT-6's registry — read leniently so either side's field
//! naming works). Attaching a disk therefore grows `df`, and a write past
//! the merged capacity is refused with `disk_full: …` instead of being
//! silently accepted.
//!
//! When `crates/disk` publishes its allocator/mount API, [`Kernel`] is the
//! one place that switches over to it: the syscall signatures do not change.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use cybermanju_db::Database;
use redb::ReadableTable;
use serde::Serialize;

/// Local scratch capacity before any provider disk is attached (1 GiB).
pub const DEFAULT_CAPACITY_BYTES: u64 = 1 << 30;

/// File descriptor handed out by [`Kernel::open`].
pub type Fd = u32;

/// Whence for [`Kernel::seek`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whence {
    Start,
    Current,
    End,
}

/// Open flags for [`Kernel::open`].
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
}

impl OpenFlags {
    /// `O_RDONLY`
    pub fn read_only() -> Self {
        Self {
            read: true,
            ..Self::default()
        }
    }

    /// `O_RDWR | O_CREAT | O_TRUNC`
    pub fn create() -> Self {
        Self {
            read: true,
            write: true,
            create: true,
            truncate: true,
            ..Self::default()
        }
    }

    /// `O_WRONLY | O_CREAT | O_APPEND`
    pub fn append() -> Self {
        Self {
            write: true,
            create: true,
            append: true,
            ..Self::default()
        }
    }
}

/// `stat(2)` result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub path: String,
    pub name: String,
    /// `"file"` or `"dir"`.
    pub kind: String,
    pub size_bytes: u64,
    pub modified_ms: u64,
    pub is_dir: bool,
}

/// One row of `readdir(3)`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    pub kind: String,
    pub size_bytes: u64,
    pub modified_ms: u64,
    pub is_dir: bool,
}

/// One `.cybermanju` disk as the OS layer sees it (AGENT-6's `disks` rows).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskRecord {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub capacity_bytes: u64,
    pub state: String,
    pub health: String,
    pub created_at: String,
    /// Bytes AGENT-6's allocator has placed on this disk.
    #[serde(default)]
    pub used_bytes: u64,
    /// Concurrent compute slots this disk's provider contributes to the
    /// fan-out scheduler (`Capabilities::compute`, AGENT-8 item 8). Rows
    /// written by AGENT-6 without the field default to 1: a newly attached
    /// provider always adds processing power.
    #[serde(default)]
    pub compute: u32,
}

impl DiskRecord {
    /// Lenient row parser: the `disks` table is shared with AGENT-6, so both
    /// `camelCase` and `snake_case` spellings are accepted.
    pub fn from_json(id: &str, raw: &str) -> Option<DiskRecord> {
        let value: serde_json::Value = serde_json::from_str(raw).ok()?;
        let get = |keys: &[&str]| -> Option<serde_json::Value> {
            keys.iter().find_map(|k| value.get(*k).cloned())
        };
        let capacity = get(&["capacityBytes", "capacity_bytes", "capacity"])?.as_u64()?;
        let state = get(&["state", "status"])
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "attached".to_string());
        let attached = value
            .get("attached")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let state = if !attached && state == "attached" {
            "detached".to_string()
        } else {
            state
        };
        let name = get(&["name", "label"])
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| id.to_string());
        let provider = get(&["provider", "providerId", "configId", "backend"])
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        let health = get(&["health", "condition"])
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "ok".to_string());
        let created_at = get(&["createdAt", "created_at"])
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        let used_bytes = get(&["usedBytes", "used_bytes", "used"])
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        // `compute` may sit top-level, under `capabilities`, or be absent.
        let compute = value
            .get("compute")
            .or_else(|| value.get("computeSlots"))
            .or_else(|| value.pointer("/capabilities/compute"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        Some(DiskRecord {
            id: id.to_string(),
            name,
            provider,
            capacity_bytes: capacity,
            state,
            health,
            created_at,
            used_bytes,
            compute,
        })
    }

    /// Not detached/destroyed — contributes capacity to `df`.
    pub fn attached(&self) -> bool {
        self.state != "detached" && self.state != "destroyed"
    }
}

/// `df(1)` result: the merged volume.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDf {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub root: String,
    pub disk_count: usize,
    pub attached_bytes: u64,
    pub scratch_bytes: u64,
    pub disks: Vec<DiskRecord>,
}

struct OpenFile {
    file: File,
    pos: u64,
    path: PathBuf,
    append: bool,
}

/// The kernel: namespace, file descriptors, capacity accounting.
pub struct Kernel {
    root: OnceLock<PathBuf>,
    fds: Mutex<HashMap<Fd, OpenFile>>,
    next_fd: AtomicU32,
    capacity: AtomicU64,
    used: AtomicU64,
    used_scanned: AtomicBool,
}

impl Kernel {
    /// Process-wide kernel.
    pub fn global() -> &'static Kernel {
        static KERNEL: OnceLock<Kernel> = OnceLock::new();
        KERNEL.get_or_init(|| Kernel {
            root: OnceLock::new(),
            fds: Mutex::new(HashMap::new()),
            next_fd: AtomicU32::new(3),
            capacity: AtomicU64::new(DEFAULT_CAPACITY_BYTES),
            used: AtomicU64::new(0),
            used_scanned: AtomicBool::new(false),
        })
    }

    /// Volume root: `CYBERMANJU_DATA_DIR` → parent of `DB_PATH` → `~/.cybermanju`,
    /// then `/volume` inside it.
    pub fn root(&self) -> &Path {
        self.root.get_or_init(|| {
            let base = cybermanju_crypto::keystore::data_dir()
                .unwrap_or_else(|| PathBuf::from(".cybermanju"));
            let root = base.join("volume");
            if let Err(e) = std::fs::create_dir_all(&root) {
                log::warn!("could not create volume root {}: {}", root.display(), e);
            }
            root
        })
    }

    /// Point the process-wide kernel at a scratch directory. Test-only: the
    /// first caller wins, which is why every test shares one volume
    /// ([`crate::testutil::volume_dir`]).
    #[cfg(test)]
    pub(crate) fn set_root_for_tests(&self, path: PathBuf) -> Result<(), String> {
        self.root
            .set(path)
            .map_err(|_| "volume root already initialised".to_string())
    }

    /// Resolve a volume path (`/a/b`, `a/../b`, `.`, `..`) to a host path.
    /// `..` is clamped at the root and any symlink component is refused, so
    /// the result can never leave the volume.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let root = self.root();
        let mut cur = root.to_path_buf();
        for comp in path.split(['/', '\\']) {
            match comp {
                "" | "." => {}
                ".." => {
                    if cur != root {
                        cur.pop();
                    }
                }
                c if c.contains('\0') => return Err(format!("invalid: bad path component {c:?}")),
                c => {
                    cur.push(c);
                    if let Ok(md) = std::fs::symlink_metadata(&cur) {
                        if md.file_type().is_symlink() {
                            return Err(format!(
                                "invalid: '{}' traverses a symlink out of the volume",
                                path
                            ));
                        }
                    }
                }
            }
        }
        if !cur.starts_with(root) {
            return Err(format!("invalid: '{}' escapes the volume", path));
        }
        Ok(cur)
    }

    /// Render a host path back into the volume namespace.
    pub fn display(&self, path: &Path) -> String {
        let root = self.root();
        match path.strip_prefix(root) {
            Ok(rest) => {
                let rest = rest.display().to_string();
                if rest.is_empty() {
                    "/".to_string()
                } else {
                    format!("/{rest}")
                }
            }
            Err(_) => path.display().to_string(),
        }
    }

    // ── capacity ────────────────────────────────────────────────────────

    /// Recompute the merged capacity from AGENT-6's `disks` rows and cache it
    /// for admission control. Cheap enough to call on every shell line.
    pub fn sync_capacity(&self, db: Option<&Database>) -> u64 {
        if let Some(db) = db {
            if let Ok(disks) = list_disks(db) {
                let attached: u64 = disks
                    .iter()
                    .filter(|d| d.attached())
                    .map(|d| d.capacity_bytes)
                    .sum();
                self.capacity
                    .store(DEFAULT_CAPACITY_BYTES + attached, Ordering::Relaxed);
            }
        }
        self.capacity.load(Ordering::Relaxed)
    }

    /// Merged capacity (local scratch + attached disks).
    pub fn capacity(&self) -> u64 {
        self.capacity.load(Ordering::Relaxed)
    }

    /// Bytes currently stored in the volume.
    pub fn used_bytes(&self) -> u64 {
        if !self.used_scanned.load(Ordering::Relaxed) {
            let bytes = dir_size(self.root());
            self.used.store(bytes, Ordering::Relaxed);
            self.used_scanned.store(true, Ordering::Relaxed);
        }
        self.used.load(Ordering::Relaxed)
    }

    fn bump_used(&self, delta: i64) {
        if !self.used_scanned.load(Ordering::Relaxed) {
            return;
        }
        let cur = self.used.load(Ordering::Relaxed);
        let next = if delta < 0 {
            cur.saturating_sub(delta.unsigned_abs())
        } else {
            cur.saturating_add(delta as u64)
        };
        self.used.store(next, Ordering::Relaxed);
    }

    fn admit(&self, incoming: u64) -> Result<(), String> {
        let used = self.used_bytes();
        let total = self.capacity();
        if used.saturating_add(incoming) > total {
            return Err(format!(
                "disk_full: {} of {} bytes in use, {} more would not fit",
                used, total, incoming
            ));
        }
        Ok(())
    }

    // ── syscalls ────────────────────────────────────────────────────────

    /// `open(2)` — resolve, register a file descriptor.
    pub fn open(&self, path: &str, flags: OpenFlags) -> Result<Fd, String> {
        let host = self.resolve(path)?;
        if host.is_dir() {
            return Err(format!("is a directory: {}", self.display(&host)));
        }
        if flags.truncate {
            if let Ok(meta) = std::fs::metadata(&host) {
                self.bump_used(-(meta.len() as i64));
            }
        }
        if flags.create && !host.exists() {
            if let Some(parent) = host.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("io error: cannot create parent of {path}: {e}"))?;
            }
        }
        let mut opts = OpenOptions::new();
        opts.read(flags.read || !flags.write)
            .write(flags.write)
            .create(flags.create)
            .truncate(flags.truncate && !flags.append)
            .append(flags.append);
        let file = opts.open(&host).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("not_found: {}", self.display(&host))
            } else {
                format!("io error: cannot open {path}: {e}")
            }
        })?;
        let fd = self.next_fd.fetch_add(1, Ordering::Relaxed);
        let pos = file.metadata().map(|m| m.len()).unwrap_or(0);
        let pos = if flags.append { pos } else { 0 };
        self.fds.lock().unwrap_or_else(|p| p.into_inner()).insert(
            fd,
            OpenFile {
                file,
                pos,
                path: host,
                append: flags.append,
            },
        );
        Ok(fd)
    }

    /// `read(2)` — up to `len` bytes from the current offset.
    pub fn read(&self, fd: Fd, len: usize) -> Result<Vec<u8>, String> {
        let mut fds = self.fds.lock().unwrap_or_else(|p| p.into_inner());
        let handle = fds.get_mut(&fd).ok_or_else(|| format!("bad fd: {fd}"))?;
        let mut buf = vec![0u8; len];
        handle
            .file
            .seek(SeekFrom::Start(handle.pos))
            .map_err(|e| format!("io error: seek failed: {e}"))?;
        let n = handle
            .file
            .read(&mut buf)
            .map_err(|e| format!("io error: read failed: {e}"))?;
        handle.pos += n as u64;
        buf.truncate(n);
        Ok(buf)
    }

    /// `write(2)` — admission-controlled: refuses data that would not fit.
    pub fn write(&self, fd: Fd, data: &[u8]) -> Result<usize, String> {
        let mut fds = self.fds.lock().unwrap_or_else(|p| p.into_inner());
        let handle = fds.get_mut(&fd).ok_or_else(|| format!("bad fd: {fd}"))?;
        let existing = std::fs::metadata(&handle.path)
            .map(|m| m.len())
            .unwrap_or(0);
        let start = if handle.append { existing } else { handle.pos };
        let end = start.saturating_add(data.len() as u64);
        self.admit(end.saturating_sub(existing))?;

        handle
            .file
            .seek(SeekFrom::Start(start))
            .map_err(|e| format!("io error: seek failed: {e}"))?;
        let n = handle
            .file
            .write(data)
            .map_err(|e| format!("io error: write failed: {e}"))?;
        handle.pos = start + n as u64;
        drop(fds);
        let grown = handle_len_delta(n as u64, start, existing);
        self.bump_used(grown);
        Ok(n)
    }

    /// `lseek(2)`.
    pub fn seek(&self, fd: Fd, whence: Whence, offset: i64) -> Result<u64, String> {
        let mut fds = self.fds.lock().unwrap_or_else(|p| p.into_inner());
        let handle = fds.get_mut(&fd).ok_or_else(|| format!("bad fd: {fd}"))?;
        let base = match whence {
            Whence::Start => 0i64,
            Whence::Current => handle.pos as i64,
            Whence::End => handle
                .file
                .metadata()
                .map(|m| m.len() as i64)
                .map_err(|e| format!("io error: stat failed: {e}"))?,
        };
        let pos = base.saturating_add(offset);
        if pos < 0 {
            return Err(format!("invalid: seek before start of file ({pos})"));
        }
        handle.pos = pos as u64;
        Ok(handle.pos)
    }

    /// `close(2)`.
    pub fn close(&self, fd: Fd) -> Result<(), String> {
        self.fds
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&fd)
            .map(|_| ())
            .ok_or_else(|| format!("bad fd: {fd}"))
    }

    /// `stat(2)`.
    pub fn stat(&self, path: &str) -> Result<Stat, String> {
        let host = self.resolve(path)?;
        let meta = std::fs::metadata(&host).map_err(|e| not_found(&e, path))?;
        let name = host
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        Ok(Stat {
            path: path.to_string(),
            name,
            kind: if meta.is_dir() { "dir" } else { "file" }.to_string(),
            size_bytes: meta.len(),
            modified_ms: mtime_ms(&meta),
            is_dir: meta.is_dir(),
        })
    }

    /// `unlink(2)` — files only; directories need [`Kernel::remove_tree`].
    pub fn unlink(&self, path: &str) -> Result<(), String> {
        let host = self.resolve(path)?;
        if host.is_dir() {
            return Err(format!("is a directory: {}", self.display(&host)));
        }
        let len = std::fs::metadata(&host).map(|m| m.len()).unwrap_or(0);
        std::fs::remove_file(&host).map_err(|e| not_found(&e, path))?;
        self.bump_used(-(len as i64));
        Ok(())
    }

    /// `rm -r` — remove a directory subtree, returning the freed bytes.
    pub fn remove_tree(&self, path: &str) -> Result<u64, String> {
        let host = self.resolve(path)?;
        if !host.is_dir() {
            return self.unlink(path).map(|_| 0);
        }
        let bytes = dir_size(&host);
        std::fs::remove_dir_all(&host).map_err(|e| not_found(&e, path))?;
        self.bump_used(-(bytes as i64));
        Ok(bytes)
    }

    /// `readdir(3)` — sorted, directories first.
    pub fn readdir(&self, path: &str) -> Result<Vec<DirEntry>, String> {
        let host = self.resolve(path)?;
        if !host.is_dir() {
            if host.exists() {
                return Err(format!("not a directory: {}", self.display(&host)));
            }
            return Err(format!("not_found: {path}"));
        }
        let rd =
            std::fs::read_dir(&host).map_err(|e| format!("io error: cannot read {path}: {e}"))?;
        let mut rows = Vec::new();
        for entry in rd {
            let entry = entry.map_err(|e| format!("io error: {e}"))?;
            let meta = entry.metadata().map_err(|e| format!("io error: {e}"))?;
            let is_dir = meta.is_dir();
            rows.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                kind: if is_dir { "dir" } else { "file" }.to_string(),
                size_bytes: if is_dir { 0 } else { meta.len() },
                modified_ms: mtime_ms(&meta),
                is_dir,
            });
        }
        rows.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        Ok(rows)
    }

    /// `mkdir(2)` — parents must exist (the shell's `mkdir -p` loops).
    pub fn mkdir(&self, path: &str) -> Result<(), String> {
        let host = self.resolve(path)?;
        if host.exists() {
            return Err(format!("already exists: {path}"));
        }
        std::fs::create_dir(&host).map_err(|e| format!("io error: cannot mkdir {path}: {e}"))
    }

    /// `rename(2)` — same-volume move.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        let a = self.resolve(from)?;
        let b = self.resolve(to)?;
        if !a.exists() {
            return Err(format!("not_found: {from}"));
        }
        if b.exists() {
            return Err(format!("already exists: {to}"));
        }
        if let Some(parent) = b.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("io error: cannot create {to}: {e}"))?;
        }
        std::fs::rename(&a, &b).map_err(|e| format!("io error: rename failed: {e}"))
    }

    /// `du` — `(bytes, file count)` for a volume path.
    pub fn du(&self, path: &str) -> Result<(u64, u64), String> {
        let host = self.resolve(path)?;
        if !host.exists() {
            return Err(format!("not_found: {path}"));
        }
        Ok(dir_size_and_files(&host))
    }

    /// `df(1)` — merged volume statistics.
    pub fn df(&self, db: Option<&Database>) -> VolumeDf {
        let disks = db.and_then(|db| list_disks(db).ok()).unwrap_or_default();
        let attached_bytes: u64 = disks
            .iter()
            .filter(|d| d.attached())
            .map(|d| d.capacity_bytes)
            .sum();
        let total = DEFAULT_CAPACITY_BYTES + attached_bytes;
        self.capacity.store(total, Ordering::Relaxed);
        let used = self.used_bytes().min(total);
        VolumeDf {
            total_bytes: total,
            used_bytes: used,
            free_bytes: total - used,
            root: self.display(self.root()),
            disk_count: disks.iter().filter(|d| d.attached()).count(),
            attached_bytes,
            scratch_bytes: DEFAULT_CAPACITY_BYTES,
            disks,
        }
    }
}

/// Bytes a write of `written` bytes starting at `start` added to a file that
/// was `existing` bytes long.
fn handle_len_delta(written: u64, start: u64, existing: u64) -> i64 {
    let end = start + written;
    if end > existing {
        (end - existing) as i64
    } else {
        0
    }
}

fn not_found(e: &std::io::Error, path: &str) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => format!("not_found: {path}"),
        std::io::ErrorKind::PermissionDenied => format!("permission denied: {path}"),
        _ => format!("io error: {path}: {e}"),
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Recursive byte size of a tree.
pub fn dir_size(path: &Path) -> u64 {
    dir_size_and_files(path).0
}

/// Recursive `(bytes, file count)` of a tree. Directories cost 0 bytes and
/// are not counted as files.
pub fn dir_size_and_files(path: &Path) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let Ok(rd) = std::fs::read_dir(path) else {
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.is_file() {
                return (meta.len(), 1);
            }
        }
        return (0, 0);
    };
    for entry in rd.flatten() {
        let p = entry.path();
        match entry.metadata() {
            Ok(meta) if meta.is_dir() => {
                let (b, f) = dir_size_and_files(&p);
                bytes += b;
                files += f;
            }
            Ok(meta) => {
                bytes += meta.len();
                files += 1;
            }
            Err(_) => {}
        }
    }
    (bytes, files)
}

// ─── the `disks` table (AGENT-6's registry, read leniently) ────────────

/// Every `.cybermanju` disk row, sorted by id.
pub fn list_disks(db: &Database) -> Result<Vec<DiskRecord>, String> {
    let tx = db.begin_read().map_err(|e| e.to_string())?;
    let table = tx
        .open_table(Database::get_disks_table())
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for entry in table.iter().map_err(|e| e.to_string())? {
        let (key, value) = entry.map_err(|e| e.to_string())?;
        if let Some(rec) = DiskRecord::from_json(key.value(), value.value()) {
            rows.push(rec);
        }
    }
    rows.sort_by_key(|a| a.id.clone());
    Ok(rows)
}

/// One disk by id.
pub fn get_disk(db: &Database, id: &str) -> Result<Option<DiskRecord>, String> {
    Ok(list_disks(db)?.into_iter().find(|d| d.id == id))
}

/// Insert or replace a disk row.
pub fn put_disk(db: &Database, rec: &DiskRecord) -> Result<(), String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    {
        let mut table = tx
            .open_table(Database::get_disks_table())
            .map_err(|e| e.to_string())?;
        let json = serde_json::to_string(rec).map_err(|e| e.to_string())?;
        table
            .insert(rec.id.as_str(), json.as_str())
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Delete a disk row.
pub fn remove_disk(db: &Database, id: &str) -> Result<bool, String> {
    let tx = db.begin_write().map_err(|e| e.to_string())?;
    let removed = {
        let mut table = tx
            .open_table(Database::get_disks_table())
            .map_err(|e| e.to_string())?;
        let removed = table.remove(id).map_err(|e| e.to_string())?.is_some();
        removed
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn kernel_under(root: &Path) -> Kernel {
        // A Kernel rooted in the temp dir: exercise the syscalls without
        // touching the developer's real volume.
        Kernel {
            root: OnceLock::from(root.to_path_buf()),
            fds: Mutex::new(HashMap::new()),
            next_fd: AtomicU32::new(3),
            capacity: AtomicU64::new(64 * 1024),
            used: AtomicU64::new(0),
            used_scanned: AtomicBool::new(true),
        }
    }

    #[test]
    fn open_write_read_seek_close_round_trip() {
        let dir = scratch();
        let k = kernel_under(dir.path());
        let fd = k.open("/note.txt", OpenFlags::create()).expect("open");
        assert_eq!(k.write(fd, b"hello ").expect("write"), 6);
        assert_eq!(k.write(fd, b"world").expect("write"), 5);
        assert_eq!(k.seek(fd, Whence::Start, 0).expect("seek"), 0);
        assert_eq!(k.read(fd, 5).expect("read"), b"hello".to_vec());
        assert_eq!(k.seek(fd, Whence::End, 0).expect("seek"), 11);
        assert!(k.read(fd, 5).expect("read").is_empty(), "EOF");
        k.close(fd).expect("close");
        assert_eq!(k.stat("/note.txt").expect("stat").size_bytes, 11);
        assert!(k.close(fd).is_err(), "double close is an error");
    }

    #[test]
    fn admission_control_refuses_overflow() {
        let dir = scratch();
        let k = kernel_under(dir.path());
        let fd = k.open("/big.bin", OpenFlags::create()).expect("open");
        let err = k.write(fd, &vec![0u8; 65_537]).expect_err("must refuse");
        assert!(err.starts_with("disk_full:"), "got {err}");
        k.write(fd, &vec![1u8; 65_536])
            .expect("exactly at capacity");
        k.close(fd).expect("close");
    }

    #[test]
    fn paths_cannot_escape_the_volume() {
        let dir = scratch();
        let k = kernel_under(dir.path());
        std::fs::create_dir_all(dir.path().parent().unwrap()).expect("mk");
        let outside = dir.path().parent().unwrap().join("cyb-outside-secret");
        std::fs::write(&outside, b"secret").expect("write");
        // `..` is clamped at the volume root — it never walks above it.
        assert_eq!(
            k.resolve("/../cyb-outside-secret").expect("clamped"),
            dir.path().join("cyb-outside-secret")
        );
        assert!(k.stat("/../cyb-outside-secret").is_err());
        // A symlink component is refused outright.
        std::os::unix::fs::symlink(dir.path().parent().unwrap(), dir.path().join("escape"))
            .expect("symlink");
        let err = k
            .stat("/escape/cyb-outside-secret")
            .expect_err("must refuse");
        assert!(err.starts_with("invalid:"), "got {err}");
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn mkdir_readdir_du_unlink_tree() {
        let dir = scratch();
        let k = kernel_under(dir.path());
        k.mkdir("/docs").expect("mkdir");
        assert!(k.mkdir("/docs").is_err(), "mkdir is not -p");
        let fd = k.open("/docs/a.txt", OpenFlags::create()).expect("open");
        k.write(fd, b"1234").expect("write");
        k.close(fd).expect("close");
        let rows = k.readdir("/").expect("readdir");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_dir);
        assert_eq!(
            k.readdir("/docs/a.txt").expect_err("file"),
            "not a directory: /docs/a.txt"
        );
        let (bytes, files) = k.du("/docs").expect("du");
        assert_eq!((bytes, files), (4, 1));
        k.rename("/docs/a.txt", "/docs/b.txt").expect("mv");
        assert!(k.rename("/docs/a.txt", "/docs/c.txt").is_err());
        k.unlink("/docs/b.txt").expect("rm");
        assert_eq!(k.unlink("/docs").expect_err("dir"), "is a directory: /docs");
        k.remove_tree("/docs").expect("rm -r");
        assert!(k.readdir("/docs").is_err());
        assert_eq!(k.stat("/docs").expect_err("gone"), "not_found: /docs");
    }

    #[test]
    fn df_reports_a_merged_volume() {
        let dir = scratch();
        let k = kernel_under(dir.path());
        let df = k.df(None);
        assert_eq!(df.total_bytes, DEFAULT_CAPACITY_BYTES);
        assert!(df.free_bytes <= df.total_bytes);
        assert_eq!(df.scratch_bytes, DEFAULT_CAPACITY_BYTES);
        assert_eq!(df.disk_count, 0);
    }

    #[test]
    fn disk_rows_round_trip_leniently() {
        let camel = r#"{"id":"d1","name":"gdrive","provider":"p","capacityBytes":42,
                        "state":"attached"}"#;
        let snake = r#"{"id":"d2","capacity_bytes":7,"attached":false}"#;
        let caps = r#"{"id":"d3","capacity_bytes":1,"capabilities":{"compute":4}}"#;
        let a = DiskRecord::from_json("d1", camel).expect("camel");
        assert_eq!(a.capacity_bytes, 42);
        assert!(a.attached());
        assert_eq!(a.compute, 1, "absent capability defaults to one slot");
        let b = DiskRecord::from_json("d2", snake).expect("snake");
        assert_eq!(b.capacity_bytes, 7);
        assert!(!b.attached());
        let c = DiskRecord::from_json("d3", caps).expect("caps");
        assert_eq!(c.compute, 4, "reads capabilities.compute");
        assert!(DiskRecord::from_json("bad", "not json").is_none());
    }
}
