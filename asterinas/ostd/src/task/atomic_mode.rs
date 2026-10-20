// SPDX-License-Identifier: MPL-2.0

//! Atomic Mode
//!
//! Multitasking, while powerful, can sometimes lead to undesirable
//! or catastrophic consequences if being misused.
//! For instance, a user of OSTD might accidentally write an IRQ handler
//! that relies on mutexes,
//! which could attempt to sleep within an interrupt context---something that must be avoided.
//! Another common mistake is
//! acquiring a spinlock in a task context and then attempting to yield or sleep,
//! which can easily lead to deadlocks.
//!
//! To mitigate the risks associated with improper multitasking,
//! we introduce the concept of atomic mode.
//! Kernel code is considered to be running in atomic mode
//! if one of the following conditions is met:
//!
//! 1. Task preemption is disabled, such as when a spinlock is held.
//! 2. Local IRQs are disabled, such as during interrupt context.
//!
//! While in atomic mode,
//! any attempt to perform "sleep-like" actions will trigger a panic:
//!
//! 1. Switching to another task.
//! 2. Switching to user space.
//!
//! This module provides API to detect such "sleep-like" actions.

use core::sync::atomic::Ordering;

/// Reports whether the current execution context can use task-blocking APIs.
///
/// Unlike [`might_sleep`], this query does not panic and never grants the
/// bootstrap-context exception: a current task, enabled local interrupts and
/// enabled preemption are all required. The result describes only the current
/// context at the instant of the query, not a transferable permission token.
pub fn can_sleep() -> bool {
    let preempt_count = super::preempt::cpu_local::get_guard_count();
    let interrupts_enabled = crate::arch::irq::is_local_enabled();
    preempt_count == 0
        && interrupts_enabled
        && crate::irq::InterruptLevel::current().is_task_context()
        && super::Task::current().is_some()
}

/// Marks a function as one that might sleep.
///
/// This function will panic if it is executed in atomic mode.
#[track_caller]
pub fn might_sleep() {
    let preempt_count = super::preempt::cpu_local::get_guard_count();
    let is_local_irq_enabled = crate::arch::irq::is_local_enabled();
    if (preempt_count != 0 || !is_local_irq_enabled)
        && !crate::IN_BOOTSTRAP_CONTEXT.load(Ordering::Relaxed)
    {
        panic!(
            "This function might break atomic mode (preempt_count = {}, is_local_irq_enabled = {})",
            preempt_count, is_local_irq_enabled
        );
    }
}

/// A marker trait for guard types that enforce the atomic mode.
///
/// Key kernel primitives such as `SpinLock` and `Rcu` rely on
/// [the atomic mode](crate::task::atomic_mode) for correctness or soundness.
/// The existence of such a guard guarantees that the current task is executing
/// in the atomic mode.
///
/// It requires [`core::fmt::Debug`] by default to make it easier to derive
/// [`Debug`] for types with `&dyn InAtomicMode`.
///
/// # Safety
///
/// The implementer must ensure that the atomic mode is maintained while
/// the guard type is alive.
///
/// A guard is only valid evidence about the task that created it: it proves
/// nothing about any other task. Therefore, such guards must be `!Send` and
/// `!Sync`, so that the guards and their references are confined to the
/// creating task.
pub unsafe trait InAtomicMode: core::fmt::Debug {}

/// Abstracts any type from which one can obtain a reference to an atomic-mode guard.
pub trait AsAtomicModeGuard {
    /// Returns a guard for the atomic mode.
    fn as_atomic_mode_guard(&self) -> &dyn InAtomicMode;
}

impl<G: InAtomicMode> AsAtomicModeGuard for G {
    fn as_atomic_mode_guard(&self) -> &dyn InAtomicMode {
        self
    }
}

impl AsAtomicModeGuard for dyn InAtomicMode + '_ {
    fn as_atomic_mode_guard(&self) -> &dyn InAtomicMode {
        self
    }
}

#[cfg(ktest)]
mod tests {
    use super::can_sleep;

    #[crate::prelude::ktest]
    fn sleep_query_tracks_preemption_guards() {
        assert!(can_sleep());
        let outer = crate::task::disable_preempt();
        assert!(!can_sleep());
        let inner = crate::task::disable_preempt();
        assert!(!can_sleep());
        drop(inner);
        assert!(!can_sleep());
        drop(outer);
        assert!(can_sleep());
    }

    #[crate::prelude::ktest]
    fn sleep_query_tracks_nested_irq_guards() {
        assert!(can_sleep());
        let outer = crate::irq::disable_local();
        assert!(!can_sleep());
        let inner = crate::irq::disable_local();
        assert!(!can_sleep());
        drop(inner);
        assert!(!can_sleep());
        drop(outer);
        assert!(can_sleep());
    }
}
