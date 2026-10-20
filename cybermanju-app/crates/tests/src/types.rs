// CyberManju OS — Types (crates/tests/src/types.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: cybermanju_types.
// Contracts: signals: engine.
use cybermanju_types::schema::*;
use cybermanju_types::sync::*;

#[test]
fn test_file_node_camel_case_serde() {
    let node = FileNode {
        id: "f1".into(),
        name: "test.txt".into(),
        file_type: "file".into(),
        parent_id: Some("root".into()),
        size_bytes: 1024,
        mime_type: Some("text/plain".into()),
        hash_blake3: Some("abc123".into()),
        encrypted: true,
        encryption_algorithm: Some("ML-KEM-1024".into()),
        compression_layers: vec!["lz4".into(), "zstd".into()],
        thumbnail_path: None,
        context_data: Some(serde_json::json!({"lines": 42})),
        tags: vec!["important".into()],
        collection_ids: vec!["c1".into()],
        face_group_ids: vec![],
        loose_group_ids: vec![],
        gps_lat: Some(40.7128),
        gps_lon: Some(-74.0060),
        created_at: "2026-01-01T00:00:00Z".into(),
        modified_at: "2026-06-13T12:00:00Z".into(),
    };
    let json = serde_json::to_string(&node).unwrap();
    assert!(json.contains("\"fileType\""));
    assert!(json.contains("\"sizeBytes\""));
    assert!(json.contains("\"hashBlake3\""));
    assert!(json.contains("\"gpsLat\""));
    let back: FileNode = serde_json::from_str(&json).unwrap();
    assert_eq!(node, back);
}

#[test]
fn test_file_node_defaults() {
    let node = FileNode {
        id: "f1".into(),
        name: "x".into(),
        file_type: "file".into(),
        parent_id: None,
        size_bytes: 0,
        mime_type: None,
        hash_blake3: None,
        encrypted: false,
        encryption_algorithm: None,
        compression_layers: vec![],
        thumbnail_path: None,
        context_data: None,
        tags: vec![],
        collection_ids: vec![],
        face_group_ids: vec![],
        loose_group_ids: vec![],
        gps_lat: None,
        gps_lon: None,
        created_at: "2026-01-01".into(),
        modified_at: "2026-01-01".into(),
    };
    assert!(node.tags.is_empty());
    assert!(node.compression_layers.is_empty());
    assert!(!node.encrypted);
    assert!(node.gps_lat.is_none());
}

#[test]
fn test_account_serde() {
    let a = Account {
        id: "a1".into(),
        name: "Local Drive".into(),
        account_type: "local".into(),
        path: Some("/home/user".into()),
        color: "#FF0000".into(),
        is_active: true,
        created_at: "2026-01-01".into(),
        updated_at: "2026-06-01".into(),
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"accountType\""));
    assert!(json.contains("\"isActive\""));
    let back: Account = serde_json::from_str(&json).unwrap();
    assert_eq!(a, back);
}

#[test]
fn test_collection_serde() {
    let c = Collection {
        id: "c1".into(),
        name: "Best Shots".into(),
        collection_type: "highlights".into(),
        color: "#00FF00".into(),
        description: Some("Best photos".into()),
        item_ids: vec!["f1".into(), "f2".into()],
        created_at: "2026-01-01".into(),
        updated_at: "2026-06-01".into(),
    };
    let json = serde_json::to_string(&c).unwrap();
    assert!(json.contains("\"collectionType\""));
    assert!(json.contains("\"itemIds\""));
    let back: Collection = serde_json::from_str(&json).unwrap();
    assert_eq!(c, back);
}

#[test]
fn test_collection_item_serde() {
    let ci = CollectionItem {
        id: "ci1".into(),
        collection_id: "c1".into(),
        file_id: "f1".into(),
        note: Some("Great shot".into()),
        added_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&ci).unwrap();
    assert!(json.contains("\"collectionId\""));
    assert!(json.contains("\"fileId\""));
    let back: CollectionItem = serde_json::from_str(&json).unwrap();
    assert_eq!(ci, back);
}

#[test]
fn test_face_group_serde() {
    let fg = FaceGroup {
        id: "fg1".into(),
        name: "Alice".into(),
        file_ids: vec!["f1".into(), "f2".into()],
        centroid_embedding: Some(vec![0.1, 0.2, 0.3]),
        binary_hash: Some(0xABCD),
        cohesion: Some(0.95),
        embedding_count: 10,
        algorithm: Some("chinese_whispers".into()),
        detection_engine: Some("heuristic-v2".into()),
        created_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&fg).unwrap();
    assert!(json.contains("\"centroidEmbedding\""));
    assert!(json.contains("\"binaryHash\""));
    assert!(json.contains("\"embeddingCount\""));
    assert!(json.contains("\"detectionEngine\""));
    let back: FaceGroup = serde_json::from_str(&json).unwrap();
    assert_eq!(fg, back);
    // Pre-engine rows (no `detectionEngine` key) still read as `None`.
    let legacy = r#"{"id":"fg0","name":"Old","fileIds":[],"centroidEmbedding":null,"binaryHash":null,"cohesion":null,"embeddingCount":0,"algorithm":null,"createdAt":"2026-06-13"}"#;
    let old: FaceGroup = serde_json::from_str(legacy).unwrap();
    assert_eq!(old.detection_engine, None);
}

#[test]
fn test_encryption_key_serde() {
    let k = EncryptionKey {
        id: "k1".into(),
        algorithm: "ml-kem-1024".into(),
        public_key: "pk_bytes".into(),
        private_key: "sk_bytes".into(),
        label: Some("Master Key".into()),
        created_at: "2026-06-13".into(),
        x25519_public_key: None,
        x25519_private_key: None,
    };
    let json = serde_json::to_string(&k).unwrap();
    assert!(json.contains("\"publicKey\""));
    assert!(json.contains("\"privateKey\""));
    let back: EncryptionKey = serde_json::from_str(&json).unwrap();
    assert_eq!(k, back);
    // Pre-P0-13 rows (no x25519 keys) still read as `None`.
    let legacy = r#"{"id":"k0","algorithm":"hybrid","publicKey":"p","privateKey":"s","label":null,"createdAt":"2026-06-13"}"#;
    let old: EncryptionKey = serde_json::from_str(legacy).unwrap();
    assert_eq!(old.x25519_public_key, None);
    assert_eq!(old.x25519_private_key, None);
}

#[test]
fn test_loose_group_serde() {
    let lg = LooseGroup {
        id: "lg1".into(),
        name: "Quick Select".into(),
        color: "#FF00FF".into(),
        file_ids: vec!["f1".into()],
        created_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&lg).unwrap();
    assert!(json.contains("\"fileIds\""));
    let back: LooseGroup = serde_json::from_str(&json).unwrap();
    assert_eq!(lg, back);
}

#[test]
fn test_user_serde() {
    let u = User {
        id: "u1".into(),
        username: "admin".into(),
        password_hash: "argon2$hash".into(),
        display_name: Some("Admin User".into()),
        role: "admin".into(),
        is_active: true,
        created_at: "2026-01-01".into(),
        updated_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&u).unwrap();
    assert!(json.contains("\"passwordHash\""));
    assert!(json.contains("\"displayName\""));
    assert!(json.contains("\"isActive\""));
    let back: User = serde_json::from_str(&json).unwrap();
    assert_eq!(u, back);
}

#[test]
fn test_user_roles() {
    for role in &["admin", "user", "viewer"] {
        let u = User {
            id: "u1".into(),
            username: "test".into(),
            password_hash: "hash".into(),
            display_name: None,
            role: role.to_string(),
            is_active: true,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
        };
        assert_eq!(u.role, *role);
    }
}

#[test]
fn test_user_file_permission_serde() {
    let p = UserFilePermission {
        id: "p1".into(),
        user_id: "u1".into(),
        file_id: "f1".into(),
        access: "write".into(),
        granted_by: "admin".into(),
        granted_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"userId\""));
    assert!(json.contains("\"fileId\""));
    assert!(json.contains("\"grantedBy\""));
    let back: UserFilePermission = serde_json::from_str(&json).unwrap();
    assert_eq!(p, back);
}

#[test]
fn test_location_serde() {
    let loc = Location {
        id: "l1".into(),
        file_id: "f1".into(),
        latitude: 40.7128,
        longitude: -74.0060,
        altitude: Some(10.5),
        place_name: Some("New York City".into()),
        created_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&loc).unwrap();
    assert!(json.contains("\"latitude\""));
    assert!(json.contains("\"longitude\""));
    assert!(json.contains("\"altitude\""));
    assert!(json.contains("\"placeName\""));
    let back: Location = serde_json::from_str(&json).unwrap();
    assert_eq!(loc, back);
}

// ─── Sync types tests ──────────────────────────────────────────────

#[test]
fn test_sync_backend_type_serde_roundtrip() {
    for bt in [
        SyncBackendType::Local,
        SyncBackendType::GitHub,
        SyncBackendType::GitLab,
        SyncBackendType::GoogleDrive,
    ] {
        let json = serde_json::to_string(&bt).unwrap();
        let back: SyncBackendType = serde_json::from_str(&json).unwrap();
        assert_eq!(bt, back);
    }
}

#[test]
fn test_sync_backend_type_display() {
    assert_eq!(SyncBackendType::Local.to_string(), "local");
    assert_eq!(SyncBackendType::GitHub.to_string(), "github");
    assert_eq!(SyncBackendType::GitLab.to_string(), "gitlab");
    assert_eq!(SyncBackendType::GoogleDrive.to_string(), "googleDrive");
}

#[test]
fn test_sync_status_serde_roundtrip() {
    for st in [
        SyncStatus::Idle,
        SyncStatus::Scanning,
        SyncStatus::Compressing,
        SyncStatus::Uploading,
        SyncStatus::Linking,
        SyncStatus::Cleaning,
        SyncStatus::Error,
        SyncStatus::Completed,
        SyncStatus::Cancelled,
    ] {
        let json = serde_json::to_string(&st).unwrap();
        let back: SyncStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(st, back);
    }
    // Legacy payloads written before the enum collapsed still parse.
    assert_eq!(
        serde_json::from_str::<SyncStatus>("\"done\"").unwrap(),
        SyncStatus::Completed
    );
    assert_eq!(
        serde_json::from_str::<SyncStatus>("\"syncing\"").unwrap(),
        SyncStatus::Scanning
    );
}

#[test]
fn test_sync_status_display() {
    assert_eq!(SyncStatus::Idle.to_string(), "idle");
    assert_eq!(SyncStatus::Scanning.to_string(), "scanning");
    assert_eq!(SyncStatus::Error.to_string(), "error");
    assert_eq!(SyncStatus::Completed.to_string(), "completed");
}

#[test]
fn test_sync_config_serde() {
    let sc = SyncConfig {
        id: "sc1".into(),
        backend_type: SyncBackendType::GitHub,
        enabled: true,
        account_id: Some("acc1".into()),
        name: Some("My Repo".into()),
        base_path: None,
        repo_name: Some("user/repo".into()),
        branch: Some("main".into()),
        token: Some("ghp_abc".into()),
        folder_id: None,
        auto_sync: false,
        compress_before_upload: true,
        create_previews: true,
        delete_raw_after_sync: false,
        max_concurrent_uploads: 4,
        encrypt_before_upload: true,
        conflict_policy: Default::default(),
        placement: Default::default(),
        parity: 1,
        require_encryption: false,
        obfuscate_names: false,
        mirror: false,
        key_holder: false,
        oauth_credentials: None,
        created_at: None,
        updated_at: None,
    };
    let json = serde_json::to_string(&sc).unwrap();
    assert!(json.contains("\"backendType\""));
    assert!(json.contains("\"repoName\""));
    assert!(json.contains("\"autoSync\""));
    assert!(json.contains("\"compressBeforeUpload\""));
    assert!(json.contains("\"maxConcurrentUploads\""));
    let back: SyncConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(sc.id, back.id);
    assert_eq!(sc.backend_type, back.backend_type);
    assert!(back.enabled);
}

#[test]
fn test_sync_progress_serde() {
    let sp = SyncProgress {
        total_files: 100,
        processed_files: 42,
        current_file: Some("photo.jpg".into()),
        status: SyncStatus::Uploading,
        bytes_uploaded: 1024 * 1024,
        errors: vec!["timeout".into()],
        started_at: Some("2026-06-13T10:00:00Z".into()),
        estimated_remaining_seconds: Some(120.5),
    };
    let json = serde_json::to_string(&sp).unwrap();
    assert!(json.contains("\"totalFiles\""));
    assert!(json.contains("\"processedFiles\""));
    assert!(json.contains("\"estimatedRemainingSeconds\""));
    assert!(json.contains("\"bytesUploaded\""));
    let back: SyncProgress = serde_json::from_str(&json).unwrap();
    assert_eq!(sp.total_files, back.total_files);
    assert_eq!(sp.processed_files, back.processed_files);
}

#[test]
fn test_sync_result_serde() {
    let sr = SyncResult {
        files_synced: 10,
        bytes_uploaded: 5 * 1024 * 1024,
        bytes_saved_by_compression: 2 * 1024 * 1024,
        errors: vec![],
        duration_ms: 30000,
    };
    let json = serde_json::to_string(&sr).unwrap();
    assert!(json.contains("\"filesSynced\""));
    assert!(json.contains("\"bytesSavedByCompression\""));
    assert!(json.contains("\"durationMs\""));
}

#[test]
fn test_cloud_account_serde() {
    let ca = CloudAccount {
        id: "ca1".into(),
        name: "GitHub Main".into(),
        backend_type: SyncBackendType::GitHub,
        token: Some("ghp_xxx".into()),
        oauth_credentials: Some(OAuthCredentials {
            access_token: "acc".into(),
            refresh_token: Some("ref".into()),
            expires_at: Some(1700000000),
            client_id: "cid".into(),
            client_secret: Some("secret".into()),
        }),
        config: serde_json::json!({"org": "myorg"}),
        created_at: "2026-01-01".into(),
        updated_at: "2026-06-13".into(),
    };
    let json = serde_json::to_string(&ca).unwrap();
    assert!(json.contains("\"oauthCredentials\""));
    assert!(json.contains("\"backendType\""));
    let back: CloudAccount = serde_json::from_str(&json).unwrap();
    assert_eq!(ca.id, back.id);
    assert_eq!(ca.backend_type, back.backend_type);
}

#[test]
fn test_remote_file_serde() {
    let rf = RemoteFile {
        name: "photo.jpg".into(),
        path: "photos/2026/photo.jpg".into(),
        size_bytes: 2048,
        modified_at: "2026-06-13".into(),
        url: "https://api.github.com/repos/user/repo/contents/photos/2026/photo.jpg".into(),
    };
    let json = serde_json::to_string(&rf).unwrap();
    let back: RemoteFile = serde_json::from_str(&json).unwrap();
    assert_eq!(rf.name, back.name);
    assert_eq!(rf.url, back.url);
    assert_eq!(rf.size_bytes, back.size_bytes);
}
