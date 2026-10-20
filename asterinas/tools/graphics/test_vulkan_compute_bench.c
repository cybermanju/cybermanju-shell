#include "vulkan_compute_bench_selection.h"

#include <assert.h>
#include <stdio.h>

typedef struct {
    VkPhysicalDeviceType type;
    uint32_t vendor;
    VkDriverId driver;
    VendorFilter filter;
    bool expected;
    const char *name;
} Fixture;

int main(void) {
    const Fixture fixtures[] = {
        { VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU, 0x10de, VK_DRIVER_ID_NVIDIA_PROPRIETARY, VENDOR_NVIDIA, true, "NVIDIA discrete official driver" },
        { VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU, 0x10de, VK_DRIVER_ID_NVIDIA_PROPRIETARY, VENDOR_ANY, true, "NVIDIA integrated official driver" },
        { VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU, 0x10de, VK_DRIVER_ID_MESA_NVK, VENDOR_NVIDIA, false, "reject nouveau/NVK for NVIDIA filter" },
        { VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU, 0x10de, VK_DRIVER_ID_MESA_LLVMPIPE, VENDOR_NVIDIA, false, "reject llvmpipe ID for NVIDIA filter" },
        { VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU, 0x1002, VK_DRIVER_ID_MESA_RADV, VENDOR_AMD, true, "AMD discrete RADV" },
        { VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU, 0x8086, VK_DRIVER_ID_INTEL_OPEN_SOURCE_MESA, VENDOR_ANY, true, "Intel integrated Mesa" },
        { VK_PHYSICAL_DEVICE_TYPE_CPU, 0x10005, VK_DRIVER_ID_MESA_LLVMPIPE, VENDOR_ANY, false, "reject CPU/llvmpipe" },
        { VK_PHYSICAL_DEVICE_TYPE_VIRTUAL_GPU, 0x1002, VK_DRIVER_ID_MESA_RADV, VENDOR_ANY, false, "reject virtual GPU" },
        { VK_PHYSICAL_DEVICE_TYPE_OTHER, 0x10de, VK_DRIVER_ID_NVIDIA_PROPRIETARY, VENDOR_ANY, false, "reject OTHER device type" },
        { VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU, 0x1002, VK_DRIVER_ID_MESA_RADV, VENDOR_NVIDIA, false, "reject wrong vendor" },
    };
    for (size_t i = 0; i < sizeof(fixtures) / sizeof(fixtures[0]); ++i) {
        const Fixture *f = &fixtures[i];
        bool actual = vkb_candidate_is_eligible(f->type, f->vendor, f->driver, f->filter);
        if (actual != f->expected) {
            fprintf(stderr, "fixture failed: %s (got %d expected %d)\n", f->name, actual, f->expected);
            return 1;
        }
    }
    puts("10 synthetic device-selection fixtures passed");
    return 0;
}
