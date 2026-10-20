#define _POSIX_C_SOURCE 200809L
#include <vulkan/vulkan.h>
#include "vulkan_compute_bench_selection.h"

#include <errno.h>
#include <inttypes.h>
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <sys/utsname.h>

#define DEFAULT_COUNT UINT64_C(1048576)
#define DEFAULT_ITERATIONS UINT64_C(10)
#define MAX_COUNT UINT64_C(16777216)
#define MAX_TOTAL_INVOCATIONS UINT64_C(100000000)
#define MAX_ITERATIONS UINT64_C(100)
#define FENCE_TIMEOUT_NS UINT64_C(30000000000)
#define LOCAL_SIZE 256u

typedef struct {
    const char *shader_path;
    VendorFilter vendor;
    uint64_t count;
    uint64_t iterations;
} Options;

typedef struct {
    VkPhysicalDevice physical;
    VkPhysicalDeviceProperties properties;
    VkPhysicalDeviceDriverProperties driver;
    uint32_t queue_family;
    uint32_t timestamp_bits;
} SelectedDevice;

static void usage(FILE *stream, const char *program) {
    fprintf(stream,
            "Usage: %s --shader PATH [--vendor nvidia|amd|any] [--count N] [--iterations N]\n"
            "Runs a bounded Vulkan compute correctness/performance test.\n",
            program);
}

static bool parse_u64(const char *text, uint64_t *value) {
    char *end = NULL;
    unsigned long long parsed;
    if (text == NULL || text[0] == '\0' || text[0] == '-') return false;
    errno = 0;
    parsed = strtoull(text, &end, 10);
    if (errno != 0 || end == text || *end != '\0') return false;
    *value = (uint64_t)parsed;
    return true;
}

static bool parse_options(int argc, char **argv, Options *options) {
    options->shader_path = NULL;
    options->vendor = VENDOR_ANY;
    options->count = DEFAULT_COUNT;
    options->iterations = DEFAULT_ITERATIONS;
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            usage(stdout, argv[0]);
            return false;
        } else if (strcmp(argv[i], "--shader") == 0 && i + 1 < argc) {
            options->shader_path = argv[++i];
        } else if (strcmp(argv[i], "--vendor") == 0 && i + 1 < argc) {
            const char *vendor = argv[++i];
            if (strcmp(vendor, "any") == 0) options->vendor = VENDOR_ANY;
            else if (strcmp(vendor, "nvidia") == 0) options->vendor = VENDOR_NVIDIA;
            else if (strcmp(vendor, "amd") == 0) options->vendor = VENDOR_AMD;
            else {
                fprintf(stderr, "Unsupported vendor filter: %s\n", vendor);
                return false;
            }
        } else if (strcmp(argv[i], "--count") == 0 && i + 1 < argc) {
            if (!parse_u64(argv[++i], &options->count)) {
                fprintf(stderr, "Invalid --count value.\n");
                return false;
            }
        } else if (strcmp(argv[i], "--iterations") == 0 && i + 1 < argc) {
            if (!parse_u64(argv[++i], &options->iterations)) {
                fprintf(stderr, "Invalid --iterations value.\n");
                return false;
            }
        } else {
            fprintf(stderr, "Unknown or incomplete argument: %s\n", argv[i]);
            usage(stderr, argv[0]);
            return false;
        }
    }
    if (options->shader_path == NULL || options->shader_path[0] == '\0') {
        fprintf(stderr, "--shader PATH is required.\n");
        return false;
    }
    if (options->count == 0 || options->count > MAX_COUNT || options->iterations == 0 ||
        options->iterations > MAX_ITERATIONS || options->count > MAX_TOTAL_INVOCATIONS / options->iterations) {
        fprintf(stderr,
                "Work bounds exceeded: count must be 1..%" PRIu64 ", iterations 1..%" PRIu64
                ", and count*iterations <= %" PRIu64 ".\n",
                MAX_COUNT, MAX_ITERATIONS, MAX_TOTAL_INVOCATIONS);
        return false;
    }
    return true;
}

static const char *vendor_label(uint32_t vendor_id) {
    if (vendor_id == UINT32_C(0x10de)) return "NVIDIA";
    if (vendor_id == UINT32_C(0x1002)) return "AMD";
    if (vendor_id == UINT32_C(0x8086)) return "Intel";
    return "unknown";
}

static bool select_device(VkInstance instance, VendorFilter filter, SelectedDevice *selected) {
    uint32_t count = 0;
    VkResult result = vkEnumeratePhysicalDevices(instance, &count, NULL);
    if (result != VK_SUCCESS || count == 0) {
        fprintf(stderr, "No Vulkan physical devices were enumerated (VkResult %d).\n", result);
        return false;
    }
    VkPhysicalDevice *devices = calloc(count, sizeof(*devices));
    if (devices == NULL) {
        fprintf(stderr, "Could not allocate physical-device list.\n");
        return false;
    }
    result = vkEnumeratePhysicalDevices(instance, &count, devices);
    if (result != VK_SUCCESS) {
        fprintf(stderr, "vkEnumeratePhysicalDevices failed (VkResult %d).\n", result);
        free(devices);
        return false;
    }

    bool found = false;
    bool saw_matching_vendor = false;
    bool saw_bad_nvidia_driver = false;
    for (uint32_t i = 0; i < count && !found; ++i) {
        VkPhysicalDeviceProperties2 properties = { .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2 };
        VkPhysicalDeviceDriverProperties driver = { .sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES };
        properties.pNext = &driver;
        vkGetPhysicalDeviceProperties2(devices[i], &properties);
        VkPhysicalDeviceProperties p = properties.properties;
        if (!vkb_vendor_matches(filter, p.vendorID)) continue;
        saw_matching_vendor = true;
        if (!vkb_candidate_is_eligible(p.deviceType, p.vendorID, driver.driverID, filter)) {
            if (p.vendorID == UINT32_C(0x10de) &&
                driver.driverID != VK_DRIVER_ID_NVIDIA_PROPRIETARY) saw_bad_nvidia_driver = true;
            continue;
        }
        if (p.apiVersion < VK_API_VERSION_1_2) continue;
        if (!p.limits.timestampComputeAndGraphics) continue;

        uint32_t family_count = 0;
        vkGetPhysicalDeviceQueueFamilyProperties(devices[i], &family_count, NULL);
        VkQueueFamilyProperties *families = calloc(family_count, sizeof(*families));
        if (families == NULL) continue;
        vkGetPhysicalDeviceQueueFamilyProperties(devices[i], &family_count, families);
        for (uint32_t q = 0; q < family_count; ++q) {
            if ((families[q].queueFlags & VK_QUEUE_COMPUTE_BIT) != 0 && families[q].timestampValidBits > 0) {
                selected->physical = devices[i];
                selected->properties = p;
                selected->driver = driver;
                selected->queue_family = q;
                selected->timestamp_bits = families[q].timestampValidBits;
                found = true;
                break;
            }
        }
        free(families);
    }
    free(devices);
    if (!found) {
        if (filter == VENDOR_NVIDIA && saw_bad_nvidia_driver) {
            fprintf(stderr, "NVIDIA GPUs were found, but none reported VK_DRIVER_ID_NVIDIA_PROPRIETARY (also used with NVIDIA open kernel modules).\n");
        } else if (!saw_matching_vendor) {
            fprintf(stderr, "No GPU matched the requested vendor filter.\n");
        } else {
            fprintf(stderr, "No matching discrete/integrated GPU with Vulkan 1.2 compute timestamps was available; CPU, virtual, and OTHER devices are rejected.\n");
        }
        return false;
    }
    return true;
}

static bool read_spirv(const char *path, uint32_t **words, size_t *word_count) {
    FILE *file = fopen(path, "rb");
    if (file == NULL) {
        fprintf(stderr, "Cannot open shader %s: %s\n", path, strerror(errno));
        return false;
    }
    if (fseek(file, 0, SEEK_END) != 0) {
        fprintf(stderr, "Cannot seek shader file.\n");
        fclose(file);
        return false;
    }
    long length = ftell(file);
    if (length < 20 || (length % 4) != 0 || fseek(file, 0, SEEK_SET) != 0) {
        fprintf(stderr, "Shader must be a nonempty, word-aligned SPIR-V module.\n");
        fclose(file);
        return false;
    }
    *words = malloc((size_t)length);
    if (*words == NULL) {
        fprintf(stderr, "Could not allocate shader buffer.\n");
        fclose(file);
        return false;
    }
    size_t got = fread(*words, 1, (size_t)length, file);
    fclose(file);
    if (got != (size_t)length || (*words)[0] != UINT32_C(0x07230203)) {
        fprintf(stderr, "Shader is truncated or has invalid SPIR-V magic.\n");
        free(*words);
        *words = NULL;
        return false;
    }
    *word_count = (size_t)length / sizeof(uint32_t);
    return true;
}

static bool check_vk(VkResult result, const char *operation) {
    if (result == VK_SUCCESS) return true;
    fprintf(stderr, "%s failed (VkResult %d).\n", operation, result);
    return false;
}

static bool monotonic_ns(uint64_t *value) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return false;
    *value = (uint64_t)now.tv_sec * UINT64_C(1000000000) + (uint64_t)now.tv_nsec;
    return true;
}

static void json_string(const char *value) {
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)value; *p != '\0'; ++p) {
        if (*p == '"' || *p == '\\') { putchar('\\'); putchar(*p); }
        else if (*p < 0x20) printf("\\u%04x", (unsigned int)*p);
        else putchar(*p);
    }
    putchar('"');
}

int main(int argc, char **argv) {
    Options options;
    if (!parse_options(argc, argv, &options)) return 2;

    VkInstance instance = VK_NULL_HANDLE;
    VkDevice device = VK_NULL_HANDLE;
    VkBuffer buffer = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    VkDescriptorSetLayout descriptor_layout = VK_NULL_HANDLE;
    VkPipelineLayout pipeline_layout = VK_NULL_HANDLE;
    VkPipeline pipeline = VK_NULL_HANDLE;
    VkDescriptorPool descriptor_pool = VK_NULL_HANDLE;
    VkDescriptorSet descriptor_set = VK_NULL_HANDLE;
    VkCommandPool command_pool = VK_NULL_HANDLE;
    VkQueryPool query_pool = VK_NULL_HANDLE;
    VkFence fence = VK_NULL_HANDLE;
    uint32_t *shader_words = NULL;
    void *mapped = NULL;
    bool success = false;
    bool abandon_device_after_timeout = false;
    VkMemoryPropertyFlags memory_flags = 0;
    SelectedDevice selected = {0};
    double gpu_elapsed_ns = 0.0;
    uint64_t host_elapsed_ns = 0;

    size_t shader_word_count = 0;
    if (!read_spirv(options.shader_path, &shader_words, &shader_word_count)) return 2;

    VkApplicationInfo app = {
        .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName = "vulkan-compute-correctness-bench",
        .applicationVersion = VK_MAKE_VERSION(1, 0, 0),
        .pEngineName = "none",
        .engineVersion = VK_MAKE_VERSION(1, 0, 0),
        .apiVersion = VK_API_VERSION_1_2,
    };
    VkInstanceCreateInfo instance_info = { .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo = &app };
    if (!check_vk(vkCreateInstance(&instance_info, NULL, &instance), "vkCreateInstance")) goto cleanup;
    if (!select_device(instance, options.vendor, &selected)) goto cleanup;

    uint64_t byte_count64 = options.count * sizeof(uint32_t);
    if (byte_count64 > selected.properties.limits.maxStorageBufferRange) {
        fprintf(stderr, "Requested output buffer exceeds maxStorageBufferRange.\n");
        goto cleanup;
    }
    uint64_t group_count64 = (options.count + LOCAL_SIZE - 1u) / LOCAL_SIZE;
    if (group_count64 > selected.properties.limits.maxComputeWorkGroupCount[0]) {
        fprintf(stderr, "Requested dispatch exceeds maxComputeWorkGroupCount[0].\n");
        goto cleanup;
    }
    if (LOCAL_SIZE > selected.properties.limits.maxComputeWorkGroupSize[0] ||
        LOCAL_SIZE > selected.properties.limits.maxComputeWorkGroupInvocations) {
        fprintf(stderr, "Selected GPU cannot execute the shader's 256-invocation workgroup.\n");
        goto cleanup;
    }
    float timestamp_period = selected.properties.limits.timestampPeriod;
    if (!isfinite(timestamp_period) || !(timestamp_period > 0.0f)) {
        fprintf(stderr, "Selected device reports an invalid timestamp period.\n");
        goto cleanup;
    }

    float queue_priority = 1.0f;
    VkDeviceQueueCreateInfo queue_info = {
        .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex = selected.queue_family,
        .queueCount = 1,
        .pQueuePriorities = &queue_priority,
    };
    VkDeviceCreateInfo device_info = {
        .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount = 1,
        .pQueueCreateInfos = &queue_info,
    };
    if (!check_vk(vkCreateDevice(selected.physical, &device_info, NULL, &device), "vkCreateDevice")) goto cleanup;
    VkQueue queue = VK_NULL_HANDLE;
    vkGetDeviceQueue(device, selected.queue_family, 0, &queue);

    VkBufferCreateInfo buffer_info = {
        .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size = byte_count64,
        .usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,
        .sharingMode = VK_SHARING_MODE_EXCLUSIVE,
    };
    if (!check_vk(vkCreateBuffer(device, &buffer_info, NULL, &buffer), "vkCreateBuffer")) goto cleanup;
    VkMemoryRequirements requirements;
    vkGetBufferMemoryRequirements(device, buffer, &requirements);
    VkPhysicalDeviceMemoryProperties memory_properties;
    vkGetPhysicalDeviceMemoryProperties(selected.physical, &memory_properties);
    uint32_t type_count = memory_properties.memoryTypeCount;
    int chosen_type = -1;
    int chosen_score = -1;
    for (uint32_t i = 0; i < type_count; ++i) {
        if ((requirements.memoryTypeBits & (1u << i)) == 0) continue;
        VkMemoryPropertyFlags flags = memory_properties.memoryTypes[i].propertyFlags;
        if ((flags & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT) == 0) continue;
        int score = ((flags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) ? 2 : 0) +
                    ((flags & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) ? 1 : 0);
        if (score > chosen_score) { chosen_type = (int)i; chosen_score = score; memory_flags = flags; }
    }
    if (chosen_type < 0) {
        fprintf(stderr, "Selected GPU has no host-visible memory type for result readback.\n");
        goto cleanup;
    }
    VkMemoryAllocateInfo allocation = {
        .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize = requirements.size,
        .memoryTypeIndex = (uint32_t)chosen_type,
    };
    if (!check_vk(vkAllocateMemory(device, &allocation, NULL, &memory), "vkAllocateMemory") ||
        !check_vk(vkBindBufferMemory(device, buffer, memory, 0), "vkBindBufferMemory")) goto cleanup;
    if (!check_vk(vkMapMemory(device, memory, 0, VK_WHOLE_SIZE, 0, &mapped), "vkMapMemory")) goto cleanup;

    /* Make unchanged/stale output fail verification, including after allocation reuse. */
    for (uint64_t i = 0; i < options.count; ++i) {
        uint32_t expected = ((uint32_t)i * UINT32_C(1664525) + UINT32_C(1013904223)) ^ ((uint32_t)i >> 3u);
        ((uint32_t *)mapped)[i] = ~expected;
    }
    if ((memory_flags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) == 0) {
        VkMappedMemoryRange initial_range = {
            .sType = VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE,
            .memory = memory,
            .offset = 0,
            .size = VK_WHOLE_SIZE,
        };
        if (!check_vk(vkFlushMappedMemoryRanges(device, 1, &initial_range), "vkFlushMappedMemoryRanges")) goto cleanup;
    }

    VkDescriptorSetLayoutBinding binding = {
        .binding = 0,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
        .descriptorCount = 1,
        .stageFlags = VK_SHADER_STAGE_COMPUTE_BIT,
    };
    VkDescriptorSetLayoutCreateInfo descriptor_layout_info = {
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        .bindingCount = 1,
        .pBindings = &binding,
    };
    if (!check_vk(vkCreateDescriptorSetLayout(device, &descriptor_layout_info, NULL, &descriptor_layout), "vkCreateDescriptorSetLayout")) goto cleanup;
    VkPipelineLayoutCreateInfo pipeline_layout_info = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        .setLayoutCount = 1,
        .pSetLayouts = &descriptor_layout,
    };
    if (!check_vk(vkCreatePipelineLayout(device, &pipeline_layout_info, NULL, &pipeline_layout), "vkCreatePipelineLayout")) goto cleanup;
    VkShaderModuleCreateInfo shader_info = {
        .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = shader_word_count * sizeof(uint32_t),
        .pCode = shader_words,
    };
    VkShaderModule shader = VK_NULL_HANDLE;
    if (!check_vk(vkCreateShaderModule(device, &shader_info, NULL, &shader), "vkCreateShaderModule")) goto cleanup;
    VkPipelineShaderStageCreateInfo stage = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
        .stage = VK_SHADER_STAGE_COMPUTE_BIT,
        .module = shader,
        .pName = "main",
    };
    VkComputePipelineCreateInfo pipeline_info = {
        .sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
        .stage = stage,
        .layout = pipeline_layout,
    };
    VkResult pipeline_result = vkCreateComputePipelines(device, VK_NULL_HANDLE, 1, &pipeline_info, NULL, &pipeline);
    vkDestroyShaderModule(device, shader, NULL);
    if (!check_vk(pipeline_result, "vkCreateComputePipelines")) goto cleanup;

    VkDescriptorPoolSize pool_size = { .type = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .descriptorCount = 1 };
    VkDescriptorPoolCreateInfo pool_info = {
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets = 1,
        .poolSizeCount = 1,
        .pPoolSizes = &pool_size,
    };
    if (!check_vk(vkCreateDescriptorPool(device, &pool_info, NULL, &descriptor_pool), "vkCreateDescriptorPool")) goto cleanup;
    VkDescriptorSetAllocateInfo set_info = {
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool = descriptor_pool,
        .descriptorSetCount = 1,
        .pSetLayouts = &descriptor_layout,
    };
    if (!check_vk(vkAllocateDescriptorSets(device, &set_info, &descriptor_set), "vkAllocateDescriptorSets")) goto cleanup;
    VkDescriptorBufferInfo descriptor_buffer = { .buffer = buffer, .offset = 0, .range = byte_count64 };
    VkWriteDescriptorSet write = {
        .sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
        .dstSet = descriptor_set,
        .dstBinding = 0,
        .descriptorCount = 1,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
        .pBufferInfo = &descriptor_buffer,
    };
    vkUpdateDescriptorSets(device, 1, &write, 0, NULL);

    VkCommandPoolCreateInfo command_pool_info = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
        .flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT,
        .queueFamilyIndex = selected.queue_family,
    };
    if (!check_vk(vkCreateCommandPool(device, &command_pool_info, NULL, &command_pool), "vkCreateCommandPool")) goto cleanup;
    VkCommandBufferAllocateInfo command_allocate = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool = command_pool,
        .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
        .commandBufferCount = 1,
    };
    VkCommandBuffer command = VK_NULL_HANDLE;
    if (!check_vk(vkAllocateCommandBuffers(device, &command_allocate, &command), "vkAllocateCommandBuffers")) goto cleanup;
    VkQueryPoolCreateInfo query_info = {
        .sType = VK_STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO,
        .queryType = VK_QUERY_TYPE_TIMESTAMP,
        .queryCount = 2,
    };
    if (!check_vk(vkCreateQueryPool(device, &query_info, NULL, &query_pool), "vkCreateQueryPool")) goto cleanup;
    VkFenceCreateInfo fence_info = { .sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO };
    if (!check_vk(vkCreateFence(device, &fence_info, NULL, &fence), "vkCreateFence")) goto cleanup;
    VkCommandBufferBeginInfo begin_info = { .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO };
    if (!check_vk(vkBeginCommandBuffer(command, &begin_info), "vkBeginCommandBuffer")) goto cleanup;
    vkCmdResetQueryPool(command, query_pool, 0, 2);
    vkCmdWriteTimestamp(command, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, query_pool, 0);
    vkCmdBindPipeline(command, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
    vkCmdBindDescriptorSets(command, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline_layout, 0, 1, &descriptor_set, 0, NULL);
    for (uint64_t iteration = 0; iteration < options.iterations; ++iteration) {
        vkCmdDispatch(command, (uint32_t)group_count64, 1, 1);
        if (iteration + 1 < options.iterations) {
            VkMemoryBarrier barrier = {
                .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
                .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT,
                .dstAccessMask = VK_ACCESS_SHADER_WRITE_BIT,
            };
            vkCmdPipelineBarrier(command, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                                 VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &barrier,
                                 0, NULL, 0, NULL);
        }
    }
    vkCmdWriteTimestamp(command, VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, query_pool, 1);
    VkMemoryBarrier host_read = {
        .sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
        .srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT,
        .dstAccessMask = VK_ACCESS_HOST_READ_BIT,
    };
    vkCmdPipelineBarrier(command, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                         VK_PIPELINE_STAGE_HOST_BIT, 0, 1, &host_read, 0, NULL, 0, NULL);
    if (!check_vk(vkEndCommandBuffer(command), "vkEndCommandBuffer")) goto cleanup;

    VkSubmitInfo submit = {
        .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .commandBufferCount = 1,
        .pCommandBuffers = &command,
    };
    uint64_t host_start = 0, host_end = 0;
    if (!monotonic_ns(&host_start)) { fprintf(stderr, "clock_gettime(CLOCK_MONOTONIC) failed.\n"); goto cleanup; }
    if (!check_vk(vkQueueSubmit(queue, 1, &submit, fence), "vkQueueSubmit")) goto cleanup;
    VkResult wait_result = vkWaitForFences(device, 1, &fence, VK_TRUE, FENCE_TIMEOUT_NS);
    if (wait_result == VK_TIMEOUT) {
        fprintf(stderr, "GPU fence wait exceeded the 30-second bound.\n");
        /* Avoid a possibly unbounded implicit wait in vkDestroyDevice; process exit reclaims it. */
        abandon_device_after_timeout = true;
        goto cleanup;
    }
    if (!check_vk(wait_result, "vkWaitForFences")) goto cleanup;
    if (!monotonic_ns(&host_end) || host_end < host_start) { fprintf(stderr, "Host monotonic timing failed.\n"); goto cleanup; }
    host_elapsed_ns = host_end - host_start;

    uint64_t timestamps[2] = {0, 0};
    if (!check_vk(vkGetQueryPoolResults(device, query_pool, 0, 2, sizeof(timestamps), timestamps,
                                        sizeof(uint64_t), VK_QUERY_RESULT_64_BIT), "vkGetQueryPoolResults")) goto cleanup;
    uint64_t elapsed_ticks;
    if (selected.timestamp_bits >= 64) elapsed_ticks = timestamps[1] - timestamps[0];
    else {
        uint64_t mask = (UINT64_C(1) << selected.timestamp_bits) - 1u;
        elapsed_ticks = (timestamps[1] - timestamps[0]) & mask;
    }
    gpu_elapsed_ns = (double)elapsed_ticks * (double)timestamp_period;
    if (!isfinite(gpu_elapsed_ns) || gpu_elapsed_ns <= 0.0 || host_elapsed_ns == 0) {
        fprintf(stderr, "GPU or host timing is zero or invalid; refusing a benchmark pass.\n");
        goto cleanup;
    }
    if ((memory_flags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) == 0) {
        VkMappedMemoryRange range = {
            .sType = VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE,
            .memory = memory,
            .offset = 0,
            .size = VK_WHOLE_SIZE,
        };
        if (!check_vk(vkInvalidateMappedMemoryRanges(device, 1, &range), "vkInvalidateMappedMemoryRanges")) goto cleanup;
    }

    const uint32_t *output = mapped;
    for (uint64_t i = 0; i < options.count; ++i) {
        uint32_t expected = ((uint32_t)i * UINT32_C(1664525) + UINT32_C(1013904223)) ^ ((uint32_t)i >> 3u);
        if (output[i] != expected) {
            fprintf(stderr, "Output mismatch at index %" PRIu64 ": got 0x%08" PRIx32 ", expected 0x%08" PRIx32 ".\n",
                    i, output[i], expected);
            goto cleanup;
        }
    }

    struct utsname os_info;
    char os[256];
    if (uname(&os_info) == 0) snprintf(os, sizeof(os), "%s %s %s", os_info.sysname, os_info.release, os_info.machine);
    else snprintf(os, sizeof(os), "unknown");
    printf("{\"status\":\"pass\",\"device\":"); json_string(selected.properties.deviceName);
    printf(",\"vendor\":"); json_string(vendor_label(selected.properties.vendorID));
    printf(",\"vendor_id\":\"0x%04" PRIx32 "\",\"device_type\":", selected.properties.vendorID);
    json_string(selected.properties.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU ? "discrete" : "integrated");
    printf(",\"driver\":"); json_string(selected.driver.driverName);
    printf(",\"driver_info\":"); json_string(selected.driver.driverInfo);
    printf(",\"driver_id\":%d,\"driver_id_name\":", (int)selected.driver.driverID);
    const char *driver_id_name = "other";
    if (selected.driver.driverID == VK_DRIVER_ID_NVIDIA_PROPRIETARY) driver_id_name = "nvidia-proprietary";
    else if (selected.driver.driverID == VK_DRIVER_ID_MESA_RADV) driver_id_name = "mesa-radv";
    else if (selected.driver.driverID == VK_DRIVER_ID_INTEL_OPEN_SOURCE_MESA) driver_id_name = "intel-open-source-mesa";
    else if (selected.driver.driverID == VK_DRIVER_ID_INTEL_PROPRIETARY_WINDOWS) driver_id_name = "intel-proprietary";
    json_string(driver_id_name);
    printf(",\"os\":"); json_string(os);
    printf(",\"count\":%" PRIu64 ",\"dispatches\":%" PRIu64 ",\"verified_outputs\":%" PRIu64
           ",\"gpu_elapsed_ns\":%.3f,\"host_elapsed_ns\":%" PRIu64 "}\n",
           options.count, options.iterations, options.count, gpu_elapsed_ns, host_elapsed_ns);
    success = true;

cleanup:
    free(shader_words);
    if (abandon_device_after_timeout) return 1;
    if (device != VK_NULL_HANDLE && mapped != NULL) vkUnmapMemory(device, memory);
    if (device != VK_NULL_HANDLE && fence != VK_NULL_HANDLE) vkDestroyFence(device, fence, NULL);
    if (device != VK_NULL_HANDLE && query_pool != VK_NULL_HANDLE) vkDestroyQueryPool(device, query_pool, NULL);
    if (device != VK_NULL_HANDLE && command_pool != VK_NULL_HANDLE) vkDestroyCommandPool(device, command_pool, NULL);
    if (device != VK_NULL_HANDLE && descriptor_pool != VK_NULL_HANDLE) vkDestroyDescriptorPool(device, descriptor_pool, NULL);
    if (device != VK_NULL_HANDLE && pipeline != VK_NULL_HANDLE) vkDestroyPipeline(device, pipeline, NULL);
    if (device != VK_NULL_HANDLE && pipeline_layout != VK_NULL_HANDLE) vkDestroyPipelineLayout(device, pipeline_layout, NULL);
    if (device != VK_NULL_HANDLE && descriptor_layout != VK_NULL_HANDLE) vkDestroyDescriptorSetLayout(device, descriptor_layout, NULL);
    if (device != VK_NULL_HANDLE && buffer != VK_NULL_HANDLE) vkDestroyBuffer(device, buffer, NULL);
    if (device != VK_NULL_HANDLE && memory != VK_NULL_HANDLE) vkFreeMemory(device, memory, NULL);
    if (device != VK_NULL_HANDLE) vkDestroyDevice(device, NULL);
    if (instance != VK_NULL_HANDLE) vkDestroyInstance(instance, NULL);
    return success ? 0 : 1;
}
