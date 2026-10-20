#ifndef VULKAN_COMPUTE_BENCH_SELECTION_H
#define VULKAN_COMPUTE_BENCH_SELECTION_H

#include <vulkan/vulkan.h>
#include <stdbool.h>
#include <stdint.h>

typedef enum { VENDOR_ANY, VENDOR_NVIDIA, VENDOR_AMD } VendorFilter;

static inline bool vkb_vendor_matches(VendorFilter filter, uint32_t vendor_id) {
    if (filter == VENDOR_NVIDIA) return vendor_id == UINT32_C(0x10de);
    if (filter == VENDOR_AMD) return vendor_id == UINT32_C(0x1002);
    return true;
}

static inline bool vkb_candidate_is_eligible(VkPhysicalDeviceType type, uint32_t vendor_id,
                                              VkDriverId driver_id, VendorFilter filter) {
    bool physical_gpu = type == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU ||
                        type == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU;
    bool acceptable_nvidia_driver = vendor_id != UINT32_C(0x10de) ||
                                    driver_id == VK_DRIVER_ID_NVIDIA_PROPRIETARY;
    /* NVIDIA's open kernel module still uses NVIDIA's Vulkan user-mode driverID. */
    return physical_gpu && vkb_vendor_matches(filter, vendor_id) && acceptable_nvidia_driver;
}

#endif
