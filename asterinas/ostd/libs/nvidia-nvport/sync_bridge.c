/* SPDX-License-Identifier: MPL-2.0 */
/*
 * Thin ABI bridge for the unmodified NVIDIA sync_unix_kernel_os.c object.
 * The bridge allocates only the upstream's opaque struct containers via the
 * Rust os_alloc_mem callback; all lock state remains in the OSTD callbacks.
 */
#include <stddef.h>

#define PORT_SYNC_IMPL
#include "nvport/nvport.h"
#include "os-interface.h"
#include "inc/sync_unix_kernel_os_def.h"

static PORT_MUTEX *mutex_new(void)
{
    PORT_MUTEX *object = NULL;
    if (os_alloc_mem((void **)&object, sizeof(*object)) != NV_OK)
        return NULL;
    if (portSyncMutexInitialize(object) != NV_OK) {
        os_free_mem(object);
        return NULL;
    }
    return object;
}

void *nvport_sync_mutex_new(void) { return mutex_new(); }
void nvport_sync_mutex_delete(void *object)
{
    if (object == NULL) return;
    portSyncMutexDestroy((PORT_MUTEX *)object);
    os_free_mem(object);
}
void nvport_sync_mutex_acquire(void *object) { portSyncMutexAcquire((PORT_MUTEX *)object); }
int nvport_sync_mutex_try_acquire(void *object)
{
    return portSyncMutexAcquireConditional((PORT_MUTEX *)object) ? 1 : 0;
}
void nvport_sync_mutex_release(void *object) { portSyncMutexRelease((PORT_MUTEX *)object); }

void *nvport_sync_semaphore_new(NvU32 count)
{
    PORT_SEMAPHORE *object = NULL;
    if (os_alloc_mem((void **)&object, sizeof(*object)) != NV_OK)
        return NULL;
    if (portSyncSemaphoreInitialize(object, count) != NV_OK) {
        os_free_mem(object);
        return NULL;
    }
    return object;
}
void nvport_sync_semaphore_delete(void *object)
{
    if (object == NULL) return;
    portSyncSemaphoreDestroy((PORT_SEMAPHORE *)object);
    os_free_mem(object);
}
void nvport_sync_semaphore_acquire(void *object) { portSyncSemaphoreAcquire((PORT_SEMAPHORE *)object); }
int nvport_sync_semaphore_try_acquire(void *object)
{
    return portSyncSemaphoreAcquireConditional((PORT_SEMAPHORE *)object) ? 1 : 0;
}
void nvport_sync_semaphore_release(void *object)
{
    if (object == NULL) return;
    /* The upstream public release routine is void and asserts callback success. */
    portSyncSemaphoreRelease((PORT_SEMAPHORE *)object);
}

/*
 * Safe Rust owners need the callback status because the upstream public
 * portSyncSemaphoreRelease ABI is void and discards it outside assertions.
 * Keep that native ABI unchanged; this helper directly reports the OS callback
 * result for Rust's checked API.
 */
NvU32 nvport_sync_semaphore_release_checked(void *object)
{
    if (object == NULL)
        return NV_ERR_INVALID_POINTER;
    return os_release_semaphore(((PORT_SEMAPHORE *)object)->sem);
}

void *nvport_sync_rwlock_new(void)
{
    PORT_RWLOCK *object = NULL;
    if (os_alloc_mem((void **)&object, sizeof(*object)) != NV_OK)
        return NULL;
    if (portSyncRwLockInitialize(object) != NV_OK) {
        os_free_mem(object);
        return NULL;
    }
    return object;
}
void nvport_sync_rwlock_delete(void *object)
{
    if (object == NULL) return;
    portSyncRwLockDestroy((PORT_RWLOCK *)object);
    os_free_mem(object);
}
void nvport_sync_rwlock_read(void *object) { portSyncRwLockAcquireRead((PORT_RWLOCK *)object); }
int nvport_sync_rwlock_try_read(void *object)
{
    return portSyncRwLockAcquireReadConditional((PORT_RWLOCK *)object) ? 1 : 0;
}
void nvport_sync_rwlock_write(void *object) { portSyncRwLockAcquireWrite((PORT_RWLOCK *)object); }
int nvport_sync_rwlock_try_write(void *object)
{
    return portSyncRwLockAcquireWriteConditional((PORT_RWLOCK *)object) ? 1 : 0;
}
void nvport_sync_rwlock_release_read(void *object) { portSyncRwLockReleaseRead((PORT_RWLOCK *)object); }
void nvport_sync_rwlock_release_write(void *object) { portSyncRwLockReleaseWrite((PORT_RWLOCK *)object); }

void *nvport_sync_spin_new(void)
{
    PORT_SPINLOCK *object = NULL;
    if (os_alloc_mem((void **)&object, sizeof(*object)) != NV_OK)
        return NULL;
    if (portSyncSpinlockInitialize(object) != NV_OK) {
        os_free_mem(object);
        return NULL;
    }
    return object;
}
void nvport_sync_spin_delete(void *object)
{
    if (object == NULL) return;
    portSyncSpinlockDestroy((PORT_SPINLOCK *)object);
    os_free_mem(object);
}
void nvport_sync_spin_acquire(void *object) { portSyncSpinlockAcquire((PORT_SPINLOCK *)object); }
void nvport_sync_spin_release(void *object) { portSyncSpinlockRelease((PORT_SPINLOCK *)object); }
