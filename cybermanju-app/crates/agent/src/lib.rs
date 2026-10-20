//! CyberManju native AI agent core (`cybermanju-agent`).
//!
//! A purpose-built coding-agent loop with zero new runtimes: pure logic here,
//! I/O at the edges. The core (providers, permissions, protocol shapes,
//! turn machine, hash-anchored edits) compiles everywhere including
//! `wasm32-unknown-unknown`; the blocking HTTP transport lives behind the
//! `native` feature (Tauri + server), while the WASM dispatcher fetches in
//! `os-wasm` and reuses the same builders/parsers.
//!
//! Tool *execution* is caller-provided: native sides run tools against the
//! Kernel/volume, the WASM side against its volume map. Both sides share one
//! tool-schema set (OpenAI function format — the shape wllama and most
//! gateways accept).

pub mod agent_loop;
pub mod config;
pub mod edit;
pub mod mcp;
pub mod memory;
pub mod protocol;
pub mod providers;
pub mod redact;
pub mod self_research;
pub mod stream;

pub use config::{decide, match_wildcard, PermissionDecision};
pub use providers::{all_presets, find_preset, resolve, ResolvedEndpoint};
