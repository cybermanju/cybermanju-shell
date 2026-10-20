//! CyberManju OS — Reed–Solomon erasure coding (AGENT-7).
//!
//! Replaces the replication-only `parity` in striped placement with real
//! `k`-data + `m`-parity shard coding, so a chunk survives `m` provider losses
//! instead of needing a full copy on each.
//!
//! [`codec`] is the whole story: `encode` splits a payload into `k + m`
//! equal-length shards, `decode` rebuilds it byte-identical from any `k` of
//! them, and the framed variants carry their own length header so an opaque
//! pipeline artifact can be sharded without the caller remembering how long
//! it was.

pub mod codec;

pub use codec::{
    check_layout, decode, decode_framed, encode, encode_framed, frame, reconstruct_shards,
    shard_len, unframe, CodecError, Encoded, FRAME_HEADER_LEN, FRAME_MAGIC, MAX_TOTAL_SHARDS,
};

/// Default number of parity shards when a config only says "parity: 1".
pub const DEFAULT_PARITY: u8 = 1;
