// CyberManju OS — Triple (crates/compression/src/triple.rs).
// Role: LZ4→ZSTD-15→Brotli-11 triple cascade + .cyb3.
// Exports: TripleCompressor, new, compress_lz4, decompress_lz4, compress_zstd, decompress_zstd, compress_brotli, decompress_brotli.
// Integrates: anyhow.
// Contracts: none special (pure logic; errors bubble to caller).
use anyhow::Result;

use crate::layers::{BrotliLayer, Lz4Layer, ZstdLayer};
use crate::types::{CompressionStats, LayerDetail};

pub struct TripleCompressor {
    zstd_level: i32,
    brotli_level: u32,
}

impl TripleCompressor {
    pub fn new() -> Self {
        Self {
            zstd_level: 15,
            brotli_level: 11,
        }
    }

    pub fn compress_lz4(&self, data: &[u8]) -> Result<Vec<u8>> {
        Lz4Layer::compress(data)
    }

    pub fn decompress_lz4(&self, data: &[u8]) -> Result<Vec<u8>> {
        Lz4Layer::decompress(data)
    }

    pub fn compress_zstd(&self, data: &[u8]) -> Result<Vec<u8>> {
        ZstdLayer::compress(data, self.zstd_level)
    }

    pub fn decompress_zstd(&self, data: &[u8]) -> Result<Vec<u8>> {
        ZstdLayer::decompress(data)
    }

    pub fn compress_brotli(&self, data: &[u8]) -> Result<Vec<u8>> {
        BrotliLayer::compress(data, self.brotli_level)
    }

    pub fn decompress_brotli(&self, data: &[u8]) -> Result<Vec<u8>> {
        BrotliLayer::decompress(data)
    }

    pub fn compress_data(&self, data: &[u8], layer: &str) -> Result<(Vec<u8>, LayerDetail)> {
        match layer {
            "lz4" => Lz4Layer::compress_with_detail(data),
            "zstd" => ZstdLayer::compress_with_detail(data, self.zstd_level),
            "brotli" => BrotliLayer::compress_with_detail(data, self.brotli_level),
            other => anyhow::bail!("Unknown compression layer: {}", other),
        }
    }

    /// Triple-layer compression: LZ4 → ZSTD → Brotli.
    /// If LZ4 ratio > 0.98 (incompressible), skip remaining layers.
    pub fn compress_triple(&self, data: &[u8]) -> Result<(Vec<u8>, CompressionStats)> {
        let original_size = data.len() as u64;
        let start = std::time::Instant::now();

        let lz4_out = Lz4Layer::compress(data)?;
        let lz4_ratio = if original_size > 0 {
            lz4_out.len() as f64 / original_size as f64
        } else {
            1.0
        };

        if lz4_ratio > 0.98 {
            let hash = blake3::hash(data);
            let duration_ms = start.elapsed().as_millis() as u64;
            return Ok((
                data.to_vec(),
                CompressionStats {
                    original_size,
                    compressed_size: original_size,
                    ratio: 1.0,
                    layer: "skipped (incompressible)".into(),
                    layer_details: vec![LayerDetail {
                        name: "LZ4 probe".into(),
                        algorithm: "lz4_flex".into(),
                        input_size: original_size,
                        output_size: lz4_out.len() as u64,
                        ratio: lz4_ratio,
                        color: "#6B7280".into(),
                    }],
                    blake3_hash: hash.to_hex().to_string(),
                    duration_ms,
                },
            ));
        }

        let lz4_detail = LayerDetail {
            name: "Layer 1: LZ4".into(),
            algorithm: "lz4_flex".into(),
            input_size: original_size,
            output_size: lz4_out.len() as u64,
            ratio: lz4_ratio,
            color: "#00D4FF".into(),
        };

        let zstd_out = ZstdLayer::compress(&lz4_out, self.zstd_level)?;
        let zstd_detail = LayerDetail {
            name: "Layer 2: Zstandard".into(),
            algorithm: format!("zstd level {}", self.zstd_level),
            input_size: lz4_out.len() as u64,
            output_size: zstd_out.len() as u64,
            ratio: if !lz4_out.is_empty() {
                zstd_out.len() as f64 / lz4_out.len() as f64
            } else {
                1.0
            },
            color: "#00FF41".into(),
        };

        let brotli_out = BrotliLayer::compress(&zstd_out, self.brotli_level)?;
        let brotli_detail = LayerDetail {
            name: "Layer 3: Brotli".into(),
            algorithm: format!("brotli level {}", self.brotli_level),
            input_size: zstd_out.len() as u64,
            output_size: brotli_out.len() as u64,
            ratio: if !zstd_out.is_empty() {
                brotli_out.len() as f64 / zstd_out.len() as f64
            } else {
                1.0
            },
            color: "#FFB800".into(),
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let hash = blake3::hash(data);

        let compressed_size = brotli_out.len() as u64;

        Ok((
            brotli_out,
            CompressionStats {
                original_size,
                compressed_size,
                ratio: if original_size > 0 {
                    compressed_size as f64 / original_size as f64
                } else {
                    1.0
                },
                layer: "triple".into(),
                layer_details: vec![lz4_detail, zstd_detail, brotli_detail],
                blake3_hash: hash.to_hex().to_string(),
                duration_ms,
            },
        ))
    }

    /// Triple-layer decompression: Brotli → ZSTD → LZ4.
    pub fn decompress_triple(&self, data: &[u8]) -> Result<(Vec<u8>, u64)> {
        // Empty input is the round-trip of an empty payload: the compress
        // probe classifies it as incompressible and returns the bytes
        // untouched, so there is no frame for a decoder to read.
        if data.is_empty() {
            return Ok((Vec::new(), 0));
        }
        let start = std::time::Instant::now();
        let brotli_out = BrotliLayer::decompress(data)?;
        let zstd_out = ZstdLayer::decompress(&brotli_out)?;
        let lz4_out = Lz4Layer::decompress(&zstd_out)?;
        let duration_ms = start.elapsed().as_millis() as u64;
        Ok((lz4_out, duration_ms))
    }

    pub fn blake3_hash(data: &[u8]) -> String {
        blake3::hash(data).to_hex().to_string()
    }
}

impl Default for TripleCompressor {
    fn default() -> Self {
        Self::new()
    }
}
