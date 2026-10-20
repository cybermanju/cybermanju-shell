/* SPDX-License-Identifier: MIT */
/* No firmware filename table is reimplemented here. */
#include <nvUnixVersion.h>
#define NV_FIRMWARE_FOR_NAME(name) "nvidia/" NV_VERSION_STRING "/" name ".bin"
#include <nv-firmware.h>
#include <nv.h>

_Static_assert(sizeof(nv_firmware_type_t) == 4, "firmware kind ABI");
_Static_assert(sizeof(nv_firmware_chip_family_t) == 4, "firmware family ABI");
_Static_assert(NV_FIRMWARE_TYPE_GSP == 0 && NV_FIRMWARE_TYPE_GSP_LOG == 1 &&
               NV_FIRMWARE_TYPE_UCODES == 2, "non-VMware firmware kind ABI");

_Static_assert(NV_FIRMWARE_CHIP_FAMILY_TU10X == 1 &&
               NV_FIRMWARE_CHIP_FAMILY_TU11X == 2 &&
               NV_FIRMWARE_CHIP_FAMILY_GA100 == 3 &&
               NV_FIRMWARE_CHIP_FAMILY_GA10X == 4 &&
               NV_FIRMWARE_CHIP_FAMILY_AD10X == 5 &&
               NV_FIRMWARE_CHIP_FAMILY_GH100 == 6 &&
               NV_FIRMWARE_CHIP_FAMILY_GB10X == 8 &&
               NV_FIRMWARE_CHIP_FAMILY_GB20X == 9 &&
               NV_FIRMWARE_CHIP_FAMILY_GR10X == 10 &&
               NV_FIRMWARE_CHIP_FAMILY_GB10Y == 11 &&
               NV_FIRMWARE_CHIP_FAMILY_GB20Y == 12, "firmware family discriminants");

const char *aster_nv_firmware_path(NvU32 kind, NvU32 family)
{
    if (kind > NV_FIRMWARE_TYPE_UCODES || family == 0 || family == 7 ||
        family >= NV_FIRMWARE_CHIP_FAMILY_END)
        return "";
    return nv_firmware_for_chip_family((nv_firmware_type_t)kind,
                                       (nv_firmware_chip_family_t)family);
}

/* Real C-to-Rust ABI round trip used by the safe owning wrapper. The port's
 * provider deliberately does not require a Linux nv_linux_state_t. */
const void *aster_nv_firmware_acquire(NvU32 kind, NvU32 family,
                                     const void **buffer, NvU32 *size)
{
    return nv_get_firmware(NULL, (nv_firmware_type_t)kind,
                          (nv_firmware_chip_family_t)family, buffer, size);
}

void aster_nv_firmware_release(const void *handle)
{
    nv_put_firmware(handle);
}

/* Type validation against the real RM ABI declarations. */
void aster_nv_firmware_abi_probe(void)
{
    const void *(NV_API_CALL *get)(nv_state_t *, nv_firmware_type_t,
        nv_firmware_chip_family_t, const void **, NvU32 *) = &nv_get_firmware;
    void (NV_API_CALL *put)(const void *) = &nv_put_firmware;
    (void)get;
    (void)put;
}
