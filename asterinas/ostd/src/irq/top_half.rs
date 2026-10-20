// SPDX-License-Identifier: MPL-2.0

//! The top half of interrupt handling.

use core::{fmt::Debug, ops::Deref};

use id_alloc::IdAlloc;
use spin::Once;

use crate::{
    Error,
    arch::{
        irq::{HwIrqLine, IRQ_NUM_MAX, IRQ_NUM_MIN, IrqRemapping},
        trap::TrapFrame,
    },
    prelude::*,
    sync::{RwLock, SpinLock, WriteIrqDisabled},
};

/// A type alias for the IRQ callback function.
pub type IrqCallbackFunction = dyn Fn(&TrapFrame) + Sync + Send + 'static;

/// An Interrupt ReQuest (IRQ) line.
///
/// Users can use [`alloc`] or [`alloc_specific`] to allocate a (specific) IRQ line.
///
/// The IRQ number is guaranteed to be an external IRQ number and users can use [`on_active`] to
/// safely register callback functions on this IRQ line. When the IRQ line is dropped, all the
/// registered callbacks will be unregistered automatically.
///
/// [`alloc`]: Self::alloc
/// [`alloc_specific`]: Self::alloc_specific
/// [`on_active`]: Self::on_active
#[must_use]
#[derive(Debug)]
pub struct IrqLine {
    inner: Arc<InnerHandle>,
    callbacks: Vec<CallbackHandle>,
}

impl IrqLine {
    /// Allocates an available IRQ line.
    pub fn alloc() -> Result<Self> {
        get_or_init_allocator()
            .lock()
            .alloc()
            .map(|id| Self::new(id as u8))
            .ok_or(Error::NotEnoughResources)
    }

    /// Allocates a specific IRQ line.
    pub fn alloc_specific(irq_num: u8) -> Result<Self> {
        get_or_init_allocator()
            .lock()
            .alloc_specific((irq_num - IRQ_NUM_MIN) as usize)
            .map(|id| Self::new(id as u8))
            .ok_or(Error::NotEnoughResources)
    }

    fn new(index: u8) -> Self {
        let inner = InnerHandle { index };
        inner.remapping.init(index + IRQ_NUM_MIN);

        Self {
            inner: Arc::new(inner),
            callbacks: Vec::new(),
        }
    }

    /// Gets the IRQ number.
    pub fn num(&self) -> u8 {
        self.inner.index + IRQ_NUM_MIN
    }

    /// Registers a callback that will be invoked when the IRQ is active.
    ///
    /// For each IRQ line, multiple callbacks may be registered.
    pub fn on_active<F>(&mut self, callback: F)
    where
        F: Fn(&TrapFrame) + Sync + Send + 'static,
    {
        // Arc owns a distinct allocation even for zero-sized closures. Their Box data
        // pointers, in contrast, may all be the same dangling pointer.
        let callback: Arc<IrqCallbackFunction> = Arc::new(callback);
        let callback_handle = {
            let mut callbacks = self.inner.callbacks.write();
            callbacks.push(callback.clone());

            CallbackHandle {
                irq_index: self.inner.index,
                callback,
            }
        };

        self.callbacks.push(callback_handle);
    }

    /// Checks if there are no registered callbacks.
    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }

    /// Gets the remapping index of the IRQ line.
    ///
    /// This method will return `None` if interrupt remapping is disabled or
    /// not supported by the architecture.
    pub fn remapping_index(&self) -> Option<u16> {
        self.inner.remapping.remapping_index()
    }
}

impl Clone for IrqLine {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            callbacks: Vec::new(),
        }
    }
}

impl Drop for IrqLine {
    fn drop(&mut self) {
        // Unregister callbacks before the last InnerHandle can return this IRQ number to the
        // allocator. Taking the callbacks write lock also waits for any in-flight `process`
        // readers to finish. Do not drop an IrqLine from an IRQ callback: that callback runs
        // under the read lock and would deadlock while waiting for the writer.
        self.callbacks.clear();
    }
}

struct Inner {
    callbacks: RwLock<Vec<Arc<IrqCallbackFunction>>, WriteIrqDisabled>,
    remapping: IrqRemapping,
}

impl Inner {
    const fn new() -> Self {
        Self {
            callbacks: RwLock::new(Vec::new()),
            remapping: IrqRemapping::new(),
        }
    }
}

const NUMBER_OF_IRQS: usize = (IRQ_NUM_MAX - IRQ_NUM_MIN) as usize + 1;

static INNERS: [Inner; NUMBER_OF_IRQS] = [const { Inner::new() }; NUMBER_OF_IRQS];
static ALLOCATOR: Once<SpinLock<IdAlloc>> = Once::new();

fn get_or_init_allocator() -> &'static SpinLock<IdAlloc> {
    ALLOCATOR.call_once(|| SpinLock::new(IdAlloc::with_capacity(NUMBER_OF_IRQS)))
}

/// A handle for an allocated IRQ line.
///
/// When the handle is dropped, the IRQ line will be released automatically.
#[must_use]
#[derive(Debug)]
struct InnerHandle {
    index: u8,
}

impl Deref for InnerHandle {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &INNERS[self.index as usize]
    }
}

impl Drop for InnerHandle {
    fn drop(&mut self) {
        ALLOCATOR.get().unwrap().lock().free(self.index as usize);
    }
}

/// A handle for a registered callback on an IRQ line.
///
/// When the handle is dropped, the callback will be unregistered automatically.
#[must_use]
struct CallbackHandle {
    irq_index: u8,
    callback: Arc<IrqCallbackFunction>,
}

impl Debug for CallbackHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CallbackHandle")
            .field("irq_index", &self.irq_index)
            .finish_non_exhaustive()
    }
}

impl Drop for CallbackHandle {
    fn drop(&mut self) {
        let removed = {
            // Wait for in-flight dispatch readers before removing the registration.
            let mut callbacks = INNERS[self.irq_index as usize].callbacks.write();
            let pos = callbacks
                .iter()
                .position(|element| Arc::ptr_eq(element, &self.callback))
                .unwrap();
            callbacks.swap_remove(pos)
        };
        // User-owned captures can have destructors that acquire IRQ registry locks.
        // Release both references outside the writer lock; `self.callback` is dropped
        // after this method returns, while IrqLine still owns the IRQ number.
        drop(removed);
    }
}

pub(super) fn process(trap_frame: &TrapFrame, hw_irq_line: &HwIrqLine) {
    let inner = &INNERS[(hw_irq_line.irq_num() - IRQ_NUM_MIN) as usize];
    for callback in &*inner.callbacks.read() {
        callback(trap_frame);
    }
    hw_irq_line.ack();
}

#[cfg(ktest)]
mod test {
    use core::sync::atomic::{AtomicU8, Ordering};

    use super::*;

    const IRQ_NUM: u8 = 64;
    const IRQ_INDEX: usize = (IRQ_NUM - IRQ_NUM_MIN) as usize;

    #[ktest]
    fn alloc_and_free_irq() {
        let irq_line = IrqLine::alloc_specific(IRQ_NUM).unwrap();
        assert!(IrqLine::alloc_specific(IRQ_NUM).is_err());

        let irq_line_cloned = irq_line.clone();
        assert!(IrqLine::alloc_specific(IRQ_NUM).is_err());

        drop(irq_line);
        assert!(IrqLine::alloc_specific(IRQ_NUM).is_err());

        drop(irq_line_cloned);
        assert!(IrqLine::alloc_specific(IRQ_NUM).is_ok());
    }

    #[ktest]
    fn register_and_unregister_callback() {
        let mut irq_line = IrqLine::alloc_specific(IRQ_NUM).unwrap();
        let mut irq_line_cloned = irq_line.clone();

        assert_eq!(INNERS[IRQ_INDEX].callbacks.read().len(), 0);

        irq_line.on_active(|_| {});
        assert_eq!(INNERS[IRQ_INDEX].callbacks.read().len(), 1);

        irq_line_cloned.on_active(|_| {});
        assert_eq!(INNERS[IRQ_INDEX].callbacks.read().len(), 2);

        irq_line_cloned.on_active(|_| {});
        assert_eq!(INNERS[IRQ_INDEX].callbacks.read().len(), 3);

        drop(irq_line);
        assert_eq!(INNERS[IRQ_INDEX].callbacks.read().len(), 2);

        drop(irq_line_cloned);
        assert_eq!(INNERS[IRQ_INDEX].callbacks.read().len(), 0);
    }

    #[ktest]
    fn callbacks_are_dropped_before_irq_is_released() {
        struct AllocationProbe {
            irq_num: u8,
            result: Arc<AtomicU8>,
        }

        impl Drop for AllocationProbe {
            fn drop(&mut self) {
                let allocation = IrqLine::alloc_specific(self.irq_num);
                self.result
                    .store(if allocation.is_err() { 1 } else { 2 }, Ordering::SeqCst);
                drop(allocation);
            }
        }

        let mut irq_line = IrqLine::alloc().unwrap();
        let irq_num = irq_line.num();
        let mut irq_line_cloned = irq_line.clone();
        let first_probe_result = Arc::new(AtomicU8::new(0));
        let second_probe_result = Arc::new(AtomicU8::new(0));

        let first_probe = AllocationProbe {
            irq_num,
            result: first_probe_result.clone(),
        };
        irq_line.on_active(move |_| {
            let _ = &first_probe;
        });
        let second_probe = AllocationProbe {
            irq_num,
            result: second_probe_result.clone(),
        };
        irq_line_cloned.on_active(move |_| {
            let _ = &second_probe;
        });

        drop(irq_line);
        assert_eq!(first_probe_result.load(Ordering::SeqCst), 1);
        assert!(IrqLine::alloc_specific(irq_num).is_err());

        drop(irq_line_cloned);
        assert_eq!(second_probe_result.load(Ordering::SeqCst), 1);
        assert!(IrqLine::alloc_specific(irq_num).is_ok());
    }

    #[ktest]
    fn zero_sized_callbacks_keep_registration_identity() {
        static FIRST_DROPPED: AtomicU8 = AtomicU8::new(0);
        static SECOND_DROPPED: AtomicU8 = AtomicU8::new(0);
        struct First;
        struct Second;
        impl Drop for First {
            fn drop(&mut self) {
                FIRST_DROPPED.fetch_add(1, Ordering::SeqCst);
            }
        }
        impl Drop for Second {
            fn drop(&mut self) {
                SECOND_DROPPED.fetch_add(1, Ordering::SeqCst);
            }
        }

        FIRST_DROPPED.store(0, Ordering::SeqCst);
        SECOND_DROPPED.store(0, Ordering::SeqCst);
        let mut first_line = IrqLine::alloc().unwrap();
        let irq_num = first_line.num();
        let index = (irq_num - IRQ_NUM_MIN) as usize;
        let mut second_line = first_line.clone();
        let first = First;
        let first_callback = move |_: &TrapFrame| {
            let _ = &first;
        };
        let second = Second;
        let second_callback = move |_: &TrapFrame| {
            let _ = &second;
        };
        assert_eq!(core::mem::size_of_val(&first_callback), 0);
        assert_eq!(core::mem::size_of_val(&second_callback), 0);
        first_line.on_active(first_callback);
        second_line.on_active(second_callback);
        assert_eq!(INNERS[index].callbacks.read().len(), 2);

        // Remove the second registration first: equal dangling Box pointers would
        // incorrectly remove and destroy First instead, leaving Second registered.
        drop(second_line);
        assert_eq!(FIRST_DROPPED.load(Ordering::SeqCst), 0);
        assert_eq!(SECOND_DROPPED.load(Ordering::SeqCst), 1);
        assert_eq!(INNERS[index].callbacks.read().len(), 1);
        assert!(IrqLine::alloc_specific(irq_num).is_err());
        drop(first_line);
        assert_eq!(FIRST_DROPPED.load(Ordering::SeqCst), 1);
        assert_eq!(SECOND_DROPPED.load(Ordering::SeqCst), 1);
        assert!(INNERS[index].callbacks.read().is_empty());
        assert!(IrqLine::alloc_specific(irq_num).is_ok());
    }

    #[ktest]
    fn callback_captures_drop_outside_registry_lock() {
        struct RegistryProbe {
            index: usize,
            result: Arc<AtomicU8>,
        }
        impl Drop for RegistryProbe {
            fn drop(&mut self) {
                // A non-blocking real lock acquisition exposes the old deadlock
                // without hanging the runner when executing the negative baseline.
                let unlocked = INNERS[self.index].callbacks.try_read().is_some();
                self.result.store(u8::from(unlocked), Ordering::SeqCst);
            }
        }

        let mut line = IrqLine::alloc().unwrap();
        let irq_num = line.num();
        let result = Arc::new(AtomicU8::new(2));
        let probe = RegistryProbe {
            index: (irq_num - IRQ_NUM_MIN) as usize,
            result: result.clone(),
        };
        line.on_active(move |_| {
            let _ = &probe;
        });
        drop(line);
        assert_eq!(result.load(Ordering::SeqCst), 1);
        assert!(IrqLine::alloc_specific(irq_num).is_ok());
    }
}
