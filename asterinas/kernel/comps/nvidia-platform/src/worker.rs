// SPDX-License-Identifier: MPL-2.0
//! Owned, single-callback deferred execution for a future per-device NVIDIA bottom half.
//!
//! This is not the upstream FIFO kthread queue or an RM ISR implementation. A real device
//! service must inspect/drain its own hardware work on each invocation. Repeated requests
//! coalesce while pending; a request during execution guarantees another invocation.
//! `flush` waits for the invocation generation accepted at its snapshot, without closing
//! admission or waiting for callback-capture destruction.
use alloc::sync::Arc;

use ostd::{
    irq::IrqLine,
    sync::{LocalIrqDisabled, SpinLock, WaitQueue},
    task::{Task, TaskOptions, atomic_mode::can_sleep},
};

/// A rejected operation on the native deferred worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerError {
    /// A blocking operation requires a sleepable, real task context.
    InvalidContext,
    /// A callback cannot synchronously wait for its own worker.
    SelfWait,
    /// Stop has closed admission; the request was not accepted.
    Closed,
    /// Generation accounting would overflow; admission was closed fail-safe.
    GenerationOverflow,
    /// OSTD could not allocate/build the actual task and its kernel stack.
    Task(ostd::Error),
    /// The transactional MSI-X guard could not complete safe teardown.
    Msix(ostd::Error),
}

/// Whether this signal installed new pending work or joined an existing request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleResult {
    Queued,
    Coalesced,
}

struct State {
    pending: bool,
    accepted_generation: u64,
    completed_generation: u64,
    closing: bool,
    stopped: bool,
}

struct Shared {
    state: SpinLock<State, LocalIrqDisabled>,
    ready: WaitQueue,
    progress: WaitQueue,
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
            if state.pending {
                Ok(ScheduleResult::Coalesced)
            } else if let Some(generation) = state.accepted_generation.checked_add(1) {
                state.pending = true;
                state.accepted_generation = generation;
                Ok(ScheduleResult::Queued)
            } else {
                // Wrapping could make a flush mistake later work for its snapshot.
                state.closing = true;
                Err(WorkerError::GenerationOverflow)
            }
        };
        if result == Err(WorkerError::GenerationOverflow) {
            self.shared.ready.wake_one();
            return result;
        }
        // Release the admission lock before entering scheduler/wait-queue machinery.
        self.shared.ready.wake_one();
        result
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
                accepted_generation: 0,
                completed_generation: 0,
                closing: false,
                stopped: false,
            }),
            ready: WaitQueue::new(),
            progress: WaitQueue::new(),
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
                let mut state = worker_shared.state.lock();
                // A callback can only run for an accepted generation.
                match state.completed_generation.checked_add(1) {
                    Some(generation) if generation <= state.accepted_generation => {
                        state.completed_generation = generation;
                    }
                    _ => state.closing = true,
                }
                drop(state);
                worker_shared.progress.wake_all();
            }
            // Completion includes capture destruction, outside all our state locks.
            drop(callback);
            worker_shared.state.lock().stopped = true;
            worker_shared.progress.wake_all();
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

    /// Waits for callback work accepted in the generation snapshot taken by this call.
    ///
    /// Requests coalesced while pending account for one invocation. Work accepted after
    /// the snapshot, including while a callback is running, does not extend this wait.
    /// Admission remains open and callback captures remain alive. This is deliberately a
    /// single callback-generation snapshot, not the upstream multi-item FIFO
    /// `kthread_q_flush` or its double-barrier behavior.
    ///
    /// Requires a sleepable task context and cannot be called by the worker callback.
    /// Rejection leaves admission unchanged.
    pub fn flush(&self) -> Result<(), WorkerError> {
        if !can_sleep() {
            return Err(WorkerError::InvalidContext);
        }
        let current = Task::current().ok_or(WorkerError::InvalidContext)?;
        if core::ptr::eq(&*current, &*self.task) {
            return Err(WorkerError::SelfWait);
        }
        let snapshot = self.shared.state.lock().accepted_generation;
        self.shared.progress.wait_until(|| {
            (self.shared.state.lock().completed_generation >= snapshot).then_some(())
        });
        Ok(())
    }

    /// Closes admission, drains accepted work, and destroys the callback captures.
    ///
    /// Repeated shutdown is harmless. Invalid-context and self-wait rejections do not
    /// close admission. Completion proves callback quiescence, not scheduler task-exit,
    /// posted-MSI drain, device reset or successful RM/GSP initialization.
    pub fn shutdown(&mut self) -> Result<(), WorkerError> {
        self.validate_shutdown_context()?;
        self.shared.close();
        self.wait_stopped();
        Ok(())
    }

    /// Retires this IRQ handle's callbacks before closing and draining the worker.
    ///
    /// Before calling, the device-specific owner must mask/quiesce its interrupt source.
    /// The supplied handle must own every callback that can signal this worker; retire
    /// separately any other handles or producers. On success the slot is empty. On an
    /// invalid-context or self-wait error it is left untouched. `IrqLine::retire` is a
    /// software callback-dispatch barrier and permanently reserves the IRQ; it does not
    /// mask the device or prove cross-CPU APIC/IRTE or DMA quiescence. After callback
    /// retirement, this method drains accepted worker invocations and destroys captures.
    pub fn shutdown_after_irq(
        &mut self,
        irq_line: &mut Option<IrqLine>,
    ) -> Result<(), WorkerError> {
        self.validate_shutdown_context()?;
        if let Some(irq_line) = irq_line.take() {
            irq_line.retire();
        }
        self.shared.close();
        self.wait_stopped();
        Ok(())
    }

    pub(crate) fn validate_shutdown_context(&self) -> Result<(), WorkerError> {
        if !can_sleep() {
            return Err(WorkerError::InvalidContext);
        }
        let current = Task::current().ok_or(WorkerError::InvalidContext)?;
        if core::ptr::eq(&*current, &*self.task) {
            return Err(WorkerError::SelfWait);
        }
        Ok(())
    }

    fn wait_stopped(&self) {
        self.shared
            .progress
            .wait_until(|| self.shared.state.lock().stopped.then_some(()));
    }
}

impl Drop for DeviceWorker {
    fn drop(&mut self) {
        self.shared.close();
    }
}

/// Owns the transactional MSI-X lease and the bottom-half worker as one lifecycle.
///
/// Every registered interrupt callback receives this owner's `WorkSignal`. Shutdown
/// first masks MSI-X and runs the caller's real device-source stop protocol through
/// `PciMsixGuard::shutdown_with_post_irq`; that guard synchronously retires the IRQ
/// callbacks, then this owner closes and drains the worker before MSI-X state restoration.
/// The source callback is mandatory because MSI-X masking alone cannot stop a device source.
/// It must not claim success until the hardware source is stopped and its pending work
/// drained. This wrapper does not implement an NVIDIA-specific source mask or RM callback.
#[must_use = "Keep the IRQ/worker owner and explicitly shut it down before releasing device state"]
pub struct NvidiaInterruptWorker<'a> {
    msix: Option<aster_pci::msix_transaction::PciMsixGuard<'a>>,
    worker: DeviceWorker,
}

impl<'a> NvidiaInterruptWorker<'a> {
    /// Acquire a fresh, masked MSI-X lease and create its deferred worker.
    ///
    /// This rejects non-sleepable or non-task context before accessing the device resources.
    /// If task creation fails after MSI-X preparation, the unused unactivated guard is dropped
    /// and performs its normal rollback; no pre-existing callbacks can be adopted.
    pub fn prepare<F>(
        resources: &'a crate::device::NvidiaDeviceResources<'_>,
        vector_count: u16,
        callback: F,
    ) -> Result<Self, WorkerError>
    where
        F: FnMut() + Send + 'static,
    {
        if !can_sleep() || Task::current().is_none() {
            return Err(WorkerError::InvalidContext);
        }
        let msix = resources
            .prepare_msix(vector_count)
            .map_err(WorkerError::Msix)?;
        let worker = DeviceWorker::start(callback)?;
        Ok(Self {
            msix: Some(msix),
            worker,
        })
    }

    /// Register a handler coupled to this worker's signaling capability.
    pub fn register_handler<F>(&mut self, index: u16, callback: F) -> Result<(), ostd::Error>
    where
        F: Fn(&ostd::arch::trap::TrapFrame, &WorkSignal) + Send + Sync + 'static,
    {
        let signal = self.worker.signal();
        self.msix
            .as_mut()
            .ok_or(ostd::Error::AccessDenied)?
            .register_handler(index, move |frame| callback(frame, &signal))
    }

    /// Activate vectors only after real callbacks have been installed by the MSI-X guard.
    pub fn activate(&mut self) -> Result<(), ostd::Error> {
        self.msix
            .as_mut()
            .ok_or(ostd::Error::AccessDenied)?
            .activate()
    }

    /// Stop the device source, retire IRQ callbacks, then close and drain deferred work.
    ///
    /// The caller must provide the actual device-specific source mask/acknowledgement and
    /// producer quiescence; this component cannot infer NVIDIA register semantics before
    /// the RM device protocol is implemented. The operation requires a sleepable task
    /// context and must not hold locks needed by handlers, the worker, or the quiesce code.
    /// On MSI-X shutdown failure the worker remains open and resources remain owned for
    /// retry. After MSI-X succeeds, worker shutdown is validated in advance and is
    /// idempotent; completion still does not prove GPU reset or RM/GSP shutdown.
    pub fn shutdown(
        &mut self,
        quiesce_source: impl FnOnce() -> Result<(), ostd::Error>,
    ) -> Result<(), WorkerError> {
        self.worker.validate_shutdown_context()?;
        if let Some(mut msix) = self.msix.take() {
            let mut worker_error = None;
            let result = msix.shutdown_with_post_irq(quiesce_source, || {
                self.worker.shutdown().map_err(|error| {
                    worker_error = Some(error);
                    ostd::Error::AccessDenied
                })
            });
            if let Err(error) = result {
                self.msix = Some(msix);
                return Err(
                    worker_error.map_or(WorkerError::Msix(error), |worker_error| worker_error)
                );
            }
            // The closed guard's Drop releases its lease; activated IRQ numbers remain
            // retired sinks as required by the PCI API's delayed-message contract. The
            // callback worker has also drained before the MSI-X state was restored.
        }
        self.worker.shutdown()
    }
}

#[cfg(ktest)]
mod tests;
