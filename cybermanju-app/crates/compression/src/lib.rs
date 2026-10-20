// CyberManju OS — Lib (crates/compression/src/lib.rs).
// Role: LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3.
// Exports: layers, triple, types.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
pub mod layers;
pub mod triple;
pub mod types;

pub use layers::*;
pub use triple::TripleCompressor;
pub use types::*;
