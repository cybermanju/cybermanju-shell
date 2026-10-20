// SPDX-License-Identifier: MPL-2.0
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ostd::{
    irq::{InterruptLevel, IrqLine},
    prelude::ktest,
};

use super::*;

struct Event {
    signaled: AtomicBool,
    queue: WaitQueue,
}

impl Event {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            signaled: AtomicBool::new(false),
            queue: WaitQueue::new(),
        })
    }
    fn signal(&self) {
        self.signaled.store(true, Ordering::Release);
        self.queue.wake_all();
    }
    fn wait(&self) {
        self.queue
            .wait_until(|| self.signaled.load(Ordering::Acquire).then_some(()));
    }
}

fn spawn_task<F>(task_fn: F)
where
    F: FnOnce() + Send + 'static,
{
    Arc::new(TaskOptions::new(task_fn).build().unwrap()).run();
}

struct DropWitness {
    drops: Arc<AtomicUsize>,
    done: Arc<Event>,
}

impl Drop for DropWitness {
    fn drop(&mut self) {
        assert!(
            can_sleep(),
            "callback captures must be destroyed by the task"
        );
        self.drops.fetch_add(1, Ordering::AcqRel);
        self.done.signal();
    }
}

#[ktest]
fn worker_coalesces_during_execution_and_shutdown_drops_captures() {
    let parent = Task::current().unwrap().cloned();
    let started = Event::new();
    let release = Event::new();
    let dropped = Event::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let witness = DropWitness {
        drops: drops.clone(),
        done: dropped.clone(),
    };
    let (worker_calls, worker_started, worker_release) =
        (calls.clone(), started.clone(), release.clone());
    let mut worker = DeviceWorker::start(move || {
        let _ = &witness;
        assert!(can_sleep());
        assert!(!core::ptr::eq(&*Task::current().unwrap(), &*parent));
        if worker_calls.fetch_add(1, Ordering::AcqRel) == 0 {
            worker_started.signal();
            worker_release.wait();
        }
    })
    .unwrap();
    let signal = worker.signal();
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    started.wait();
    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    assert_eq!(signal.clone().schedule(), Ok(ScheduleResult::Coalesced));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(drops.load(Ordering::Acquire), 0);
    release.signal();
    worker.shutdown().unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(drops.load(Ordering::Acquire), 1);
    assert!(dropped.signaled.load(Ordering::Acquire));
    assert_eq!(signal.schedule(), Err(WorkerError::Closed));
    worker.shutdown().unwrap();
    assert_eq!(drops.load(Ordering::Acquire), 1);
    ostd::early_println!(
        "NVIDIA native worker: actual task, pending coalescence, accepted-work drain and capture destruction passed; not RM/GPU"
    );
}

#[ktest]
fn worker_shutdown_rejects_atomic_context_without_closing_admission() {
    let calls = Arc::new(AtomicUsize::new(0));
    let worker_calls = calls.clone();
    let mut worker = DeviceWorker::start(move || {
        worker_calls.fetch_add(1, Ordering::AcqRel);
    })
    .unwrap();
    let signal = worker.signal();
    {
        let guard = ostd::task::disable_preempt();
        assert_eq!(worker.shutdown(), Err(WorkerError::InvalidContext));
        assert_eq!(worker.flush(), Err(WorkerError::InvalidContext));
        assert!(matches!(
            DeviceWorker::start(|| {}),
            Err(WorkerError::InvalidContext)
        ));
        drop(guard);
    }
    {
        let guard = ostd::irq::disable_local();
        assert_eq!(worker.shutdown(), Err(WorkerError::InvalidContext));
        assert_eq!(worker.flush(), Err(WorkerError::InvalidContext));
        assert!(matches!(
            DeviceWorker::start(|| {}),
            Err(WorkerError::InvalidContext)
        ));
        // Signaling itself is non-blocking and permitted with maskable IRQs off.
        assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
        drop(guard);
    }
    worker.shutdown().unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 1);
    assert_eq!(signal.schedule(), Err(WorkerError::Closed));
}

#[ktest]
fn worker_rejects_self_wait_without_closing_admission() {
    let slot = Arc::new(SpinLock::<Option<DeviceWorker>, LocalIrqDisabled>::new(
        None,
    ));
    let result = Event::new();
    let worker_slot = slot.clone();
    let worker_result = result.clone();
    let worker = DeviceWorker::start(move || {
        // Never call shutdown while holding the slot spin lock: that would only
        // exercise the atomic-context rejection rather than actual self-wait.
        let mut owner = worker_slot.lock().take().unwrap();
        assert_eq!(owner.shutdown(), Err(WorkerError::SelfWait));
        assert_eq!(owner.flush(), Err(WorkerError::SelfWait));
        assert!(!owner.shared.state.lock().closing);
        *worker_slot.lock() = Some(owner);
        worker_result.signal();
    })
    .unwrap();
    let signal = worker.signal();
    *slot.lock() = Some(worker);
    signal.schedule().unwrap();
    result.wait();
    let mut owner = slot.lock().take().unwrap();
    owner.shutdown().unwrap();
    assert_eq!(signal.schedule(), Err(WorkerError::Closed));
}

#[ktest]
fn dropping_owner_closes_without_waiting_and_task_still_drains() {
    let started = Event::new();
    let release = Event::new();
    let dropped = Event::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let witness = DropWitness {
        drops: drops.clone(),
        done: dropped.clone(),
    };
    let (worker_calls, worker_started, worker_release) =
        (calls.clone(), started.clone(), release.clone());
    let worker = DeviceWorker::start(move || {
        let _ = &witness;
        if worker_calls.fetch_add(1, Ordering::AcqRel) == 0 {
            worker_started.signal();
            worker_release.wait();
        }
    })
    .unwrap();
    let signal = worker.signal();
    signal.schedule().unwrap();
    started.wait();
    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    // Drop cannot block behind a callback that this same task must release.
    let guard = ostd::irq::disable_local();
    drop(worker);
    drop(guard);
    assert_eq!(signal.schedule(), Err(WorkerError::Closed));
    assert_eq!(drops.load(Ordering::Acquire), 0);
    release.signal();
    dropped.wait();
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[ktest]
fn callback_can_rearm_itself_without_inline_execution_or_lost_request() {
    let slot = Arc::new(SpinLock::<Option<WorkSignal>, LocalIrqDisabled>::new(None));
    let calls = Arc::new(AtomicUsize::new(0));
    let twice = Event::new();
    let (worker_slot, worker_calls, worker_twice) = (slot.clone(), calls.clone(), twice.clone());
    let mut worker = DeviceWorker::start(move || {
        let invocation = worker_calls.fetch_add(1, Ordering::AcqRel);
        if invocation == 0 {
            let signal = worker_slot.lock().take().unwrap();
            assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
            assert_eq!(signal.schedule(), Ok(ScheduleResult::Coalesced));
            assert_eq!(worker_calls.load(Ordering::Acquire), 1);
        } else {
            assert_eq!(invocation, 1);
            worker_twice.signal();
        }
    })
    .unwrap();
    let signal = worker.signal();
    *slot.lock() = Some(signal.clone());
    signal.schedule().unwrap();
    twice.wait();
    worker.shutdown().unwrap();
    assert_eq!(calls.load(Ordering::Acquire), 2);
}

#[ktest]
fn actual_timer_irq_signals_worker_that_runs_in_sleepable_task() {
    let completed = Event::new();
    let irq_observed = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let (worker_completed, worker_calls) = (completed.clone(), calls.clone());
    let mut worker = DeviceWorker::start(move || {
        assert!(can_sleep());
        assert!(InterruptLevel::current().is_task_context());
        worker_calls.fetch_add(1, Ordering::AcqRel);
        worker_completed.signal();
    })
    .unwrap();
    let slot = Arc::new(SpinLock::<Option<WorkSignal>, LocalIrqDisabled>::new(Some(
        worker.signal(),
    )));
    let timer_observed = irq_observed.clone();
    // OSTD has no unregister API. The callback becomes inert after taking its
    // one signal; the retained empty slot contains no worker or device resource.
    ostd::timer::register_callback_on_cpu(move || {
        let signal = slot.lock().take();
        if let Some(signal) = signal {
            assert!(InterruptLevel::current().is_interrupt_context());
            assert!(!can_sleep());
            timer_observed.store(true, Ordering::Release);
            assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
        }
    });
    completed.wait();
    worker.shutdown().unwrap();
    assert!(irq_observed.load(Ordering::Acquire));
    assert_eq!(calls.load(Ordering::Acquire), 1);
    ostd::early_println!(
        "NVIDIA native worker: actual APIC timer IRQ to sleepable task handoff passed; not NVIDIA IRQ or GPU"
    );
}

#[ktest]
fn worker_flush_waits_for_snapshot_and_preserves_later_work_and_captures() {
    let first_started = Event::new();
    let first_release = Event::new();
    let second_started = Event::new();
    let second_release = Event::new();
    let flush_entered = Event::new();
    let flush_returned = Event::new();
    let third_completed = Event::new();
    let captures_dropped = Event::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let witness = DropWitness {
        drops: drops.clone(),
        done: captures_dropped.clone(),
    };
    let (callback_calls, callback_first_started, callback_first_release) =
        (calls.clone(), first_started.clone(), first_release.clone());
    let (callback_second_started, callback_second_release, callback_third_completed) = (
        second_started.clone(),
        second_release.clone(),
        third_completed.clone(),
    );
    let worker = DeviceWorker::start(move || {
        let _ = &witness;
        match callback_calls.fetch_add(1, Ordering::AcqRel) {
            0 => {
                callback_first_started.signal();
                callback_first_release.wait();
            }
            1 => {
                callback_second_started.signal();
                callback_second_release.wait();
            }
            2 => callback_third_completed.signal(),
            n => panic!("unexpected worker invocation {n}"),
        }
    })
    .unwrap();
    let worker = Arc::new(worker);
    let signal = worker.signal();

    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    first_started.wait();
    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    assert_eq!(signal.schedule(), Ok(ScheduleResult::Coalesced));

    let flushing_worker = worker.clone();
    let (task_flush_entered, task_flush_returned) = (flush_entered.clone(), flush_returned.clone());
    spawn_task(move || {
        task_flush_entered.signal();
        assert_eq!(flushing_worker.flush(), Ok(()));
        task_flush_returned.signal();
    });
    flush_entered.wait();
    first_release.signal();
    second_started.wait();
    assert!(
        !flush_returned.signaled.load(Ordering::Acquire),
        "flush returned before the pending snapshot invocation completed"
    );
    second_release.signal();
    flush_returned.wait();
    assert_eq!(calls.load(Ordering::Acquire), 2);
    assert_eq!(drops.load(Ordering::Acquire), 0, "flush destroyed captures");

    // A completed flush is a point-in-time barrier, not a close: later requests are
    // still admitted and the callback remains owned until shutdown/Drop.
    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    third_completed.wait();
    assert_eq!(calls.load(Ordering::Acquire), 3);
    assert_eq!(drops.load(Ordering::Acquire), 0);
    drop(worker);
    captures_dropped.wait();
    assert_eq!(drops.load(Ordering::Acquire), 1);
}

#[ktest]
fn generation_overflow_closes_admission_without_wrapping() {
    let mut worker = DeviceWorker::start(|| {}).unwrap();
    let signal = worker.signal();
    {
        let mut state = worker.shared.state.lock();
        state.accepted_generation = u64::MAX;
        state.completed_generation = u64::MAX;
    }
    assert_eq!(signal.schedule(), Err(WorkerError::GenerationOverflow));
    assert_eq!(signal.schedule(), Err(WorkerError::Closed));
    worker.shutdown().unwrap();
}

#[ktest]
fn irq_callbacks_retire_before_worker_drain_and_capture_release() {
    let callback_started = Event::new();
    let callback_release = Event::new();
    let worker_captures_dropped = Event::new();
    let irq_captures_dropped = Event::new();
    let shutdown_returned = Event::new();
    let worker_calls = Arc::new(AtomicUsize::new(0));
    let worker_drops = Arc::new(AtomicUsize::new(0));
    let irq_drops = Arc::new(AtomicUsize::new(0));
    let worker_witness = DropWitness {
        drops: worker_drops.clone(),
        done: worker_captures_dropped.clone(),
    };
    let (worker_started, worker_release) = (callback_started.clone(), callback_release.clone());
    let worker_calls_in_callback = worker_calls.clone();
    let mut worker = DeviceWorker::start(move || {
        let _ = &worker_witness;
        worker_started.signal();
        worker_release.wait();
        worker_calls_in_callback.fetch_add(1, Ordering::AcqRel);
    })
    .unwrap();

    let signal = worker.signal();
    let mut irq_line = Some(IrqLine::alloc().unwrap());
    let irq_witness = DropWitness {
        drops: irq_drops.clone(),
        done: irq_captures_dropped.clone(),
    };
    let irq_signal = signal.clone();
    irq_line.as_mut().unwrap().on_active(move |_| {
        let _ = (&irq_signal, &irq_witness);
    });

    assert_eq!(signal.schedule(), Ok(ScheduleResult::Queued));
    callback_started.wait();
    {
        let guard = ostd::task::disable_preempt();
        assert_eq!(
            worker.shutdown_after_irq(&mut irq_line),
            Err(WorkerError::InvalidContext)
        );
        assert!(
            irq_line.is_some(),
            "rejected shutdown consumed the IRQ owner"
        );
        assert!(!irq_captures_dropped.signaled.load(Ordering::Acquire));
        drop(guard);
    }

    let shutdown_returned_task = shutdown_returned.clone();
    spawn_task(move || {
        worker.shutdown_after_irq(&mut irq_line).unwrap();
        assert!(irq_line.is_none());
        shutdown_returned_task.signal();
    });

    // `IrqLine::retire` synchronously drops this handle's callback captures before the
    // worker shutdown can wait for the currently accepted invocation to finish.
    irq_captures_dropped.wait();
    assert_eq!(irq_drops.load(Ordering::Acquire), 1);
    assert!(!shutdown_returned.signaled.load(Ordering::Acquire));
    assert_eq!(worker_calls.load(Ordering::Acquire), 0);
    callback_release.signal();
    shutdown_returned.wait();

    assert_eq!(worker_calls.load(Ordering::Acquire), 1);
    assert_eq!(worker_drops.load(Ordering::Acquire), 1);
    assert!(worker_captures_dropped.signaled.load(Ordering::Acquire));
    assert_eq!(signal.schedule(), Err(WorkerError::Closed));
    ostd::early_println!(
        "NVIDIA native worker: IRQ callback retirement preceded accepted-work drain and capture release; not GPU IRQ/RM"
    );
}
