// CyberManju OS — Backup (crates/os/src/backup.rs).
// Role: Vault snapshot exports as passphrase-sealed CYBMJ01 containers.
// Exports: backup_name, is_backup_name, export_volume, export_provider,
//   list_volume, list_provider, verify_bytes, BackupReport, BackupEntry,
//   VerifyReport, BACKUP_DIR, DEFAULT_KEEP.
// Integrates: cybermanju_crypto::container, cybermanju_db, api::Kernel,
//   provider_fs, cybermanju_compression (BLAKE3 fingerprints).
// Contracts: containers are byte-compatible with `src/utils/container.ts`
//   (same magic/flags/KDF), so a desktop export restores on Pages and vice
//   versa; every failure carries an AGENT-1 prefix (`auth:/not_found:/
//   too_large:/integrity:/network:`); fingerprints are BLAKE3 hex of the
//   sealed artifact, comparable across transports.

use crate::api::{Kernel, OpenFlags};
use cybermanju_db::Database;

/// Default vault directory for local snapshot exports.
pub const BACKUP_DIR: &str = "/backups";
/// Default prune depth (mirrors `scripts/backup.sh --keep`).
pub const DEFAULT_KEEP: usize = 7;
/// Snapshot images larger than this refuse instead of OOMing the shell.
const SNAPSHOT_CAP: u64 = 256 * 1024 * 1024;
const PREFIX: &str = "cybermanju-";
const EXT: &str = ".cybermanju";

/// One finished export.
pub struct BackupReport {
    pub path: String,
    pub bytes: u64,
    pub blake3: String,
    pub encrypted: bool,
    pub pruned: Vec<String>,
}

/// One existing snapshot file.
pub struct BackupEntry {
    pub name: String,
    pub bytes: u64,
}

/// Header (+ optional deep) verification of a snapshot file.
pub struct VerifyReport {
    pub path: String,
    pub legacy: bool,
    pub version: u8,
    pub encrypted: bool,
    pub compressed: bool,
    pub body_bytes: usize,
    pub total_bytes: usize,
    pub blake3: String,
    pub image_bytes: Option<usize>,
}

/// UTC stamp shaped like `scripts/backup.sh` (`20261011T201500Z`).
pub fn utc_stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

/// Fresh snapshot file name (`cybermanju-<stamp>.cybermanju`).
pub fn backup_name() -> String {
    format!("{PREFIX}{}{EXT}", utc_stamp())
}

/// True for files this module manages (prune never touches anything else).
pub fn is_backup_name(name: &str) -> bool {
    name.starts_with(PREFIX) && name.ends_with(EXT)
}

fn snapshot_image(db: &Database) -> Result<Vec<u8>, String> {
    let image = db
        .snapshot_bytes()
        .map_err(|e| format!("integrity: database snapshot failed: {e:#}"))?;
    if image.len() as u64 > SNAPSHOT_CAP {
        return Err("too_large: database snapshot exceeds the 256 MiB cap".to_string());
    }
    Ok(image)
}

fn fingerprint(data: &[u8]) -> String {
    cybermanju_compression::TripleCompressor::blake3_hash(data)
}

fn seal(image: &[u8], passphrase: &str) -> Result<(Vec<u8>, bool), String> {
    let bytes = cybermanju_crypto::container::encode_container(image, passphrase)?;
    Ok((bytes, !passphrase.is_empty()))
}

fn ensure_volume_dir(kernel: &Kernel, dir: &str) -> Result<(), String> {
    if kernel.readdir(dir).is_ok() {
        return Ok(());
    }
    match kernel.mkdir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.starts_with("already exists") => Ok(()),
        Err(e) => Err(e),
    }
}

fn write_volume(kernel: &Kernel, path: &str, data: &[u8]) -> Result<(), String> {
    let fd = kernel.open(path, OpenFlags::create())?;
    let mut at = 0;
    while at < data.len() {
        let n = kernel.write(fd, &data[at..])?;
        if n == 0 {
            let _ = kernel.close(fd);
            return Err(format!("io error: short write to {path}"));
        }
        at += n;
    }
    kernel.close(fd)
}

fn read_volume(kernel: &Kernel, path: &str) -> Result<Vec<u8>, String> {
    let fd = kernel.open(path, OpenFlags::read_only())?;
    let mut out = Vec::new();
    loop {
        let chunk = kernel.read(fd, 64 * 1024)?;
        if chunk.is_empty() {
            break;
        }
        out.extend_from_slice(&chunk);
    }
    kernel.close(fd)?;
    Ok(out)
}

/// Names in `dir` beyond `keep` (oldest first) — the prune victims.
fn prune_victims(mut names: Vec<String>, keep: usize) -> Vec<String> {
    names.sort();
    if names.len() <= keep {
        return Vec::new();
    }
    names[..names.len() - keep].to_vec()
}

/// Snapshot → sealed container → vault volume file (+ prune).
pub fn export_volume(
    db: &Database,
    kernel: &Kernel,
    dir: &str,
    name: Option<&str>,
    keep: usize,
    passphrase: &str,
) -> Result<BackupReport, String> {
    ensure_volume_dir(kernel, dir)?;
    let mut file = name.unwrap_or("").to_string();
    if file.is_empty() {
        file = backup_name();
    }
    if file.contains('/') || file.contains('\0') {
        return Err("invalid: backup file name must be a plain file name".to_string());
    }
    let image = snapshot_image(db)?;
    let (sealed, encrypted) = seal(&image, passphrase)?;
    let path = format!("{}/{file}", dir.trim_end_matches('/'));
    write_volume(kernel, &path, &sealed)?;
    let mut pruned = Vec::new();
    if keep > 0 {
        let names: Vec<String> = kernel
            .readdir(dir)?
            .into_iter()
            .filter(|e| !e.is_dir && is_backup_name(&e.name) && e.name != file)
            .map(|e| e.name)
            .collect();
        for victim in prune_victims(names, keep.saturating_sub(1)) {
            let victim_path = format!("{}/{victim}", dir.trim_end_matches('/'));
            // A failed prune never fails the export that just landed.
            if kernel.unlink(&victim_path).is_ok() {
                pruned.push(victim);
            }
        }
    }
    Ok(BackupReport {
        path,
        bytes: sealed.len() as u64,
        blake3: fingerprint(&sealed),
        encrypted,
        pruned,
    })
}

/// Snapshot → sealed container → provider path (`head` + `rest` already
/// split by the caller). Mirrors `export_volume`, same report shape.
pub fn export_provider(
    db: &Database,
    head: &str,
    rest: &str,
    name: Option<&str>,
    keep: usize,
    passphrase: &str,
) -> Result<BackupReport, String> {
    let mut file = name.unwrap_or("").to_string();
    if file.is_empty() {
        file = backup_name();
    }
    if file.contains('/') || file.contains('\0') {
        return Err("invalid: backup file name must be a plain file name".to_string());
    }
    let clean = rest.trim().trim_matches('/');
    let remote = if clean.is_empty() {
        file.clone()
    } else {
        format!("{clean}/{file}")
    };
    let image = snapshot_image(db)?;
    let (sealed, encrypted) = seal(&image, passphrase)?;
    crate::provider_fs::write_file(db, head, &remote, &sealed)?;
    let mut pruned = Vec::new();
    if keep > 0 {
        let names: Vec<String> = crate::provider_fs::list_dir(db, head, clean)?
            .into_iter()
            .filter(|e| !e.is_dir && is_backup_name(&e.name) && e.name != file)
            .map(|e| e.name)
            .collect();
        for victim in prune_victims(names, keep.saturating_sub(1)) {
            let victim_remote = if clean.is_empty() {
                victim.clone()
            } else {
                format!("{clean}/{victim}")
            };
            if crate::provider_fs::delete_file(db, head, &victim_remote).is_ok() {
                pruned.push(victim);
            }
        }
    }
    Ok(BackupReport {
        path: format!("/providers/{head}/{remote}"),
        bytes: sealed.len() as u64,
        blake3: fingerprint(&sealed),
        encrypted,
        pruned,
    })
}

/// Snapshot files in a vault volume directory (missing dir = none yet).
pub fn list_volume(kernel: &Kernel, dir: &str) -> Result<Vec<BackupEntry>, String> {
    let entries = match kernel.readdir(dir) {
        Ok(rows) => rows,
        Err(e) if e.starts_with("not_found:") => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out: Vec<BackupEntry> = entries
        .into_iter()
        .filter(|e| !e.is_dir && is_backup_name(&e.name))
        .map(|e| BackupEntry {
            name: e.name,
            bytes: e.size_bytes,
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Snapshot files in a provider directory.
pub fn list_provider(db: &Database, head: &str, rest: &str) -> Result<Vec<BackupEntry>, String> {
    let mut out: Vec<BackupEntry> = crate::provider_fs::list_dir(db, head, rest)?
        .into_iter()
        .filter(|e| !e.is_dir && is_backup_name(&e.name))
        .map(|e| BackupEntry {
            name: e.name,
            bytes: e.size_bytes,
        })
        .collect();
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Verify container bytes: header always, full trial-decrypt on `deep`.
/// A wrong passphrase fails closed (`auth:`) — surfacing it is the point.
pub fn verify_bytes(
    path: &str,
    bytes: &[u8],
    passphrase: &str,
    deep: bool,
) -> Result<VerifyReport, String> {
    if !cybermanju_crypto::container::is_container(bytes) {
        return Ok(VerifyReport {
            path: path.to_string(),
            legacy: true,
            version: 0,
            encrypted: false,
            compressed: false,
            body_bytes: bytes.len(),
            total_bytes: bytes.len(),
            blake3: fingerprint(bytes),
            image_bytes: Some(bytes.len()),
        });
    }
    let meta = cybermanju_crypto::container::parse_header(bytes)?;
    let mut image_bytes = None;
    if deep {
        let decoded = cybermanju_crypto::container::decode_container(bytes, passphrase)?;
        image_bytes = Some(decoded.image.len());
    }
    Ok(VerifyReport {
        path: path.to_string(),
        legacy: false,
        version: meta.version,
        encrypted: meta.encrypted,
        compressed: meta.compressed,
        body_bytes: meta.body_len,
        total_bytes: meta.total_len,
        blake3: fingerprint(bytes),
        image_bytes,
    })
}

/// Read + verify a vault volume snapshot file.
pub fn verify_volume(
    kernel: &Kernel,
    path: &str,
    passphrase: &str,
    deep: bool,
) -> Result<VerifyReport, String> {
    let bytes = read_volume(kernel, path)?;
    verify_bytes(path, &bytes, passphrase, deep)
}

/// Read + verify a provider snapshot file.
pub fn verify_provider(
    db: &Database,
    head: &str,
    rest: &str,
    passphrase: &str,
    deep: bool,
) -> Result<VerifyReport, String> {
    let bytes = crate::provider_fs::read_file(db, head, rest)?;
    verify_bytes(
        &format!("/providers/{head}/{rest}"),
        &bytes,
        passphrase,
        deep,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_shapes_the_file_name() {
        let name = backup_name();
        assert!(is_backup_name(&name), "shaped: {name}");
        assert!(!is_backup_name("notes.md"));
        assert!(!is_backup_name("cybermanju-x.txt"));
        let stamp = utc_stamp();
        assert_eq!(stamp.len(), 16, "stamp: {stamp}");
        assert!(stamp.ends_with('Z'));
    }

    #[test]
    fn prune_keeps_the_newest() {
        let names = vec![
            "cybermanju-20261011T200000Z.cybermanju".to_string(),
            "cybermanju-20261011T210000Z.cybermanju".to_string(),
            "cybermanju-20261011T220000Z.cybermanju".to_string(),
        ];
        assert_eq!(
            prune_victims(names.clone(), 2),
            vec!["cybermanju-20261011T200000Z.cybermanju".to_string()]
        );
        assert!(prune_victims(names.clone(), 3).is_empty());
        assert_eq!(prune_victims(names, 0).len(), 3);
    }
}
