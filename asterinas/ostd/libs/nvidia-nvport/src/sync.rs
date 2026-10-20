// SPDX-License-Identifier: MPL-2.0

//! OSTD-backed implementation of NVIDIA's Unix-kernel synchronization ABI.
//!
//! The C bridge owns the opaque NVIDIA `PORT_*` layouts and calls the unmodified
//! upstream `sync_unix_kernel_os.c` entry points. This module supplies that
//! source's `os_*` synchronization callbacks and safe RAII owners for exercising
//! the real upstream entry points. Sleeping locks require task context; spinlocks
//! disable local IRQs and keep OSTD's non-migratable IRQ guard until release.

#![cfg(target_os = "none")]

use alloc::alloc::{alloc, dealloc};
use core::{
    alloc::Layout,
    cell::UnsafeCell,
    ffi::c_void,
    marker::PhantomData,
    ptr::{self, NonNull},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};

use ostd::{
    irq::{self, DisabledLocalIrqGuard},
    sync::WaitQueue,
};

const NV_OK: u32 = 0;
const NV_ERR_INVALID_POINTER: u32 = 0x3d;
const NV_ERR_INVALID_ARGUMENT: u32 = 0x1f;
const NV_ERR_INVALID_REQUEST: u32 = 0x3f;
const NV_ERR_TIMEOUT_RETRY: u32 = 0x66;
const NV_ERR_NO_MEMORY: u32 = 0x51;

struct MutexState {
    locked: AtomicBool,
    waiters: WaitQueue,
}
impl MutexState {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
            waiters: WaitQueue::new(),
        }
    }
    fn try_lock(&self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}

struct SemaphoreState {
    count: AtomicU32,
    waiters: WaitQueue,
}
impl SemaphoreState {
    const fn new(count: u32) -> Self {
        Self {
            count: AtomicU32::new(count),
            waiters: WaitQueue::new(),
        }
    }
    fn try_down(&self) -> bool {
        let mut count = self.count.load(Ordering::Acquire);
        while count != 0 {
            match self.count.compare_exchange_weak(
                count,
                count - 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => count = actual,
            }
        }
        false
    }
}

const WRITER: u32 = 1 << 31;
const READERS: u32 = WRITER - 1;
struct RwState {
    state: AtomicU32,
    waiters: WaitQueue,
}
impl RwState {
    const fn new() -> Self {
        Self {
            state: AtomicU32::new(0),
            waiters: WaitQueue::new(),
        }
    }
    fn try_read(&self) -> bool {
        let mut state = self.state.load(Ordering::Acquire);
        while state & WRITER == 0 && state & READERS != READERS {
            match self.state.compare_exchange_weak(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => state = actual,
            }
        }
        false
    }
    fn try_write(&self) -> bool {
        self.state
            .compare_exchange(0, WRITER, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}

/// Spinlock storage. The guard is created and ultimately dropped on the lock
/// owner's CPU; local IRQ disable also prevents task migration while held.
struct SpinState {
    held: AtomicBool,
    irq_guard: UnsafeCell<Option<DisabledLocalIrqGuard>>,
}
impl SpinState {
    const fn new() -> Self {
        Self {
            held: AtomicBool::new(false),
            irq_guard: UnsafeCell::new(None),
        }
    }
}
// SAFETY: `held` serializes access to `irq_guard`. The ABI requires release by
// the same non-preemptible execution context that acquired the spinlock.
unsafe impl Sync for SpinState {}

unsafe fn allocate<T>(value: T, output: *mut *mut c_void) -> u32 {
    if output.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    // Clear output before any failure path.
    unsafe { output.write(ptr::null_mut()) };
    if irq::InterruptLevel::current().is_interrupt_context() {
        return NV_ERR_INVALID_REQUEST;
    }
    let layout = Layout::new::<T>();
    // SAFETY: `layout` is a valid, non-zero Rust layout for T.
    let ptr = unsafe { alloc(layout) }.cast::<T>();
    if ptr.is_null() {
        return NV_ERR_NO_MEMORY;
    }
    // SAFETY: allocation is aligned and large enough for a T.
    unsafe { ptr.write(value) };
    // SAFETY: output is a valid writable pointer slot by contract.
    unsafe { output.write(ptr.cast()) };
    NV_OK
}

unsafe fn free<T>(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    let typed = ptr.cast::<T>();
    // SAFETY: caller passes the unique live pointer created by allocate::<T>.
    unsafe { typed.drop_in_place() };
    // SAFETY: exact same layout and pointer used for allocation.
    unsafe { dealloc(typed.cast(), Layout::new::<T>()) };
}

/// Allocate a native spinlock state.
/// # Safety
/// `out` is a writable pointer slot; the returned handle is freed exactly once
/// after all lock users have stopped.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_alloc_spinlock(out: *mut *mut c_void) -> u32 {
    // SAFETY: forwarded caller contract; initialize with no held owner.
    unsafe { allocate(SpinState::new(), out) }
}
/// # Safety
/// `lock` is the unique handle from `os_alloc_spinlock` and no lock operation
/// is in progress. If a caller has forgotten an acquired lock guard, the
/// allocation is intentionally leaked rather than dropping its CPU-local IRQ
/// guard on a potentially different CPU.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_free_spinlock(lock: *mut c_void) {
    if !lock.is_null() {
        // SAFETY: caller provides the unique handle and has stopped lock users.
        let state = unsafe { &*lock.cast::<SpinState>() };
        if state.held.load(Ordering::Acquire) {
            // `irq_guard` is !Send and its destructor restores IRQ state on the
            // acquiring CPU. Forgetting the public lock guard can outlive that
            // CPU context, so retain the whole state instead of freeing it.
            return;
        }
    }
    unsafe { free::<SpinState>(lock) }
}
/// Acquires with irq-save semantics. `oldIrql` is an opaque upstream token; the
/// matching OSTD guard, rather than a reconstructed IRQ bit, restores state.
/// # Safety
/// `lock` is live and release occurs on this same CPU/context.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_acquire_spinlock(lock: *mut c_void) -> u64 {
    if lock.is_null() {
        return 0;
    }
    // SAFETY: valid callback handle.
    let state = unsafe { &*lock.cast::<SpinState>() };
    loop {
        let irq_guard = irq::disable_local();
        if state
            .held
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // SAFETY: successful exclusive acquisition gives sole access to the slot.
            unsafe { *state.irq_guard.get() = Some(irq_guard) };
            return 0;
        }
        drop(irq_guard);
        core::hint::spin_loop();
    }
}
/// # Safety
/// `lock` is held by this context and `old_irql` is its matching token.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_release_spinlock(lock: *mut c_void, _old_irql: u64) {
    if lock.is_null() {
        return;
    }
    // SAFETY: valid callback handle and caller owns the lock.
    let state = unsafe { &*lock.cast::<SpinState>() };
    // Take the non-Send guard before releasing the state so no next owner can
    // overwrite it. Drop only after the interlock is available.
    // SAFETY: only the owner accesses this slot while held.
    let irq_guard = unsafe { (&mut *state.irq_guard.get()).take() };
    state.held.store(false, Ordering::Release);
    drop(irq_guard);
}

/// # Safety
/// `out` is writable; returned handle is freed once, after all users stop.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_alloc_mutex(out: *mut *mut c_void) -> u32 {
    unsafe { allocate(MutexState::new(), out) }
}
/// # Safety
/// `mutex` is unique and quiescent.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_free_mutex(mutex: *mut c_void) {
    unsafe { free::<MutexState>(mutex) }
}
/// # Safety
/// `mutex` is live; caller is in task context and not in OSTD atomic mode.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_acquire_mutex(mutex: *mut c_void) -> u32 {
    if mutex.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    ostd::task::atomic_mode::might_sleep();
    let lock = unsafe { &*mutex.cast::<MutexState>() };
    lock.waiters.wait_until(|| lock.try_lock().then_some(()));
    NV_OK
}
/// # Safety
/// `mutex` is live; lock acquisition is nonblocking.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_cond_acquire_mutex(mutex: *mut c_void) -> u32 {
    if mutex.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    if !ostd::task::atomic_mode::can_sleep() {
        return NV_ERR_INVALID_REQUEST;
    }
    if unsafe { &*mutex.cast::<MutexState>() }.try_lock() {
        NV_OK
    } else {
        NV_ERR_TIMEOUT_RETRY
    }
}
/// # Safety
/// The caller owns one successful acquisition of `mutex`.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_release_mutex(mutex: *mut c_void) {
    if mutex.is_null() {
        return;
    }
    let lock = unsafe { &*mutex.cast::<MutexState>() };
    lock.locked.store(false, Ordering::Release);
    lock.waiters.wake_one();
}

/// # Safety
/// Returned handle is freed once, only when there are no outstanding operations.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_alloc_semaphore(count: u32) -> *mut c_void {
    let mut out = ptr::null_mut();
    // SAFETY: local writable output slot.
    if unsafe { allocate(SemaphoreState::new(count), &mut out) } == NV_OK {
        out
    } else {
        ptr::null_mut()
    }
}
/// # Safety
/// `sem` is unique and quiescent.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_free_semaphore(sem: *mut c_void) {
    unsafe { free::<SemaphoreState>(sem) }
}
/// # Safety
/// `sem` is live; caller is in task context and not atomic mode.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_acquire_semaphore(sem: *mut c_void) -> u32 {
    if sem.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    ostd::task::atomic_mode::might_sleep();
    let sem = unsafe { &*sem.cast::<SemaphoreState>() };
    sem.waiters.wait_until(|| sem.try_down().then_some(()));
    NV_OK
}
/// # Safety
/// `sem` is live; this is a nonblocking operation.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_cond_acquire_semaphore(sem: *mut c_void) -> u32 {
    if sem.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    if unsafe { &*sem.cast::<SemaphoreState>() }.try_down() {
        NV_OK
    } else {
        NV_ERR_TIMEOUT_RETRY
    }
}
/// # Safety
/// `sem` is live and count must not overflow; caller uses NVport's release contract.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_release_semaphore(sem: *mut c_void) -> u32 {
    if sem.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    let sem = unsafe { &*sem.cast::<SemaphoreState>() };
    if sem
        .count
        .try_update(Ordering::Release, Ordering::Relaxed, |n| n.checked_add(1))
        .is_err()
    {
        return NV_ERR_INVALID_ARGUMENT;
    }
    sem.waiters.wake_one();
    NV_OK
}

/// # Safety
/// Returned handle is freed exactly once after all users stop.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_alloc_rwlock() -> *mut c_void {
    let mut out = ptr::null_mut();
    if unsafe { allocate(RwState::new(), &mut out) } == NV_OK {
        out
    } else {
        ptr::null_mut()
    }
}
/// # Safety
/// `rw` is unique and quiescent.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_free_rwlock(rw: *mut c_void) {
    unsafe { free::<RwState>(rw) }
}
/// # Safety
/// `rw` is live and caller can sleep.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_acquire_rwlock_read(rw: *mut c_void) -> u32 {
    if rw.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    ostd::task::atomic_mode::might_sleep();
    let rw = unsafe { &*rw.cast::<RwState>() };
    rw.waiters.wait_until(|| rw.try_read().then_some(()));
    NV_OK
}
/// # Safety
/// `rw` is live; nonblocking attempt.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_cond_acquire_rwlock_read(rw: *mut c_void) -> u32 {
    if rw.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    if unsafe { &*rw.cast::<RwState>() }.try_read() {
        NV_OK
    } else {
        NV_ERR_TIMEOUT_RETRY
    }
}
/// # Safety
/// `rw` is live and caller can sleep.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_acquire_rwlock_write(rw: *mut c_void) -> u32 {
    if rw.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    ostd::task::atomic_mode::might_sleep();
    let rw = unsafe { &*rw.cast::<RwState>() };
    rw.waiters.wait_until(|| rw.try_write().then_some(()));
    NV_OK
}
/// # Safety
/// `rw` is live; nonblocking attempt.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_cond_acquire_rwlock_write(rw: *mut c_void) -> u32 {
    if rw.is_null() {
        return NV_ERR_INVALID_POINTER;
    }
    if unsafe { &*rw.cast::<RwState>() }.try_write() {
        NV_OK
    } else {
        NV_ERR_TIMEOUT_RETRY
    }
}
/// # Safety
/// Caller owns one successful read acquisition.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_release_rwlock_read(rw: *mut c_void) {
    if rw.is_null() {
        return;
    }
    let rw = unsafe { &*rw.cast::<RwState>() };
    if rw.state.fetch_sub(1, Ordering::Release) == 1 {
        rw.waiters.wake_all();
    }
}
/// # Safety
/// Caller owns the successful write acquisition.
#[unsafe(no_mangle)]
unsafe extern "C" fn os_release_rwlock_write(rw: *mut c_void) {
    if rw.is_null() {
        return;
    }
    let rw = unsafe { &*rw.cast::<RwState>() };
    rw.state.store(0, Ordering::Release);
    rw.waiters.wake_all();
}

/// Reports whether the current task can safely block according to OSTD's
/// current-task, preemption, and local-IRQ checks.
#[unsafe(no_mangle)]
extern "C" fn os_semaphore_may_sleep() -> u8 {
    u8::from(ostd::task::atomic_mode::can_sleep())
}

unsafe extern "C" {
    fn nvport_sync_mutex_new() -> *mut c_void;
    fn nvport_sync_mutex_delete(mutex: *mut c_void);
    fn nvport_sync_mutex_acquire(mutex: *mut c_void);
    fn nvport_sync_mutex_try_acquire(mutex: *mut c_void) -> i32;
    fn nvport_sync_mutex_release(mutex: *mut c_void);
    fn nvport_sync_semaphore_new(count: u32) -> *mut c_void;
    fn nvport_sync_semaphore_delete(sem: *mut c_void);
    fn nvport_sync_semaphore_acquire(sem: *mut c_void);
    fn nvport_sync_semaphore_try_acquire(sem: *mut c_void) -> i32;
    fn nvport_sync_semaphore_release_checked(sem: *mut c_void) -> u32;
    fn nvport_sync_rwlock_new() -> *mut c_void;
    fn nvport_sync_rwlock_delete(lock: *mut c_void);
    fn nvport_sync_rwlock_read(lock: *mut c_void);
    fn nvport_sync_rwlock_try_read(lock: *mut c_void) -> i32;
    fn nvport_sync_rwlock_write(lock: *mut c_void);
    fn nvport_sync_rwlock_try_write(lock: *mut c_void) -> i32;
    fn nvport_sync_rwlock_release_read(lock: *mut c_void);
    fn nvport_sync_rwlock_release_write(lock: *mut c_void);
    fn nvport_sync_spin_new() -> *mut c_void;
    fn nvport_sync_spin_delete(lock: *mut c_void);
    fn nvport_sync_spin_acquire(lock: *mut c_void);
    fn nvport_sync_spin_release(lock: *mut c_void);
}

/// Error creating a C-layout NVIDIA synchronization object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitError;

/// A safe owner of an upstream NVIDIA `PORT_MUTEX`.
pub struct NvMutex(NonNull<c_void>);
// SAFETY: The handle owns a native state whose mutable operations are serialized
// by the mutex; the owner can be moved or shared while it remains uniquely owned.
unsafe impl Send for NvMutex {}
unsafe impl Sync for NvMutex {}
impl NvMutex {
    /// Create through upstream `portSyncMutexInitialize` and the OSTD callbacks.
    pub fn new() -> Result<Self, InitError> {
        NonNull::new(unsafe { nvport_sync_mutex_new() })
            .map(Self)
            .ok_or(InitError)
    }
    /// Acquire, blocking in valid task context.
    pub fn lock(&self) -> NvMutexGuard<'_> {
        unsafe { nvport_sync_mutex_acquire(self.0.as_ptr()) };
        NvMutexGuard {
            lock: self,
            _not_send: PhantomData,
        }
    }
    /// Try to acquire without sleeping.
    pub fn try_lock(&self) -> Option<NvMutexGuard<'_>> {
        (unsafe { nvport_sync_mutex_try_acquire(self.0.as_ptr()) } != 0).then(|| NvMutexGuard {
            lock: self,
            _not_send: PhantomData,
        })
    }
}
impl Drop for NvMutex {
    fn drop(&mut self) {
        unsafe { nvport_sync_mutex_delete(self.0.as_ptr()) }
    }
}
/// Guard that releases its NVport mutex on drop; cannot move between CPUs/tasks.
pub struct NvMutexGuard<'a> {
    lock: &'a NvMutex,
    _not_send: PhantomData<*mut ()>,
}
impl Drop for NvMutexGuard<'_> {
    fn drop(&mut self) {
        unsafe { nvport_sync_mutex_release(self.lock.0.as_ptr()) }
    }
}

/// A safe owner of an upstream NVIDIA `PORT_SEMAPHORE`.
pub struct NvSemaphore(NonNull<c_void>);
// SAFETY: Semaphore state is atomic and waiters use an OSTD wait queue.
unsafe impl Send for NvSemaphore {}
unsafe impl Sync for NvSemaphore {}
impl NvSemaphore {
    /// Create with the requested initial count.
    pub fn new(count: u32) -> Result<Self, InitError> {
        NonNull::new(unsafe { nvport_sync_semaphore_new(count) })
            .map(Self)
            .ok_or(InitError)
    }
    /// Acquire one permit, sleeping as required.
    pub fn acquire(&self) {
        unsafe { nvport_sync_semaphore_acquire(self.0.as_ptr()) }
    }
    /// Nonblocking permit acquisition.
    pub fn try_acquire(&self) -> bool {
        unsafe { nvport_sync_semaphore_try_acquire(self.0.as_ptr()) != 0 }
    }
    /// Add one permit, reporting counter overflow instead of silently
    /// discarding the callback's status as NVIDIA's native void ABI does.
    pub fn release(&self) -> Result<(), SemaphoreReleaseError> {
        if unsafe { nvport_sync_semaphore_release_checked(self.0.as_ptr()) } == NV_OK {
            Ok(())
        } else {
            Err(SemaphoreReleaseError)
        }
    }
}
impl Drop for NvSemaphore {
    fn drop(&mut self) {
        unsafe { nvport_sync_semaphore_delete(self.0.as_ptr()) }
    }
}

/// The semaphore's permit counter was already at its maximum value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemaphoreReleaseError;

/// A safe owner of an upstream NVIDIA `PORT_RWLOCK`.
pub struct NvRwLock(NonNull<c_void>);
// SAFETY: The native state uses atomic reader/writer ownership and an OSTD queue.
unsafe impl Send for NvRwLock {}
unsafe impl Sync for NvRwLock {}
impl NvRwLock {
    /// Construct through upstream NVport initialization.
    pub fn new() -> Result<Self, InitError> {
        NonNull::new(unsafe { nvport_sync_rwlock_new() })
            .map(Self)
            .ok_or(InitError)
    }
    /// Acquire shared access.
    pub fn read(&self) -> NvRwReadGuard<'_> {
        unsafe { nvport_sync_rwlock_read(self.0.as_ptr()) };
        NvRwReadGuard {
            lock: self,
            _not_send: PhantomData,
        }
    }
    /// Try shared access without sleeping.
    pub fn try_read(&self) -> Option<NvRwReadGuard<'_>> {
        (unsafe { nvport_sync_rwlock_try_read(self.0.as_ptr()) } != 0).then(|| NvRwReadGuard {
            lock: self,
            _not_send: PhantomData,
        })
    }
    /// Acquire exclusive access.
    pub fn write(&self) -> NvRwWriteGuard<'_> {
        unsafe { nvport_sync_rwlock_write(self.0.as_ptr()) };
        NvRwWriteGuard {
            lock: self,
            _not_send: PhantomData,
        }
    }
    /// Try exclusive access without sleeping.
    pub fn try_write(&self) -> Option<NvRwWriteGuard<'_>> {
        (unsafe { nvport_sync_rwlock_try_write(self.0.as_ptr()) } != 0).then(|| NvRwWriteGuard {
            lock: self,
            _not_send: PhantomData,
        })
    }
}
impl Drop for NvRwLock {
    fn drop(&mut self) {
        unsafe { nvport_sync_rwlock_delete(self.0.as_ptr()) }
    }
}
/// RAII shared-lock lease.
pub struct NvRwReadGuard<'a> {
    lock: &'a NvRwLock,
    _not_send: PhantomData<*mut ()>,
}
impl Drop for NvRwReadGuard<'_> {
    fn drop(&mut self) {
        unsafe { nvport_sync_rwlock_release_read(self.lock.0.as_ptr()) }
    }
}
/// RAII exclusive-lock lease.
pub struct NvRwWriteGuard<'a> {
    lock: &'a NvRwLock,
    _not_send: PhantomData<*mut ()>,
}
impl Drop for NvRwWriteGuard<'_> {
    fn drop(&mut self) {
        unsafe { nvport_sync_rwlock_release_write(self.lock.0.as_ptr()) }
    }
}

/// A safe owner of an upstream NVIDIA `PORT_SPINLOCK`.
pub struct NvSpinLock(NonNull<c_void>);
// SAFETY: The state is an irq-save spinlock; guard values are !Send, and lock
// operations synchronize access to its saved IRQ guard.
unsafe impl Send for NvSpinLock {}
// SAFETY: The IRQ-save atomic interlock serializes all access to its state.
unsafe impl Sync for NvSpinLock {}
impl NvSpinLock {
    /// Construct through the upstream spinlock initializer.
    pub fn new() -> Result<Self, InitError> {
        NonNull::new(unsafe { nvport_sync_spin_new() })
            .map(Self)
            .ok_or(InitError)
    }
    /// Acquire with local IRQ save and disable semantics.
    pub fn lock(&self) -> NvSpinLockGuard<'_> {
        unsafe { nvport_sync_spin_acquire(self.0.as_ptr()) };
        NvSpinLockGuard {
            lock: self,
            _not_send: PhantomData,
        }
    }
}
impl Drop for NvSpinLock {
    fn drop(&mut self) {
        unsafe { nvport_sync_spin_delete(self.0.as_ptr()) }
    }
}
/// RAII spinlock lease; deliberately non-Send to preserve CPU-local IRQ state.
pub struct NvSpinLockGuard<'a> {
    lock: &'a NvSpinLock,
    _not_send: PhantomData<*mut ()>,
}
impl Drop for NvSpinLockGuard<'_> {
    fn drop(&mut self) {
        unsafe { nvport_sync_spin_release(self.lock.0.as_ptr()) }
    }
}

#[cfg(ktest)]
mod tests {
    use alloc::sync::Arc;

    use ostd::task::TaskOptions;

    use super::*;

    #[ostd::prelude::ktest]
    fn upstream_mutex_contention_across_ostd_tasks() {
        let lock = Arc::new(NvMutex::new().unwrap());
        let value = Arc::new(AtomicU32::new(0));
        let completed = Arc::new(AtomicU32::new(0));
        let done = Arc::new(WaitQueue::new());

        for _ in 0..4 {
            let (lock, value, completed, done) =
                (lock.clone(), value.clone(), completed.clone(), done.clone());
            TaskOptions::new(move || {
                for _ in 0..1000 {
                    let _guard = lock.lock();
                    value.fetch_add(1, Ordering::Relaxed);
                }
                completed.fetch_add(1, Ordering::Release);
                done.wake_all();
            })
            .spawn()
            .unwrap();
        }

        done.wait_until(|| (completed.load(Ordering::Acquire) == 4).then_some(()));
        assert_eq!(value.load(Ordering::Relaxed), 4000);
    }

    #[ostd::prelude::ktest]
    fn upstream_semaphore_rwlock_and_irq_spinlock() {
        let sem = NvSemaphore::new(2).unwrap();
        assert!(sem.try_acquire());
        assert!(sem.try_acquire());
        assert!(!sem.try_acquire());
        sem.release().unwrap();
        assert!(sem.try_acquire());
        assert!(!sem.try_acquire());

        let rw = NvRwLock::new().unwrap();
        let read_a = rw.read();
        let read_b = rw.try_read().unwrap();
        assert!(rw.try_write().is_none());
        drop((read_a, read_b));
        let write = rw.write();
        assert!(rw.try_read().is_none());
        drop(write);
        assert!(rw.try_read().is_some());

        let spin = NvSpinLock::new().unwrap();
        drop(spin.lock());
    }

    #[ostd::prelude::ktest]
    fn sleep_predicate_tracks_atomic_mode_and_nested_spin_irqs() {
        assert!(ostd::task::atomic_mode::can_sleep());
        {
            let preempt = ostd::task::disable_preempt();
            assert!(!ostd::task::atomic_mode::can_sleep());
            drop(preempt);
        }
        assert!(ostd::task::atomic_mode::can_sleep());

        let spin = NvSpinLock::new().unwrap();
        {
            let outer_irq = irq::disable_local();
            assert!(!ostd::task::atomic_mode::can_sleep());
            drop(spin.lock());
            // Releasing the spinlock must preserve the caller's prior IRQ-off
            // state; the outer nested guard alone restores enabled IRQs.
            assert!(!ostd::task::atomic_mode::can_sleep());
            drop(outer_irq);
        }
        assert!(ostd::task::atomic_mode::can_sleep());
    }

    #[ostd::prelude::ktest]
    fn semaphore_overflow_is_reported_without_wrapping() {
        let sem = NvSemaphore::new(u32::MAX).unwrap();
        assert_eq!(sem.release(), Err(SemaphoreReleaseError));
        assert!(sem.try_acquire());
        sem.release().unwrap();
        assert_eq!(sem.release(), Err(SemaphoreReleaseError));
    }

    #[ostd::prelude::ktest]
    fn upstream_semaphore_and_rwlock_wake_contenders() {
        let sem = Arc::new(NvSemaphore::new(0).unwrap());
        let sem_started = Arc::new(AtomicU32::new(0));
        let sem_done = Arc::new(AtomicU32::new(0));
        let sem_wait = Arc::new(WaitQueue::new());
        {
            let (sem, started, done, wait) = (
                sem.clone(),
                sem_started.clone(),
                sem_done.clone(),
                sem_wait.clone(),
            );
            TaskOptions::new(move || {
                started.store(1, Ordering::Release);
                wait.wake_all();
                sem.acquire();
                done.store(1, Ordering::Release);
                wait.wake_all();
            })
            .spawn()
            .unwrap();
        }
        sem_wait.wait_until(|| (sem_started.load(Ordering::Acquire) != 0).then_some(()));
        sem.release().unwrap();
        sem_wait.wait_until(|| (sem_done.load(Ordering::Acquire) != 0).then_some(()));

        let rw = Arc::new(NvRwLock::new().unwrap());
        let held_writer = rw.write();
        let rw_started = Arc::new(AtomicU32::new(0));
        let rw_done = Arc::new(AtomicU32::new(0));
        let rw_wait = Arc::new(WaitQueue::new());
        {
            let (rw, started, done, wait) = (
                rw.clone(),
                rw_started.clone(),
                rw_done.clone(),
                rw_wait.clone(),
            );
            TaskOptions::new(move || {
                started.store(1, Ordering::Release);
                wait.wake_all();
                drop(rw.read());
                done.store(1, Ordering::Release);
                wait.wake_all();
            })
            .spawn()
            .unwrap();
        }
        rw_wait.wait_until(|| (rw_started.load(Ordering::Acquire) != 0).then_some(()));
        drop(held_writer);
        rw_wait.wait_until(|| (rw_done.load(Ordering::Acquire) != 0).then_some(()));
    }
}
