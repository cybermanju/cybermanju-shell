// CyberManju OS — Keystore
//
// Passphrase-derived keys at rest: Argon2id key derivation + ChaCha20Poly1305
// AEAD. This module is the single source of truth for the machine-local
// master passphrase, the `KeyHandle` API AGENT-2 consumes, and the
// `seal`/`open_sealed` primitives used to wrap encryption private keys and
// stored OAuth credentials.
//
// <<< AGENT-3 KEYS AT REST >>>

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit as AeadKeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ─── Parameters ───────────────────────────────────────────────────────

/// OWASP Argon2id parameters — identical to the password-hash settings so
/// every derived key in the product costs the same to attack.
const ARGON2_M_COST: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;
const ARGON2_OUTPUT_LEN: usize = 32;

/// AEAD nonce length (ChaCha20-Poly1305, 96-bit).
const NONCE_LEN: usize = 12;
/// Random data-encryption-key length.
const KEY_LEN: usize = 32;
/// Salt length for Argon2id.
const SALT_LEN: usize = 16;

// ─── Master passphrase ────────────────────────────────────────────────

/// Resolve the directory used for persisted secrets.
///
/// Order: `CYBERMANJU_DATA_DIR` → parent of `DB_PATH` → platform data dir →
/// `~/.cybermanju`. `None` when nothing resolves (nothing can be persisted).
/// On Android `run()` exports `CYBERMANJU_DATA_DIR` from the probed
/// app-private files dir before any use (`HOME` is unset in app processes),
/// so the first branch answers there too instead of failing closed.
pub fn data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CYBERMANJU_DATA_DIR") {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    if let Ok(path) = std::env::var("DB_PATH") {
        if let Some(parent) = Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                return Some(parent.to_path_buf());
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(dir) = std::env::var("APPDATA") {
            return Some(PathBuf::from(dir).join("CyberManjuOS"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(PathBuf::from(home).join("Library/Application Support/CyberManjuOS"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                std::env::var("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from(&home).join(".local/share"))
                    .join("cybermanju-os"),
            );
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".cybermanju"))
    // P0-17: fail closed when HOME is unset. The old fallback
    // (`temp_dir()/cybermanju`) was a predictable, world-readable directory —
    // key material must never land there. Callers already handle `None`
    // (`master_passphrase` propagates it, `handle_path` errors, disk/os fall
    // back to an explicit relative dir).
}

/// Resolve the machine-local master passphrase.
///
/// Order: `CYBERMANJU_MASTER_PASSPHRASE` → `<data dir>/master.passphrase`
/// (generated on first use, 0600) → `None`.
pub fn master_passphrase() -> Option<String> {
    if let Ok(passphrase) = std::env::var("CYBERMANJU_MASTER_PASSPHRASE") {
        if !passphrase.trim().is_empty() {
            return Some(passphrase);
        }
    }
    let dir = data_dir()?;
    let path = dir.join("master.passphrase");
    load_or_create_master_passphrase(&path)
}

fn load_or_create_master_passphrase(path: &Path) -> Option<String> {
    // Only a genuinely absent path permits first-use creation. Empty,
    // unreadable, invalid-UTF8 or symlinked secrets must never be replaced.
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => {
            return read_secret_file(path).filter(|secret| !secret.is_empty());
        }
        Ok(_) => return None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
    }
    let mut raw = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut raw);
    let encoded = BASE64.encode(raw);
    match write_secret_file(path, &encoded) {
        Ok(()) => {
            log::info!("master passphrase created at {}", path.display());
            Some(encoded)
        }
        // Another creator won the race. Read its secret, never overwrite it.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            read_secret_file(path).filter(|secret| !secret.is_empty())
        }
        Err(_) => None,
    }
}

/// Fail-closed permission gate: a secret file readable by group/other is
/// refused (fail-closed) instead of silently trusted. Non-unix platforms
/// skip the check (no mode bits to inspect).
fn secret_file_mode_ok(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(meta) if meta.permissions().mode() & 0o077 == 0 => true,
            Ok(_) => {
                log::warn!(
                    "refusing world/group-readable secret file {} (fix with chmod 600)",
                    path.display()
                );
                false
            }
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

fn read_secret_file(path: &Path) -> Option<String> {
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    if !secret_file_mode_ok(path) {
        return None;
    }
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.chars().filter(|c| !c.is_whitespace()).collect::<String>())
}

fn write_secret_file(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

// ─── KeyHandle API ────────────────────────────────────────────────────

/// An opaque handle to a 32-byte key stored at rest.
///
/// The key itself never appears in this struct: it is a randomly generated
/// data key wrapped by an Argon2id-derived key-encryption key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyHandle {
    /// Caller-chosen identifier (e.g. `"master"` or a file-key id).
    pub id: String,
    /// base64 Argon2id salt for the key-encryption key.
    pub salt: String,
    /// base64 AEAD nonce used to wrap `ciphertext`.
    pub nonce: String,
    /// base64 wrapped 32-byte data key.
    pub ciphertext: String,
}

static HANDLE_STORE_LOCK: Mutex<()> = Mutex::new(());

/// Get the handle for `id`, deriving and persisting one on first use.
///
/// The handle is only accepted back when `passphrase` still opens it. A
/// changed passphrase no longer re-wraps silently (P0-17): the old handle is
/// kept and the call fails with `auth:` — loudly, because re-wrapping under a
/// new passphrase orphans everything sealed under the old one. No automatic
/// passphrase-rotation procedure is provided.
pub fn get_or_derive(id: &str, passphrase: &str) -> Result<KeyHandle, String> {
    let _guard = HANDLE_STORE_LOCK
        .lock()
        .map_err(|_| "integrity: keystore update lock is poisoned".to_string())?;
    if id.trim().is_empty() {
        return Err("keystore id is required".to_string());
    }
    if let Some(handle) = load_handle(id)? {
        if open(&handle, passphrase).is_ok() {
            return Ok(handle);
        }
        log::error!(
            "keystore entry '{}' did not open with the current passphrase; refusing re-wrap (old handle kept)",
            id
        );
        return Err(format!(
            "auth: keystore entry '{id}' does not open with this passphrase — \
             supply the original passphrase (old key kept, nothing overwritten; no automatic rotation is available)"
        ));
    }

    // A new random key is safe only when the keystore was read successfully
    // and this id is genuinely absent. Corrupt/unreadable storage fails above.
    let mut salt = [0u8; SALT_LEN];
    rand_core::OsRng.fill_bytes(&mut salt);
    let kek = derive_key(passphrase, &salt)?;

    let mut data_key = [0u8; KEY_LEN];
    rand_core::OsRng.fill_bytes(&mut data_key);

    let (nonce, ciphertext) = seal_with_key(&kek, &data_key)?;
    let handle = KeyHandle {
        id: id.to_string(),
        salt: BASE64.encode(salt),
        nonce: BASE64.encode(nonce),
        ciphertext: BASE64.encode(ciphertext),
    };
    save_handle(&handle)?;
    Ok(handle)
}

/// Open a handle and return the raw 32-byte key it wraps.
pub fn open(handle: &KeyHandle, passphrase: &str) -> Result<[u8; 32], String> {
    let salt = decode(&handle.salt, "salt")?;
    let nonce = decode(&handle.nonce, "nonce")?;
    let ciphertext = decode(&handle.ciphertext, "ciphertext")?;

    let kek = derive_key(passphrase, &salt)?;
    let plaintext = open_with_key(&kek, &nonce, &ciphertext)?;
    <[u8; 32]>::try_from(plaintext.as_slice())
        .map_err(|_| "keystore payload is not a 32-byte key".to_string())
}

/// Forget a handle.
pub fn remove(id: &str) -> Result<(), String> {
    let _guard = HANDLE_STORE_LOCK
        .lock()
        .map_err(|_| "integrity: keystore update lock is poisoned".to_string())?;
    let path = handle_path()?;
    let mut handles = load_handles()?;
    if handles.remove(id).is_some() {
        write_handles(&path, &handles)?;
    }
    Ok(())
}

// ─── AEAD helpers ─────────────────────────────────────────────────────

/// Encrypt `plaintext` under `passphrase`.
///
/// Returns `nonce || ciphertext` (12 bytes prepended).
pub fn seal(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; SALT_LEN];
    rand_core::OsRng.fill_bytes(&mut salt);
    let key = derive_key(passphrase, &salt)?;
    let (nonce, ciphertext) = seal_with_key(&key, plaintext)?;
    let mut out = Vec::with_capacity(SALT_LEN + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a blob produced by [`seal`].
pub fn open_sealed(passphrase: &str, blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < SALT_LEN + NONCE_LEN + 16 {
        return Err("sealed blob is too short".to_string());
    }
    let (salt, rest) = blob.split_at(SALT_LEN);
    let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
    let key = cached_kek(passphrase, salt)?;
    open_with_key(&key, nonce, ciphertext)
}

/// Bounded KEK cache (P0-16): bulk restores open many blobs sealed under one
/// passphrase, and a fresh Argon2id per blob turns that into a CPU DoS.
/// Entries key by `(blake3(passphrase), salt)` — the passphrase itself never
/// rests — capped at 8 with oldest-first eviction, process-local only.
/// One KEK cache row: `(blake3(passphrase), salt, derived_key)`.
type KekCacheEntry = ([u8; 32], Vec<u8>, [u8; 32]);

fn cached_kek(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    static CACHE: OnceLock<Mutex<Vec<KekCacheEntry>>> = OnceLock::new();
    let tag = *blake3::hash(passphrase.as_bytes()).as_bytes();
    if let Ok(cache) = CACHE.get_or_init(|| Mutex::new(Vec::new())).lock() {
        if let Some((_, _, key)) = cache.iter().find(|(t, s, _)| *t == tag && *s == salt) {
            return Ok(*key);
        }
    }
    let key = derive_key(passphrase, salt)?;
    if let Ok(mut cache) = CACHE.get_or_init(|| Mutex::new(Vec::new())).lock() {
        if cache.len() >= 8 {
            cache.remove(0);
        }
        cache.push((tag, salt.to_vec(), key));
    }
    Ok(key)
}

/// [`seal`] with a base64 result — convenient for JSON columns.
pub fn seal_str(passphrase: &str, plaintext: &[u8]) -> Result<String, String> {
    seal(passphrase, plaintext).map(|bytes| BASE64.encode(bytes))
}

/// [`open_sealed`] for a base64 blob produced by [`seal_str`].
pub fn open_sealed_str(passphrase: &str, encoded: &str) -> Result<String, String> {
    let blob = decode(encoded, "sealed blob")?;
    let plain = open_sealed(passphrase, &blob)?;
    String::from_utf8(plain).map_err(|_| "sealed blob is not UTF-8".to_string())
}

fn seal_with_key(key: &[u8], plaintext: &[u8]) -> Result<([u8; NONCE_LEN], Vec<u8>), String> {
    let cipher = cipher(key)?;
    let mut nonce = [0u8; NONCE_LEN];
    rand_core::OsRng.fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|_| "encryption failed".to_string())?;
    Ok((nonce, ciphertext))
}

fn open_with_key(key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != NONCE_LEN {
        return Err("invalid nonce length".to_string());
    }
    let cipher = cipher(key)?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| "decryption failed: wrong passphrase or corrupted data".to_string())
}

fn cipher(key: &[u8]) -> Result<ChaCha20Poly1305, String> {
    if key.len() != KEY_LEN {
        return Err("invalid key length".to_string());
    }
    ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| format!("invalid ChaCha20Poly1305 key: {}", e))
}

/// Argon2id key derivation with pinned parameters.
pub fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(ARGON2_OUTPUT_LEN),
    )
    .map_err(|e| format!("Invalid Argon2 parameters: {}", e))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; ARGON2_OUTPUT_LEN];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| format!("Argon2 derivation failed: {}", e))?;
    Ok(out)
}

// ─── Handle persistence ───────────────────────────────────────────────

fn handle_path() -> Result<PathBuf, String> {
    data_dir()
        .map(|dir| dir.join("keystore.json"))
        .ok_or_else(|| "No data directory available for the keystore".to_string())
}

fn load_handles() -> Result<std::collections::HashMap<String, KeyHandle>, String> {
    let path = handle_path()?;
    read_handles(&path)
}
fn read_handles(path: &Path) -> Result<std::collections::HashMap<String, KeyHandle>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Default::default());
        }
        Err(err) => {
            return Err(format!(
                "Could not read keystore {}: {}",
                path.display(),
                err
            ));
        }
    };
    serde_json::from_str(&raw).map_err(|err| {
        format!(
            "integrity: keystore {} is invalid JSON: {}",
            path.display(),
            err
        )
    })
}
fn load_handle(id: &str) -> Result<Option<KeyHandle>, String> {
    Ok(load_handles()?.remove(id))
}
fn save_handle(handle: &KeyHandle) -> Result<(), String> {
    let path = handle_path()?;
    let mut handles = read_handles(&path)?;
    handles.insert(handle.id.clone(), handle.clone());
    write_handles(&path, &handles)
}

fn write_handles(
    path: &Path,
    handles: &std::collections::HashMap<String, KeyHandle>,
) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let encoded = serde_json::to_vec(handles).map_err(|e| e.to_string())?;
    let staged = parent.join(format!(".keystore-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<(), String> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&staged).map_err(|e| e.to_string())?;
        file.write_all(&encoded)
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("Could not stage keystore update: {e}"))?;
        drop(file);
        // The previous store is intact if staging or rename fails: never
        // truncate the only copy while adding/removing a recovery key.
        std::fs::rename(&staged, path)
            .map_err(|e| format!("Could not install keystore update: {e}"))?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| format!("Could not flush keystore directory: {e}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

fn decode(value: &str, what: &str) -> Result<Vec<u8>, String> {
    BASE64
        .decode(value)
        .map_err(|_| format!("keystore {} is not valid base64", what))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn master_secret_creation_never_replaces_existing_or_empty_files() {
        let dir = std::env::temp_dir().join(format!(
            "cybermanju-master-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("master.passphrase");
        let original = load_or_create_master_passphrase(&path).unwrap();
        assert_eq!(load_or_create_master_passphrase(&path).unwrap(), original);
        assert_eq!(
            write_secret_file(&path, "replacement").unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        std::fs::write(&path, b"").unwrap();
        assert!(load_or_create_master_passphrase(&path).is_none());
        assert!(std::fs::read(&path).unwrap().is_empty());
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(load_or_create_master_passphrase(&path).is_none());
        assert_eq!(std::fs::read(&path).unwrap(), [0xff, 0xfe]);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn missing_keystore_is_empty_but_corrupt_keystore_fails_closed() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "cybermanju-keystore-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keystore.json");

        assert!(read_handles(&path).unwrap().is_empty());
        std::fs::write(&path, b"not-json").unwrap();
        let err = read_handles(&path).unwrap_err();
        assert!(
            err.starts_with("integrity: keystore"),
            "unexpected error: {err}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn seal_round_trip() {
        let blob = seal("hunter2", b"private key bytes").unwrap();
        // The plaintext must not survive in the clear.
        assert!(!blob.windows(16).any(|w| w == b"private key byte"));
        let back = open_sealed("hunter2", &blob).unwrap();
        assert_eq!(back, b"private key bytes");
    }

    #[test]
    fn seal_rejects_wrong_passphrase() {
        let blob = seal("hunter2", b"secret").unwrap();
        assert!(open_sealed("wrong", &blob).is_err());
    }

    #[test]
    fn seal_str_round_trip() {
        let encoded = seal_str("pw", b"{\"a\":1}").unwrap();
        let back = open_sealed_str("pw", &encoded).unwrap();
        assert_eq!(back, "{\"a\":1}");
    }

    #[test]
    fn derive_key_is_deterministic() {
        let a = derive_key("pw", b"0123456789abcdef").unwrap();
        let b = derive_key("pw", b"0123456789abcdef").unwrap();
        assert_eq!(a, b);
        let c = derive_key("pw", b"fedcba9876543210").unwrap();
        assert_ne!(a, c);
    }
}
