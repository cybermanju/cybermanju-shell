// CyberManju OS — Native crypto IPC regressions (src-tauri/src/native_crypto_tests.rs).
// Role: exercise the production Android/desktop command registry with real bytes.
// Exports: tests only; no test commands are added to shipping builds.
// Integrates: Tauri MockRuntime, redb, shared PQC/AEAD and filesystem sidecars.
// Contracts: host IPC tests, NOT Android device/WebView tests; no crypto mocks.

use crate::{native_command_handler, AppState};
use cybermanju_crypto::pqc::EncryptedFileMeta;
use cybermanju_db::Database;
use cybermanju_types::schema::FileNode;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::Manager;

const CHILD_DIR: &str = "CYBERMANJU_NATIVE_CRYPTO_TEST_DIR";

// Use a child process rather than mutating process-global environment variables
// while other workspace tests may be resolving their own master passphrase.
fn isolated(name: &str, test: impl FnOnce(&Path)) {
    if let Some(dir) = std::env::var_os(CHILD_DIR) {
        test(Path::new(&dir));
        std::fs::write(Path::new(&dir).join("completed"), name).unwrap();
        return;
    }
    let dir = std::env::temp_dir().join(format!("native-crypto-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD_DIR, &dir)
        .env("CYBERMANJU_DATA_DIR", &dir)
        .env_remove("CYBERMANJU_MASTER_PASSPHRASE")
        .status();
    let completed = std::fs::read_to_string(dir.join("completed"));
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        result.unwrap().success(),
        "native crypto child failed: {name}"
    );
    assert_eq!(completed.unwrap(), name, "child test did not execute");
}

struct Fixture {
    // Drop native state (and its DB/index handles) before the parent removes files.
    app: tauri::App<MockRuntime>,
    window: tauri::WebviewWindow<MockRuntime>,
    path: PathBuf,
    cipher: PathBuf,
    meta: PathBuf,
    plaintext: Vec<u8>,
    original: FileNode,
}

impl Fixture {
    fn new(root: &Path, name: &str) -> Self {
        let root = root.join(name);
        std::fs::create_dir(&root).unwrap();
        let path = root.join("binary payload.bin");
        // Every byte value, embedded NULs and invalid UTF-8 over multiple blocks.
        let plaintext: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
        assert!(std::str::from_utf8(&plaintext).is_err());
        std::fs::write(&path, &plaintext).unwrap();
        let original: FileNode = serde_json::from_value(json!({
            "id": "binary-file", "name": "binary payload.bin", "fileType": "file",
            "parentId": null, "sizeBytes": plaintext.len(),
            "mimeType": "application/octet-stream",
            "hashBlake3": blake3::hash(&plaintext).to_hex().to_string(),
            "encrypted": false, "encryptionAlgorithm": null, "compressionLayers": [],
            "thumbnailPath": null,
            "contextData": {"original_path": path.to_str().unwrap(), "note": "preserve me"},
            "tags": ["binary", "native"], "collectionIds": ["collection-1"],
            "faceGroupIds": [], "looseGroupIds": [], "gpsLat": null, "gpsLon": null,
            "createdAt": "2026-01-01T00:00:00Z", "modifiedAt": "2026-01-01T00:00:00Z"
        }))
        .unwrap();
        let db = Database::new(root.join("db.redb").to_str().unwrap()).unwrap();
        let tx = db.begin_write().unwrap();
        {
            let mut table = tx.open_table(Database::get_files_table()).unwrap();
            let row = serde_json::to_string(&original).unwrap();
            table.insert(original.id.as_str(), row.as_str()).unwrap();
        }
        tx.commit().unwrap();
        let state = AppState {
            db: Arc::new(RwLock::new(db)),
            tantivy_index: Arc::new(RwLock::new(
                crate::search::SearchIndex::new(root.join("index").to_str().unwrap()).unwrap(),
            )),
            compression: crate::compression::TripleCompressor::new(),
            hmac_secret: [0; 32],
        };
        let app = mock_builder()
            .manage(state)
            .invoke_handler(native_command_handler())
            .build(mock_context(noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self {
            app,
            window,
            cipher: PathBuf::from(format!("{}.enc", path.display())),
            meta: PathBuf::from(format!("{}.enc.meta.json", path.display())),
            path,
            plaintext,
            original,
        }
    }

    fn invoke(&self, command: &str, body: Value) -> Result<Value, Value> {
        tauri::test::get_ipc_response(
            &self.window,
            tauri::webview::InvokeRequest {
                cmd: command.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: tauri::ipc::InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.to_string(),
            },
        )
        .map(|response| response.deserialize().unwrap())
    }

    fn ok(&self, command: &str, body: Value) -> Value {
        self.invoke(command, body)
            .unwrap_or_else(|error| panic!("{command}: {error}"))
    }

    fn row(&self) -> FileNode {
        serde_json::from_value(self.ok("get_file", json!({"fileId": self.original.id}))).unwrap()
    }

    fn assert_state(&self, encrypted: bool, algorithm: Option<&str>) {
        let row = self.row();
        assert_eq!(row.encrypted, encrypted);
        assert_eq!(row.encryption_algorithm.as_deref(), algorithm);
        chrono::DateTime::parse_from_rfc3339(&row.modified_at).unwrap();
        let mut unchanged = row;
        unchanged.encrypted = self.original.encrypted;
        unchanged.encryption_algorithm = self.original.encryption_algorithm.clone();
        unchanged.modified_at = self.original.modified_at.clone();
        assert_eq!(unchanged, self.original, "unrelated file metadata changed");
        let status = self.ok("get_encryption_status", json!({}));
        assert_eq!(status["isEncrypted"], encrypted);
    }

    fn key(&self, algorithm: &str) -> Value {
        let key = self.ok("generate_keypair", json!({"algorithm": algorithm}));
        assert!(key["privateKey"]
            .as_str()
            .unwrap()
            .starts_with("sealed:v1:"));
        // Check persistence, not just the returned object; material stays sealed.
        let state = self.app.state::<AppState>();
        let db = state.db.read().unwrap();
        let tx = db.begin_read().unwrap();
        let table = tx
            .open_table(Database::get_encryption_keys_table())
            .unwrap();
        let stored = table.get(key["id"].as_str().unwrap()).unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(stored.value()).unwrap(), key);
        key
    }

    fn encrypt(&self, algorithm: &str) -> Value {
        self.ok(
            "encrypt_file",
            json!({"fileId": self.original.id, "algorithm": algorithm}),
        )
    }

    fn decrypt(&self) -> Value {
        self.ok("decrypt_file", json!({"fileId": self.original.id}))
    }
}

#[test]
fn native_crypto_ipc_binary_roundtrip() {
    let name = "native_crypto_tests::native_crypto_ipc_binary_roundtrip";
    isolated(name, |root| {
        for algorithm in ["kyber1024", "hybrid", "aes256"] {
            let f = Fixture::new(root, algorithm);
            let key = f.key(algorithm);
            f.assert_state(false, None);
            let response = f.encrypt(algorithm);
            assert_eq!(response["fileId"], f.original.id);
            assert_eq!(response["encrypted"], true);
            chrono::DateTime::parse_from_rfc3339(response["updatedAt"].as_str().unwrap()).unwrap();
            assert!(
                !f.path.exists(),
                "successful encryption left plaintext behind"
            );
            let ciphertext = std::fs::read(&f.cipher).unwrap();
            assert!(!ciphertext.is_empty());
            assert_ne!(
                ciphertext, f.plaintext,
                "metadata-only encryption is not success"
            );
            let metadata_bytes = std::fs::read(&f.meta).unwrap();
            let metadata: EncryptedFileMeta = serde_json::from_slice(&metadata_bytes).unwrap();
            assert_eq!(metadata.key_id, key["id"]);
            assert_eq!(metadata.algorithm, response["algorithm"]);
            assert_eq!(
                metadata.blake3_original,
                f.original.hash_blake3.clone().unwrap()
            );
            chrono::DateTime::parse_from_rfc3339(&metadata.encrypted_at).unwrap();
            f.assert_state(true, Some(&metadata.algorithm));
            assert_eq!(f.ok("get_encryption_status", json!({}))["keyId"], key["id"]);

            // Repeated encryption must not encrypt ciphertext again or lose sidecars.
            f.encrypt(algorithm);
            assert_eq!(std::fs::read(&f.cipher).unwrap(), ciphertext);
            assert_eq!(std::fs::read(&f.meta).unwrap(), metadata_bytes);
            let response = f.decrypt();
            assert_eq!(response["fileId"], f.original.id);
            assert_eq!(response["encrypted"], false);
            assert!(response["algorithm"].is_null());
            chrono::DateTime::parse_from_rfc3339(response["updatedAt"].as_str().unwrap()).unwrap();
            assert_eq!(std::fs::read(&f.path).unwrap(), f.plaintext);
            assert!(!f.cipher.exists());
            assert!(!f.meta.exists());
            f.assert_state(false, None);
            f.decrypt();
            assert_eq!(std::fs::read(&f.path).unwrap(), f.plaintext);
            f.assert_state(false, None);
        }
    });
}

#[test]
fn native_crypto_ipc_rejects_tampering_without_changing_state() {
    isolated(
        "native_crypto_tests::native_crypto_ipc_rejects_tampering_without_changing_state",
        |root| {
            let f = Fixture::new(root, "tamper");
            f.key("aes256");
            f.encrypt("aes256");
            let encrypted_row = f.row();
            let ciphertext = std::fs::read(&f.cipher).unwrap();
            let metadata = std::fs::read(&f.meta).unwrap();
            let mut corrupt = ciphertext.clone();
            corrupt[0] ^= 0x80;
            std::fs::write(&f.cipher, &corrupt).unwrap();
            let error = f
                .invoke("decrypt_file", json!({"fileId": f.original.id}))
                .unwrap_err();
            assert!(error.as_str().unwrap().starts_with("integrity:"), "{error}");
            assert!(!f.path.exists());
            assert_eq!(f.row(), encrypted_row);
            assert_eq!(std::fs::read(&f.cipher).unwrap(), corrupt);
            assert_eq!(std::fs::read(&f.meta).unwrap(), metadata);
            f.assert_state(true, encrypted_row.encryption_algorithm.as_deref());
            // Authentic bytes still recover after the refused attempt.
            std::fs::write(&f.cipher, &ciphertext).unwrap();
            f.decrypt();
            assert_eq!(std::fs::read(&f.path).unwrap(), f.plaintext);
            f.assert_state(false, None);
        },
    );
}

#[test]
fn native_crypto_ipc_failures_do_not_mark_plaintext_encrypted() {
    isolated(
        "native_crypto_tests::native_crypto_ipc_failures_do_not_mark_plaintext_encrypted",
        |root| {
            let f = Fixture::new(root, "errors");
            for (algorithm, prefix) in [
                ("not-an-algorithm", "unsupported:"),
                ("aes256", "not_found:"),
            ] {
                let error = f
                    .invoke(
                        "encrypt_file",
                        json!({"fileId": f.original.id, "algorithm": algorithm}),
                    )
                    .unwrap_err();
                assert!(error.as_str().unwrap().starts_with(prefix), "{error}");
                assert_eq!(f.row(), f.original);
                assert_eq!(std::fs::read(&f.path).unwrap(), f.plaintext);
                assert!(!f.cipher.exists());
                assert!(!f.meta.exists());
                f.assert_state(false, None);
            }
        },
    );
}
