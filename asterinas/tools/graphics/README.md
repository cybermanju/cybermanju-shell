# GPUI graphics readiness diagnostic

Run from the repository root (or invoke by absolute path):

```sh
python3 tools/graphics/check_graphics.py
```

It uses only the Python standard library plus the host tools `vulkaninfo` and `wayland-info`. Exit status is **0 only when all three required checks pass**; a failed or unavailable check produces an actionable message and a nonzero exit status.

## Checks and limits

- **DRM nodes:** looks only for accessible character-device nodes named `/dev/dri/cardN` (primary) and `/dev/dri/renderDNNN` (render), and requires at least one of each. It does not create, emulate, or infer devices. Node presence/access is not proof of a real GPU, active driver, working display path, or successful GPUI rendering.
- **Vulkan device:** runs `vulkaninfo --summary` to enumerate device indices, then runs `vulkaninfo --json=<index>` per device and inspects the generated Vulkan Profiles JSON at `capabilities.device.properties.VkPhysicalDeviceProperties.deviceType` and `deviceName`. CPU devices (including software implementations such as llvmpipe) do not satisfy this check. Integrated, discrete, and virtual GPU enum values are reported as non-CPU candidates; unknown `OTHER` values are not treated as confirmed candidates. The Vulkan specification says this type is informational, so this cannot certify physical hardware or hardware acceleration. The JSON command's per-device output behavior follows [`vulkaninfo` documentation](https://vulkan.lunarg.com/doc/view/latest/windows/vulkaninfo.html); type meanings and their limitation follow [`VkPhysicalDeviceType`](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceType.html).
- **Wayland layer-shell:** connects with `wayland-info` and requires its compositor-global listing to include `zwlr_layer_shell_v1`. This checks advertised protocol availability in the active session; it does not create a surface or test the protocol's runtime behavior. See the upstream [wlr layer-shell protocol definition](https://github.com/swaywm/wlr-protocols/blob/master/unstable/wlr-layer-shell-unstable-v1.xml).

The diagnostic is a staged prerequisite probe, not a substitute for GPUI build, boot, session, or end-to-end rendering acceptance tests. Run it as the same user and inside the intended graphical session; missing device permissions or session variables are reported as failures rather than inferred from another user's environment.

## Tests

Run the standard-library unit tests with temporary test-only fixtures and mocked subprocess responses:

```sh
python3 -m unittest discover -s tools/graphics -v
```

## Experimental NVIDIA source integration and native acceptance

The optional [`ostd-nvidia-nvport`](../../ostd/libs/nvidia-nvport/README.md) crate compiles and links real NVIDIA 615.78.08 CPU-memory routines against OSTD services. It is not a GPU driver. See [native NVIDIA acceptance](NATIVE_NVIDIA.md) for the protected VFIO guest workflow, exact evidence contract, current boot failure and remaining kernel-driver work. A Linux-host compute pass is never treated as an Asterinas-native pass.
