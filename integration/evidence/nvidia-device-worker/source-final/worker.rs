// SPDX-License-Identifier: MPL-2.0
//! Owned, single-callback deferred execution for a future per-device NVIDIA bottom half.
//!
//! This is not the upstream FIFO kthread queue or an RM ISR implementation. A real device
//! service must inspect/drain its own hardware work on each invocation. Repeated requests
//! coalesce while pending; a request during execution guarantees another invocation.
use alloc::sync::Arc;

use ostd::{
    sync::{LocalIrqDisabled, SpinLock, WaitQueue},
    task::{Task, TaskOptions, atomic_mode::can_sleep},
};

/// A rejected operation on the native deferred worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerError {
    /// Creation and synchronous shutdown require a sleepable, real task context.
    InvalidContext,
    /// A callback cannot synchronously wait for its own worker to stop.
    SelfWait,
    /// Stop has closed admission; the request was not accepted.
    Closed,
    /// OSTD could not allocate/build the actual task and its kernel stack.
    Task(ostd::Error),
}

/// Whether this signal installed new pending work or joined an existing request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleResult {
    Queued,
    Coalesced,
}

struct State {
    pending: bool,
    closing: bool,
    stopped: bool,
}

struct Shared {
    state: SpinLock<State, LocalIrqDisabled>,
    ready: WaitQueue,
    stopped: WaitQueue,
}

impl Shared {
    fn close(&self) {
        self.state.lock().closing = true;
        self.ready.wake_one();
    }
}

/// Cloneable signaling capability. It cannot run a callback or reopen a closed worker.
///
/// `schedule` only touches IRQ-safe state and wakes the OSTD task. It does not allocate
/// a work item or execute device code inline. Normal maskable IRQ and task context are
/// supported; this spin-lock-based service is not NMI-safe. No caller may invoke it while
/// holding a scheduler/WaitQueue internal lock. It does not acknowledge a device IRQ.
#[derive(Clone)]
pub struct WorkSignal {
    shared: Arc<Shared>,
}

impl WorkSignal {
    pub fn schedule(&self) -> Result<ScheduleResult, WorkerError> {
        let result = {
            let mut state = self.shared.state.lock();
            if state.closing {
                return Err(WorkerError::Closed);
            }
            let result = if state.pending {
                ScheduleResult::Coalesced
            } else {
                ScheduleResult::Queued
            };
            state.pending = true;
            result
        };
        // Release the admission lock before entering scheduler/wait-queue machinery.
        self.shared.ready.wake_one();
        Ok(result)
    }
}

/// Exclusive lifecycle owner for one real OSTD worker task and its callback captures.
///
/// The task owns the callback, not the signal handles. Close and admission serialize on
/// one IRQ-safe lock. Shutdown drains every accepted/coalesced pending invocation and
/// drops the callback/captures before reporting completion. It does not wait for arbitrary
/// tasks spawned by that callback, nor stop DMA, mask a device, or synchronize its IRQs.
/// The device driver must do that before releasing its resources. Captures must be owned
/// (`'static`); no borrowed pre-RM resources are smuggled across their PCI reservation.
///
/// Drop closes admission without blocking, even in atomic/self-worker context. The task
/// retains its captures until the accepted work drains; use `shutdown` before releasing
/// external device state. Shutdown must not hold any lock needed by the callback or by
/// the callback's capture destructors. A callback that never returns cannot be drained.
#[must_use = "Keep the worker owner and explicitly shut it down before releasing device state"]
pub struct DeviceWorker {
    shared: Arc<Shared>,
    task: Arc<Task>,
}

impl DeviceWorker {
    /// Creates a real scheduled task. No work runs until a signal is accepted.
    pub fn start<F>(mut callback: F) -> Result<Self, WorkerError>
    where
        F: FnMut() + Send + 'static,
    {
        if !can_sleep() {
            return Err(WorkerError::InvalidContext);
        }
        let shared = Arc::new(Shared {
            state: SpinLock::new(State {
                pending: false,
                closing: false,
                stopped: false,
            }),
            ready: WaitQueue::new(),
            stopped: WaitQueue::new(),
        });
        let worker_shared = shared.clone();
        let task = TaskOptions::new(move || {
            loop {
                let run = worker_shared.ready.wait_until(|| {
                    let mut state = worker_shared.state.lock();
                    // Even after close, work accepted before close must run.
                    if state.pending {
                        state.pending = false;
                        Some(true)
                    } else if state.closing {
                        Some(false)
                    } else {
                        None
                    }
                });
                if !run {
                    break;
                }
                callback();
            }
            // Completion includes capture destruction, outside all our state locks.
            drop(callback);
            worker_shared.state.lock().stopped = true;
            worker_shared.stopped.wake_all();
        })
        .build()
        .map_err(WorkerError::Task)?;
        let task = Arc::new(task);
        task.run();
        Ok(Self { shared, task })
    }

    /// Creates a signaling handle without handing out ownership of the worker task.
    pub fn signal(&self) -> WorkSignal {
        WorkSignal {
            shared: self.shared.clone(),
        }
    }

    /// Closes admission, drains accepted work, and destroys the callback captures.
    ///
    /// Repeated shutdown is harmless. Invalid-context and self-wait rejections do not
    /// close admission. Completion proves callback quiescence, not scheduler task-exit,
    /// posted-MSI drain, device reset or successful RM/GSP initialization.
    pub fn shutdown(&mut self) -> Result<(), WorkerError> {
        if !can_sleep() {
            return Err(WorkerError::InvalidContext);
        }
        let current = Task::current().ok_or(WorkerError::InvalidContext)?;
        if core::ptr::eq(&*current, &*self.task) {
            return Err(WorkerError::SelfWait);
        }
        self.shared.close();
        self.shared
            .stopped
            .wait_until(|| self.shared.state.lock().stopped.then_some(()));
        Ok(())
    }
}

impl Drop for DeviceWorker {
    fn drop(&mut self) {
        self.shared.close();
    }
}

#[cfg(ktest)]
mod tests;
