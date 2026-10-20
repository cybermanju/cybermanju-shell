// SPDX-License-Identifier: MPL-2.0
//! Asterinas clock callbacks for the NVIDIA NVport runtime.
//!
//! Install this only after Asterinas' time component and core system-time
//! initialization. Realtime follows the same policy as Asterinas `SystemTime`:
//! RTC calendar time at boot plus the calibrated monotonic elapsed time. If the
//! calendar value is invalid, before the Unix epoch, or overflows, realtime
//! reads fail closed. Asterinas currently falls back to a 1970 RTC dummy when
//! no RTC is found; this callback deliberately preserves that kernel policy
//! rather than pretending it is a hardware wall clock.
use core::time::Duration;

use ostd_nvidia_nvport::runtime::{self, Clock, Error};

/// Installs immutable callbacks backed by the initialized Asterinas time source.
pub(super) fn install_runtime() -> Result<(), Error> {
    runtime::install_clock(Clock::new(
        monotonic_ns,
        realtime_ns,
        monotonic_resolution_ns,
    ))
}

fn monotonic_ns() -> Option<u64> {
    duration_ns(aster_time::read_monotonic_time())
}

fn realtime_ns() -> Option<u64> {
    let base_ns = calendar_to_unix_ns(aster_time::read_start_time())?;
    base_ns.checked_add(duration_ns(aster_time::read_monotonic_time())?)
}

fn duration_ns(duration: Duration) -> Option<u64> {
    duration.as_nanos().try_into().ok()
}

fn monotonic_resolution_ns() -> Option<u64> {
    let frequency_hz = aster_time::default_clocksource().freq();
    if frequency_hz == 0 {
        return None;
    }
    // Report the nominal duration of one counter tick, rounded upward; this is
    // the clocksource's tick interval, not a claim about absolute TSC accuracy.
    Some(1_000_000_000u64.div_ceil(frequency_hz))
}

fn calendar_to_unix_ns(time: aster_time::SystemTime) -> Option<u64> {
    let year = u32::from(time.year);
    if year < 1970
        || time.hour > 23
        || time.minute > 59
        || time.second > 59
        || time.nanos >= 1_000_000_000
    {
        return None;
    }
    let month = u32::from(time.month);
    let day = u32::from(time.day);
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month)? {
        return None;
    }

    // RTC years are bounded by u16, so this loop is small and avoids a
    // dependency on a second calendar/time implementation in the driver shim.
    let mut days = 0u64;
    for prior_year in 1970..year {
        days = days.checked_add(if is_leap_year(prior_year) { 366 } else { 365 })?;
    }
    for prior_month in 1..month {
        days = days.checked_add(u64::from(days_in_month(year, prior_month)?))?;
    }
    days = days.checked_add(u64::from(day - 1))?;

    let seconds = days
        .checked_mul(86_400)?
        .checked_add(u64::from(time.hour) * 3_600)?
        .checked_add(u64::from(time.minute) * 60)?
        .checked_add(u64::from(time.second))?;
    seconds.checked_mul(1_000_000_000)?.checked_add(time.nanos)
}

fn days_in_month(year: u32, month: u32) -> Option<u32> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

const fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[cfg(any(test, ktest))]
mod tests {
    use super::*;

    fn date(year: u16, month: u8, day: u8) -> aster_time::SystemTime {
        aster_time::SystemTime {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
            second: 0,
            nanos: 0,
        }
    }

    #[cfg_attr(test, test)]
    #[cfg_attr(ktest, ostd::prelude::ktest)]
    fn converts_calendar_to_unix_epoch_and_leap_day() {
        assert_eq!(calendar_to_unix_ns(date(1970, 1, 1)), Some(0));
        assert_eq!(
            calendar_to_unix_ns(date(1970, 1, 2)),
            Some(86_400_000_000_000)
        );
        assert_eq!(
            calendar_to_unix_ns(date(2000, 3, 1)),
            Some(951_868_800_000_000_000)
        );
        assert_eq!(calendar_to_unix_ns(date(1900, 1, 1)), None);
        assert_eq!(calendar_to_unix_ns(date(2001, 2, 29)), None);
    }
}
