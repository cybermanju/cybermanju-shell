// CyberManju OS — Lib (crates/db/src/lib.rs).
// Role: redb store (cybermanju.db): tables + sessions/memories.
// Exports: database.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
pub mod database;

pub use database::Database;
