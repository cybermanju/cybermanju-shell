/* SPDX-License-Identifier: MIT */
/* Compile-only ABI check for NVIDIA 615.78.08's Unix nvport memory backend. */
#include <nvport/nvport.h>
#include "os-interface.h"

_Static_assert(sizeof(void *) == 8, "nvport target ABI must be x86_64");
_Static_assert(sizeof(NvU8) == 1, "NvU8 ABI width mismatch");
_Static_assert(sizeof(NvU32) == 4, "NvU32 ABI width mismatch");
_Static_assert(sizeof(NvS32) == 4, "NvS32 ABI width mismatch");
_Static_assert(sizeof(NvU64) == 8, "NvU64 ABI width mismatch");
_Static_assert(sizeof(NvLength) == sizeof(NvU64), "NvLength ABI width mismatch");
_Static_assert(sizeof(NV_STATUS) == sizeof(NvS32), "NV_STATUS ABI width mismatch");
_Static_assert(sizeof(os_page_size) == sizeof(NvU64), "os_page_size ABI width mismatch");

/* Assignments force the C compiler to check each imported symbol's exact
 * prototype and calling convention; this translation unit is syntax-only and
 * contributes neither stubs nor symbols to the archive. */
void nvidia_nvport_memory_abi_probe(void)
{
    NV_STATUS (NV_API_CALL *alloc_mem)(void **, NvU64) = &os_alloc_mem;
    void (NV_API_CALL *free_mem)(void *) = &os_free_mem;
    void *(NV_API_CALL *mem_copy)(void *, const void *, NvU32) = &os_mem_copy;
    void *(NV_API_CALL *mem_set)(void *, NvU8, NvU32) = &os_mem_set;
    NvS32 (NV_API_CALL *mem_cmp)(const NvU8 *, const NvU8 *, NvU32) = &os_mem_cmp;
    NV_STATUS (NV_API_CALL *copy_from_user)(void *, const void *, NvU32) = &os_memcpy_from_user;
    NV_STATUS (NV_API_CALL *copy_to_user)(void *, const void *, NvU32) = &os_memcpy_to_user;

    (void)alloc_mem;
    (void)free_mem;
    (void)mem_copy;
    (void)mem_set;
    (void)mem_cmp;
    (void)copy_from_user;
    (void)copy_to_user;
}
