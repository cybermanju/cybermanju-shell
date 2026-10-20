//! CyberManju OS — `.cybermanju` disk & volume substrate (AGENT-6).
//!
//! The decentralized-OS storage object: a provider-backed virtual disk with an
//! adjustable, choosable capacity, a superblock that is sealed and
//! content-addressed, a block allocator with refcounts, and a volume manager
//! that merges N attached disks into one logical address space.
//!
//! ```text
//! superblock  CYBMJU1 header, keystore-sealed, checksums the block map
//! allocator   slots, BLAKE3 dedup, refcounts for AGENT-7's GC
//! disk        create · attach · detach · resize · check · destroy
//! volume      df, admission control, spanned placement, block read/write
//! mount       FUSE where the platform allows it, HTTP block API everywhere
//! ```
//!
//! This crate deliberately does **not** depend on `cybermanju-web` or
//! `cybermanju-os`; the dependency arrow points *into* it.

pub mod allocator;
pub mod catalog;
pub mod disk;
pub mod mount;
pub mod superblock;
pub mod volume;

#[cfg(test)]
mod testkit;

/// On-disk container format version for `.cybermanju` superblocks.
pub const FORMAT_VERSION: u32 = 1;

/// Magic prefix of a `.cybermanju` superblock ("CYBMJU1").
pub const DISK_MAGIC: &[u8; 7] = b"CYBMJU1";

/// Bytes per block — the granularity of the address space.
pub const DEFAULT_BLOCK_SIZE: u32 = 64 * 1024;

pub use catalog::{BlockRow, DiskRow, VolumeRow};
pub use disk::CheckReport;
pub use volume::{BlockInfo, DiskUsage, VolumeDf};

// ─── volume hooks (MISSING.md D1) ────────────────────────────────────────────
//
// `cybermanju-sync` cannot depend on this crate (this crate already depends
// on it), so the write path asks through the hooks in `quota` instead. They
// are installed from a static constructor, which arms them before `main` —
// no call-ordering requirement on the HTTP layer, the auto-sync scheduler or
// a test.

use cybermanju_db::Database;
use cybermanju_sync::quota::{self, VolumeHooks, VolumeUsage};

fn hook_usage(db: &Database) -> Result<VolumeUsage, String> {
    let usage = volume::df(db)?;
    Ok(VolumeUsage {
        total_bytes: usage.total_bytes,
        used_bytes: usage.used_bytes,
        free_bytes: usage.free_bytes,
        disk_count: usage.disk_count as u32,
    })
}

fn hook_admit(db: &Database, bytes: u64) -> Result<(), String> {
    volume::admit(db, bytes)
}

fn hook_charge(db: &Database, config_id: &str, bytes: u64) -> Result<(), String> {
    volume::charge(db, config_id, bytes)
}

/// Install the hooks. Exposed so a test can assert the wiring directly.
pub fn arm_volume_hooks() -> Result<(), String> {
    quota::register_volume_hooks(VolumeHooks {
        usage: hook_usage,
        admit: hook_admit,
        charge: hook_charge,
    })
}

#[ctor::ctor]
fn arm_volume_hooks_at_startup() {
    // A second registration can only mean two copies of this crate are
    // linked; the first one wins and that is fine.
    let _ = arm_volume_hooks();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_hooks_are_armed_and_agree_with_df() {
        assert!(
            quota::volume_hooks().is_some(),
            "the static constructor arms them before any test runs"
        );
        let err = arm_volume_hooks().expect_err("registration is once only");
        assert!(err.contains("already registered"), "{err}");

        let dir = tempfile::tempdir().expect("tempdir");
        let db = Database::new(dir.path().join("hooks.redb").to_str().unwrap()).expect("db");

        // Without any disk there is nothing to police, but `usage` still has
        // to answer honestly (zeros, not a guess) now that a substrate exists.
        let usage = quota::volume_usage(&db).expect("usage");
        assert_eq!(usage.total_bytes, 0);
        assert_eq!(usage.disk_count, 0);
        quota::admit_write(&db, u64::MAX).expect("nothing to enforce");

        let json = serde_json::to_string(&usage).expect("json");
        assert!(json.contains("\"freeBytes\":0"), "{json}");
        assert!(json.contains("\"diskCount\":0"), "{json}");
    }
}
