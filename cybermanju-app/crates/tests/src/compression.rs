// CyberManju OS — Compression (crates/tests/src/compression.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: cybermanju_compression.
// Contracts: none special (pure logic; errors bubble to caller).
use cybermanju_compression::layers::{BrotliLayer, Lz4Layer, ZstdLayer};
use cybermanju_compression::triple::TripleCompressor;
use cybermanju_compression::types::{CompressionStats, CompressionType, LayerDetail};

/// High-entropy bytes for "incompressible" assertions.
///
/// BLAKE3 in counter mode: deterministic (no RNG dependency) and with no
/// repeating structure. `(i * k) & 0xFF` sequences repeat every 256 bytes,
/// which LZ4 matches away — they are compressible, not random.
fn incompressible_bytes(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 32);
    let mut counter = 0u64;
    while out.len() < len {
        out.extend_from_slice(blake3::hash(&counter.to_le_bytes()).as_bytes());
        counter += 1;
    }
    out.truncate(len);
    out
}

// ─── CompressionType tests ─────────────────────────────────────────

#[test]
fn test_compression_type_display_names() {
    assert_eq!(CompressionType::None.display_name(), "Uncompressed");
    assert_eq!(CompressionType::Lz4.display_name(), "LZ4 (Real-Time)");
    assert_eq!(CompressionType::Zstd.display_name(), "Zstandard (Balanced)");
    assert_eq!(
        CompressionType::TripleLayer.display_name(),
        "Triple-Layer (LZ4→ZSTD→Brotli)"
    );
}

#[test]
fn test_compression_type_colors() {
    assert_eq!(CompressionType::None.color(), "#6B7280");
    assert_eq!(CompressionType::Lz4.color(), "#00D4FF");
    assert_eq!(CompressionType::Zstd.color(), "#00FF41");
    assert_eq!(CompressionType::TripleLayer.color(), "#FFB800");
}

#[test]
fn test_compression_type_speed_labels() {
    assert_eq!(CompressionType::None.speed_label(), "N/A");
    assert!(CompressionType::Lz4.speed_label().contains("400 MB/s"));
    assert!(CompressionType::Zstd.speed_label().contains("configurable"));
    assert!(CompressionType::TripleLayer
        .speed_label()
        .contains("maximum ratio"));
}

#[test]
fn test_compression_type_serde_roundtrip() {
    for ct in [
        CompressionType::None,
        CompressionType::Lz4,
        CompressionType::Zstd,
        CompressionType::TripleLayer,
    ] {
        let json = serde_json::to_string(&ct).unwrap();
        let back: CompressionType = serde_json::from_str(&json).unwrap();
        assert_eq!(ct, back);
    }
}

#[test]
fn test_layer_detail_serde() {
    let detail = LayerDetail {
        name: "LZ4".into(),
        algorithm: "lz4_flex".into(),
        input_size: 1000,
        output_size: 800,
        ratio: 0.8,
        color: "#00D4FF".into(),
    };
    let json = serde_json::to_string(&detail).unwrap();
    let back: LayerDetail = serde_json::from_str(&json).unwrap();
    assert_eq!(detail.name, back.name);
    assert_eq!(detail.ratio, back.ratio);
}

#[test]
fn test_compression_stats_serde() {
    let stats = CompressionStats {
        original_size: 10000,
        compressed_size: 5000,
        ratio: 0.5,
        layer: "triple".into(),
        layer_details: vec![],
        blake3_hash: "abc123".into(),
        duration_ms: 42,
    };
    let json = serde_json::to_string(&stats).unwrap();
    assert!(json.contains("\"originalSize\""));
    assert!(json.contains("\"compressedSize\""));
    assert!(json.contains("\"blake3Hash\""));
}

// ─── LZ4 layer tests ──────────────────────────────────────────────

#[test]
fn test_lz4_roundtrip_repeated_data() {
    let data = vec![42u8; 10000];
    let compressed = Lz4Layer::compress(&data).unwrap();
    let decompressed = Lz4Layer::decompress(&compressed).unwrap();
    assert_eq!(data, decompressed);
    assert!(compressed.len() < data.len() / 10);
}

#[test]
fn test_lz4_roundtrip_text_data() {
    let text = b"The quick brown fox jumps over the lazy dog. \
                  The quick brown fox jumps over the lazy dog. \
                  The quick brown fox jumps over the lazy dog.";
    let compressed = Lz4Layer::compress(text).unwrap();
    let decompressed = Lz4Layer::decompress(&compressed).unwrap();
    assert_eq!(text.as_slice(), decompressed.as_slice());
}

#[test]
fn test_lz4_roundtrip_empty() {
    let compressed = Lz4Layer::compress(b"").unwrap();
    let decompressed = Lz4Layer::decompress(&compressed).unwrap();
    assert!(decompressed.is_empty());
}

#[test]
fn test_lz4_roundtrip_single_byte() {
    let data = [0xFF];
    let compressed = Lz4Layer::compress(&data).unwrap();
    let decompressed = Lz4Layer::decompress(&compressed).unwrap();
    assert_eq!(&data, decompressed.as_slice());
}

#[test]
fn test_lz4_detail() {
    let data = b"compression test data here";
    let (compressed, detail) = Lz4Layer::compress_with_detail(data).unwrap();
    assert_eq!(detail.name, "Layer 1: LZ4");
    assert_eq!(detail.algorithm, "lz4_flex");
    assert_eq!(detail.input_size, data.len() as u64);
    assert_eq!(detail.output_size, compressed.len() as u64);
}

#[test]
fn test_lz4_probe_ratio_compressible() {
    let repeated = vec![0u8; 10000];
    let ratio = Lz4Layer::probe_ratio(&repeated);
    assert!(ratio < 0.1, "repeated data should compress massively");
}

#[test]
fn test_lz4_probe_ratio_incompressible() {
    let random = incompressible_bytes(10_000);
    let ratio = Lz4Layer::probe_ratio(&random);
    assert!(ratio > 0.5, "incompressible data should not compress well");
}

// ─── Zstd layer tests ─────────────────────────────────────────────

#[test]
fn test_zstd_roundtrip_repeated_data() {
    let data = vec![0xABu8; 50000];
    let compressed = ZstdLayer::compress(&data, 3).unwrap();
    let decompressed = ZstdLayer::decompress(&compressed).unwrap();
    assert_eq!(data, decompressed);
    assert!(compressed.len() < data.len() / 10);
}

#[test]
fn test_zstd_roundtrip_text_data() {
    let text = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                  Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
    let compressed = ZstdLayer::compress(text, 3).unwrap();
    let decompressed = ZstdLayer::decompress(&compressed).unwrap();
    assert_eq!(text.as_slice(), decompressed.as_slice());
}

#[test]
fn test_zstd_roundtrip_empty() {
    let compressed = ZstdLayer::compress(b"", 3).unwrap();
    let decompressed = ZstdLayer::decompress(&compressed).unwrap();
    assert!(decompressed.is_empty());
}

#[test]
fn test_zstd_higher_level_better_ratio() {
    let data = vec![0u8; 50000];
    let fast = ZstdLayer::compress(&data, 1).unwrap();
    let max = ZstdLayer::compress(&data, 22).unwrap();
    assert!(max.len() <= fast.len());
}

#[test]
fn test_zstd_detail() {
    let data = b"zstd detail test";
    let (_, detail) = ZstdLayer::compress_with_detail(data, 5).unwrap();
    assert_eq!(detail.name, "Layer 2: Zstandard");
    assert!(detail.algorithm.contains("zstd level 5"));
}

// ─── Brotli layer tests ───────────────────────────────────────────

#[test]
fn test_brotli_roundtrip_repeated_data() {
    let data = vec![0xCDu8; 50000];
    let compressed = BrotliLayer::compress(&data, 6).unwrap();
    let decompressed = BrotliLayer::decompress(&compressed).unwrap();
    assert_eq!(data, decompressed);
    assert!(compressed.len() < data.len() / 10);
}

#[test]
fn test_brotli_roundtrip_text_data() {
    let text = b"Brotli compression is excellent for web assets. \
                  It achieves high compression ratios with reasonable speed.";
    let compressed = BrotliLayer::compress(text, 6).unwrap();
    let decompressed = BrotliLayer::decompress(&compressed).unwrap();
    assert_eq!(text.as_slice(), decompressed.as_slice());
}

#[test]
fn test_brotli_roundtrip_empty() {
    let compressed = BrotliLayer::compress(b"", 6).unwrap();
    let decompressed = BrotliLayer::decompress(&compressed).unwrap();
    assert!(decompressed.is_empty());
}

#[test]
fn test_brotli_higher_level_better_ratio() {
    let data = vec![0u8; 50000];
    let fast = BrotliLayer::compress(&data, 1).unwrap();
    let max = BrotliLayer::compress(&data, 11).unwrap();
    assert!(max.len() <= fast.len());
}

#[test]
fn test_brotli_detail() {
    let data = b"brotli detail test";
    let (_, detail) = BrotliLayer::compress_with_detail(data, 9).unwrap();
    assert_eq!(detail.name, "Layer 3: Brotli");
    assert!(detail.algorithm.contains("brotli level 9"));
}

// ─── TripleCompressor tests ───────────────────────────────────────

#[test]
fn test_triple_roundtrip_compressible() {
    let data = vec![b'A'; 100000];
    let compressor = TripleCompressor::new();
    let (compressed, stats) = compressor.compress_triple(&data).unwrap();
    assert!(stats.compressed_size < data.len() as u64);
    assert!(stats.ratio < 0.5);
    assert_eq!(stats.layer_details.len(), 3);

    let (decompressed, _ms) = compressor.decompress_triple(&compressed).unwrap();
    assert_eq!(data, decompressed);
}

#[test]
fn test_triple_skips_incompressible() {
    let data = incompressible_bytes(10_000);
    let compressor = TripleCompressor::new();
    let (compressed, stats) = compressor.compress_triple(&data).unwrap();
    assert_eq!(stats.layer, "skipped (incompressible)");
    assert_eq!(data, compressed);
}

#[test]
fn test_triple_compression_stats() {
    let data = vec![b'X'; 50000];
    let compressor = TripleCompressor::new();
    let (_, stats) = compressor.compress_triple(&data).unwrap();
    assert_eq!(stats.original_size, 50000);
    assert!(stats.compressed_size > 0);
    assert!(!stats.blake3_hash.is_empty());
    assert!(stats.duration_ms <= 10000);
    assert_eq!(stats.layer_details.len(), 3);
}

#[test]
fn test_triple_compression_text() {
    let text = b"The quick brown fox jumps over the lazy dog. \
                  Pack my box with five dozen liquor jugs. \
                  How vexingly quick daft zebras jump! \
                  The five boxing wizards jump quickly.";
    let compressor = TripleCompressor::new();
    let (compressed, _stats) = compressor.compress_triple(text).unwrap();
    let (decompressed, _) = compressor.decompress_triple(&compressed).unwrap();
    assert_eq!(text.as_slice(), decompressed.as_slice());
}

#[test]
fn test_triple_compression_empty() {
    let compressor = TripleCompressor::new();
    let (compressed, stats) = compressor.compress_triple(b"").unwrap();
    let (decompressed, _) = compressor.decompress_triple(&compressed).unwrap();
    assert!(decompressed.is_empty());
    assert_eq!(stats.original_size, 0);
}

#[test]
fn test_compress_data_lz4() {
    let data = vec![0u8; 10000];
    let compressor = TripleCompressor::new();
    let (compressed, detail) = compressor.compress_data(&data, "lz4").unwrap();
    assert_eq!(detail.name, "Layer 1: LZ4");
    let decompressed = Lz4Layer::decompress(&compressed).unwrap();
    assert_eq!(data, decompressed);
}

#[test]
fn test_compress_data_zstd() {
    let data = vec![0u8; 10000];
    let compressor = TripleCompressor::new();
    let (compressed, detail) = compressor.compress_data(&data, "zstd").unwrap();
    assert_eq!(detail.name, "Layer 2: Zstandard");
    let decompressed = ZstdLayer::decompress(&compressed).unwrap();
    assert_eq!(data, decompressed);
}

#[test]
fn test_compress_data_brotli() {
    let data = vec![0u8; 10000];
    let compressor = TripleCompressor::new();
    let (compressed, detail) = compressor.compress_data(&data, "brotli").unwrap();
    assert_eq!(detail.name, "Layer 3: Brotli");
    let decompressed = BrotliLayer::decompress(&compressed).unwrap();
    assert_eq!(data, decompressed);
}

#[test]
fn test_compress_data_unknown_layer() {
    let compressor = TripleCompressor::new();
    let result = compressor.compress_data(b"test", "unknown");
    assert!(result.is_err());
}

#[test]
fn test_blake3_hash_deterministic() {
    let data = b"deterministic hash test";
    let h1 = TripleCompressor::blake3_hash(data);
    let h2 = TripleCompressor::blake3_hash(data);
    assert_eq!(h1, h2);
    assert!(!h1.is_empty());
}

#[test]
fn test_blake3_hash_different_data() {
    let h1 = TripleCompressor::blake3_hash(b"data1");
    let h2 = TripleCompressor::blake3_hash(b"data2");
    assert_ne!(h1, h2);
}
