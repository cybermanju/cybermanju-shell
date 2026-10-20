// CyberManju OS — Mod (src-tauri/src/commands/mod.rs).
// Role: Tauri IPC commands (19 modules, 130 handlers).
// Exports: accounts, agent, audit, batch, collections, compression, cron, dashboard.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
pub mod accounts;
pub mod agent;
pub mod audit;
pub mod batch;
pub mod collections;
pub mod compression;
pub mod cron;
pub mod dashboard;
pub mod devices;
pub mod disk;
pub mod encryption;
pub mod faces;
pub mod files;
pub mod import;
pub mod launcher;
pub mod map;
pub mod os_metrics;
pub mod os_shell;
pub mod p2p;
pub mod search;
pub mod secrets;
pub mod share;
pub mod sync;
pub mod todos;
pub mod trash;
pub mod users;
pub mod vault_snapshot;
pub mod versions;
