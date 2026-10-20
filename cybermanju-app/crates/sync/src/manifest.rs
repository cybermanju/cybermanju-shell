// CyberManju OS — Chunk manifest + multi-provider placement (AGENT-2 item 10)
//
// The "decentralized PC" core: a file is split into 4 MiB chunks, each
// addressed by the BLAKE3 of its **plaintext**, and the chunks are placed
// round-robin across every enabled config. The manifest — which copy lives
// where, and the hash of each uploaded artifact — is stored in the
// `sync_files` row (`manifest_ref`), so restore works after eviction and
// across restarts.
//
// Parity has two encodings, and both mean the same thing — "how many provider
// losses this chunk survives" (`SyncConfig.parity`, unchanged, so existing
// configs still parse):
//
//   * **replication** — `1 + min(parity, n-1)` whole copies. What the upload
//     pipeline writes today, and what [`redundancy_for`] returns for the
//     layouts where extra copies are the cheapest option.
//   * **Reed–Solomon** (AGENT-7 item 3) — `data` + `parity` shards from
//     `cybermanju-erasure`. Cheaper than copies for the same durability:
//     `n` placements survive `parity` losses with only `n/(n-parity)`× the
//     storage. `repair::restripe` converts a replicated chunk to shards, and
//     every reader here derives the mode from the entry itself (see
//     [`redundancy_for`]), so manifests written before RS landed keep
//     restoring exactly as they did.
//
// The mode is *derived*, never stored: `total = 1 + replicas.len()` and the
// manifest's `parity`. The two writer formulas below are chosen so that
// deriving from the written locator count always reproduces the writer's
// decision — an entry can never be read back as the wrong mode.
//
// Degrade rule: striping with fewer than 2 enabled configs has nothing to
// stripe across — `pipeline` falls back to whole-file mode with a WARNING
// instead of pretending to be decentralized.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use cybermanju_compression::TripleCompressor;
use cybermanju_crypto::keystore;
use cybermanju_types::sync::{SyncConfig, SyncFile};
use serde::{Deserialize, Serialize};

use crate::backends::create_backend;
use crate::pipeline::CYBE_MAGIC;
use crate::transfer;

/// Chunk size for striped placement (4 MiB).
pub const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

/// Manifest format version — bumped when the struct changes shape.
pub const MANIFEST_VERSION: u32 = 1;

/// Where one copy of a chunk lives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChunkLoc {
    /// Sync config (provider binding) holding this copy.
    pub config_id: String,
    /// Remote locator (always `cybermanju_sync/chunks/{plaintext_hash}` —
    /// content-addressed, so a re-sync overwrites in place).
    pub remote_path: String,
    /// BLAKE3 of the exact bytes uploaded here (ciphertext when encryption
    /// is on) — the download-back verification baseline for this copy.
    pub artifact_hash: String,
}

/// One plaintext chunk and every copy of it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChunkEntry {
    /// Position in the file (0-based).
    pub index: u32,
    /// BLAKE3 of the plaintext chunk — the chunk's address and restore
    /// verification baseline.
    pub hash: String,
    /// Plaintext size of this chunk (`CHUNK_SIZE` except the last).
    pub size: u64,
    pub primary: ChunkLoc,
    /// Additional copies (see parity in [`placements`]).
    pub replicas: Vec<ChunkLoc>,
}

/// The full placement record for one striped file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChunkManifest {
    pub version: u32,
    /// BLAKE3 of the whole plaintext file (restore's final check).
    pub file_hash: String,
    pub chunk_size: u64,
    pub total_size: u64,
    pub chunks: Vec<ChunkEntry>,
    /// Replicas per chunk at plan time (for diagnostics; the actual copies
    /// are recorded per entry).
    pub parity: u8,
}

/// Placement of chunk `index` across `n` participants: primary index plus
/// replica indices. `n >= 1`; replicas are clamped to `n - 1` so a copy
/// never lands twice on one provider.
pub fn placements(index: usize, n: usize, parity: u8) -> (usize, Vec<usize>) {
    let n = n.max(1);
    let replicas = (parity as usize).min(n.saturating_sub(1));
    let primary = index % n;
    let replica_idxs = (1..=replicas).map(|j| (index + j) % n).collect();
    (primary, replica_idxs)
}

/// Content-addressed remote path for a chunk (plaintext hash).
pub fn chunk_remote_path(plaintext_hash: &str) -> String {
    format!("cybermanju_sync/chunks/{}", plaintext_hash)
}

/// Content-addressed remote path for erasure shard `index` of a chunk. Same
/// namespace as [`chunk_remote_path`] (so scrub, repair and GC see one flat
/// `cybermanju_sync/chunks/` listing), with the shard index as a suffix.
pub fn shard_remote_path(plaintext_hash: &str, index: u8) -> String {
    format!("{}.s{}", chunk_remote_path(plaintext_hash), index)
}

// ===========================================================================
// Reed–Solomon placement (AGENT-7 item 3)
// ===========================================================================

/// How a chunk's placements provide redundancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redundancy {
    /// Reed–Solomon: `data` shards of the (framed) artifact plus `parity`
    /// parity shards. Survives `parity` locator losses.
    Erasure { data: u8, parity: u8 },
    /// Whole-chunk copies: every locator holds the full artifact.
    Replica { copies: usize },
}

impl Redundancy {
    /// How many locators the manifest must record.
    pub fn loc_count(&self) -> usize {
        match self {
            Redundancy::Erasure { data, parity } => *data as usize + *parity as usize,
            Redundancy::Replica { copies } => *copies,
        }
    }

    /// Provider losses the chunk still survives.
    pub fn survives(&self) -> u8 {
        match self {
            Redundancy::Erasure { parity, .. } => *parity,
            Redundancy::Replica { copies } => copies.saturating_sub(1) as u8,
        }
    }

    /// Is this Reed–Solomon rather than plain copies?
    pub fn is_erasure(&self) -> bool {
        matches!(self, Redundancy::Erasure { .. })
    }
}

/// The redundancy for `total` placements of a chunk whose config says
/// `parity`. **Single source of truth** for both writers and readers:
///
/// * writer side — `total` is how many providers are available to place on;
/// * reader side — `total` is `1 + entry.replicas.len()` (the locators the
///   manifest actually recorded).
///
/// Reed–Solomon needs at least three placements with `data = total - parity
/// >= 2`; anything cheaper to replicate (`parity == 0`, two placements, or a
/// parity that would leave a single data shard) stays whole copies. Because
/// the replica writer always records `1 + min(parity, available-1)` locators,
/// reading that count back through this function always yields
/// `Replica` again — the derived mode can never flip.
pub fn redundancy_for(total: usize, parity: u8) -> Redundancy {
    if total == 0 {
        return Redundancy::Replica { copies: 0 };
    }
    let p = parity as usize;
    if total >= 3 && parity >= 1 && p <= total - 2 {
        Redundancy::Erasure {
            data: (total - p) as u8,
            parity,
        }
    } else {
        Redundancy::Replica {
            copies: 1 + p.min(total - 1),
        }
    }
}

/// Redundancy recorded by one manifest entry (see [`redundancy_for`]).
pub fn entry_redundancy(entry: &ChunkEntry, parity: u8) -> Redundancy {
    redundancy_for(1 + entry.replicas.len(), parity)
}

/// Locator rotation for an erasure-coded chunk: shard `j` of chunk `index`
/// lands on participant `(index + j) % n`, so shards spread across providers
/// exactly like [`placements`] spreads copies.
pub fn shard_placements(index: usize, n: usize) -> Vec<usize> {
    let n = n.max(1);
    (0..n).map(|j| (index + j) % n).collect()
}

/// The locators of an entry in **shard order** (index 0..n). For a replicated
/// entry these are the copies; for an erasure-coded entry they are the
/// `data + parity` shards.
pub fn entry_locs(entry: &ChunkEntry) -> Vec<&ChunkLoc> {
    std::iter::once(&entry.primary)
        .chain(entry.replicas.iter())
        .collect()
}

/// Shard an artifact into `data + parity` framed shards
/// (`cybermanju_erasure::encode_framed`).
pub fn encode_shards(
    artifact: &[u8],
    data: u8,
    parity: u8,
) -> Result<Vec<Vec<u8>>, cybermanju_erasure::CodecError> {
    cybermanju_erasure::encode_framed(artifact, data, parity).map(|e| e.shards)
}

/// Rebuild the artifact from any `data` present `(shard_index, bytes)` pairs.
pub fn decode_shards(
    data: u8,
    parity: u8,
    present: &[(u8, Vec<u8>)],
) -> Result<Vec<u8>, cybermanju_erasure::CodecError> {
    cybermanju_erasure::decode_framed(data, parity, present)
}

/// Undo whatever the sync pipeline did to a payload: decrypt (magic-prefixed
/// sealed blob), then decompress when it is a compressed artifact. Best
/// effort — the caller must still verify the plaintext hash; this function
/// never fabricates data, it only unwraps known layers.
pub fn decode_artifact(mut bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.starts_with(CYBE_MAGIC) {
        let passphrase = keystore::master_passphrase().ok_or_else(|| {
            "integrity: artifact is encrypted but no master passphrase is available".to_string()
        })?;
        bytes = keystore::open_sealed(&passphrase, &bytes[CYBE_MAGIC.len()..])
            .map_err(|e| format!("integrity: could not decrypt: {}", e))?;
    }
    if let Ok((plain, _size)) = TripleCompressor::new().decompress_triple(&bytes) {
        return Ok(plain);
    }
    Ok(bytes)
}

/// Download one locator's artifact bytes into a per-call temp file. No
/// verification — callers check `artifact_hash` (shards) or the plaintext
/// hash (copies) against what they expect.
fn download_artifact(
    loc: &ChunkLoc,
    configs: &HashMap<&str, &SyncConfig>,
    tag: &str,
) -> Result<Vec<u8>, String> {
    let config = configs
        .get(loc.config_id.as_str())
        .ok_or_else(|| format!("config '{}' is gone", loc.config_id))?;
    let backend = create_backend(config)?;

    // Unique per call (tag + nonce) so two concurrent restores of different
    // files never share a sidecar.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!(
        "cybermanju-chunk-{}-{}-{}-{}.part",
        std::process::id(),
        tag,
        nonce,
        loc.config_id
    ));
    let tmp_str = tmp.to_string_lossy().to_string();

    let attempt = backend
        .download_file(&loc.remote_path, &tmp_str)
        .and_then(|()| fs::read(&tmp).map_err(|e| format!("restore read failed: {}", e)));
    let _ = fs::remove_file(&tmp);
    attempt
}

/// Download and verify **one chunk** from its manifest entry, returning the
/// plaintext bytes.
///
/// * replicated entries try `primary` then each replica until one copy
///   decodes to the recorded plaintext hash;
/// * erasure-coded entries gather verified shards (stopping as soon as
///   `data` of them are in hand), rebuild the artifact and only then unwrap
///   it — a shard whose `artifact_hash` does not match is never used.
///
/// `tag` is a short caller-supplied label (the file hash) that keeps the
/// temp files of concurrent restores apart.
pub fn restore_chunk(
    entry: &ChunkEntry,
    parity: u8,
    configs: &[SyncConfig],
    tag: &str,
) -> Result<Vec<u8>, String> {
    let by_id: HashMap<&str, &SyncConfig> = configs.iter().map(|c| (c.id.as_str(), c)).collect();
    let locs = entry_locs(entry);

    match entry_redundancy(entry, parity) {
        Redundancy::Replica { .. } => {
            let mut errors: Vec<String> = Vec::new();
            for loc in &locs {
                match download_artifact(loc, &by_id, tag) {
                    Ok(bytes) => match decode_artifact(bytes) {
                        Ok(plain) => {
                            if transfer::blake3_hex(&plain) == entry.hash {
                                return Ok(plain);
                            }
                            errors.push(format!("copy on '{}' failed hash check", loc.config_id));
                        }
                        Err(e) => errors.push(e),
                    },
                    Err(e) => errors.push(format!("copy on '{}': {}", loc.config_id, e)),
                }
            }
            Err(format!(
                "integrity: chunk {} could not be restored from any copy ({})",
                entry.index,
                errors.join("; ")
            ))
        }
        Redundancy::Erasure { data, parity: m } => {
            let need = data as usize;
            let mut present: Vec<(u8, Vec<u8>)> = Vec::with_capacity(need);
            let mut errors: Vec<String> = Vec::new();
            let mut next = 0usize;

            // The healthy path stops at `data` verified shards (exactly what
            // reconstruction needs); only a decode failure pulls the
            // remaining locators into the search.
            loop {
                while present.len() < need && next < locs.len() {
                    let index = next as u8;
                    let loc = locs[next];
                    next += 1;
                    match download_artifact(loc, &by_id, tag) {
                        Ok(bytes) => {
                            if transfer::blake3_hex(&bytes) == loc.artifact_hash {
                                present.push((index, bytes));
                            } else {
                                errors.push(format!(
                                    "shard {} on '{}' failed artifact check",
                                    index, loc.config_id
                                ));
                            }
                        }
                        Err(e) => {
                            errors.push(format!("shard {} on '{}': {}", index, loc.config_id, e))
                        }
                    }
                }

                if present.len() < need {
                    return Err(format!(
                        "integrity: chunk {} needs {} erasure shards, only {} are usable ({})",
                        entry.index,
                        need,
                        present.len(),
                        errors.join("; ")
                    ));
                }

                present.sort_by_key(|(i, _)| *i);
                let outcome = decode_shards(data, m, &present)
                    .map_err(|e| e.to_string())
                    .and_then(decode_artifact)
                    .and_then(|plain| {
                        if transfer::blake3_hex(&plain) == entry.hash {
                            Ok(plain)
                        } else {
                            Err(format!(
                                "integrity: chunk {} failed its plaintext hash check",
                                entry.index
                            ))
                        }
                    });

                match outcome {
                    Ok(plain) => return Ok(plain),
                    Err(e) if next < locs.len() => errors.push(e),
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

/// Reassemble a striped file: download each chunk (primary, then replicas),
/// verify every chunk against its plaintext hash, verify the assembled file
/// against `manifest.file_hash`, and only then publish `dest`.
///
/// Writes stream to `{dest}.restore.part` — a partial download never touches
/// the destination path, and any failure removes the sidecar.
pub fn restore(
    manifest: &ChunkManifest,
    configs: &[SyncConfig],
    dest: &str,
) -> Result<u64, String> {
    if manifest.version != MANIFEST_VERSION {
        return Err(format!(
            "unsupported: manifest version {} (this build speaks {})",
            manifest.version, MANIFEST_VERSION
        ));
    }

    let part = format!("{}.restore.part", dest);

    let outcome = (|| -> Result<u64, String> {
        let mut out =
            fs::File::create(&part).map_err(|e| format!("restore create failed: {}", e))?;
        let mut written: u64 = 0;

        for entry in &manifest.chunks {
            let plain = restore_chunk(entry, manifest.parity, configs, &manifest.file_hash)?;
            out.write_all(&plain)
                .map_err(|e| format!("restore write failed: {}", e))?;
            written += plain.len() as u64;
        }

        out.flush()
            .map_err(|e| format!("restore flush failed: {}", e))?;
        drop(out);

        if written != manifest.total_size {
            return Err(format!(
                "integrity: restored {} bytes, manifest says {}",
                written, manifest.total_size
            ));
        }
        let file_hash = transfer::hash_file(&part)?;
        if file_hash != manifest.file_hash {
            return Err("integrity: reassembled file does not match the manifest hash".to_string());
        }

        if let Some(parent) = Path::new(dest).parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("restore could not create '{}': {}", parent.display(), e)
                })?;
            }
        }
        fs::rename(&part, dest).map_err(|e| format!("restore publish failed: {}", e))?;
        Ok(written)
    })();

    if outcome.is_err() {
        let _ = fs::remove_file(&part);
    }
    outcome
}

// ===========================================================================
// Catalog replication + rebuild-from-remote (AGENT-7 item 4 / MISSING C4)
// ===========================================================================

/// Directory holding published catalogs (content-addressed objects plus one
/// stable pointer). Flat, so any backend's `list_files` can enumerate it.
pub const CATALOG_DIR: &str = "cybermanju_sync/catalog/";

/// Stable pointer every publish overwrites: the catalog of last resort when
/// a rebuild cannot enumerate the directory.
pub const CATALOG_POINTER: &str = "cybermanju_sync/catalog/latest.json";

/// Directory every chunk object (copy or shard) lives in — the GC sweep and
/// the rebuild's existence check both use it.
pub const CHUNKS_DIR: &str = "cybermanju_sync/chunks";

/// Catalog document format version.
pub const CATALOG_VERSION: u32 = 1;

/// The self-describing catalog: every `sync_files` row (which carries its
/// chunk manifest), written to providers so the pool survives losing the
/// local redb file — or the whole laptop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDoc {
    pub version: u32,
    pub written_at: String,
    /// Device id of the writer (feeds the version vectors in `lease.rs`).
    pub node: String,
    pub entries: Vec<SyncFile>,
}

impl CatalogDoc {
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("integrity: catalog encode failed: {}", e))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let doc: CatalogDoc = serde_json::from_slice(bytes)
            .map_err(|e| format!("integrity: catalog decode failed: {}", e))?;
        if doc.version > CATALOG_VERSION {
            return Err(format!(
                "unsupported: catalog version {} (this build speaks {})",
                doc.version, CATALOG_VERSION
            ));
        }
        Ok(doc)
    }

    /// Content hash of the serialized document — the catalog object's name.
    pub fn content_hash(&self) -> Result<String, String> {
        Ok(transfer::blake3_hex(&self.to_bytes()?))
    }

    /// Content-addressed object path for this document.
    pub fn object_path(&self) -> Result<String, String> {
        Ok(format!(
            "{}{}.json",
            CATALOG_DIR,
            self.content_hash()?.chars().take(64).collect::<String>()
        ))
    }
}

/// What [`publish_catalog`] actually did.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPublish {
    pub object_path: String,
    /// Providers the object landed on (should be ≥ 2 to survive a loss).
    pub providers: Vec<String>,
    pub entries: usize,
    pub bytes: usize,
    pub warnings: Vec<String>,
}

/// Write the catalog to **every enabled provider**: the content-addressed
/// object first, then the stable pointer.
///
/// Fewer than two providers is reported as a warning rather than silently
/// "success" — a one-provider catalog cannot survive losing that provider,
/// and the caller surfaces that.
pub fn publish_catalog(
    records: &[SyncFile],
    configs: &[SyncConfig],
    node: &str,
) -> Result<CatalogPublish, String> {
    let doc = CatalogDoc {
        version: CATALOG_VERSION,
        written_at: chrono::Utc::now().to_rfc3339(),
        node: node.to_string(),
        entries: records.to_vec(),
    };
    let bytes = doc.to_bytes()?;
    let object_path = doc.object_path()?;
    let pointer_bytes = object_path.clone().into_bytes();

    let mut providers = Vec::new();
    let mut warnings = Vec::new();
    let mut last_err: Option<String> = None;

    let tmp = std::env::temp_dir().join(format!(
        "cybermanju-catalog-{}-{}.json",
        std::process::id(),
        transfer::blake3_hex(&bytes)
            .chars()
            .take(12)
            .collect::<String>()
    ));
    fs::write(&tmp, &bytes).map_err(|e| format!("catalog write failed: {}", e))?;
    let tmp_str = tmp.to_string_lossy().to_string();

    let pointer_tmp = format!("{}.pointer", tmp_str);
    fs::write(&pointer_tmp, &pointer_bytes).map_err(|e| format!("catalog write failed: {}", e))?;

    for config in configs.iter().filter(|c| c.enabled) {
        match create_backend(config) {
            Ok(backend) => {
                match backend
                    .upload_file(&tmp_str, &object_path)
                    .and_then(|_| backend.upload_file(&pointer_tmp, CATALOG_POINTER))
                {
                    Ok(_) => providers.push(config.id.clone()),
                    Err(e) => {
                        last_err = Some(format!("{}: {}", config.id, e));
                        warnings.push(format!("catalog upload to '{}' failed: {}", config.id, e));
                    }
                }
            }
            Err(e) => warnings.push(format!("catalog upload to '{}' failed: {}", config.id, e)),
        }
    }
    let _ = fs::remove_file(&tmp);
    let _ = fs::remove_file(&pointer_tmp);

    if providers.is_empty() {
        return Err(last_err.unwrap_or_else(|| {
            "unsupported: no enabled provider accepted the catalog".to_string()
        }));
    }
    if providers.len() < 2 {
        warnings.push(format!(
            "catalog replica count is {}; two providers are required for a \
             catalog that survives provider loss",
            providers.len()
        ));
    }

    Ok(CatalogPublish {
        object_path,
        providers,
        entries: records.len(),
        bytes: bytes.len(),
        warnings,
    })
}

/// The catalog document as retrieved from a provider, and where it came from.
#[derive(Debug, Clone)]
pub struct CatalogFetch {
    pub doc: CatalogDoc,
    pub provider: String,
    pub bytes: usize,
    /// Providers that were tried and failed (each with its error).
    pub failures: Vec<String>,
}

/// Pull the newest catalog from the first provider that answers: the stable
/// pointer first, then any content-addressed object in the directory.
pub fn fetch_catalog(configs: &[SyncConfig]) -> Result<CatalogFetch, String> {
    let mut failures = Vec::new();
    let mut tmp_counter = 0u64;

    for config in configs.iter().filter(|c| c.enabled) {
        let backend = match create_backend(config) {
            Ok(backend) => backend,
            Err(e) => {
                failures.push(format!("{}: {}", config.id, e));
                continue;
            }
        };

        let mut candidates: Vec<String> = vec![CATALOG_POINTER.to_string()];
        match backend.list_files(CATALOG_DIR) {
            Ok(files) => {
                let mut paths: Vec<String> = files
                    .into_iter()
                    .map(|f| f.path)
                    .filter(|p| p.ends_with(".json"))
                    .collect();
                paths.sort();
                candidates.extend(paths);
            }
            Err(e) => failures.push(format!(
                "{}: could not list {}: {}",
                config.id, CATALOG_DIR, e
            )),
        }

        for remote in candidates {
            tmp_counter += 1;
            let tmp = std::env::temp_dir().join(format!(
                "cybermanju-catalog-fetch-{}-{}-{}.part",
                std::process::id(),
                config.id,
                tmp_counter
            ));
            let tmp_str = tmp.to_string_lossy().to_string();
            let attempt = backend
                .download_file(&remote, &tmp_str)
                .and_then(|()| fs::read(&tmp).map_err(|e| format!("read failed: {}", e)));
            let _ = fs::remove_file(&tmp);
            match attempt {
                Ok(bytes) => match CatalogDoc::from_bytes(&bytes) {
                    Ok(doc) => {
                        return Ok(CatalogFetch {
                            doc,
                            provider: config.id.clone(),
                            bytes: bytes.len(),
                            failures,
                        })
                    }
                    Err(e) => failures.push(format!("{}: {}: {}", config.id, remote, e)),
                },
                Err(e) => failures.push(format!("{}: {}: {}", config.id, remote, e)),
            }
        }
    }

    Err(format!(
        "unrecoverable: no catalog could be fetched from any provider ({})",
        if failures.is_empty() {
            "no enabled providers".to_string()
        } else {
            failures.join("; ")
        }
    ))
}

/// The catalog rebuilt from providers: documents fetched, manifests
/// reconciled against what each provider actually holds.
#[derive(Debug, Clone)]
pub struct CatalogRebuild {
    pub doc: CatalogDoc,
    /// Entries with their manifests reconstructed (and, where provably safe,
    /// locators reconciled against the providers' chunk listings).
    pub files: Vec<SyncFile>,
    pub providers: Vec<String>,
    /// Chunk objects seen across every provider's listing.
    pub chunks_seen: usize,
    /// Locator rows dropped because the object is provably absent.
    pub locators_dropped: usize,
    pub warnings: Vec<String>,
}

/// Rebuild the catalog from remote providers only — this is what makes the
/// pool survive **losing the local redb file**.
///
/// 1. fetch the catalog document from a provider (≥2 have a copy);
/// 2. list `cybermanju_sync/chunks/` on every provider;
/// 3. reconstruct each manifest "by hash and position": every entry keeps its
///    recorded chunk hashes and indexes, and replicated locators whose object
///    is provably gone are dropped (erasure locators are kept — losing one
///    shard must not silently change the chunk's `k`/`m`).
pub fn rebuild_from_remote(configs: &[SyncConfig]) -> Result<CatalogRebuild, String> {
    let fetch = fetch_catalog(configs)?;

    let mut listings: HashMap<String, (HashSet<String>, bool)> = HashMap::new();
    let mut warnings: Vec<String> = fetch.failures.clone();
    let mut chunks_seen = 0usize;
    let mut providers: Vec<String> = Vec::new();

    for config in configs.iter().filter(|c| c.enabled) {
        match create_backend(config) {
            Ok(backend) => match backend.list_files(CHUNKS_DIR) {
                Ok(files) => {
                    let set: HashSet<String> = files.into_iter().map(|f| f.path).collect();
                    chunks_seen += set.len();
                    providers.push(config.id.clone());
                    listings.insert(config.id.clone(), (set, true));
                }
                Err(e) => {
                    // Unknown listing = unknown existence: never drop a
                    // locator we cannot disprove.
                    warnings.push(format!(
                        "provider '{}' could not be listed ({}); its locators are kept as recorded",
                        config.id, e
                    ));
                    listings.insert(config.id.clone(), (HashSet::new(), false));
                }
            },
            Err(e) => {
                warnings.push(format!("provider '{}' is unusable: {}", config.id, e));
                listings.insert(config.id.clone(), (HashSet::new(), false));
            }
        }
    }

    let mut files = Vec::with_capacity(fetch.doc.entries.len());
    let mut locators_dropped = 0usize;

    for mut record in fetch.doc.entries.clone() {
        if let Some(json) = record.manifest_ref.clone() {
            match serde_json::from_str::<ChunkManifest>(&json) {
                Ok(mut manifest) => {
                    for entry in &mut manifest.chunks {
                        let erasure = entry_redundancy(entry, manifest.parity).is_erasure();
                        if erasure {
                            // Dropping a shard locator would change the
                            // derived layout; the restore path already
                            // tolerates an unreachable shard.
                            for loc in entry_locs(entry) {
                                if let Some((_, listed)) = listings.get(&loc.config_id) {
                                    if *listed
                                        && !listings[&loc.config_id].0.contains(&loc.remote_path)
                                    {
                                        warnings.push(format!(
                                            "chunk {}: shard on '{}' is missing at {} (kept)",
                                            entry.index, loc.config_id, loc.remote_path
                                        ));
                                    }
                                }
                            }
                            continue;
                        }

                        let mut locs = entry_locs(entry)
                            .into_iter()
                            .cloned()
                            .collect::<Vec<ChunkLoc>>();
                        let before = locs.len();
                        locs.retain(|loc| match listings.get(&loc.config_id) {
                            Some((set, true)) => set.contains(&loc.remote_path),
                            // Not listed → existence unknown → keep.
                            _ => true,
                        });
                        if locs.is_empty() {
                            warnings.push(format!(
                                "chunk {}: no surviving copy could be confirmed",
                                entry.index
                            ));
                            continue;
                        }
                        locators_dropped += before - locs.len();
                        entry.primary = locs[0].clone();
                        entry.replicas = locs[1..].to_vec();
                    }
                    record.manifest_ref = Some(
                        serde_json::to_string(&manifest)
                            .map_err(|e| format!("integrity: manifest re-encode failed: {}", e))?,
                    );
                }
                Err(e) => warnings.push(format!(
                    "file {}: stored manifest is unreadable ({}); kept verbatim",
                    record.id, e
                )),
            }
        }
        files.push(record);
    }

    Ok(CatalogRebuild {
        doc: fetch.doc,
        files,
        providers,
        chunks_seen,
        locators_dropped,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_round_robin_with_one_replica() {
        let n = 3;
        let parity = 1;
        assert_eq!(placements(0, n, parity), (0, vec![1]));
        assert_eq!(placements(1, n, parity), (1, vec![2]));
        assert_eq!(placements(2, n, parity), (2, vec![0]));
        assert_eq!(placements(3, n, parity), (0, vec![1]));
    }

    #[test]
    fn placements_clamp_replicas_to_n_minus_one() {
        // Asked for 5 copies on 3 providers — every provider gets one copy,
        // never two on the same one.
        let (primary, replicas) = placements(1, 3, 5);
        assert_eq!(primary, 1);
        assert_eq!(replicas, vec![2, 0]);
        // parity 0 → primary only
        assert_eq!(placements(4, 3, 0), (1, vec![]));
        // single participant → no replicas possible
        assert_eq!(placements(7, 1, 3), (0, vec![]));
    }

    #[test]
    fn chunk_paths_are_content_addressed() {
        let a = chunk_remote_path("abc123");
        let b = chunk_remote_path("abc123");
        assert_eq!(a, b);
        assert_eq!(a, "cybermanju_sync/chunks/abc123");
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = ChunkManifest {
            version: MANIFEST_VERSION,
            file_hash: "f".repeat(64),
            chunk_size: CHUNK_SIZE,
            total_size: CHUNK_SIZE + 10,
            chunks: vec![ChunkEntry {
                index: 0,
                hash: "a".repeat(64),
                size: CHUNK_SIZE,
                primary: ChunkLoc {
                    config_id: "cfg-1".to_string(),
                    remote_path: chunk_remote_path(&"a".repeat(64)),
                    artifact_hash: "b".repeat(64),
                },
                replicas: vec![ChunkLoc {
                    config_id: "cfg-2".to_string(),
                    remote_path: chunk_remote_path(&"a".repeat(64)),
                    artifact_hash: "b".repeat(64),
                }],
            }],
            parity: 1,
        };
        let json = serde_json::to_string(&manifest).expect("serialize");
        let back: ChunkManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(manifest, back);
        assert!(json.contains("artifactHash"));
    }

    #[test]
    fn decode_passes_plain_bytes_through() {
        let raw = b"not compressed, not sealed".to_vec();
        assert_eq!(decode_artifact(raw.clone()).unwrap(), raw);
    }
}
