// CyberManju OS — Superblock (crates/disk/src/superblock.rs).
//! The `.cybermanju` container: a sealed superblock header, the block map it
//! commits to, and a footer that points back at it.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//!  0  magic "CYBMJU1"                       7 bytes
//!  7  format version (u32)                   4 bytes
//! 11  block-map section length (u64)         8 bytes
//! 19  block-map section          ← block_map_footer points here
//! ..  sealed superblock length (u64)         8 bytes
//! ..  sealed superblock (keystore::seal)
//! ..  footer: section offset (u64) │ BLAKE3(section) (32) │ magic (7)
//! ```
//!
//! The header is sealed under the disk passphrase (Argon2id + ChaCha20-
//! Poly1305, the same `CYBE1`-style wrapping the artifacts use), so the
//! container carries no plaintext capacity, provider id or checksum. The
//! section is plain — it is authenticated by the checksum inside the *sealed*
//! header as well as by the footer, so tampering with either is detected by
//! [`Superblock::verify`].

use crate::{DISK_MAGIC, FORMAT_VERSION};
use cybermanju_crypto::keystore;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Byte offset of the block-map section in every container.
pub const SECTION_OFFSET: u64 = 7 + 4 + 8;

/// Bytes of the plaintext footer: `section offset (8) │ checksum (32) │ magic (7)`.
pub const FOOTER_LEN: usize = 8 + 32 + DISK_MAGIC.len();

/// Header of a `.cybermanju` disk — what makes the object self-describing
/// and portable without the local catalog (MISSING.md A4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Superblock {
    /// Always `"CYBMJU1"`; duplicated in the binary prefix so a file can be
    /// sniffed without decrypting it.
    pub magic: String,
    /// Container format version this header was written with.
    pub format_version: u32,
    /// Identity of this disk.
    pub disk_id: String,
    /// UUID of the logical volume the disk belongs to.
    pub volume_uuid: String,
    /// Provider (`sync_configs.id`) whose blocks back this disk.
    pub config_id: String,
    /// The choosable size the user picked, in bytes.
    pub capacity_bytes: u64,
    /// Bytes per block — the granularity of the address space.
    pub block_size: u32,
    /// Keystore handle id guarding this disk (`disk/<id>`).
    pub key_handle: String,
    pub created_at: String,
    pub updated_at: String,
    /// BLAKE3 (hex) over the block-map section.
    pub block_map_checksum: String,
    /// Byte offset of the block-map section — the footer pointer.
    pub block_map_footer: u64,
}

impl Superblock {
    /// Build a header for a freshly created disk.
    ///
    /// Eight arguments because the header is eight facts about a disk —
    /// splitting them into builders would only move the same fields around.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        disk_id: impl Into<String>,
        volume_uuid: impl Into<String>,
        config_id: impl Into<String>,
        capacity_bytes: u64,
        block_size: u32,
        key_handle: impl Into<String>,
        now: impl Into<String>,
        block_map_checksum: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            magic: String::from_utf8_lossy(DISK_MAGIC).into_owned(),
            format_version: FORMAT_VERSION,
            disk_id: disk_id.into(),
            volume_uuid: volume_uuid.into(),
            config_id: config_id.into(),
            capacity_bytes,
            block_size,
            key_handle: key_handle.into(),
            created_at: now.clone(),
            updated_at: now,
            block_map_checksum: block_map_checksum.into(),
            block_map_footer: SECTION_OFFSET,
        }
    }

    /// Serialize to bytes: magic, format version, length, JSON header.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let json = serde_json::to_vec(self)
            .map_err(|e| format!("integrity: superblock could not be serialized: {}", e))?;
        if json.len() > u32::MAX as usize {
            return Err("integrity: superblock header is too large".to_string());
        }
        let mut out = Vec::with_capacity(7 + 4 + 4 + json.len());
        out.extend_from_slice(DISK_MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&json);
        Ok(out)
    }

    /// Inverse of [`Superblock::encode`], validating magic and version.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 7 + 4 + 4 {
            return Err("integrity: superblock header is truncated".to_string());
        }
        if &bytes[..7] != DISK_MAGIC {
            return Err(format!(
                "integrity: not a .cybermanju superblock (magic {:02x?})",
                &bytes[..7.min(bytes.len())]
            ));
        }
        let version = u32::from_le_bytes(bytes[7..11].try_into().expect("4 bytes"));
        if version != FORMAT_VERSION {
            return Err(format!(
                "unsupported: container format version {} (this build speaks {})",
                version, FORMAT_VERSION
            ));
        }
        let json_len = u32::from_le_bytes(bytes[11..15].try_into().expect("4 bytes")) as usize;
        let body = bytes
            .get(15..15 + json_len)
            .ok_or_else(|| "integrity: superblock header body is truncated".to_string())?;
        let header: Superblock = serde_json::from_slice(body)
            .map_err(|e| format!("integrity: superblock header is not valid JSON: {}", e))?;
        if header.magic != String::from_utf8_lossy(DISK_MAGIC) {
            return Err("integrity: superblock magic inside the header does not match".to_string());
        }
        if header.format_version != FORMAT_VERSION {
            return Err(format!(
                "unsupported: superblock declares format version {} (this build speaks {})",
                header.format_version, FORMAT_VERSION
            ));
        }
        if header.block_map_footer != SECTION_OFFSET {
            return Err(format!(
                "integrity: block-map footer pointer is {} (expected {})",
                header.block_map_footer, SECTION_OFFSET
            ));
        }
        Ok(header)
    }

    /// Verify that `section` is exactly the block map this header commits to.
    pub fn verify(&self, section: &[u8]) -> Result<(), String> {
        let actual = blake3_hex(section);
        if actual != self.block_map_checksum {
            return Err(format!(
                "integrity: block map checksum mismatch (header {}, file {})",
                self.block_map_checksum, actual
            ));
        }
        Ok(())
    }

    /// Stamp an update (resize, checkpoint) while keeping `created_at`.
    pub fn touch(&mut self, capacity_bytes: u64, updated_at: impl Into<String>) {
        self.capacity_bytes = capacity_bytes;
        self.updated_at = updated_at.into();
    }
}

/// BLAKE3 (hex) of `bytes` — the checksum the footer and the header share.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Write a container: `encode → seal → file`, atomically (temp file +
/// rename) so a crash can never leave a half-written superblock behind.
pub fn write_container(
    path: &Path,
    passphrase: &str,
    superblock: &Superblock,
    section: &[u8],
) -> Result<(), String> {
    let header = superblock.encode()?;
    let sealed = keystore::seal(passphrase, &header)
        .map_err(|e| format!("integrity: could not seal superblock: {}", e))?;
    if sealed.len() > u64::MAX as usize {
        return Err("integrity: sealed superblock is too large".to_string());
    }

    let mut file =
        Vec::with_capacity(SECTION_OFFSET as usize + section.len() + 8 + sealed.len() + FOOTER_LEN);
    file.extend_from_slice(DISK_MAGIC);
    file.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    file.extend_from_slice(&(section.len() as u64).to_le_bytes());
    file.extend_from_slice(section);
    file.extend_from_slice(&(sealed.len() as u64).to_le_bytes());
    file.extend_from_slice(&sealed);
    // Footer: where the section lives, and its digest.
    file.extend_from_slice(&SECTION_OFFSET.to_le_bytes());
    file.extend_from_slice(blake3::hash(section).as_bytes());
    file.extend_from_slice(DISK_MAGIC);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("integrity: cannot create '{}': {}", parent.display(), e))?;
    }
    let tmp = path.with_extension("cybermanju.tmp");
    std::fs::write(&tmp, &file)
        .map_err(|e| format!("integrity: cannot write '{}': {}", tmp.display(), e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("integrity: cannot install '{}': {}", path.display(), e)
    })
}

/// Read a container back: `file → open → decode → verify checksum`.
///
/// Returns the header and the block-map section it commits to.
pub fn read_container(path: &Path, passphrase: &str) -> Result<(Superblock, Vec<u8>), String> {
    let file = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "not_found: .cybermanju container '{}' does not exist",
                path.display()
            )
        } else {
            format!("integrity: cannot read '{}': {}", path.display(), e)
        }
    })?;
    if file.len() < SECTION_OFFSET as usize + 8 + FOOTER_LEN {
        return Err(format!(
            "integrity: '{}' is too short to be a .cybermanju container",
            path.display()
        ));
    }
    if &file[..7] != DISK_MAGIC {
        return Err(format!(
            "integrity: '{}' does not start with the CYBMJU1 magic",
            path.display()
        ));
    }
    let version = u32::from_le_bytes(file[7..11].try_into().expect("4 bytes"));
    if version != FORMAT_VERSION {
        return Err(format!(
            "unsupported: container format version {} (this build speaks {})",
            version, FORMAT_VERSION
        ));
    }
    let section_len = u64::from_le_bytes(file[11..19].try_into().expect("8 bytes")) as usize;
    let section_end = SECTION_OFFSET as usize + section_len;
    let sealed_len_pos = section_end;
    let section = file
        .get(SECTION_OFFSET as usize..section_end)
        .ok_or_else(|| "integrity: block-map section is truncated".to_string())?
        .to_vec();

    let sealed_len = read_u64(&file, sealed_len_pos)?;
    let sealed_pos = sealed_len_pos + 8;
    let sealed_end = sealed_pos
        .checked_add(sealed_len as usize)
        .ok_or_else(|| "integrity: sealed header length overflows".to_string())?;
    let sealed = file
        .get(sealed_pos..sealed_end)
        .ok_or_else(|| "integrity: sealed superblock is truncated".to_string())?;

    let header_bytes = keystore::open_sealed(passphrase, sealed).map_err(|e| {
        format!(
            "auth: superblock did not open — wrong passphrase or corrupted container ({})",
            e
        )
    })?;
    let superblock = Superblock::decode(&header_bytes)?;

    // Footer: section offset, checksum, magic.
    let footer = file
        .get(sealed_end..sealed_end + FOOTER_LEN)
        .ok_or_else(|| "integrity: container footer is missing".to_string())?;
    let footer_offset = u64::from_le_bytes(footer[0..8].try_into().expect("8 bytes"));
    if footer_offset != SECTION_OFFSET {
        return Err(format!(
            "integrity: footer points at {} (section starts at {})",
            footer_offset, SECTION_OFFSET
        ));
    }
    let footer_checksum = &footer[8..40];
    if footer_checksum != blake3::hash(&section).as_bytes() {
        return Err("integrity: footer checksum does not match the block map".to_string());
    }
    if &footer[40..47] != DISK_MAGIC {
        return Err("integrity: container footer is missing its magic".to_string());
    }
    superblock.verify(&section)?;

    Ok((superblock, section))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    let slice = bytes
        .get(at..at + 8)
        .ok_or_else(|| "integrity: container is truncated".to_string())?;
    Ok(u64::from_le_bytes(slice.try_into().expect("8 bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_container(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cybermanju-sb-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_file(&dir);
        dir
    }

    fn sample() -> (Superblock, Vec<u8>) {
        let section = b"block-map-section".to_vec();
        let sb = Superblock::new(
            "disk-1",
            "vol-uuid-1",
            "config-1",
            4 * 64 * 1024,
            64 * 1024,
            "disk/disk-1",
            "2026-10-05T00:00:00Z",
            blake3_hex(&section),
        );
        (sb, section)
    }

    #[test]
    fn encode_seal_open_decode_verifies_the_checksum() {
        let path = tmp_container("roundtrip.cybermanju");
        let (sb, section) = sample();
        write_container(&path, "hunter2", &sb, &section).expect("write");

        let (opened, read_section) = read_container(&path, "hunter2").expect("read");
        assert_eq!(opened, sb, "header must round-trip byte-for-byte");
        assert_eq!(read_section, section, "section must round-trip");
        opened.verify(&read_section).expect("checksum");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn wrong_passphrase_and_tampering_are_refused() {
        let path = tmp_container("tamper.cybermanju");
        let (sb, section) = sample();
        write_container(&path, "right", &sb, &section).expect("write");

        let err = read_container(&path, "wrong").expect_err("must not open");
        assert!(err.starts_with("auth:"), "{err}");

        // Corrupt one byte of the sealed header → the AEAD must refuse.
        let mut bytes = std::fs::read(&path).expect("read raw");
        let idx = bytes.len() - FOOTER_LEN - 1;
        bytes[idx] ^= 0xff;
        std::fs::write(&path, &bytes).expect("rewrite");
        let err = read_container(&path, "right").expect_err("must not open");
        assert!(
            err.starts_with("auth:") || err.starts_with("integrity:"),
            "{err}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn decode_rejects_foreign_magic_and_future_versions() {
        let err = Superblock::decode(b"NOTCYBM00000000000").expect_err("magic");
        assert!(err.starts_with("integrity:"), "{err}");

        let mut good = Superblock::new("d", "v", "c", 1, 64 * 1024, "disk/d", "now", "00")
            .encode()
            .expect("encode");
        good[7] = 99; // bump the binary format version
        let err = Superblock::decode(&good).expect_err("version");
        assert!(err.starts_with("unsupported:"), "{err}");
    }

    #[test]
    fn verify_detects_a_swapped_block_map() {
        let (_sb, mut section) = sample();
        let mut sb = Superblock::new(
            "d",
            "v",
            "c",
            1,
            64 * 1024,
            "disk/d",
            "now",
            blake3_hex(&section),
        );
        sb.verify(&section).expect("clean");
        section.push(0x01);
        let err = sb.verify(&section).expect_err("tampered");
        assert!(err.starts_with("integrity:"), "{err}");
        sb.block_map_footer = 7;
        assert!(Superblock::decode(&sb.encode().expect("encode")).is_err());
    }
}
