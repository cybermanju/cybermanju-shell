// SPDX-License-Identifier: MPL-2.0
//! Runtime services for the NVIDIA NVport OS-facing time and thread APIs.
//!
//! A platform component installs immutable clock callbacks after its clock
//! source is initialized. Reads before installation (or callbacks that cannot
//! produce a valid value) fail rather than manufacturing timestamps. The
//! thread helpers are available only in a real OSTD task context.
use core::sync::atomic::{AtomicBool, Ordering};

use spin::Once;
#[cfg(test)]
extern crate std;

/// A fallible source of a nanosecond clock value.
///
/// `None` means the source is not ready or cannot represent a valid value.
pub type ClockRead = fn() -> Option<u64>;

/// Immutable clock sources supplied by the platform.
#[derive(Clone, Copy)]
pub struct Clock {
    monotonic_ns: ClockRead,
    realtime_ns: ClockRead,
    monotonic_resolution_ns: ClockRead,
}

impl Clock {
    /// Creates a clock backed by platform-owned, static callback functions.
    pub const fn new(
        monotonic_ns: ClockRead,
        realtime_ns: ClockRead,
        monotonic_resolution_ns: ClockRead,
    ) -> Self {
        Self {
            monotonic_ns,
            realtime_ns,
            monotonic_resolution_ns,
        }
    }
}

/// Runtime-service errors. No error case is converted to a synthetic value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// No platform clock has been installed.
    NotInstalled,
    /// A platform clock was already installed; it cannot be replaced.
    AlreadyInstalled,
    /// The installed clock could not provide a valid value.
    ClockUnavailable,
    /// The caller is not running in an OSTD task context.
    WrongContext,
    /// This operation is not supported by this build target.
    Unsupported,
}

static CLOCK: Once<Clock> = Once::new();
static CLOCK_INSTALL_CLAIMED: AtomicBool = AtomicBool::new(false);

/// Installs the one immutable clock source for this kernel instance.
///
/// Concurrent or subsequent registration attempts return [`Error::AlreadyInstalled`];
/// the first successful registration is never silently replaced.
pub fn install_clock(clock: Clock) -> Result<(), Error> {
    CLOCK_INSTALL_CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| Error::AlreadyInstalled)?;
    // The callback set is copied by value and contains only function pointers,
    // so initialization cannot borrow caller-owned state or fail partway.
    CLOCK.call_once(|| clock);
    Ok(())
}

/// Reads monotonic nanoseconds since the platform's clock epoch.
pub fn monotonic_ns() -> Result<u64, Error> {
    (CLOCK.get().ok_or(Error::NotInstalled)?.monotonic_ns)().ok_or(Error::ClockUnavailable)
}

/// Reads realtime nanoseconds since the Unix epoch.
pub fn realtime_ns() -> Result<u64, Error> {
    (CLOCK.get().ok_or(Error::NotInstalled)?.realtime_ns)().ok_or(Error::ClockUnavailable)
}

/// Returns the clocksource's nominal minimum tick interval in nanoseconds.
pub fn monotonic_resolution_ns() -> Result<u64, Error> {
    let ns = (CLOCK
        .get()
        .ok_or(Error::NotInstalled)?
        .monotonic_resolution_ns)()
    .ok_or(Error::ClockUnavailable)?;
    if ns == 0 {
        return Err(Error::ClockUnavailable);
    }
    Ok(ns)
}

unsafe extern "C" {
    fn portTimeGetUptimeNanoseconds() -> u64;
    fn portTimeGetUptimeNanosecondsHighPrecision() -> u64;
    fn portTimeGetUptimeResolution() -> u64;
    fn portTimeGetMicroseconds() -> u64;
    fn portThreadGetCurrentThreadId() -> u64;
    fn portThreadYield();
}

/// Reads uptime through unchanged upstream C and the installed OS callback.
pub fn upstream_uptime_ns() -> Result<u64, Error> {
    monotonic_ns()?;
    // SAFETY: Clock installation was checked and this function has no pointers.
    Ok(unsafe { portTimeGetUptimeNanoseconds() })
}

/// Reads the high-precision API using the same calibrated platform clocksource.
pub fn upstream_uptime_high_precision_ns() -> Result<u64, Error> {
    monotonic_ns()?;
    // SAFETY: As above; this does not invent a separate precision domain.
    Ok(unsafe { portTimeGetUptimeNanosecondsHighPrecision() })
}

/// Returns the actual installed clocksource's nominal tick interval through C.
pub fn upstream_resolution_ns() -> Result<u64, Error> {
    monotonic_resolution_ns()?;
    // SAFETY: The callback has a nonzero, representable resolution.
    Ok(unsafe { portTimeGetUptimeResolution() })
}

/// Reads realtime through C only when its u32-seconds ABI can represent it.
pub fn upstream_realtime_us() -> Result<u64, Error> {
    let ns = realtime_ns()?;
    u32::try_from(ns / 1_000_000_000).map_err(|_| Error::ClockUnavailable)?;
    // SAFETY: The real clock is available and within the upstream ABI range.
    Ok(unsafe { portTimeGetMicroseconds() })
}

/// Gets the current live task's opaque identity through upstream C.
pub fn upstream_task_identity() -> Result<u64, Error> {
    current_task_identity()?;
    // SAFETY: Current execution is a real task, checked above.
    Ok(unsafe { portThreadGetCurrentThreadId() })
}

/// Actually yields a sleepable OSTD task through upstream C and `os_schedule`.
pub fn upstream_yield() -> Result<(), Error> {
    current_task_identity()?;
    #[cfg(target_os = "none")]
    if !ostd::task::atomic_mode::can_sleep() {
        return Err(Error::WrongContext);
    }
    // SAFETY: Real task context and sleepability checked; upstream has no args.
    unsafe { portThreadYield() };
    Ok(())
}

/// Returns an opaque identity for the currently executing OSTD task.
///
/// OSTD does not expose a numeric task ID. The stable address of the live
/// `Task` allocation is therefore used as an opaque equality token: it is
/// unique for simultaneously-live tasks and may be reused after task exit,
/// just as kernel thread IDs may be reused. It is not a POSIX TID.
#[cfg(target_os = "none")]
pub fn current_task_identity() -> Result<u64, Error> {
    use ostd::{irq::InterruptLevel, task::Task};

    if !InterruptLevel::current().is_task_context() {
        return Err(Error::WrongContext);
    }
    let task = Task::current().ok_or(Error::WrongContext)?;
    let identity = (&*task as *const Task as usize) as u64;
    if identity == 0 {
        return Err(Error::WrongContext);
    }
    Ok(identity)
}

/// Yields the currently executing task to the OSTD scheduler.
#[cfg(target_os = "none")]
pub fn yield_current_task() -> Result<(), Error> {
    use ostd::{irq::InterruptLevel, task::Task};

    if !InterruptLevel::current().is_task_context()
        || Task::current().is_none()
        || !ostd::task::atomic_mode::can_sleep()
    {
        return Err(Error::WrongContext);
    }
    Task::yield_now();
    Ok(())
}

/// Host builds do not have an OSTD scheduler or task identity.
#[cfg(not(target_os = "none"))]
pub fn current_task_identity() -> Result<u64, Error> {
    Err(Error::Unsupported)
}

/// Host builds do not have an OSTD scheduler to yield to.
#[cfg(not(target_os = "none"))]
pub fn yield_current_task() -> Result<(), Error> {
    Err(Error::Unsupported)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::OnceLock,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static MONOTONIC_START: OnceLock<Instant> = OnceLock::new();

    fn host_monotonic_ns() -> Option<u64> {
        MONOTONIC_START
            .get_or_init(Instant::now)
            .elapsed()
            .as_nanos()
            .try_into()
            .ok()
    }

    fn host_realtime_ns() -> Option<u64> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
            .try_into()
            .ok()
    }

    fn unknown_resolution() -> Option<u64> {
        // std exposes no portable clocksource resolution; do not claim one.
        None
    }

    #[test]
    fn host_clock_callbacks_use_real_clocks_and_fail_closed_for_unknown_resolution() {
        install_clock(Clock::new(
            host_monotonic_ns,
            host_realtime_ns,
            unknown_resolution,
        ))
        .unwrap();
        let first = monotonic_ns().unwrap();
        let second = monotonic_ns().unwrap();
        assert!(second >= first);
        assert!(realtime_ns().unwrap() > 1_700_000_000_000_000_000);
        assert!(upstream_uptime_ns().unwrap() >= second);
        assert!(upstream_uptime_high_precision_ns().unwrap() >= second);
        assert!(upstream_realtime_us().unwrap() > 1_700_000_000_000_000);
        assert_eq!(monotonic_resolution_ns(), Err(Error::ClockUnavailable));
        assert_eq!(upstream_resolution_ns(), Err(Error::ClockUnavailable));
        assert_eq!(current_task_identity(), Err(Error::Unsupported));
        assert_eq!(yield_current_task(), Err(Error::Unsupported));
        assert_eq!(upstream_task_identity(), Err(Error::Unsupported));
        assert_eq!(upstream_yield(), Err(Error::Unsupported));
        assert_eq!(
            install_clock(Clock::new(
                host_monotonic_ns,
                host_realtime_ns,
                unknown_resolution,
            )),
            Err(Error::AlreadyInstalled)
        );
    }
}

#[cfg(ktest)]
mod native_tests {
    use alloc::sync::Arc;
    use core::sync::atomic::AtomicU64;

    use ostd::{sync::WaitQueue, task::TaskOptions};

    use super::*;

    fn timer_ns() -> Option<u64> {
        ostd::timer::Jiffies::elapsed()
            .as_duration()
            .as_nanos()
            .try_into()
            .ok()
    }
    fn timer_resolution() -> Option<u64> {
        Some(1_000_000_000u64.div_ceil(ostd::timer::TIMER_FREQ))
    }
    fn no_wall_clock_in_ostd_only_test() -> Option<u64> {
        None
    }

    #[ostd::prelude::ktest]
    fn actual_timer_and_distinct_live_tasks_through_upstream_c() {
        assert_eq!(monotonic_ns(), Err(Error::NotInstalled));
        install_clock(Clock::new(
            timer_ns,
            no_wall_clock_in_ostd_only_test,
            timer_resolution,
        ))
        .unwrap();
        let before = upstream_uptime_ns().unwrap();
        assert!(upstream_uptime_high_precision_ns().unwrap() >= before);
        assert_eq!(
            upstream_resolution_ns(),
            timer_resolution().ok_or(Error::ClockUnavailable)
        );
        assert_eq!(upstream_realtime_us(), Err(Error::ClockUnavailable));
        let parent = upstream_task_identity().unwrap();
        let child = Arc::new(AtomicU64::new(0));
        let done = Arc::new(WaitQueue::new());
        let (child_out, child_done) = (child.clone(), done.clone());
        TaskOptions::new(move || {
            child_out.store(upstream_task_identity().unwrap(), Ordering::Release);
            child_done.wake_all();
        })
        .spawn()
        .unwrap();
        upstream_yield().unwrap();
        done.wait_until(|| (child.load(Ordering::Acquire) != 0).then_some(()));
        assert_ne!(child.load(Ordering::Acquire), parent);
        assert_eq!(upstream_task_identity(), Ok(parent));
        assert!(upstream_uptime_ns().unwrap() >= before);
        let guard = ostd::task::disable_preempt();
        assert_eq!(upstream_yield(), Err(Error::WrongContext));
        drop(guard);
        assert_eq!(upstream_yield(), Ok(()));
    }
}
