// CyberManju OS — Shared command logic
//
// Single source of truth for business logic used by BOTH the Tauri IPC
// commands (`src-tauri/src/commands`) and the HTTP REST routes
// (`crates/web/src/lib.rs`). Commands take an already-locked `&Database`;
// callers are responsible for acquiring the right lock for the operation.

pub mod accounts;
pub mod agent_api;
pub mod audit;
pub mod backup;
pub mod batch;
pub mod code;
pub mod collections;
pub mod cron_api;
// <<< CYBERMANJU OS PUSH: pre-wired route families — each owned by one agent.
// The dispatcher in lib.rs already calls all three; implement `route()`. >>>
pub mod devices;
pub mod disk_api;
pub mod faces_api;
pub mod files;
pub mod oauth;
pub mod oplog;
pub mod os_api;
pub mod permissions;
pub mod repair_api;
pub mod search_api;
pub mod secrets;
pub mod share;
pub mod sync_api;
pub mod todos;
pub mod trash;
pub mod users;
pub mod versions;
