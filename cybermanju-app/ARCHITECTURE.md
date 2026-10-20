# CyberManju OS — Architecture

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Module Dependency Graph](#2-module-dependency-graph)
3. [Data Flow](#3-data-flow)
4. [Database Schema](#4-database-schema)
5. [Storage Backend Architecture](#5-storage-backend-architecture)
6. [Compression Pipeline](#6-compression-pipeline)
7. [Encryption Architecture](#7-encryption-architecture)
8. [Sync Pipeline](#8-sync-pipeline)
9. [ZimaOS Integration](#9-zimaos-integration)
10. [Web Dashboard REST API](#10-web-dashboard-rest-api)
11. [Deployment Options](#11-deployment-options)

---

## 1. System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        CYBERMANJU OS                                │
│                                                                     │
│  ┌──────────────────────┐    ┌──────────────────────────────────┐   │
│  │   FRONTEND (Vue 3)   │    │         BACKEND (Rust)           │   │
│  │                      │    │                                  │   │
│  │  ┌────────────────┐  │    │  ┌────────────────────────────┐  │   │
│  │  │  16 Vue        │  │    │  │     Tauri IPC Handlers     │  │   │
│  │  │  Components    │  │    │  │     (commands/*.rs)        │  │   │
│  │  └───────┬────────┘  │    │  └────────────┬───────────────┘  │   │
│  │          │           │    │               │                  │   │
│  │  ┌───────▼────────┐  │    │  ┌────────────▼───────────────┐  │   │
│  │  │  Pinia Store   │  │    │  │     Core Modules           │  │   │
│  │  │  (stores/app)  │  │    │  │                            │  │   │
│  │  └───────┬────────┘  │    │  │  ┌────────┐ ┌───────────┐ │  │   │
│  │          │           │    │  │  │  db/   │ │  crypto/  │ │  │   │
│  │  ┌───────▼────────┐  │    │  │  │  redb  │ │ PQC+AEAD  │ │  │   │
│  │  │  useTauri.ts   │◄─┼────┼──┼─►├────────┤ ├───────────┤ │  │   │
│  │  │  Dual-mode:    │  │    │  │  │ 11 tbls│ │ChaCha20   │ │  │   │
│  │  │  IPC / REST    │  │    │  │  └────────┘ │ML-KEM/DSA │ │  │   │
│  │  └────────┬────────┘  │    │  │  ┌────────┐ └───────────┘ │  │   │
│  │           │           │    │  │  │ search/│ ┌───────────┐ │  │   │
│  └───────────┼───────────┘    │  │  │Tantivy │ │compress/  │ │  │   │
│              │                │  │  │BM25    │ │LZ4/ZSTD/  │ │  │   │
│     ┌────────┴─────────┐      │  │  └────────┘ │Brotli     │ │  │   │
│     │                  │      │  │  ┌────────┐ └───────────┘ │  │   │
│  ┌──▼──┐  ┌────────┐   │      │  │  │ sync/ │ ┌───────────┐ │  │   │
│  │Tauri│  │HTTP    │   │      │  │  │Backends│ │ faces/    │ │  │   │
│  │ IPC │  │REST    │   │      │  │  │4 types │ │ONNX/DBSCAN│ │  │   │
│  └──┬──┘  └────┬───┘   │      │  │  └────────┘ └───────────┘ │  │   │
│     │         │        │      │  └────────────────────────────┘  │   │
└─────┼─────────┼────────┴──────┴──────────────────────────────────┘   │
      │         │                                                       │
   Desktop   Browser                                                     │
  (native)  (port 3456)                                                  │
                                                                        │
│  ┌────────────────────────────────────────────────────────────────┐   │
│  │                  EXTERNAL SERVICES                              │   │
│  │  ┌────────┐ ┌──────────────┐ ┌───────────┐  │   │
│  │  │ Local  │ │   GitHub     │ │Google Drive│  │   │
│  │  │  FS    │ │Contents API  │ │Drive API  │  │   │
│  │  └────────┘ └──────────────┘ └───────────┘  │   │
│  └────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────┘
```

### Runtime Modes

| Mode | Entry Point | Transport | Use Case |
|------|------------|-----------|----------|
| **Tauri Desktop** | `src-tauri/src/main.rs` | Tauri IPC (`invoke`) | Native desktop app with full filesystem access |
| **Web Dashboard** | Embedded HTTP on `0.0.0.0:3456` | REST API (`fetch`) | Browser access from any device; Docker/ZimaOS |
| **GitHub Pages** | `dist-wasm/` static files | SPA (read-only demo) | Public showcase, no backend |

The frontend composable `useTauri.ts` auto-detects the environment via `window.__TAURI__` and routes commands to Tauri IPC or the REST API transparently.

---

## 2. Module Dependency Graph

```
                          lib.rs (orchestrator)
                               │
              ┌────────────────┼────────────────────┐
              │                │                    │
         commands/        web_dashboard/         AppState
       (12 modules)      (HTTP server)        (db + index + compressor)
              │
    ┌─────────┼──────────┬──────────┬──────────┬──────────┐
    │         │          │          │          │          │
  files.rs  search.rs  sync.rs   users.rs  faces.rs  map.rs
    │         │          │          │          │          │
    │    ┌────┘    ┌─────┘          │          │          │
    │    │         │                │          │          │
    ▼    ▼         ▼                ▼          ▼          ▼
  ┌──────────────────────────────────────────────────────────┐
  │                     Core Modules                          │
  │                                                          │
  │   db/          crypto/      compression/    search/      │
  │   ├── mod.rs   ├── mod.rs   ├── mod.rs      ├── mod.rs   │
  │   └── schema.rs└── pqc.rs   └── triple.rs   └── tantivy  │
  │                                         _index.rs        │
  │   faces/        tree_sitter/    preview/    sync/         │
  │   └── mod.rs    └── mod.rs      └── mod.rs  ├── mod.rs   │
  │                                                ├── backends│
  │                                                ├── models │
  │                                                └── pipeline│
  └──────────────────────────────────────────────────────────┘
```

### Module Responsibilities

| Module | Path | Responsibility |
|--------|------|---------------|
| **commands/** | `src-tauri/src/commands/` | 12 Tauri IPC handler files translating frontend calls into core module operations |
| **db/** | `src-tauri/src/db/` | redb ACID database wrapper with 11 tables, read/write transaction management |
| **crypto/** | `src-tauri/src/crypto/` | Post-quantum cryptography: ChaCha20Poly1305 AEAD + rustpq ML-KEM/ML-DSA key management |
| **compression/** | `src-tauri/src/compression/` | Triple-layer cascading compression (LZ4 -> ZSTD-15 -> Brotli-11) |
| **search/** | `src-tauri/src/search/` | Tantivy full-text search with BM25 ranking, faceted filtering, term dictionary autocomplete |
| **faces/** | `src-tauri/src/faces/` | Face detection (ONNX SCRFD+ArcFace where linked, `heuristic-v2` skin segmentation everywhere else — every result engine-labeled) and DBSCAN clustering via connected components |
| **tree_sitter/** | `src-tauri/src/tree_sitter/` | Code intelligence: real tree-sitter grammars (rust/python/js/ts/go/bash) + heuristic fallback for the rest; `"engine"` reports which ran |
| **preview/** | `src-tauri/src/preview/` | Lanczos3 thumbnail generation, media metadata extraction |
| **sync/** | `src-tauri/src/sync/` | Storage backend trait + 3 implementations (Local, GitHub, Google Drive) |
| **web_dashboard/** | `src-tauri/src/web_dashboard/` | Embedded HTTP/1.1 server on port 3456 with REST API |

---

## 3. Data Flow

### Desktop Mode (Tauri IPC)

```
Vue Component
    │
    ▼
Pinia Store (stores/app.ts)
    │  calls action like loadFiles()
    ▼
useTauri.ts → invoke("list_files", { parentPath: "/" })
    │
    ▼ (Tauri IPC bridge — serialized JSON)
    │
commands/files.rs → list_files()
    │
    ├──► db.lock().begin_read()
    │       └──► redb: scan files table, filter by parentId
    │
    ▼
Serde JSON (camelCase) → Frontend
    │
    ▼
Pinia Store updates → Vue reactivity → UI re-renders
```

### Web Dashboard Mode (REST)

```
Vue Component (served from /static)
    │
    ▼
Pinia Store → useTauri.ts → isWebMode() = true
    │
    ▼
REST_ROUTES mapping: list_files → GET /api/files
    │
    ▼
fetch("http://localhost:3456/api/files", { headers: { Authorization: Bearer ... } })
    │
    ▼
web_dashboard/mod.rs → TcpListener → handle_connection()
    │
    ├──► redb: open database, scan files table
    │
    ▼
HTTP/1.1 200 JSON response (snake_case)
    │
    ▼
transformResponseKeys(): snake_case → camelCase
    │
    ▼
Pinia Store → Vue reactivity → UI re-renders
```

### Write Pipeline Example (File Import)

```
1. User selects files via dialog (Tauri) or upload (Web)
2. import_file() command:
   a. Read file bytes from disk
   b. Compute BLAKE3 hash → deduplication check
   c. Detect MIME type via `infer` crate
   d. Extract EXIF GPS via `kamadak-exif` (if image)
   e. Create FileNode with UUID, timestamps, metadata
   f. Serialize to JSON, write to redb files table
   g. Index in Tantivy (file_name, tags, content_text)
   h. Update FileNode with Tantivy doc ID
3. Pinia store refreshes file list
```

---

## 4. Database Schema

All data is stored in a single redb file (`cybermanju.db`) using ACID MVCC transactions. Each table maps `&str` keys to `&str` JSON values.

### Table: `files`

The central table. One row per file or folder tracked by the application.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 primary key |
| `name` | `string` | Display filename |
| `fileType` | `string` | `"file"` or `"folder"` |
| `parentId` | `string?` | Parent folder ID (null = root) |
| `sizeBytes` | `u64` | Original file size in bytes |
| `mimeType` | `string?` | Detected MIME type (infer crate) |
| `hashBlake3` | `string?` | BLAKE3 hex digest for deduplication |
| `encrypted` | `bool` | Whether the file is encrypted |
| `encryptionAlgorithm` | `string?` | Algorithm identifier (e.g., "ML-KEM+ChaCha20Poly1305") |
| `compressionLayers` | `string[]` | Applied compression layers: ["lz4", "zstd", "brotli"] |
| `thumbnailPath` | `string?` | Path to generated preview thumbnail |
| `contextData` | `object?` | Extensible metadata: original_path, sync_url, sync_backend, synced_at |
| `tags` | `string[]` | User-defined searchable tags |
| `collectionIds` | `string[]` | IDs of collections containing this file |
| `faceGroupIds` | `string[]` | IDs of face groups this file belongs to |
| `looseGroupIds` | `string[]` | IDs of user-defined loose groups |
| `gpsLat` | `f64?` | EXIF GPS latitude |
| `gpsLon` | `f64?` | EXIF GPS longitude |
| `createdAt` | `string` | ISO 8601 creation timestamp |
| `modifiedAt` | `string` | ISO 8601 last modification timestamp |

### Table: `accounts`

Storage origin accounts (local directories, cloud services, network mounts).

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `name` | `string` | Display name |
| `accountType` | `string` | `"local"`, `"cloud"`, or `"network"` |
| `path` | `string?` | Local filesystem path |
| `color` | `string` | UI color hex code |
| `isActive` | `bool` | Whether this account is currently active |
| `createdAt` | `string` | ISO 8601 |
| `updatedAt` | `string` | ISO 8601 |

### Table: `collections`

Curated groups of files (highlights, best moments, custom albums).

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `name` | `string` | Collection name |
| `collectionType` | `string` | `"highlights"`, `"best_moments"`, or `"custom"` |
| `color` | `string` | UI color hex code |
| `description` | `string?` | Optional description |
| `itemIds` | `string[]` | File IDs in this collection |
| `createdAt` | `string` | ISO 8601 |
| `updatedAt` | `string` | ISO 8601 |

### Table: `collection_items`

Junction table linking files to collections with optional notes.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `collectionId` | `string` | Parent collection ID |
| `fileId` | `string` | Referenced file ID |
| `note` | `string?` | User annotation |
| `addedAt` | `string` | ISO 8601 |

### Table: `face_groups`

Clusters of files containing the same person's face.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `name` | `string` | Person label (e.g., "Person 1") |
| `fileIds` | `string[]` | File IDs in this face group |
| `centroidEmbedding` | `f32[]?` | 128-dim centroid vector for the cluster |
| `createdAt` | `string` | ISO 8601 |

### Table: `loose_groups`

User-defined ad-hoc file groupings.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `name` | `string` | Group name |
| `color` | `string` | UI color hex code |
| `fileIds` | `string[]` | File IDs in this group |
| `createdAt` | `string` | ISO 8601 |

### Table: `encryption_keys`

PQC and classical encryption keypairs.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `algorithm` | `string` | Algorithm identifier |
| `publicKey` | `string` | Base64-encoded public key bytes |
| `privateKey` | `string` | Base64-encoded private key bytes |
| `label` | `string?` | User-friendly label |
| `createdAt` | `string` | ISO 8601 |

### Table: `users`

Application users with role-based access control.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `username` | `string` | Unique login name |
| `passwordHash` | `string` | Argon2id hash string |
| `displayName` | `string?` | Human-readable name |
| `role` | `string` | `"admin"`, `"user"`, or `"viewer"` |
| `isActive` | `bool` | Whether account is active |
| `createdAt` | `string` | ISO 8601 |
| `updatedAt` | `string` | ISO 8601 |

### Table: `user_file_perms`

Per-file access control entries.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `userId` | `string` | Referenced user ID |
| `fileId` | `string` | Referenced file ID |
| `access` | `string` | `"read"`, `"write"`, or `"admin"` |
| `grantedBy` | `string` | Admin user ID who granted |
| `grantedAt` | `string` | ISO 8601 |

### Table: `sync_configs`

Backend sync configuration entries.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `backendType` | `string` | `"local"`, `"github"`, `"googleDrive"` |
| `enabled` | `bool` | Whether this sync config is active |
| `basePath` | `string?` | Local or remote base path |
| `repoName` | `string?` | GitHub repo (owner/repo) |
| `branch` | `string?` | Git branch |
| `token` | `string?` | OAuth token / personal access token |
| `folderId` | `string?` | Google Drive folder ID |
| `autoSync` | `bool` | Enable automatic sync |
| `compressBeforeUpload` | `bool` | Apply triple compression before upload |
| `createPreviews` | `bool` | Generate Lanczos3 thumbnails |
| `deleteRawAfterSync` | `bool` | Delete original after successful sync |
| `maxConcurrentUploads` | `u32` | Upload concurrency limit |

### Table: `agent_configs`

Native agent configurations. Rows are keyless — provider keys live in
`sync_secrets` as `agent:key:<config_id>` and responses report `hasKey`.

| JSON Field | Type | Description |
|------------|------|-------------|
| `id` | `string` | UUID v4 |
| `name` | `string` | Display name |
| `providerId` | `string` | Preset id or `"custom"` |
| `model` | `string` | Model id (refreshable via provider) |
| `baseUrlOverride` | `string?` | Custom endpoint |
| `dialectOverride` | `string?` | `"openAi"` / `"anthropic"` (custom) |
| `authSchemeOverride` | `string?` | `"bearer"` / `"header"` / `"query"` / `"none"` |
| `workingDir` | `string` | Tool root, volume-relative |
| `agentKind` | `string` | `"build"` or `"plan"` (read-only) |
| `permission` | `object` | `{default, rules}` allow/ask/deny + wildcards |
| `autoApprove` | `bool` | Auto-approve asks (never denies) |
| `maxTurns` | `u32` | Turn budget (clamped 1–50) |

### Table: `agent_sessions`

Agent transcripts (full `messages[]` + usage), export/import-compatible.

### Table: `locations`

Geographic and storage location metadata.

| JSON Field | Type | Description |
|------------|------|-------------|
| (dynamic) | - | Extensible location records |

---

## 5. Storage Backend Architecture

The sync system uses a trait-based backend architecture. All HTTP backends use `curl` subprocess calls to avoid external HTTP dependencies.

```
┌───────────────────────────────────────────────┐
│              StorageBackend Trait             │
│                                               │
│  + name() -> &str                             │
│  + backend_type() -> SyncBackendType          │
│  + upload_file(local, remote) -> url          │
│  + download_file(remote, local)               │
│  + delete_file(remote)                        │
│  + list_files(prefix) -> Vec<RemoteFile>      │
│  + get_file_url(remote) -> url                │
│  + test_connection() -> bool                  │
└───────────┬───────────┬───────────┬───────────┘
            │           │           │
    ┌───────▼──┐  ┌────▼────┐  ┌──▼────────┐
    │  Local   │  │ GitHub  │  │Google Drive│
    │  Backend │  │ Backend │  │  Backend   │
    │          │  │         │  │            │
    │ fs::copy │  │ Contents│  │ Drive API  │
    │ rename   │  │ API v3  │  │ v3 + curl  │
    │          │  │Releases │  │            │
    │          │  │ + curl  │  │ multipart  │
    └──────────┘  └─────────┘  │ upload     │
                                └─────────────┘
```

### Backend Details

| Backend | Auth | Upload Mechanism | Max File Size |
|---------|------|-----------------|---------------|
| **Local** | None | `fs::copy` | Unlimited |
| **GitHub** | Personal Access Token | Contents API (base64), Releases for >100MB | 100MB (Contents), 2GB (Releases) |
| **Google Drive** | OAuth2 token | Drive API v3 multipart upload via `curl` | 5TB |

---

## 6. Compression Pipeline

### Triple-Layer Cascading Compression

```
Plaintext
    │
    ▼
┌─────────────────────────┐
│  Layer 1: LZ4 (lz4_flex)│  ~400 MB/s — ultra-fast, size-prefixed
│  Color: #00D4FF          │
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│  Layer 2: ZSTD (level 15)│  Fast, configurable 1-22, excellent ratio
│  Color: #00FF41           │
└────────────┬────────────┘
             │
             ▼
┌─────────────────────────┐
│  Layer 3: Brotli (level 11)│  Maximum compression ratio, slowest
│  Color: #FFB800            │  Window: 22
└────────────┬────────────┘
             │
             ▼
     Compressed (.cyb3)
     + BLAKE3 hash of original
```

### Decompression (Reverse Order)

```
Compressed (.cyb3)
    │
    ▼
Brotli decode → ZSTD decode → LZ4 decode → Original Plaintext
```

### Single-Layer Modes

| Mode | Algorithm | Speed | Use Case |
|------|-----------|-------|----------|
| `None` | Pass-through | Instant | Already-compressed files |
| `LZ4` | lz4_flex | ~400 MB/s | Real-time previews, streaming |
| `ZSTD` | zstd (configurable) | Fast | Balanced ratio/speed |
| `TripleLayer` | LZ4 -> ZSTD-15 -> Brotli-11 | Slow | Maximum archival compression |

### BLAKE3 Content Hashing

Every file is hashed with BLAKE3 before and after compression. The hash is stored in `FileNode.hashBlake3` and `CompressionStats.blake3Hash` for:
- Deduplication: skip files with identical hashes
- Integrity verification: confirm decompression matches original
- Content-addressed storage foundation

---

## 7. Encryption Architecture

### Hybrid PQC + Classical Design

```
┌─────────────────────────────────────────────────────┐
│                 Encryption Pipeline                  │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │  Key Generation (PqcEngine)                   │   │
│  │                                               │   │
│  │  ML-KEM-1024 (FIPS 203)  ◄── NIST Level 5   │   │
│  │  ML-DSA-65   (FIPS 204)  ◄── NIST Level 5   │   │
│  │  SLH-DSA-128f (FIPS 205) ◄── NIST Level 1   │   │
│  │  Hybrid (ML-KEM + X25519) ◄── Transitional   │   │
│  │  AES-256-GCM              ◄── Classical only  │   │
│  └──────────────────┬───────────────────────────┘   │
│                     │                                │
│                     ▼                                │
│  ┌──────────────────────────────────────────────┐   │
│  │  Symmetric Layer: ChaCha20Poly1305 (AEAD)    │   │
│  │                                               │   │
│  │  • 256-bit key from CSPRNG (OsRng)            │   │
│  │  • 96-bit random nonce per operation           │   │
│  │  • Authenticated encryption (Poly1305 MAC)     │   │
│  │  • ~2^96 nonce space — collision-proof         │   │
│  └──────────────────┬───────────────────────────┘   │
│                     │                                │
│                     ▼                                │
│  ┌──────────────────────────────────────────────┐   │
│  │  Integrity Layer: BLAKE3                      │   │
│  │                                               │   │
│  │  • Hash of original plaintext stored in       │   │
│  │    FileEncryptedData.blake3Original            │   │
│  │  • Verified on decryption                     │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

### FileEncryptedData Package

Every encrypted file is stored as a `FileEncryptedData` struct containing:

| Field | Type | Description |
|-------|------|-------------|
| `ciphertext` | `Vec<u8>` | Ciphertext + Poly1305 auth tag (appended by AEAD) |
| `nonce` | `[u8; 12]` | Random 96-bit nonce |
| `algorithm` | `String` | Full algorithm string (e.g., "ML-KEM (Kyber-1024) — FIPS 203+ChaCha20Poly1305") |
| `keyId` | `String` | Reference to the EncryptionKey used |
| `blake3Original` | `String` | BLAKE3 hex of the original plaintext |
| `encryptedAt` | `String` | ISO 8601 timestamp |

### ML-DSA Digital Signatures

The `rustpq` crate provides FIPS 204 compliant signing/verification:

- **ML-DSA-65 (Dilithium-5)**: 1952-byte public key, 3309-byte signature, NIST Level 5
- **ML-DSA-44**: 1312-byte public key, 2420-byte signature

`sign_message()` and `verify_signature()` functions in `crypto/pqc.rs` handle key loading, signing, and verification via `rustpq::ml_dsa`.

### Authentication

User passwords are hashed with **Argon2id** (memory-hard, salted, key-stretched) via the `argon2` crate. Session tokens are generated as UUID v4 strings on successful authentication.

---

## 8. Sync Pipeline

### File Processing Flow

```
                    ┌──────────────┐
                    │  File IDs    │
                    │  (selection) │
                    └──────┬───────┘
                           │
                           ▼
              ┌────────────────────────┐
              │   1. SCAN (Scanning)   │
              │   Read FileNode from   │
              │   redb, resolve path   │
              └────────────┬───────────┘
                           │
                           ▼
              ┌────────────────────────┐
              │ 2. COMPRESS (Compressing)│
              │ Triple-layer pipeline   │
              │ LZ4 -> ZSTD -> Brotli   │
              │ Output: file.cyb3       │
              └────────────┬───────────┘
                           │
                           ▼
              ┌────────────────────────┐
              │ 3. PREVIEW (Linking)   │
              │ Lanczos3 resize to     │
              │ 512px max, PNG output  │
              └────────────┬───────────┘
                           │
                           ▼
              ┌────────────────────────┐
              │ 4. UPLOAD (Uploading)  │
              │ Backend-specific:      │
              │ fs::copy / curl POST   │
              │ Remote: cybermanju_sync/│
              └────────────┬───────────┘
                           │
                           ▼
              ┌────────────────────────┐
              │ 5. LINK (Linking)      │
              │ Update FileNode        │
              │ context_data with:     │
              │  - sync_url            │
              │  - sync_backend        │
              │  - synced_at           │
              └────────────┬───────────┘
                           │
                           ▼
              ┌────────────────────────┐
              │ 6. CLEAN (Cleaning)    │
              │ Delete original if     │
              │ deleteRawAfterSync &&  │
              │ compressed exists      │
              └────────────┬───────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │  SyncResult  │
                    │  files_synced│
                    │  bytes_uploaded│
                    │  bytes_saved │
                    │  duration_ms │
                    └──────────────┘
```

### Progress Tracking

`SyncPipeline` maintains atomic progress state accessible from any thread:

- `totalFiles` / `processedFiles` (AtomicU32)
- `currentFile` (Mutex<Option<String>>)
- `status` (Mutex<SyncStatus>) — one of: Idle, Scanning, Compressing, Uploading, Linking, Cleaning, Error, Done
- `bytesUploaded` (AtomicU64)
- `errors` (Mutex<Vec<String>>)
- `estimatedRemainingSeconds` — computed from elapsed time / files processed rate

Cancellation is supported via `AtomicBool` flag checked between each file.

---

## 9. ZimaOS Integration

### Docker Compose

`docker-compose.yml` is the single source of truth for the container shape —
the snippet below is a copy kept in sync by inspection. Deploys bind
LAN-only via `${CYBERMANJU_HOST_IP:-127.0.0.1}` (never a wide-open
`3456:3456`), run read-only with all capabilities dropped, and probe
**readiness** at `GET /api/readyz` (`GET /api/health` is liveness-only).

```yaml
services:
  cybermanju-os:
    image: cybermanju-os:latest
    build:
      context: .
      dockerfile: Dockerfile
    container_name: cybermanju-os
    restart: unless-stopped
    # Hardening: immutable root FS (/data volume + /tmp tmpfs writable only),
    # every capability dropped, no privilege escalation, memory capped.
    read_only: true
    tmpfs:
      - /tmp
    cap_drop:
      - ALL
    security_opt:
      - no-new-privileges:true
    mem_limit: 1g
    ports:
      # LAN only: bound to a private address, never 0.0.0.0. Override with
      # CYBERMANJU_HOST_IP (e.g. 192.168.1.50) to serve other devices.
      - "${CYBERMANJU_HOST_IP:-127.0.0.1}:3456:3456"
    volumes:
      - /DATA/AppData/cybermanju-os/config:/data
    environment:
      - RUST_LOG=info
      - PORT=3456
      - DB_PATH=/data/cybermanju.db
      - SEARCH_INDEX_PATH=/data/tantivy_index
      - STATIC_DIR=/app/static
      - TZ=UTC
    healthcheck:
      # /api/readyz is 200 only when redb, the Tantivy index and the data
      # volume are all usable; /api/health (liveness) is always 200.
      test: ["CMD", "wget", "--spider", "-q", "http://localhost:3456/api/readyz"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 15s
```

### Multi-Stage Docker Build

```
┌─────────────────────┐   ┌──────────────────────────┐   ┌──────────────────┐
│  Stage 1: Frontend  │   │  Stage 2: Rust Backend   │   │  Stage 3: Runtime│
│  node:24-alpine     │   │  rust:1.85-alpine        │   │  alpine:3.21     │
│                     │   │                          │   │                  │
│  npm install        │   │  cargo build --release   │   │  ca-certificates │
│  npm run build:wasm │──►│  (web_dashboard only)    │──►│  wget            │
│  → dist-wasm/       │   │  → cybermanju-os-     │   │  non-root user   │
│                     │   │    server binary          │   │                  │
└─────────────────────┘   └──────────────────────────┘   │  /app/static/    │
                                                        │  /data/ (volume) │
                                                        └──────────────────┘
```

The Docker build uses a standalone Rust server (`docker/server/`) that links the `web_dashboard` module directly — no Tauri or GTK dependencies. The Vue frontend is built with `DOCKER_BUILD=true` to set the correct base path.

### ZimaOS App Store Metadata

The `x-casaos` section in `docker-compose.yml` provides ZimaOS App Store integration:

- **Architectures:** `amd64`, `arm64`
- **Category:** File Sync, Utilities
- **Port:** 3456
- **Volume:** `/DATA/AppData/cybermanju-os/config` mapped to `/data`
- **Health Check:** readiness `GET /api/readyz` (wget, every 30s —
  `interval: 30s`, `timeout: 10s`, `retries: 3`, `start_period: 15s`);
  `GET /api/health` is liveness-only, not the container probe
- **Icon:** pinned to
  `https://raw.githubusercontent.com/cybermanju/cybermanju.github.io/main/src-tauri/icons/icon.svg` —
  it tracks `src-tauri/icons/icon.svg` on `main`; change both together
- **Routing consistency:** ZimaOS `index: /` + `port_map: "3456"` assume the
  app is served from root — this matches `vite.config.wasm.ts` (`base: "/"`
  by default, `VITE_BASE` override only when embedding under a sub-path)
  and Docker serving `STATIC_DIR=/app/static` at `/`
- **Localization:** English (en_us) and Chinese (zh_cn)

---

## 10. Web Dashboard REST API

The embedded HTTP server uses `std::net::TcpListener` with manual HTTP/1.1 parsing (no external HTTP crate). It opens the redb database independently per request.

### Authentication

Login via `POST /api/users/login` with `{ "username": "...", "password": "..." }`. Returns a JWT-like token (UUID v4). Pass as `Authorization: Bearer <token>` header on subsequent requests.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| **Files** | | |
| `GET` | `/api/files` | List all file nodes |
| `GET` | `/api/files/{id}` | Get single file by ID |
| `DELETE` | `/api/files/{id}` | Delete a file |
| **Accounts** | | |
| `GET` | `/api/accounts` | List all storage accounts |
| **Collections** | | |
| `GET` | `/api/collections` | List all collections |
| `GET` | `/api/collection-items` | List all collection items |
| **Face Groups** | | |
| `GET` | `/api/face-groups` | List all face groups |
| **Loose Groups** | | |
| `GET` | `/api/loose-groups` | List all loose groups |
| **Encryption** | | |
| `GET` | `/api/encryption/status` | Get encryption engine status and supported algorithms |
| `GET` | `/api/encryption/keys` | List all encryption keys |
| **Geo** | | |
| `GET` | `/api/geo-files` | List files with GPS coordinates |
| **Search** | | |
| `GET` | `/api/search?q={term}` | Search files by name, path, or content |
| `GET` | `/api/search/paginated?q={term}&limit=&offset=` | Paginated search + total count |
| `POST` | `/api/search/federated` | Cross-device search port: `{ query, limit?, peers: [{ baseUrl, deviceId?, token? }], timeoutSecs? }` → merged ranked hits (local tagged `local`); ≤8 peers, dead peers degrade to zero hits; only an explicit per-peer `token` is forwarded to that peer; the caller's bearer is never forwarded |
| **Locations** | | |
| `GET` | `/api/locations` | List all locations |
| **Users** | | |
| `GET` | `/api/users` | List users (password hashes redacted) |
| `POST` | `/api/users/register` | Register a new user (bootstrap-only: open while the user table is empty or `CYBERMANJU_ALLOW_REGISTRATION=1`; never grants `admin`) |
| `POST` | `/api/users/login` | Authenticate and receive a token |
| `GET` | `/api/auth/status` | Public first-run probe → `{ "registrationOpen": bool }` (drives the Docker/web login gate on every device) |
| `POST` | `/api/auth/logout` | Revoke the calling session |
| **Permissions** | | |
| `GET` | `/api/permissions/{fileId}` | Get permissions for a file |
| `POST` | `/api/permissions` | Grant a file permission |
| `POST` | `/api/permissions/verify` | Verify file access for a user |
| **AI Agent** | | |
| `GET` | `/api/agent/providers` | LLM presets (endpoints, families, models — no secrets) |
| `GET`/`POST` | `/api/agent/configs` | List/save configs (keyless rows, `hasKey` only) |
| `GET`/`DELETE` | `/api/agent/configs/{id}` | Get/delete a config |
| `PUT` | `/api/agent/configs/{id}/key` | Seal a provider API key |
| `GET` | `/api/agent/configs/{id}/models` | Refresh models from provider |
| `GET`/`POST` | `/api/agent/sessions` | List/create transcripts |
| `GET`/`DELETE` | `/api/agent/sessions/{id}` | Get/delete a transcript |
| `POST` | `/api/agent/sessions/import` | Import a transcript (re-keyed) |
| `POST` | `/api/agent/prompt` | Start a detached run (202 job) |
| `GET` | `/api/agent/jobs`, `/api/agent/jobs/{id}` | List/poll jobs (turns, usage, approvals) |
| `POST` | `/api/agent/jobs/{id}/abort` | Cancel a run |
| `POST` | `/api/agent/jobs/{id}/approve` | Answer an approval/question |
| `POST` | `/api/agent/sessions/{id}/compact` | Summarize into a fresh session (old kept) |
| `POST`/`DELETE` | `/api/agent/configs/{id}/mcp`, `…/mcp/{name}` | Attach/detach MCP servers (admin) |
| `GET` | `/api/agent/configs/{id}/mcp/tools` | Discover MCP tools |
| `GET`/`PUT` | `/api/files/{id}/content` | Read/overwrite managed file text (1 MiB cap, versioned saves) |
| `POST` | `/api/code/parse` | Parse source text (shared heuristic core) |
| **Devices** | | |
| `GET`/`POST` | `/api/devices` | List/register device identities (ML-DSA public keys; enrollment idempotent) |
| `DELETE` | `/api/devices/{id}` | Remove a device row (admin) |
| `POST` | `/api/devices/{id}/revoke` | Revoke a device — row stays, fails closed (admin) |
| `GET` | `/api/peers` | Active mDNS LAN scan (`_cybermanju._tcp.local`, 2 s cap) |
| `GET` | `/api/peers/known` | Server-assisted presence: fresh heartbeats for active devices owned by the caller (5 min TTL; admins may audit all) |
| `POST` | `/api/peers/announce` | Heartbeat `{ deviceId, host, port }` for an active device owned by the caller (once per launch; sync still verifies signatures) |
| **Todos** | | |
| `GET`/`POST` | `/api/todos` | Owner-scoped shared task list (agent tools use the same rows) |
| `PUT`/`DELETE` | `/api/todos/{id}` | Complete/reopen or delete a todo (owner-scoped, 404 across owners) |
| **Op-log** | | |
| `GET` | `/api/oplog/since?since=N&limit=M` | Pull ops after seq N for the authenticated owner's active devices (`{ head, ops }`, page capped at 5,000) |
| `POST` | `/api/oplog/push` | Merge only a signed envelope from the registered owner device (`{ deviceId, ops, signature, algorithm }`; unsigned/bad signature rejected) |
| `GET` | `/api/oplog/conflicts` | Fork pairs from the authenticated owner's devices only (admins may audit all) |
| `POST` | `/api/oplog/resolve` | Append a `resolve` verdict only when both ops and the resolving device belong to the caller (`keep-a`/`keep-b`/`merge`; history never deleted) |
| `POST` | `/api/peers/{deviceId}/sync` | Attested device↔device op-log exchange; signature/algorithm/device id must match an active owner registry row (`{ ops, signature, algorithm }` → `{ appended, skipped, head, verified: true }`; unsigned, unknown or revoked sender rejected) |
| **Relay (NAT rendezvous)** | | |
| `POST` | `/api/relay/drop` | Drop `{ from, to, kind, payload }` only when both devices belong to the authenticated owner (256 KiB cap; both sides outbound HTTPS) |
| `POST` | `/api/relay/poll` | List only the caller-owned recipient's queued envelopes, oldest first (non-destructive — ack clears) |
| `POST` | `/api/relay/ack` | Clear only the caller-owned recipient's delivered envelopes (`{ cleared }`) |
| **Sneakernet** | | |
| `GET` | `/api/sync/bundle/export?since=N` | BLAKE3-manifested op delta (`cyb-bundle/1`, admin) |
| `POST` | `/api/sync/bundle/build` | Build a bundle carrying file bytes (`files: [{ fileId, name, bytesBase64 }]`; 5 MiB/file, 100 files, 50 MiB total, admin) |
| `POST` | `/api/sync/bundle/import` | Validate + merge a bundle, every payload BLAKE3-verified (`{ appended, skipped, from, payloads }`, idempotent, admin) |
| **Health** | | |
| `GET` | `/api/health` | Health check (returns `{ service, status, timestamp }`) |

### Response Format

All responses are JSON. List endpoints return arrays where each item includes a `_key` field (the redb primary key). The frontend `useTauri.ts` composable automatically strips `_key`/`_raw` fields and transforms `snake_case` keys to `camelCase` to match TypeScript types.

### Static File Serving

In Docker mode, the compiled Vue frontend (`/app/static/`) is served for all non-`/api/` paths. The standalone server at `docker/server/src/main.rs` handles this routing.

---

## 11. Deployment Options

### 1. Tauri Desktop App

Full-featured native desktop application with filesystem access, system tray, and all features.

**Build:**
```bash
npm run tauri:build
```

**Output:** Platform-specific installer (`.dmg`, `.msi`, `.deb`, `.AppImage`)

**Requirements:**
- Node.js 24+
- Rust 1.85+
- Platform-specific: GTK3/WebKit2GTK (Linux), WebView2 (Windows), nothing extra (macOS)

### 2. Docker / ZimaOS Container

Headless deployment for NAS devices, servers, and ZimaOS. Runs the web dashboard on port 3456.

**Build:**
```bash
docker build -t cybermanju-os:latest .
```

**Run:**
```bash
docker compose up -d
```

**Requirements:** Docker with BuildKit support.

**Architecture Support:** `linux/amd64`, `linux/arm64`

### 3. WASM / GitHub Pages

Static frontend-only deployment for public showcase. Connects to a remote CyberManju OS instance via REST API, or runs in read-only demo mode.

**Build:**
```bash
npm run build:wasm
```

**Output:** `dist-wasm/` directory served via GitHub Actions to GitHub Pages.

**Transports:** `tauri` IPC (desktop; `os_*/disk/*` are `REST_FIRST` → `:3456`), `rest` (Docker/web), `wasm` (Pages: `os-wasm` localStorage volume + BM25-lite; server ops answer `unsupported:`). Settings shows the active transport (`VITE_TRANSPORT` override).

## 12. Decentralized OS layer (disks, durability, cybsh)

```
providers → .cybermanju disks (CYBMJU1 sealed superblock, choosable capacity)
  → merged volume (spanned placement, df, admit→place→upload→commit)
  → durability (scrub→repair queue, RS k+m, rebuild-from-remote, refcount GC, leases)
  → cybsh (Kernel syscall boundary → os_api → REST / Tauri-REST_FIRST / WASM)
```

- **Crates:** `disk` (superblock/disk/allocator/catalog/volume/mount), `erasure` (RS codec), `os` (shell/task/compute/api), `sync/{scrub,repair,gc,health,lease,manifest}`.
- **Tables (17+):** files, accounts, collections, collection_items, face_groups, loose_groups, encryption_keys, users, user_file_perms, sync_configs (+`sync_files/sync_runs/sync_secrets/schema_version`), `disks/volumes/block_map`, `scrub_runs/repairs/chunk_refs/leases/provider_health`, `compute_tasks/shell_history`.
- **Sync jobs:** `POST /api/sync/start → 202 {jobId}`, `GET /api/sync/jobs/{id}|runs|status|progress`, `POST /api/sync/cancel|restore`, `DELETE /api/sync/remote` (501 on `unsupported:`), `GET /api/sync/usage/{id}`.
- **Durability:** `GET /api/repair/status|tasks|health`, `POST /api/repair/run|rebuild|gc`, `POST /api/scrub/run`, `GET /api/scrub/runs`, `POST /api/lease/acquire|release`, `GET /api/lease/status`.
- **OS:** `POST /api/os/exec {line}` (incl. real `sync start …` → detached job), `GET /api/os/complete/{prefix}|stat|ls|du|df|ps|top|workers|jobs` (`ps`/`top` merge OS + repair rows); `GET /api/disk/*`, `GET|PUT /api/volume/block/{lba}` (base64, body range).
- **Code:** `POST /api/code/parse {fileName, content}` (shared heuristic core, 1 MiB cap, 413 over); desktop Tauri `parse_text` runs the real grammars.
- **Editor:** `GET|PUT /api/files/{id}/content` (1 MiB cap; version snapshot before overwrite; honest `encrypted:`/`binary:`/`too_large:` refusals); desktop Tauri twins; Pages edits the WASM volume (`write` op, localStorage).
- **Auth:** fail-closed `Authenticated` default, `is_known_route` 404-before-401, `Claims{role}` RBAC, bootstrap-only register, sealed secrets.
- **Known honest limits:** faces fall back to the labeled `heuristic-v2` detector without ONNX; code parse heuristic for unbundled languages; WASM `sync` needs the dashboard.

### CI/CD Pipeline

```
Push to main
    │
    ├──► rust-check: cargo fmt, clippy, test
    │
    ├──► docker-build: multi-stage Docker image
    │
    ├──► wasm-build: npm ci, vue-tsc, npm run build:wasm
    │         │
    │         └──► deploy-pages: upload to GitHub Pages
    │
    └──► (all jobs run in parallel where possible)
```
