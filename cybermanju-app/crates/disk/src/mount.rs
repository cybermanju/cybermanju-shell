// CyberManju OS — Mount (crates/disk/src/mount.rs).
//! `cybermanju mount <path>` — make it a disk, not a sync job.
//!
//! The portable path is the HTTP block API (item 8): Termux/Android has no
//! `/dev/fuse`, Docker and the web build use HTTP, and this build cannot
//! link `libfuse` at all. So the FUSE backend is compiled for linux only
//! and guarded by a runtime probe that *degrades* — `mount` reports why FUSE
//! is unavailable and points at the block API instead of panicking (item 9).

use cybermanju_db::Database;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// One mounted volume. Dropping it unmounts.
pub struct Mount {
    #[cfg(target_os = "linux")]
    _session: fuser::BackgroundSession,
    mountpoint: PathBuf,
}

impl Mount {
    pub fn mountpoint(&self) -> &Path {
        &self.mountpoint
    }
}

/// Is a FUSE mount possible on this host?
///
/// Two independent reasons to say no: the platform was never compiled with
/// a FUSE backend, or the kernel device node is missing.
pub fn fuse_available() -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err(format!(
            "unsupported: FUSE is unavailable on {} — use the HTTP block API \
             (GET/PUT /api/volume/block/{{lba}})",
            std::env::consts::OS
        ));
    }
    if !Path::new("/dev/fuse").exists() {
        return Err(
            "unsupported: FUSE unavailable (no /dev/fuse) — use the HTTP block API \
             (GET/PUT /api/volume/block/{lba})"
                .to_string(),
        );
    }
    Ok(())
}

/// Mount the merged volume read-only at `mountpoint`.
pub fn mount(db: Arc<RwLock<Database>>, mountpoint: &Path) -> Result<Mount, String> {
    fuse_available()?;
    {
        let guard = db.read().map_err(|e| e.to_string())?;
        let usage = crate::volume::df(&guard)?;
        if usage.disk_count == 0 {
            return Err(
                "not_found: no .cybermanju disk is attached — attach one before mounting"
                    .to_string(),
            );
        }
    }
    std::fs::create_dir_all(mountpoint).map_err(|e| {
        format!(
            "unsupported: cannot create mountpoint '{}': {}",
            mountpoint.display(),
            e
        )
    })?;

    #[cfg(target_os = "linux")]
    {
        let options = [
            fuser::MountOption::FSName("cybermanju".to_string()),
            fuser::MountOption::Subtype("cybermanju".to_string()),
            fuser::MountOption::RO,
        ];
        let session = fuser::spawn_mount2(VolumeFs { db }, mountpoint, &options)
            .map_err(|e| format!("network: could not mount '{}': {}", mountpoint.display(), e))?;
        Ok(Mount {
            _session: session,
            mountpoint: mountpoint.to_path_buf(),
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = db;
        let _ = mountpoint;
        // `fuse_available()` already said no; this branch only exists so the
        // function has the same shape everywhere.
        Err(format!(
            "unsupported: FUSE is unavailable on {} — use the HTTP block API",
            std::env::consts::OS
        ))
    }
}

// ─── the filesystem ──────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod fs {
    use super::*;
    use crate::volume;
    use fuser::{
        FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty,
        ReplyEntry, ReplyOpen, FUSE_ROOT_ID,
    };
    use std::time::{Duration, SystemTime};

    const FILE_INO: u64 = 2;
    const FILE_NAME: &str = "volume.bin";
    const TTL: Duration = Duration::from_secs(1);
    const BLKSIZE: u32 = 4096;

    const ENOENT: i32 = 2;
    const EIO: i32 = 5;
    const EACCES: i32 = 13;
    const EISDIR: i32 = 21;

    /// A read-only view of the merged volume as a single sparse file:
    /// byte offset `n` is block `n / block_size`, unwritten blocks read as
    /// holes.
    pub struct VolumeFs {
        // `pub(super)`: the literal is constructed in `mount()` above, one
        // module up — E0451 otherwise (CI, run 37303836853).
        pub(super) db: Arc<RwLock<Database>>,
    }

    impl VolumeFs {
        fn with_db<T>(&self, f: impl FnOnce(&Database) -> Result<T, String>) -> Result<T, String> {
            let guard = self.db.read().map_err(|e| e.to_string())?;
            f(&guard)
        }

        fn block_size(&self) -> u64 {
            self.with_db(|db| {
                Ok(crate::catalog::attached_disks(db)?
                    .first()
                    .map(|row| u64::from(row.block_size))
                    .unwrap_or(u64::from(crate::DEFAULT_BLOCK_SIZE)))
            })
            .unwrap_or(u64::from(crate::DEFAULT_BLOCK_SIZE))
        }

        fn attr(&self, ino: u64) -> Result<FileAttr, String> {
            let now = SystemTime::now();
            match ino {
                FUSE_ROOT_ID => Ok(FileAttr {
                    ino,
                    size: 4096,
                    blocks: 8,
                    atime: now,
                    mtime: now,
                    ctime: now,
                    crtime: now,
                    kind: FileType::Directory,
                    perm: 0o555,
                    nlink: 2,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    blksize: BLKSIZE,
                    flags: 0,
                }),
                FILE_INO => {
                    let usage = self.with_db(volume::df)?;
                    Ok(FileAttr {
                        ino,
                        size: usage.total_bytes,
                        blocks: usage.used_bytes.div_ceil(512),
                        atime: now,
                        mtime: now,
                        ctime: now,
                        crtime: now,
                        kind: FileType::RegularFile,
                        perm: 0o444,
                        nlink: 1,
                        uid: 0,
                        gid: 0,
                        rdev: 0,
                        blksize: self.block_size().min(u64::from(u32::MAX)) as u32,
                        flags: 0,
                    })
                }
                _ => Err(format!("not_found: inode {} does not exist", ino)),
            }
        }

        fn errno(message: &str) -> i32 {
            if message.starts_with("not_found:") {
                ENOENT
            } else if message.starts_with("auth:") || message.starts_with("unavailable:") {
                EACCES
            } else {
                EIO
            }
        }
    }

    impl Filesystem for VolumeFs {
        fn lookup(
            &mut self,
            _req: &fuser::Request<'_>,
            parent: u64,
            name: &std::ffi::OsStr,
            reply: ReplyEntry,
        ) {
            if parent != FUSE_ROOT_ID || name.to_string_lossy() != FILE_NAME {
                reply.error(ENOENT);
                return;
            }
            match self.attr(FILE_INO) {
                Ok(attr) => reply.entry(&TTL, &attr, 0),
                Err(err) => reply.error(Self::errno(&err)),
            }
        }

        fn getattr(&mut self, _req: &fuser::Request<'_>, ino: u64, reply: ReplyAttr) {
            match self.attr(ino) {
                Ok(attr) => reply.attr(&TTL, &attr),
                Err(err) => reply.error(Self::errno(&err)),
            }
        }

        fn opendir(&mut self, _req: &fuser::Request<'_>, ino: u64, _flags: i32, reply: ReplyOpen) {
            if ino == FUSE_ROOT_ID {
                reply.opened(0, 0);
            } else {
                reply.error(ENOENT);
            }
        }

        fn readdir(
            &mut self,
            _req: &fuser::Request<'_>,
            ino: u64,
            _fh: u64,
            offset: i64,
            mut reply: ReplyDirectory,
        ) {
            if ino != FUSE_ROOT_ID {
                reply.error(ENOENT);
                return;
            }
            let entries = [
                (FUSE_ROOT_ID, ".", FileType::Directory),
                (FUSE_ROOT_ID, "..", FileType::Directory),
                (FILE_INO, FILE_NAME, FileType::RegularFile),
            ];
            for (index, (entry_ino, name, kind)) in entries.iter().enumerate() {
                let next = (index + 1) as i64;
                if offset >= next {
                    continue;
                }
                if !reply.add(*entry_ino, next, *kind, *name) {
                    break;
                }
            }
            reply.ok();
        }

        fn open(&mut self, _req: &fuser::Request<'_>, ino: u64, _flags: i32, reply: ReplyOpen) {
            if ino == FILE_INO {
                reply.opened(0, 0);
            } else {
                reply.error(EISDIR);
            }
        }

        fn read(
            &mut self,
            _req: &fuser::Request<'_>,
            ino: u64,
            _fh: u64,
            offset: i64,
            size: u32,
            _flags: i32,
            _lock_owner: Option<u64>,
            reply: ReplyData,
        ) {
            if ino != FILE_INO {
                reply.error(EISDIR);
                return;
            }
            if offset < 0 {
                reply.error(EIO);
                return;
            }
            let block_size = self.block_size().max(1);
            let mut out: Vec<u8> = Vec::with_capacity(size as usize);
            let mut cursor = offset as u64;
            let end = cursor.saturating_add(u64::from(size));
            while cursor < end && out.len() < size as usize {
                let lba = cursor / block_size;
                let inner = (cursor % block_size) as usize;
                let want = ((block_size as usize) - inner).min((end - cursor) as usize);
                match self.with_db(|db| volume::read_block(db, lba, inner, Some(want))) {
                    Ok(bytes) => {
                        let mut slice = bytes;
                        if slice.len() < want {
                            slice.resize(want, 0);
                        }
                        out.extend_from_slice(&slice[..want]);
                    }
                    Err(err) if err.starts_with("not_found:") => out.resize(out.len() + want, 0),
                    Err(err) => {
                        reply.error(Self::errno(&err));
                        return;
                    }
                }
                cursor += want as u64;
            }
            reply.data(&out);
        }

        fn release(
            &mut self,
            _req: &fuser::Request<'_>,
            _ino: u64,
            _fh: u64,
            _flags: i32,
            _lock_owner: Option<u64>,
            _flush: bool,
            reply: ReplyEmpty,
        ) {
            reply.ok();
        }
    }
}

#[cfg(target_os = "linux")]
use fs::VolumeFs;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuse_reports_a_reason_instead_of_panicking_where_it_cannot_work() {
        match fuse_available() {
            Ok(()) => {}
            Err(reason) => {
                assert!(reason.starts_with("unsupported:"), "{reason}");
                assert!(
                    reason.contains("block API"),
                    "the message must point at the portable path: {reason}"
                );
            }
        }
    }

    #[test]
    fn mounting_without_a_disk_says_so() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("m.redb").to_str().unwrap()).expect("db");
        let db = Arc::new(RwLock::new(db));
        let mountpoint = dir.path().join("mnt");
        let err = match mount(db, &mountpoint) {
            Err(err) => err,
            Ok(_) => panic!("mounting an empty volume must fail"),
        };
        assert!(
            err.starts_with("unsupported:") || err.starts_with("not_found:"),
            "{err}"
        );
    }
}
