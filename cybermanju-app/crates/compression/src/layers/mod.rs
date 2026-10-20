// CyberManju OS — Mod (crates/compression/src/layers/mod.rs).
// Role: LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3.
// Exports: brotli_layer, lz4, zstd_layer.
// Integrates: standalone (no imports).
// Contracts: none special (pure logic; errors bubble to caller).
pub mod brotli_layer;
pub mod lz4;
pub mod zstd_layer;

pub use brotli_layer::BrotliLayer;
pub use lz4::Lz4Layer;
pub use zstd_layer::ZstdLayer;
