// SPDX-License-Identifier: MPL-2.0
//! Pure placement constraints for a direct x86-64 PC32/PLT32 relocation.
//!
//! This models only `S + A - P`; it does not model a real PLT, scan ELF
//! relocations, or prove that all relocations are reachable. The returned
//! interval does not guarantee canonical x86 addresses, VA reservation, mmap,
//! W^X, or validity of remaining relocations.

/// Inclusive module-base interval satisfying one direct PC32/PLT32 relocation.
///
/// Fields are private so callers can only create windows through checked
/// operations. The interval ensures `base + module_span` is representable;
/// it does not reserve or map that address range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relative32BaseWindow {
    min_base: u64,
    max_base: u64,
}

impl Relative32BaseWindow {
    #[cfg(test)]
    pub(crate) const fn test_window(min_base: u64, max_base: u64) -> Self {
        Self { min_base, max_base }
    }

    /// Lowest allowed base, inclusive.
    pub const fn min_base(self) -> u64 {
        self.min_base
    }

    /// Highest allowed base, inclusive.
    pub const fn max_base(self) -> u64 {
        self.max_base
    }

    /// Whether `base` is in this inclusive interval.
    pub const fn contains(self, base: u64) -> bool {
        self.min_base <= base && base <= self.max_base
    }

    /// Intersect two inclusive intervals.
    pub const fn intersect(self, other: Self) -> Option<Self> {
        let min_base = if self.min_base > other.min_base {
            self.min_base
        } else {
            other.min_base
        };
        let max_base = if self.max_base < other.max_base {
            self.max_base
        } else {
            other.max_base
        };
        if min_base <= max_base {
            Some(Self { min_base, max_base })
        } else {
            None
        }
    }

    /// Restrict this interval to another inclusive interval.
    pub const fn restrict(self, min: u64, max: u64) -> Option<Self> {
        if min > max {
            return None;
        }
        self.intersect(Self {
            min_base: min,
            max_base: max,
        })
    }

    /// Return the first base in this window aligned to a nonzero power of two.
    pub const fn first_aligned(self, alignment: u64) -> Option<u64> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let mask = alignment - 1;
        let Some(adjusted) = self.min_base.checked_add(mask) else {
            return None;
        };
        let aligned = adjusted & !mask;
        if aligned <= self.max_base {
            Some(aligned)
        } else {
            None
        }
    }
}

/// Compute bases for which the direct displacement `S + A - (base + place_offset)` fits
/// in a signed 32-bit value, while `base + module_span` remains representable.
///
/// `place_offset` is the offset of the relocation field within the module.
pub fn relative32_base_window(
    symbol: u64,
    addend: i64,
    place_offset: u64,
    module_span: u64,
) -> Option<Relative32BaseWindow> {
    if module_span == 0 || place_offset.checked_add(4)? > module_span {
        return None;
    }

    let center = i128::from(symbol) + i128::from(addend) - i128::from(place_offset);
    let low = center - i128::from(i32::MAX);
    let high = center - i128::from(i32::MIN);
    let min_base = low.max(0);
    let max_base = high.min(i128::from(u64::MAX - module_span));
    if min_base > max_base {
        return None;
    }
    Some(Relative32BaseWindow {
        min_base: u64::try_from(min_base).ok()?,
        max_base: u64::try_from(max_base).ok()?,
    })
}
