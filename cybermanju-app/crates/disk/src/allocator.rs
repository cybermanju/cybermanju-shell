// CyberManju OS — Allocator (crates/disk/src/allocator.rs).
//! Block allocator for one `.cybermanju` disk.
//!
//! Three jobs, all of them needed before this can be called a disk:
//!
//! * **Slots** — a free/used bitmap over `capacity_bytes / block_size`, so
//!   allocation is dense, deterministic (lowest free slot first, which is
//!   what "fill a disk to its high-water mark" means at the slot level) and
//!   never hands the same slot out twice.
//! * **Content addressing** — every chunk is named by its BLAKE3, so writing
//!   identical bytes twice stores one copy.
//! * **Refcounts** — per chunk hash, so AGENT-7's GC can tell "referenced"
//!   from "orphaned": a chunk may back several slots, and it is only garbage
//!   when the last one is released.
//!
//! The encoded form is what the `.cybermanju` container stores in its
//! block-map section, which is why [`Allocator::encode`] is a plain,
//! deterministic byte stream (slots ascending, refcounts sorted by hash).

use std::collections::{BTreeMap, HashMap};

/// Hex length of a BLAKE3 digest.
pub const HASH_HEX_LEN: usize = 64;

/// One allocated slot: which volume LBA it answers to, what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRef {
    pub lba: u64,
    pub hash: String,
    pub data_len: u32,
}

/// Free/used bitmap over `len` slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    words: Vec<u64>,
    len: usize,
}

impl Bitmap {
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0u64; len.div_ceil(64)],
            len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn bounds(&self, slot: u64) -> Option<(usize, usize)> {
        let idx = usize::try_from(slot).ok()?;
        if idx >= self.len {
            return None;
        }
        Some((idx / 64, idx % 64))
    }

    pub fn is_set(&self, slot: u64) -> bool {
        match self.bounds(slot) {
            Some((word, bit)) => self.words[word] & (1u64 << bit) != 0,
            None => false,
        }
    }

    /// Mark `slot` used. `false` when it is out of range or already taken.
    pub fn set(&mut self, slot: u64) -> bool {
        match self.bounds(slot) {
            Some((word, bit)) => {
                let mask = 1u64 << bit;
                if self.words[word] & mask != 0 {
                    return false;
                }
                self.words[word] |= mask;
                true
            }
            None => false,
        }
    }

    /// Free `slot`. `false` when it is out of range or was not set.
    pub fn clear(&mut self, slot: u64) -> bool {
        match self.bounds(slot) {
            Some((word, bit)) => {
                let mask = 1u64 << bit;
                if self.words[word] & mask == 0 {
                    return false;
                }
                self.words[word] &= !mask;
                true
            }
            None => false,
        }
    }

    /// Lowest unset slot — the high-water mark allocator fills first.
    pub fn first_clear(&self) -> Option<u64> {
        (0..self.len as u64).find(|&slot| !self.is_set(slot))
    }

    pub fn count_set(&self) -> usize {
        self.words
            .iter()
            .enumerate()
            .map(|(i, word)| {
                let mut w = *word;
                if i + 1 == self.words.len() && !self.len.is_multiple_of(64) {
                    w &= (1u64 << (self.len % 64)) - 1;
                }
                w.count_ones() as usize
            })
            .sum()
    }

    pub fn free_slots(&self) -> Vec<u64> {
        (0..self.len as u64).filter(|s| !self.is_set(*s)).collect()
    }
}

/// Slot allocation + chunk refcounts for a single disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocator {
    block_size: u32,
    block_count: usize,
    /// slot → what lives there (ordered, so the encoded map is stable).
    occupied: BTreeMap<u64, BlockRef>,
    /// chunk hash → how many slots still reference it.
    refs: HashMap<String, u32>,
}

impl Allocator {
    pub fn new(block_size: u32, block_count: usize) -> Self {
        Self {
            block_size,
            block_count,
            occupied: BTreeMap::new(),
            refs: HashMap::new(),
        }
    }

    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    pub fn block_count(&self) -> usize {
        self.block_count
    }

    pub fn used_slots(&self) -> usize {
        self.occupied.len()
    }

    pub fn free_slots(&self) -> Vec<u64> {
        let taken: std::collections::HashSet<u64> = self.occupied.keys().copied().collect();
        (0..self.block_count as u64)
            .filter(|s| !taken.contains(s))
            .collect()
    }

    pub fn is_full(&self) -> bool {
        self.occupied.len() >= self.block_count
    }

    pub fn get(&self, slot: u64) -> Option<&BlockRef> {
        self.occupied.get(&slot)
    }

    /// Every allocated slot with what it holds, in slot order.
    pub fn blocks(&self) -> impl Iterator<Item = (u64, &BlockRef)> {
        self.occupied.iter().map(|(slot, block)| (*slot, block))
    }

    /// Lowest free slot, or `None` when the disk is full.
    pub fn allocate(&self) -> Option<u64> {
        (0..self.block_count as u64).find(|s| !self.occupied.contains_key(s))
    }

    /// Record a block in `slot`. Refuses double allocation and out-of-range
    /// slots, and bumps the chunk's refcount (dedup: same bytes → one copy).
    pub fn occupy(
        &mut self,
        slot: u64,
        lba: u64,
        hash: &str,
        data_len: u32,
    ) -> Result<bool, String> {
        if slot >= self.block_count as u64 {
            return Err(format!(
                "unsupported: slot {} is beyond this disk's {} blocks",
                slot, self.block_count
            ));
        }
        if self.occupied.contains_key(&slot) {
            return Err(format!(
                "integrity: slot {} is already allocated (double allocation refused)",
                slot
            ));
        }
        if hash.len() != HASH_HEX_LEN {
            return Err(format!(
                "unsupported: chunk hash '{}' is not a BLAKE3 hex digest",
                hash
            ));
        }
        let fresh = !self.refs.contains_key(hash);
        self.occupied.insert(
            slot,
            BlockRef {
                lba,
                hash: hash.to_string(),
                data_len,
            },
        );
        let refs = self.refs.entry(hash.to_string()).or_insert(0);
        *refs = refs.saturating_add(1);
        Ok(fresh)
    }

    /// Remove a slot's block, releasing one refcount. Returns what was there.
    pub fn vacate(&mut self, slot: u64) -> Option<BlockRef> {
        let block = self.occupied.remove(&slot)?;
        if let Some(refs) = self.refs.get_mut(&block.hash) {
            *refs = refs.saturating_sub(1);
            if *refs == 0 {
                self.refs.remove(&block.hash);
            }
        }
        Some(block)
    }

    /// One more reference to `hash` — refcounts only ever climb here.
    pub fn retain(&mut self, hash: &str) -> u32 {
        let refs = self.refs.entry(hash.to_string()).or_insert(0);
        *refs = refs.saturating_add(1);
        *refs
    }

    /// One fewer reference. Saturates at 0 (never underflows) and forgets
    /// the hash entirely at zero — that is the GC signal.
    pub fn release(&mut self, hash: &str) -> u32 {
        match self.refs.get_mut(hash) {
            Some(refs) => {
                *refs = refs.saturating_sub(1);
                let now = *refs;
                if now == 0 {
                    self.refs.remove(hash);
                }
                now
            }
            None => 0,
        }
    }

    pub fn refcount(&self, hash: &str) -> u32 {
        self.refs.get(hash).copied().unwrap_or(0)
    }

    /// Hashes still referenced — the "do not sweep these" list for GC.
    pub fn referenced_hashes(&self) -> Vec<&str> {
        self.refs.keys().map(String::as_str).collect()
    }

    /// Deterministic byte stream: what the container's block-map section is.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.block_size.to_le_bytes());
        out.extend_from_slice(&(self.block_count as u32).to_le_bytes());
        out.extend_from_slice(&(self.occupied.len() as u32).to_le_bytes());
        for (slot, block) in &self.occupied {
            out.extend_from_slice(&slot.to_le_bytes());
            out.extend_from_slice(&block.lba.to_le_bytes());
            out.extend_from_slice(&block.data_len.to_le_bytes());
            let raw = hex_decode(&block.hash);
            out.extend_from_slice(&raw);
        }
        let mut hashes: Vec<&String> = self.refs.keys().collect();
        hashes.sort();
        out.extend_from_slice(&(hashes.len() as u32).to_le_bytes());
        for hash in hashes {
            out.extend_from_slice(&hex_decode(hash));
            out.extend_from_slice(&self.refs[hash].to_le_bytes());
        }
        out
    }

    /// Inverse of [`Allocator::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(bytes);
        let block_size = cursor.u32()?;
        let block_count = cursor.u32()? as usize;
        let occupied = cursor.u32()? as usize;
        let mut map = BTreeMap::new();
        let mut refs: HashMap<String, u32> = HashMap::new();
        for _ in 0..occupied {
            let slot = cursor.u64()?;
            let lba = cursor.u64()?;
            let data_len = cursor.u32()?;
            let hash = hex_encode(cursor.take(32)?);
            if slot >= block_count as u64 {
                return Err(format!(
                    "integrity: block map holds slot {} but the disk has {} blocks",
                    slot, block_count
                ));
            }
            map.insert(
                slot,
                BlockRef {
                    lba,
                    hash: hash.clone(),
                    data_len,
                },
            );
            let entry = refs.entry(hash).or_insert(0);
            *entry = entry.saturating_add(1);
        }
        let ref_rows = cursor.u32()? as usize;
        let mut declared: HashMap<String, u32> = HashMap::new();
        for _ in 0..ref_rows {
            let hash = hex_encode(cursor.take(32)?);
            let count = cursor.u32()?;
            declared.insert(hash, count);
        }
        if !cursor.is_empty() {
            return Err(format!(
                "integrity: block map has {} trailing bytes",
                cursor.remaining()
            ));
        }
        // The refcount section must agree with what the slots imply — a
        // disagreement means the file was edited by hand.
        if declared != refs {
            return Err("integrity: block map refcounts do not match its slot table".to_string());
        }
        Ok(Self {
            block_size,
            block_count,
            occupied: map,
            refs,
        })
    }

    /// Rebuild the allocator this disk's block-map rows describe.
    pub fn from_rows(
        block_size: u32,
        block_count: usize,
        rows: impl IntoIterator<Item = (u64, u64, String, u32)>,
    ) -> Result<Self, String> {
        let mut allocator = Self::new(block_size, block_count);
        for (slot, lba, hash, len) in rows {
            allocator.occupy(slot, lba, &hash, len)?;
        }
        Ok(allocator)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| "integrity: block map length overflows".to_string())?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| "integrity: block map is truncated".to_string())?;
        self.at = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("8 bytes"),
        ))
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }

    fn is_empty(&self) -> bool {
        self.at >= self.bytes.len()
    }
}

fn hex_decode(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate().take(32) {
        let hi = hex_nibble(chunk[0]);
        let lo = hex_nibble(*chunk.get(1).unwrap_or(&b'0'));
        out[i] = (hi << 4) | lo;
    }
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> String {
        format!("{:02x}{}", byte, "ab".repeat(31))
    }

    #[test]
    fn never_allocates_the_same_slot_twice() {
        let mut allocator = Allocator::new(64 * 1024, 4);
        let mut seen = Vec::new();
        for i in 0..4 {
            let slot = allocator.allocate().expect("slot");
            assert!(!seen.contains(&slot), "slot {slot} handed out twice");
            allocator
                .occupy(slot, i, &hash(i as u8), 10)
                .expect("occupy");
            seen.push(slot);
        }
        assert_eq!(seen, vec![0, 1, 2, 3], "fills low to high (high-water)");
        assert!(
            allocator.allocate().is_none(),
            "full disk hands out nothing"
        );
        assert!(allocator.is_full());
    }

    #[test]
    fn double_allocation_is_refused_even_after_a_vacate_loses_a_race() {
        let mut allocator = Allocator::new(64 * 1024, 2);
        allocator.occupy(0, 0, &hash(1), 8).expect("first");
        let err = allocator.occupy(0, 1, &hash(2), 8).expect_err("second");
        assert!(err.contains("already allocated"), "{err}");
        let err = allocator.occupy(7, 2, &hash(3), 8).expect_err("range");
        assert!(err.starts_with("unsupported:"), "{err}");
    }

    #[test]
    fn free_list_round_trips_through_the_encoded_map() {
        let mut allocator = Allocator::new(64 * 1024, 5);
        for slot in [0u64, 2, 4] {
            allocator
                .occupy(slot, slot * 10, &hash(slot as u8), 64)
                .expect("occupy");
        }
        let free = allocator.free_slots();
        assert_eq!(free, vec![1, 3]);

        let bytes = allocator.encode();
        let restored = Allocator::decode(&bytes).expect("decode");
        assert_eq!(restored, allocator, "encode → decode must be lossless");
        assert_eq!(restored.free_slots(), free, "free list survives the trip");
        assert_eq!(restored.block_count(), 5);
        assert_eq!(restored.block_size(), 64 * 1024);
    }

    #[test]
    fn refcounts_are_monotonic_and_hit_zero_exactly_once() {
        let mut allocator = Allocator::new(4096, 8);
        let h = hash(9);
        assert_eq!(allocator.retain(&h), 1);
        assert_eq!(allocator.retain(&h), 2);
        assert_eq!(allocator.retain(&h), 3);
        assert_eq!(allocator.refcount(&h), 3);
        assert_eq!(allocator.release(&h), 2);
        assert_eq!(allocator.release(&h), 1);
        assert_eq!(allocator.release(&h), 0);
        assert_eq!(allocator.release(&h), 0, "never underflows");
        assert_eq!(allocator.refcount(&h), 0, "forgotten at zero");

        // Two slots sharing one chunk: releasing one keeps it alive.
        allocator.occupy(0, 0, &h, 16).expect("a");
        allocator.occupy(1, 1, &h, 16).expect("b");
        assert_eq!(allocator.refcount(&h), 2);
        assert!(
            !allocator.occupy(2, 2, &h, 16).expect("fresh?"),
            "not fresh"
        );
        assert_eq!(allocator.refcount(&h), 3);
        allocator.vacate(0).expect("vacate");
        assert_eq!(allocator.refcount(&h), 2, "still referenced");
        allocator.vacate(1).expect("vacate");
        allocator.vacate(2).expect("vacate");
        assert_eq!(allocator.refcount(&h), 0, "orphan only after the last slot");
    }

    #[test]
    fn dedup_reports_whether_the_chunk_was_new() {
        let mut allocator = Allocator::new(4096, 8);
        let h = hash(4);
        assert!(
            allocator.occupy(0, 0, &h, 16).expect("new chunk"),
            "first write is new"
        );
        assert!(
            !allocator.occupy(1, 1, &h, 16).expect("existing chunk"),
            "second write of the same bytes dedups"
        );
        assert_eq!(allocator.refcount(&h), 2);
    }

    #[test]
    fn decode_refuses_a_map_that_lies_about_its_refcounts() {
        let mut allocator = Allocator::new(4096, 2);
        allocator.occupy(0, 0, &hash(1), 8).expect("occupy");
        let mut bytes = allocator.encode();
        // Corrupt the trailing refcount entry (last 4 bytes).
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        let err = Allocator::decode(&bytes).expect_err("tampered");
        assert!(err.starts_with("integrity:"), "{err}");
    }

    #[test]
    fn bitmap_tracks_bits_and_the_high_water_mark() {
        let mut bitmap = Bitmap::new(70);
        assert_eq!(bitmap.len(), 70);
        assert!(bitmap.set(0));
        assert!(!bitmap.set(0), "second set of the same bit reports false");
        assert!(bitmap.set(69), "last bit lives in a different word");
        assert!(!bitmap.set(70), "out of range");
        assert_eq!(bitmap.count_set(), 2);
        assert_eq!(bitmap.first_clear(), Some(1), "0 and 69 are taken");
        assert!(bitmap.clear(0));
        assert!(!bitmap.clear(0));
        assert_eq!(bitmap.first_clear(), Some(0));
        assert_eq!(bitmap.count_set(), 1);
        let free = bitmap.free_slots();
        assert_eq!(free.len(), 69, "every bit but 69 is free again");
        assert_eq!(free.last(), Some(&68), "bit 69 is still taken");
    }
}
