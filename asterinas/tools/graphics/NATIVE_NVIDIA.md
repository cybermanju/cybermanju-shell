# NVIDIA acceptance inside Asterinas

**There is no native GPU pass yet.** The implemented NVport CPU-memory component is linked into Asterinas, but the RM/modeset driver and its required GPU lifecycle and userspace ABI are not ported. This workflow defines a strict, failing-until-implemented hardware acceptance path; it does not make the port complete.

## Two different tests

`nvidia-vulkan-benchmark` runs compute on a Linux host. It can validate the machine's NVIDIA Vulkan installation, but cannot validate Asterinas.

`asterinas-native-nvidia` builds the dispatched Asterinas kernel, creates an initramfs from an administrator-provided guest tree, boots that exact image in QEMU/KVM with preauthorized VFIO devices, and runs the benchmark only inside the guest. No host compute result is accepted as a guest result. This is native-kernel **compute** acceptance, not GPUI, Wayland, display, presentation, synchronization-sharing, or full-driver acceptance.

## Provisioning required outside the workflow

Use an isolated, disposable self-hosted Linux x86-64 runner labeled `nvidia-vfio`. Configure the GitHub environment `asterinas-native-gpu` with required reviewers and branch restrictions before enabling it. A YAML environment name alone does not configure protection. Untrusted PR code must never execute on this privileged runner. Keep this runner separate from a Linux-host benchmark runner using its GPU normally.

An administrator must prepare and explicitly authorize the complete PCI device assignment and IOMMU-group isolation. Select an NVIDIA GPU supported by the open kernel sources and GSP firmware. Bind the intended functions to `vfio-pci` beforehand; all IOMMU peers must be VFIO-bound or unbound. The script never unbinds a host driver, changes PCI bindings, uses unsafe no-IOMMU mode, or falls back to CPU emulation. The runner needs access to `/dev/kvm` and the relevant `/dev/vfio` group devices. GPU assignment/reset and a kernel under test can hang or reset a device, which is why this belongs on a dedicated test machine.

Install the repository-pinned Rust toolchain, `cargo-osdk`, matching vDSO/build dependencies, a C compiler, Vulkan development headers, `glslangValidator`, `cpio`, `find`, `grub-mkrescue`, `xorriso`, Python 3.11+ and QEMU with KVM/VFIO support. Ordinary PR CI uses the project's pinned kernel-dev container for the C/Rust host tests and complete kernel link.

Configure environment/repository variables:

- `ASTER_GPU_VFIO_DEVICES`: explicit comma-separated full-domain PCI addresses, such as `0000:01:00.0,0000:01:00.1`. These are examples, not autodetected or authorized device choices.
- `ASTER_GPU_ROOTFS`: an absolute path to an administrator-controlled, device-free guest tree. It must contain `/bin/sh`, `mount`, `cat`, `poweroff`, `/proc`, `/sys`, `/dev`, the Vulkan loader, NVIDIA Vulkan ICD configuration, matching NVIDIA **615.78.08** userspace libraries and GSP firmware, with all dynamic-loader dependencies needed by a benchmark compiled on that runner. Prepare these legally and verify guest compatibility. The workflow does not fetch or redistribute proprietary userspace.

Do not give the guest tree `/aster-native-gpu-bench`, `/aster-native-gpu.spv` or `/aster-native-gpu-init`; those reserved names are rejected. Use an output directory outside the checkout. The script requires a clean checkout so its commit marker is not silently attached to uncommitted source changes. It creates an exclusive temporary `kernel/OSDK.toml` to avoid inheriting the workspace's interactive `sh -l` init arguments, and removes that file after building; it refuses to overwrite an existing package manifest.

Manually dispatch the workflow on `main` and explicitly select `run_native_gpu`. The native job has a separate concurrency group and protected environment, fixed source SHA, read-only repository permissions, pinned action revisions and bounded guest execution. NVIDIA source is fetched publicly at the pinned commit. No GitHub authentication or remote run was performed during local preparation.

For an already prepared, clean checkout and authorized machine, the equivalent entrypoint is:

```sh
export NVIDIA_SOURCE_DIR=/absolute/path/to/pinned-nvidia-checkout
python3 tools/graphics/run_native_nvidia.py \
  --rootfs "$ASTER_GPU_ROOTFS" \
  --vfio-devices "$ASTER_GPU_VFIO_DEVICES" \
  --output /absolute/path/outside-checkout
```

## What counts as evidence

The kernel emits a boot marker containing the dispatched commit and a fresh run nonce through an explicitly enabled `earlycon`. Guest init runs the real C/SPIR-V benchmark, emits its JSON only on correctness/timing success, and emits its exit status with the same nonce. The validator requires exactly one ordered boot/result/success chain, NVIDIA vendor and driver identity, explicit **615.78.08** driver metadata, hardware device type, complete output verification, bounded workloads and positive finite GPU/host timings. Missing firmware, device ABI, real GPU, driver version, boot marker or result is a failure, never a skip-pass.

`provenance.json` records the source commit, nonce, exact image/initramfs/benchmark/shader SHA-256 hashes and assigned device list. `native-result.json` is written only after evidence validates. Serial and build logs are retained on failure. CI uploads only logs and JSON, not the guest image or its NVIDIA libraries.

This is a trusted-runner log chain, **not cryptographic attestation**. A compromised runner can forge logs or binaries. `uname` is not native-kernel proof: Asterinas itself reports Linux-compatible identity. The image build, hash, boot marker and guest-only execution establish the intended chain under the trusted-runner assumption.

## Current local evidence and blockers

Eight real C/Rust NVport memory tests now passed **inside Asterinas/OSTD under QEMU TCG**, as well as on the host. The OSTD runner reported 225 passed tests, including four new Multiboot2 regressions; one VMX/EPT-dependent test body reported its explicit unavailable-hardware skip. Three NVIDIA PCI platform validation tests also passed inside the kernel. No full RM/modeset object was linked.

The fault at `0xffff80007ffe2321` was reproduced and traced to reading ACPI MADT data outside the new kernel linear mapping. SeaBIOS placed the ACPI root/tables in reserved RAM beyond the last usable range (`0x7ffdf000`); Multiboot2 failed to apply the ACPI reserved-region preservation already used by other boot paths. Applying `effective_region_type` with the actual RSDT/XSDT extends preserved metadata/mapping through that region (`0x80000000` in this VM). Reserved RAM unrelated to ACPI is not promoted. The same boot then passed APIC/ACPI initialization.

The next failure was an EGA text console misinterpreted as an RGB pixel framebuffer at VGA text memory. Multiboot2 now returns no pixel framebuffer for EGA text, rather than letting the framebuffer component attempt to acquire the wrong memory type. This does not fix the separate general pitch/pixel-mask limitations or create display acceleration.

The corrected complete kernel ran the NVport self-test, executed a real static `/init` in userspace, printed `ASTERINAS_CPU_USERSPACE_SMOKE passed (not a GPU test)` and successfully invoked `reboot(POWER_OFF)`. QEMU's `isa-debug-exit` success encoding is process status **33**, not Unix status 0. Local logs and explicit CPU-only result are in `../evidence/nvport/` relative to the checkout: `native-eight-tests.log`, `native-ostd-suite.log`, `native-pci-platform-tests.log`, `cpu-userspace-boot.log`, `cpu-userspace-result.json`, and `tcg-ci-smoke/result.json`.

`aster-nvidia-platform` now consumes the real `aster-pci` enumerator's identity and measured BAR metadata, validates NVIDIA display BAR0/ranges, and provides bounded, read-only config access. The optional `nvidia-pci-diagnostics` feature emits resource inventories without claiming a device or initializing RM/GSP. `native-gpu-ci` includes that diagnostic. There is no successful NVIDIA hardware probe in this sandbox; no GPU is assigned, and the native GPU entrypoint continues to reject missing `/dev/kvm`.

Even with a working boot and GPU runner, native Vulkan cannot work until the missing NVIDIA RM/GSP initialization, DMA/IOMMU/pinning/interrupt services, device-node ioctl/mmap ABI, GPU memory/submission/fence/reset/teardown and accelerated DRM integration are genuinely implemented. GPUI/Wayland rendering then needs separate end-to-end acceptance. Neither a GitHub token nor a passing Linux benchmark supplies these missing kernel components.

## CPU-only CI is not hardware acceptance

Ordinary PR/push CI runs `bash tools/graphics/run_cpu_kernel_tests.sh OUTPUT_DIRECTORY` for the NVport, PCI platform and OSTD suites with TCG, no networking and no GPU passthrough. Each run is bounded, uses an exclusive temporary package manifest, retains logs, and rejects a zero-tests result. The kernel build then enables NVport plus PCI diagnostics and `run_nvport_smoke.py` requires the exact data-checking NVport success line. That smoke ends at the CPU marker and deliberately does not certify subsequent full boot or GPU activity. These commands were exercised locally, but no Actions workflow was uploaded or run remotely.

`validate_native_gpu_evidence.py` continues to identify a possible future hardware compute result as compute-only, with `full_gpui_wayland_vulkan_acceptance = false`. Missing hardware/GSP/ABI is a failure, not a skip-pass; neither the CPU smoke nor a Linux-host result can satisfy the native GPU requirement.

## Bounded multifunction prerequisite tests

The CPU runner also creates genuine QEMU PCIe root ports and a two-function NVMe slot, without changing vendor/device IDs. The PCI group API probes all eight function numbers, requires every present function to be unclaimed and BME-off, and holds the whole slot against probing. MMIO preparation rolls back earlier function mappings on a later failure. Each function must independently pass the same stable root-port/ACS route and DMAR authorization; the complete group receives one owned VT-d root and DID. Partial reservation, function-zero omissions, unsupported siblings, translation/alias services, switches and unsupported firmware fail closed. No production path enables bus mastering or starts NVIDIA RM/GSP.

The `pci-iommu` run requires real Identify Controller DMA from both NVMe functions across repeated group attach/detach cycles, actual CID/status/vendor/serial/model verification, live duplicate-domain rejection, controller shutdown and BME-off checks. Both PCI runs require missing-sibling, active-BME, invalid-sibling-BAR rollback and single-function/subset rejection checks. A second real NVMe-plus-conventional-EDU slot must be rejected as an unsupported group. The no-IOMMU run must reject group DMA; it is not a passing isolated-DMA test. The OSTD suite also tests that IRQ callbacks are removed before an interrupt number can be recycled; this is a teardown-order prerequisite, not NVIDIA IRQ integration.

This is a deliberately narrow group contract: slot zero directly below one segment-zero ACS-verified root port, with all functions exclusively owned and each selected MMIO BAR usable. Device-specific quiescence is still the caller's obligation; BME-off is not proof of completion. Hotplug and route reconfiguration are unsupported. All functions share one trust boundary—there is no isolation between siblings. These tests do not establish support for any actual NVIDIA GPU, RM/GSP, firmware authentication, the NVIDIA ioctl/mmap ABI, Vulkan userspace, Wayland, display or GPUI. The protected native-GPU job remains a separate failing-until-implemented gate and cannot be satisfied by these QEMU devices or a host-Linux benchmark.

## Transactional MSI-X prerequisites (current local checkpoint)

`PciMmioGuard::prepare_msix` and `NvidiaDeviceResources::prepare_msix` prepare a borrowed, exclusive transaction: bounded table/PBA mappings, actual IRQ allocation, saved entries/control/command, Function Mask and per-vector masks before message programming. Preparation never enables BME and rejects enabled MSI-X, inherited pending messages, invalid contexts, duplicate ownership and insufficient vectors. Every assigned vector needs a caller-supplied real handler before `activate` can unmask it. No RM handler or successful NVIDIA probe is fabricated.

`shutdown` masks with readback, requires the actual driver's stop/completion/worker-drain operation to succeed, checks BME-off and empty PBA, synchronously disconnects callbacks, and verifies exact table/control/command restoration. A failed stop retains the transaction for retry. Abandoning an activated transaction quarantines its device and MMIO against reprobe instead of declaring a successful stop. A forgotten transaction also leaves its owner's lease set and causes quarantine. Neither shutdown nor Drop may hold a lock required by a handler.

The current interrupt core cannot prove that an already posted MSI has reached every CPU or that APIC/IRTE state is fully drained. Activated IRQ numbers are therefore **permanently retired as empty sinks**, not returned to the allocator. This deliberately bounded fail-closed policy consumes vectors until reboot; it is not a reusable production GPU IRQ lifecycle or an SMP-drain proof. Device-specific deferred work/DMA must still be drained by the real driver, not by a no-op success closure.

The existing NVMe and modern/legacy Virtio PCI consumers now configure masked vectors, register their actual callbacks, and explicitly enable only routed vectors afterward. Their BME decision lives in the transport, not the PCI interrupt constructor. Their legacy MSI-X owner's Drop masks and retires IRQs before mappings are dropped, but cannot supply controller reset, active-state rollback or arbitrary driver-worker drainage; NVIDIA must use the borrowed transactional path.

The CPU runner requires a genuine QEMU NVMe Identify completion while masked with its real callback already registered, a physical PBA pending bit, callback delivery only after unmask, a cleared PBA afterward, and two stop/reconfigure cycles in both direct-DMA and VT-d runs. Capture-destruction probes must survive a failed quiesce and be synchronously released after a successful stop. It also checks real allocator exhaustion/rollback, atomic-context rejection, missing handlers, legacy BME/index/replacement rejection and abandoned-transaction quarantine. These are PCI/IRQ prerequisites, **not NVIDIA/GPU execution**. Current local counts before the separate IRQ-storage follow-up are 20 NVport, 4 platform, 13 PCI, 13 PCI-with-VT-d and 255 reported OSTD tests; three virtualization-dependent bodies explicitly skip unavailable VMX/EPT hardware. The strict NVIDIA benchmark still rejects the CPU-only sandbox and emits no passing GPU JSON. The workflow changes are local and have not been pushed or executed remotely.

The first additional full-kernel storage boot exposed a pre-existing Virtio-legacy blocker: PCI I/O BAR access was an unimplemented unit variant that returned `IoError`. The port path now owns each byte through the existing OSTD allocator, rolls back partial acquisition, caches shared ownership per BAR and checks the full access width before executing an 8/16/32-bit port instruction. The existing generic `PodOnce` MMIO API, including 64-bit accesses, is preserved; unsupported PIO widths/architectures fail closed. Three real kernel tests cover port conflicts, partial rollback and invalid accesses without issuing hardware I/O. Port access is not implemented through overlapping reservations or unowned raw instructions. See the local evidence index for the separate full-kernel consumer read/write/flush result rather than inferring it from these allocation tests.
