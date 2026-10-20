# Local PR drafts — Asterinas and GPUi Shell

Nothing has been committed, pushed, authenticated to GitHub, or published in this continuation. The original five patches and subsequent additions are bounded contributions, not a complete GPU port. No functional AMD/NVIDIA kernel driver, mature Wayland stack, or hardware Vulkan support has been established on Asterinas. No stubbed-success driver callbacks or placeholder driver crate were added.

## Asterinas: fix dumb-buffer pixel-stride rounding

Base: `asterinas/asterinas` main, `00a4842d976db6723b8224be2e57ac8105f13ee2`.

Patch: `asterinas-dumb-buffer-layout.patch`.

The existing calculation rounded the combined row bit count rather than each pixel's storage. For an eight-pixel row, 15-bit and 30-bit modes produced 15 and 30 bytes instead of 16 and 32. The new checked layout helper rounds BPP to whole bytes before multiplying width, rejects zero inputs and overflowing BPP/pitch/size, and checks page rounding within the existing u32 size boundary. Handle rollback and flags behavior are preserved. Four kernel regression tests cover these layouts, common 24/32-bit layouts, invalid inputs, arithmetic overflow and the maximum page-aligned boundary. This generic layout does not claim support for every optional packed color format.

Validation:

- Rust formatting and `git diff --check` passed.
- Final `cargo osdk check --ktests -p aster-drm -p aster-simpledrm` passed with the pinned nightly and vDSO. This compiles the tests; it does not execute them.
- Runtime attempts through OSDK with BIOS-compatible legacy images and pinned `llvm-strip` timed out after 360 seconds per crate. A separate direct 60-second QEMU TCG run of the legacy DRM image also stopped at BIOS/ROM boot. No kernel regression test execution or success was observed. KVM is unavailable in this sandbox.
- Patch application was checked against a temporary clean worktree at the base revision.

Reference: the current [Linux DRM dumb-buffer core](https://raw.githubusercontent.com/torvalds/linux/master/drivers/gpu/drm/drm_dumb_buffers.c) validates whole-byte pixel stride and size before calling a driver. Dumb buffers are scanout storage, not GPU acceleration.

## Asterinas: use exact boot framebuffer pixel formats in simpledrm

Base: the Asterinas revision above.

Patch: `asterinas-simpledrm-exact-pixel-format.patch`.

Replace the unconditional XRGB8888 fallback with mappings matching the framebuffer API's supported byte layouts: BgrReserved to XRGB8888, and Rgb888 to BGR888. Reject Grayscale8 and Rgb565 instead of advertising an inexact format. The current Rgb565 renderer emits big-endian bytes; treating it as ordinary little-endian DRM RGB565 would be incorrect. Add two kernel regression tests using the repository's `cfg(ktest)` and `#[ktest]` conventions.

Validation:

- Pinned-nightly Rust formatting, `git diff --check`, normal DRM/simpledrm compile checks and final ktest compile checks passed.
- The original 900-second QEMU attempt reached BIOS but did not report executed tests. The later legacy-image attempt also timed out. Neither is a runtime pass.
- Patch application was checked against the clean base.

This corrects the contract at the existing framebuffer API boundary. Boot metadata currently guesses pixel layout from BPP and omits firmware pitch; this patch does not fix that separate limitation or add GPU acceleration.

## GPUi Shell: add an opt-in NixOS session module

Base and intended PR target: `andre-brandao/gpui-shell` dev, `fb4173c9c23cd8dc16732f800ba4921eb1233a50`.

Patch: `gpui-shell-nixos-session-module.patch`.

Export `nixosModules.default`; add `programs.gpuishell.enable`, a package override, and optional `autostart`. Install the package when enabled. Autostart remains disabled by default; when enabled it adds a user service wanted by, and part of, `graphical-session.target`, conditioned on `WAYLAND_DISPLAY`. Document that this does not configure a compositor and requires a session manager that starts the target and supplies its environment.

The evaluation test now imports the locked nixpkgs's real NixOS `eval-config.nix`, rather than approximating NixOS option types. It checks package selection/overrides, disabled and opt-in combinations, and the generated unit's `WantedBy`, `PartOf`, `ConditionEnvironment` and `ExecStart`. Test executables are evaluation fixtures, not real graphical applications.

Validation:

- `nix build .#checks.x86_64-linux.nixos-module-evaluation --no-link --print-out-paths --show-trace` passed; output `/nix/store/fkm0qffyji6b7wgrhz98d7xas2i0s29r-gpuishell-nixos-module-evaluation`.
- The aarch64 check's derivation evaluated successfully; no aarch64 build or runtime was performed.
- `nix flake check --no-build --show-trace` passed, with the expected host-incompatible aarch64 omission separately covered by evaluation above.
- The original real package build and its `--validate` configuration path passed. The earlier `--help` attempt entered startup and aborted without a supported compositor; it is not a passing CLI/UI test.
- Nix formatting, diff checks and clean-base patch application passed. Existing upstream Nix deprecation warnings remain.

No live NixOS/systemd session or Asterinas runtime compatibility is claimed. `ConditionEnvironment` checks the user service manager's environment, not merely an interactive terminal's environment; see [systemd unit documentation](https://www.freedesktop.org/software/systemd/man/latest/systemd.unit.html).

## GPUi Shell: report PulseAudio startup failures without aborting the shell

Base: the GPUI Shell revision above.

Patch: `gpui-shell-audio-startup-errors.patch`.

An actual nested Niri test connected the shell to the compositor, then the release process aborted on an `.expect()` when PulseAudio was absent. Replace fallible proplist/mainloop/context/connect setup with an error-returning initializer. The listener logs the error, publishes the existing lifecycle Error state, and exits cleanly instead of panicking. Cancellation checks prevent an already-retired listener from proceeding through initialization. Normal audio operation is not replaced by fake data or a success callback.

Add a regression test with an explicit nonexistent Unix-socket server, avoiding global environment mutation and the need for an audio daemon. It verifies a lifecycle Error is reported without panicking.

Validation:

- Focused audio regression: one passed.
- Entire `cargo test -p services` suite: 19 passed, zero failed. Evidence: `../evidence/gpui-services-tests.log`.
- Rust formatting, diff checks and independent clean-base patch application passed.
- The real corrected `nix build .#default` passed; its output is rooted by `../installed/gpuishell-audio-fixed`. Evidence: `../evidence/gpui-audio-fixed-build.log`.
- A real nested-session smoke test passed with the corrected package, Niri 26.04, Xvfb and Mesa 26.1.5. An explicitly nonexistent PulseAudio socket produced a logged service error, while the shell created its bar layer and remained alive for the five-second observation window. The captured 1280×800 frame visibly contains the shell's vertical bar.
- GPUI logs confirm selection of `llvmpipe (LLVM 21.1.8, 256 bits)` with the Vulkan backend and CPU device type. This is software Vulkan, not hardware acceleration. The corrected package is `/nix/store/khjr0fdvrgdmhcrp426cdwq3jc60pxw6-gpuishell-0.1.0`.

The modified APIs were checked against current [Context](https://docs.rs/libpulse-binding/latest/libpulse_binding/context/struct.Context.html) and [Proplist](https://docs.rs/libpulse-binding/latest/libpulse_binding/proplist/struct.Proplist.html) documentation while retaining the project's locked dependencies.

### Reproduce the nested-session evidence

The [Niri getting-started guide](https://niri-wm.github.io/niri/Getting-Started.html) documents running Niri inside an existing desktop session. This local test gives it a separate Xvfb server, private runtime/config directories and a private D-Bus session; it does not alter the existing desktop or install a global login session.

From `/workspace/asterinas-gpui`, run:

```sh
dbus-run-session -- python3 evidence/run_nested_gpui_smoke.py \
  --niri /workspace/asterinas-gpui/installed/niri/bin/niri \
  --gpui /workspace/asterinas-gpui/installed/gpuishell-audio-fixed/bin/gpuishell \
  --mesa /workspace/asterinas-gpui/installed/mesa \
  --library-path /workspace/asterinas-gpui/evidence/nested-library-path.txt \
  --output /workspace/asterinas-gpui/evidence/nested-smoke-audio-fixed
```

Evidence is in `../evidence/nested-smoke-audio-fixed/`: `result.json`, `gpui.log`, `niri.log`, `layers.json`, `wayland-info.txt` and `gpui-nested.png`. The runner stops its child processes on completion. It verifies surface creation and brief process survival, not keyboard/mouse interactions, audio functionality, all desktop services, physical modesetting, GPU performance or Asterinas execution.

## Asterinas: add a graphics prerequisite diagnostic

Base: the Asterinas revision above.

Patch: `asterinas-graphics-prerequisite-check.patch`.

Add a standard-library Python diagnostic for accessible DRM character nodes, actual per-device Vulkan Profiles JSON from `vulkaninfo`, and a compositor's advertised layer-shell global via `wayland-info`. CPU-only Vulkan does not satisfy the non-CPU-device check. Missing dependencies and failed checks produce actionable errors and nonzero exit status. Its README explicitly states that passing these probes does not certify hardware acceleration or GPUI integration.

Validation:

- All nine unit tests passed; they use temporary fixtures and mocked subprocess responses. Python byte compilation passed.
- On the ordinary sandbox session the diagnostic exited 1: `/dev/dri` is absent, Vulkan exposes only CPU llvmpipe, and no Wayland compositor is available there. This expected negative result is not an integration pass.
- Patch application was checked against the clean base.

## NVIDIA source reuse — evidence, not a driver PR

The unmodified NVIDIA 615.78.08 OS-agnostic RM and modeset source trees compiled at `ca2d03e6fafd4d4b56b383497d931c25b283bfcf`. The relocatable objects retain 425 and 73 unresolved symbol names respectively. These include OS services, adapter entrypoints, crypto dependencies and compiler support; they are not counts of mandatory APIs for one selected GPU.

The inventory is `../evidence/nvidia-ffi-unresolved-symbols.txt`. The objects were not linked into Asterinas or tested on NVIDIA hardware. [NVIDIA's source documentation](https://github.com/NVIDIA/open-gpu-kernel-modules) separates these components from its Linux interface layers; DRM and UVM have no separate OS-agnostic component, and userspace/GSP firmware must match the driver release.

FFI can implement a source-level adapter boundary, but cannot itself implement memory pinning and lifetime rules, device-bound DMA/IOMMU semantics, interrupt/sleeping rules, GPU page tables, firmware initialization, submission, fences, reset, or teardown. Reusing C also needs a deliberately reviewed unsafe/TCB boundary; hiding calls behind FFI does not preserve Asterinas's safe-kernel model automatically. A prebuilt Ubuntu `.ko` additionally depends on Linux's version/configuration-specific internal ABI.

Current Asterinas DRM exposes dumb-buffer GEM and legacy KMS, not a complete accelerated-driver ABI. Missing operations include driver-private GPU submission, PRIME/dma-buf sharing, syncobj/timeline synchronization and atomic modesetting. For NVIDIA, the RM device-node/ioctl/mmap ABI and DRM/GEM interop are both required. AMD is a separate driver port: [RADV](https://docs.mesa3d.org/drivers/radv.html) requires the AMDGPU kernel UAPI, including its memory and submission semantics. Installing userspace libraries or creating device names supplies neither port.

## Local delivery

All five patches are local review material, with separate Asterinas and GPUI Shell targets. Their source changes remain uncommitted in the existing worktrees. Temporary clean verification worktrees and temporary per-crate OSDK manifests were removed. Nothing was pushed or published, and saved GitHub tokens were not used.

## Additional local patch: strict Vulkan compute benchmark and GitHub Actions

Patch: `asterinas-vulkan-compute-ci.patch`, targeting the same Asterinas base. This is a sixth bounded contribution, not a native NVIDIA or AMD kernel driver. It adds no stub-success driver callbacks and makes no hardware acceleration claim.

The benchmark builds a GLSL compute shader into SPIR-V, selects only integrated/discrete Vulkan 1.2 devices with compute timestamps, and can filter NVIDIA or AMD. NVIDIA candidates must identify NVIDIA's Vulkan userspace driver; this driver ID is also used with NVIDIA's open kernel modules. CPU, virtual and OTHER device types cannot pass. Workload, workgroup and storage limits are checked. The program initializes output to the complement of expected values, submits real compute dispatches, waits on a bounded fence, uses an explicit shader-write-to-host-read memory barrier and noncoherent cache operations when needed, and compares every final output against a host calculation. Only successful correctness and positive timing checks emit passing JSON, including host OS and selected device/driver metadata. The check verifies final output after the dispatch sequence, not an independent readback of every dispatch or a full rendering workload.

The workflow at `asterinas/.github/workflows/graphics_gpu.yml` runs tests and strict C/shader builds for pull requests and pushes on a hosted runner. A separate NVIDIA compute run is manual-only, requires a dispatch on `main`, checks out the exact dispatch commit, uses minimal read-only repository permissions and SHA-pinned actions, requires a `gpu-benchmark` environment and a self-hosted `[self-hosted, linux, x64, nvidia-gpu]` runner, bounds process execution, and retains logs even on failure. Administrators must configure protected-environment reviewers/branch restrictions and an isolated ephemeral GPU runner; the workflow cannot create those safeguards automatically. `.github/actionlint.yaml` registers the custom runner label for linting. No workflow was published or executed remotely.

Validation in this continuation:

- All 13 Python-discovered graphics tests passed, including ten synthetic C device-selection fixtures, workload/CLI checks and a real local negative device-selection run. Fixtures are not hardware evidence.
- C compilation with `-std=c11 -Wall -Wextra -Werror -O2`, shader compilation targeting Vulkan 1.2, shell syntax checks, `git diff --check` and actionlint v1.7.7 passed.
- The explicit NVIDIA benchmark command exited **1**, reporting `No GPU matched the requested vendor filter.` Its stdout was empty: no passing benchmark JSON or GPU timing was produced. The only local Vulkan device is CPU llvmpipe; `/dev/dri`, `/dev/nvidia*` and `/dev/kvm` are absent.
- Evidence: `../evidence/graphics-expanded-unit-tests.log`, `../evidence/graphics-actionlint.log`, `../evidence/gpu-environment.json`, and `../evidence/nvidia-compute-benchmark.{status,stderr,json}`. The last JSON file is deliberately empty because the run failed hardware selection.

Recent official NVIDIA sources still identify **615.78.08**, not an assumed 360.x, as the newest published open-source release: [releases](https://github.com/NVIDIA/open-gpu-kernel-modules/releases), [source and matching-firmware/userspace requirement](https://github.com/NVIDIA/open-gpu-kernel-modules). The already-compiled local source revision remains unchanged at `ca2d03e6fafd4d4b56b383497d931c25b283bfcf`; no updated or re-linked kernel driver is claimed. The benchmark reports the installed host driver version rather than installing or enforcing a particular version.

The unmet acceptance requirement remains **GPUI/Wayland/Vulkan running on real GPU hardware under Asterinas**. Even a future Linux NVIDIA compute pass will not satisfy it. Asterinas still needs the reviewed OS-service/unsafe boundary, memory pinning and device-bound DMA/IOMMU semantics, RM device/ioctl/mmap ABI, GPU initialization/GSP and memory/submission/fence/reset lifecycle, plus DRM synchronization/sharing/display integration. The local audit also identifies an independent native framebuffer bug worth fixing next: bootloader pitch is not preserved through `ostd::boot::BootloaderFramebufferArg`, framebuffer reservation and `FrameBuffer` construction. That fix was not implemented here and would not supply GPU acceleration. Progress toward the user's full hardware acceptance is blocked by both the unfinished native driver port and the absence of an authorized GPU-capable execution environment; a token or a Linux package install alone removes neither blocker.

All six patches and all implementation changes remain local and uncommitted. No saved token was used; no push, PR, publication or remote Actions run occurred.

## Seventh local patch: real NVIDIA NVport component linked into Asterinas

Patch: `asterinas-nvidia-nvport-native-ci.patch`. This is incremental: apply the four existing Asterinas patches listed in `../evidence/nvport/patch-validation.json` first. The two GPUI Shell patches target a different repository and are not prerequisites. All seven patches remain local and uncommitted.

This phase goes beyond benchmark preparation. The new opt-in `ostd-nvidia-nvport` crate compiles NVIDIA 615.78.08's **unmodified** `memory_unix_kernel_os.c` at revision `ca2d03e6fafd4d4b56b383497d931c25b283bfcf`, validates actual upstream C ABI declarations, and supplies real heap, copy, fill, comparison, page-size and fault-aware usercopy OS services through OSTD. The source checkout and toolchain are trusted build inputs; pinning a commit and checking tracked changes is not a hermetic build guarantee. The kernel uses the no-red-zone/non-PIC x86-64 kernel C ABI. A safe owning buffer wrapper bounds NVIDIA's u32-narrowed memory operations and exercises its actual pattern-fill and overlap-move code. Interrupt-context allocation is rejected, and host usercopy explicitly reports unsupported rather than faking native behavior. This is CPU heap memory, not GPU/DMA memory.

`nvidia-nvport-selftest` links this C/Rust component into the real kernel and runs a data-checking CPU memory self-test at the assembler entry. `native-gpu-ci` additionally compiles commit/nonce boot markers. Both complete kernel variants linked successfully. The new C/FFI boundary deliberately extends OSTD's trusted computing base; the kernel assembler itself remains safe Rust. No full RM/modeset object, PCI driver or fake-success GPU callback is added.

The Actions workflow now also compiles/links this real integration on ordinary PR/push CI in the project's kernel-dev container. A separate opt-in manual `asterinas-native-nvidia` job on main uses an administrator-protected `asterinas-native-gpu` environment and `[self-hosted, linux, x64, nvidia-vfio]` runner. It requires prebound VFIO devices and a matching guest tree, builds the current dispatched kernel, runs compute only inside its Asterinas guest, and validates an ordered commit/nonce/benchmark/exit serial chain with NVIDIA hardware identity, explicit 615.78.08 metadata, correctness and finite positive timing requirements. Linux-host compute stays a separately named test and cannot satisfy this chain. The executor never changes PCI bindings or enables no-IOMMU mode. Actual environment protection and runner isolation must be configured by an administrator; YAML does not establish those protections. The chain is trusted-runner evidence, not cryptographic attestation, and a native compute pass would still not certify GPUI/Wayland rendering.

Validation:

- Eight real C/Rust host memory tests passed, covering allocation boundaries/alignment, copy/fill/compare lengths, overlap moves in both directions, partial patterns and invalid operations. `../evidence/nvport/host-tests-final.log`.
- `cargo osdk check --ktests -p ostd-nvidia-nvport` passed. These same eight tests are registered natively; this compile check is **not runtime execution**. `../evidence/nvport/kernel-ktests-check.log`.
- Full kernel builds with `nvidia-nvport-selftest` and `native-gpu-ci` passed. The unstripped linked ELF contained real NVIDIA `portMem*` and Rust `os_*` symbols and had zero undefined symbols. This does not mean the separately compiled, unlinked full RM/modeset objects have been resolved. `../evidence/nvport/kernel-build.log`, `native-ci-kernel-build.log`, `kernel-linked-symbols.txt`, and `kernel-unresolved-symbols.txt`.
- All 30 graphics/orchestration/evidence tests passed, including synthetic VFIO group checks, serial capture, wrong/missing markers, nonce/commit mismatches, software device rejection and unknown/mismatched releases. Synthetic fixtures are never benchmark evidence. `../evidence/nvport/graphics-tests-final.log`.
- Strict C and SPIR-V builds, Rust formatting, Python compilation, actionlint and diff checks passed. Clean application of all Asterinas patches plus the seventh patch, source equality, 30 host tests and unchanged original Git index were verified. Temporary verification worktrees were removed. `../evidence/nvport/patch-validation.json` and `clean-patch-tests.log`.
- A real OSDK build confirmed that the temporary package-local manifest removes inherited interactive `sh -l` arguments and emits exactly `earlycon console=ttyS0 rdinit=/bin/sh -- /aster-native-gpu-init`. The temporary manifest was removed. This checks image construction/configuration, not successful guest execution. `../evidence/nvport/isolated-osdk-validation.json`.
- Initial Multiboot2 TCG boots timed out without serial output. Explicit `earlycon` exposed an actual OSTD kernel page fault at `0xffff80007ffe2321` after xAPIC initialization and **before** the NVport self-test/assembler entry. The machine halted and the bounded run exited 124. Root cause is not proved or fixed here; there is no native memory-test pass. `../evidence/nvport/earlycon-boot.log` and `earlycon-boot.exit`.
- The native GPU entrypoint itself exited 1 on this sandbox because accessible `/dev/kvm` is absent. No NVIDIA GPU is assigned. No native benchmark result or GPU timing was produced. `../evidence/nvport/native-negative.log` and `native-negative.exit`.

Implementation documentation: `../asterinas/ostd/libs/nvidia-nvport/README.md` and `../asterinas/tools/graphics/NATIVE_NVIDIA.md`. The user acceptance target remains unmet: early boot must work; the genuine RM/GSP, pinned/DMA/IOMMU memory, interrupts, device ioctl/mmap ABI, GPU memory/submission/fence/reset/teardown and accelerated DRM/Wayland integration still need engineering and hardware testing. Neither the kernel link nor additional CI configuration replaces that work. No token was accessed, no commit was created, and nothing was pushed, published, authenticated or run remotely.

## Eighth local patch: fix native boot, exercise NVport in-kernel, inspect real PCI resources

Patch: `asterinas-nvidia-boot-pci-tcg.patch`. Apply the four Asterinas prerequisites and then `asterinas-nvidia-nvport-native-ci.patch` first. This section supersedes the earlier unresolved-boot status; the prior logs remain as historical failures. NVIDIA sources remain pinned to the reviewed, unmodified 615.78.08 revision, not an assumed 360.x release.

The page fault at `0xffff80007ffe2321` was reproduced. Multiboot2 did not preserve the reserved RAM region containing ACPI's root/tables when OSTD replaced the boot page table; the physical address lay above the last usable RAM at `0x7ffdf000`. The fix reuses the existing ACPI-root-aware `effective_region_type` logic from other x86 boot paths, retaining the reserved ACPI region through `0x80000000` in this VM without making it allocatable RAM. RSDT, XSDT and EFI-without-ACPI regression fixtures cover that decision. The next exposed failure was an EGA text buffer incorrectly treated as a pixel framebuffer; Multiboot2 now rejects the text format, with a fourth regression test. This does not solve general framebuffer pitch/mask handling or supply accelerated display.

The new, optional `aster-nvidia-platform` component uses Asterinas' existing **real** PCI framework. It validates NVIDIA display identity and BAR0, copies measured BAR ranges/attributes from `BarManager`, rejects zero/overflow resources, and bounds read-only conventional config access. A borrowed unclaimed-device iterator lets the diagnostic inspect enumerated functions without taking ownership. `nvidia-pci-diagnostics` links it into the kernel; `native-gpu-ci` enables it too. It registers no `PciDriver`, never maps/claims a BAR and explicitly reports RM/GSP as uninitialized. The earlier isolated byte-header decoder was removed in favor of this framework-backed path. No NVIDIA resource/config access was exercised against a real GPU here.

Validation and evidence under `../evidence/nvport/`:

- Eight real NVport C/Rust memory tests passed **inside OSTD/Asterinas under QEMU TCG**, not just on the host: `native-eight-tests.log`. Eight host memory tests also passed: `host-tests-continuation.log`.
- The OSTD runner reported **225 passed**, including the four new Multiboot2 regressions: `native-ostd-suite.log`. The VMX/EPT-dependent test body explicitly reported unavailable hardware; this is not VMX hardware validation. An earlier filtered run executed zero tests and is not counted as evidence.
- Three PCI-platform validation tests passed inside the kernel: `native-pci-platform-tests.log`. These validate pure identity/range/config-bounds logic, not GPU hardware.
- The corrected kernel completed the NVport self-test, launched an actual static `/init`, wrote the explicit CPU-only marker from userspace, and invoked the implemented `reboot(POWER_OFF)` syscall: `cpu-userspace-boot.log` and `cpu-userspace-result.json`. The raw QEMU status is **33**, the expected `isa-debug-exit` encoding of guest success, not a benchmark failure. The static init source and its initramfs are retained locally as CPU boot evidence.
- The bounded `run_nvport_smoke.py` executor was run against that real ISO and passed: `tcg-ci-smoke/result.json`. It requires the exact NVport success line and rejects panic-before-marker, empty/truncated output and timeout. Its CPU-only scope does not certify subsequent boot or any GPU activity. Local execution caught and fixed a conflicting QEMU acceleration option that mocked unit tests had not caught.
- All **38** graphics/orchestration/synthetic-evidence tests passed: `graphics-tests-continuation.log`. These include the explicitly compute-only native evidence scope; no fixture is hardware evidence. Actionlint, shell syntax, formatting and diff checks passed.

Ordinary PR/push Actions now install the QEMU/BIOS rescue prerequisites, execute the NVport/PCI-platform/OSTD suites with the same `run_cpu_kernel_tests.sh` entrypoint, build NVport plus PCI diagnostics, boot the ISO under TCG, and retain logs/CPU-only result even on failure. The test executor creates package manifests exclusively, detaches stdin to avoid job-control stops, strips ANSI colors before checking nonzero test counts, and removes its temporary manifests. The separately protected, manual real-NVIDIA/VFIO compute job remains hardware-only and cannot consume CPU/host results. Its validator always marks full GPUI/Wayland acceptance false because compute alone is insufficient. No Actions run has been published or executed remotely.

**Still not implemented:** the full NVIDIA RM/modeset port, actual GSP firmware loading/boot handshake and RM startup, device-bound pinned/DMA/IOMMU services, GPU interrupts, device-node ioctl/mmap ABI, submission/fences/reset/teardown and accelerated DRM/Wayland/GPUI integration. The sandbox has neither `/dev/kvm` nor an assigned NVIDIA GPU. There is no native GPU benchmark result or GPU timing, and the user's full-hardware acceptance remains unmet. A token only authorizes future publication; it cannot supply hardware or these missing kernel components.

All changes and patches remain local and uncommitted. Saved GitHub tokens were not used. No authentication, commit, push, publication, PR or remote Actions execution occurred.

## Next local patch: real RM OS services and native synchronization

Patch: `asterinas-nvidia-rm-os-services.patch`. Apply **after** the full prior ordered Asterinas stack through `asterinas-nvidia-boot-pci-tcg.patch`; GPUI Shell patches still target their separate repository. The incremental patch applies to the preserved starting files and reproduces current source bytes exactly. The Git index remains unchanged. Evidence: `../evidence/nvport-lifecycle/patch-validation.json`.

### Official source pin corrected without changing source trees

The latest official release remains **615.78.08**, not an assumed 360.x. Its public annotated tag object `4bfe30bb78e7364959e59c16cfefd88e8c14670e` peels to `1a82fbfda8f321b056eb3568850f9316c5e465c5`. That commit and the old `ca2d03e6fafd4d4b56b383497d931c25b283bfcf` have the **identical** source tree `e121e9c407c996e4aaca25a801232a53f7dc1b5b` and an empty source diff. The old checkout is preserved; a separate detached checkout is at `../vendor-drivers/open-gpu-kernel-modules-615.78.08-tag`. Build, Actions and provenance pins now use the exact official commit. The tag signature was not independently trusted-signers verified. See `../evidence/nvport-lifecycle/source-pin.json` and `upstream-sources.md`.

### Implemented functionality

Unchanged upstream time/thread/synchronization/allocator-tracking translation units now join the memory port. Real OSTD services supply mutex/semaphore/rwlock blocking and wakeups, IRQ-save spinlocks, scheduler yielding and live-task identity. Spin IRQ guards remain CPU-local; destruction of still-held raw state leaks it rather than dropping its guard on another CPU. Safe semaphore release reports overflow. Try-acquire errors match NVIDIA's timeout-retry status. The new tested OSTD `can_sleep()` query reflects actual task/IRQ/preemption state.

The opt-in platform runtime installs Asterinas monotonic/realtime/nominal-resolution callbacks at first-kthread initialization, after core clocks are initialized. Safe calls check readiness; unguarded foreign time calls halt on unavailable clocks instead of fabricating timestamps. Identity is a live-task equality token, not a POSIX TID. Standalone PCI diagnostics remains independent of NVIDIA source compilation; both kernel NVIDIA features enable the runtime.

The real `nv_get_firmware`/`nv_put_firmware` ABI now acquires/releases distinct handles to immutable guest boot files. A bounded uncompressed-newc parser rejects malformed/truncated archives, relevant duplicate names, links, invalid sizes and cross-release fallback. NVIDIA's original selector supplies the exact filenames. Safe leases execute a genuine C/Rust ABI round trip. This implements file acquisition, **not** GSP validation/DMA/bootstrap.

Allocator dispatch uses unchanged NVIDIA tracking source, not dummy callbacks. Real bounded diagnostic logging includes integer/pointer/string formatting, widths/precisions/lengths, explicit truncation and libc-comparison tests. Floating, `%n` and unknown/custom directives fail explicitly; this is not full libc or every-platform NVIDIA formatting. Upstream debug breakpoints panic instead of silently returning.

### Executed checks

- **14 host C/Rust tests passed**, plus real formatter regressions against libc and **38 graphics regression tests**.
- Ordinary two-CPU QEMU TCG executed **20 NVport**, **4 platform**, and **227 reported OSTD tests** successfully. Three OSTD VMX/VMCS/EPT bodies explicitly skipped unavailable virtualization hardware; they are not hardware passes. Logs and final runner markers: `../evidence/nvport-lifecycle/final-native-suites-v3/`; counts/skips: `test-counts.json`.
- A real full-kernel boot acquired, read all **116,999,192 bytes** of four actual firmware entries on the guest CPU, matched independent host/guest noncryptographic checksums, and released all handles. The real clock/thread/yield round trip ran, static `/init` emitted its CPU-only marker, and poweroff returned debug-exit **33**. Evidence: `../evidence/nvport-lifecycle/real-firmware-boot/result.json` and `serial.log`; reproducer: `../evidence/nvport-lifecycle/run_real_firmware_boot.py`.
- Actionlint, shell syntax, formatting/diff checks and exact patch round-trip checks passed. Prepared ordinary Actions also runs the formatter and optional platform runtime tests. No remote Actions run occurred.

### Preserved failures and limitations

The first ordinary-TCG firmware boot read/released all four files but panicked in the **unchanged** coefficient assertion after implausible TSC calibration. It did not reach `/init` and remains a failed result in `../evidence/nvport-lifecycle/real-firmware-boot-clock-failure/`. The successful repeat explicitly uses **`-icount shift=3,align=off,sleep=off`**, instruction-counted virtual CPU time. It is not clock-accuracy, wall-time or performance evidence and does not fix the general ordinary-TCG clock-calibration issue. Earlier compile failures and the corrected contention-test wakeup-handshake failure remain preserved too.

The official installer transfer repeatedly truncated near its tail. Four complete firmware entries precede that tail and were extracted strictly as data using the explicit local `--partial-archive-cpu-only` mode; the extractor's default still rejects incomplete archives. Individual SHA-256 hashes identify tested bytes, not authenticity: the full installer corruption checksum and vendor signatures were **not verified**. Those limits are recorded in `firmware-provenance.json` and the CPU boot result. No installer or GPU firmware was executed, no host driver was installed, and firmware bytes are neither included in the patch nor uploaded.

### Full NVIDIA/GPUI acceptance is still unmet

There is still no `/dev/kvm`, `/dev/dri` or `/dev/nvidia*` here. **No physical GPU benchmark, GPU timing or native hardware pass exists.** CPU boot and Linux-host compute cannot satisfy that requirement.

Remaining engineering is the real device lifecycle: matched PCI/BAR claim with rollback, per-device/RM private state, device-bound DMA/IOMMU and IRQ/task lifetimes, `nv_start_device`/`rm_init_adapter`, validated firmware upload and `kgspInitRm_IMPL`/`_kgspBootGspRm`, NVIDIA open/ioctl/mmap ABI, GPU allocation/submission/fences/reset, and quiescent `nv_stop_device`/`rm_shutdown_adapter` teardown. Asterinas has reusable PCI/BAR/MMIO/DMA/IRQ/task facilities, but they are not connected to that NVIDIA lifecycle. PCI removal also needs an explicit lifetime/removal policy.

Full display acceptance further requires a real DRM device tied to RM per-open state, GPU GEM, sharing/synchronization, modesetting/scanout, and Wayland/GPUI rendering/presentation. Even a genuine future compute pass would not prove that. The protected native VFIO Actions path remains prepared locally; it needs these kernel paths, an administrator-protected isolated GPU runner, matching guest userspace and verified firmware. A token removes neither engineering nor hardware blockers.

All prior changes are preserved. Everything remains local and uncommitted. No saved token, GitHub authentication, commit, push, publication, PR or remote Actions execution was used.

## Next local patch: reversible NVIDIA PCI/MMIO resource ownership

Patch: `asterinas-nvidia-resource-ownership.patch`, applied **after** all previous ordered Asterinas patches through `asterinas-nvidia-rm-os-services.patch`. The official unmodified NVIDIA source remains 615.78.08 at `1a82fbfda8f321b056eb3568850f9316c5e465c5`; its checkout was not modified. All prior work is preserved, local and uncommitted.

### Real implementation, not successful GPU probe

- `PciBus::reserve_unclaimed` temporarily removes the actual enumerated function from the probe queue and returns it on guard destruction. The bus borrow prevents concurrent probing; there is no success claim or registered fake `PciDriver`. Restricted mutable access prevents independently owned MSI-X state from escaping this reservation API. Existing raw config APIs remain trusted-kernel interfaces, not an adversarial security boundary.
- `NvidiaDeviceResources::prepare_reserved` validates actual NVIDIA display identity, measured BAR metadata and non-prefetchable BAR0, then uses the real `PciMmioGuard`/`MemoryBar::acquire_owned`/`IoMem` stack. The guard rejects already-active bus mastering or MSI/MSI-X, non-D0 power, malformed capabilities and invalid execution context. It maps the real register aperture, enables memory decode with DMA/INTx disabled, bounds aligned CPU register reads, and unwinds failures/releases in mapping-then-command order. It neither reads GPU registers automatically nor starts RM/GSP. Responsive-device command restoration is best-effort; QEMU tests verify exact readback. Physical removal/hotplug is not supported.
- Core enumeration no longer unconditionally enables bus mastering or changes the firmware decode policy. PCI Command writes zero the adjacent Status halfword instead of inadvertently replaying W1C bits. Capability parsing is bounded during initial enumeration, rejects cycles/out-of-range pointers, and does not read a truncated MSI-X header. The previous generic `write16` API remains unchanged; the safe Command path deliberately avoids its RMW semantics.
- IoMem now shares an owned reservation across all clones/slices. The final backing owner drops the virtual mapping before returning the original complete range to its allocator. Sensitive/system-device reservations never recycle into the public allocator. Allocation/free exclude local IRQs around the preemption-only allocator lock; IRQs are restored before mapping/TLB work. An OSTD read-only IRQ-state query supports context validation.

### Executed validation

- Final native two-CPU QEMU TCG suites: **20 NVport, 4 NVIDIA-platform, 5 PCI and 228 reported OSTD tests passed**, with final runner markers. Three VMX/VMCS/EPT-dependent bodies still explicitly skipped unavailable hardware. Evidence: `../evidence/nvidia-ownership/final-native-suites/` and `test-counts.json`.
- The real PCI suite used QEMU's genuine `pci-testdev` identity **1b36:0005**, not a spoofed NVIDIA ID. Eight actual reserve/map/register-read/rollback/reacquire cycles checked live overlap rejection, missing-BAR and occupied-range rollback, alignment/out-of-bounds/overflow rejection, clone/slice last-owner recycling and IRQ-disabled rejection without command mutation. This exercises the actual generic service used by the NVIDIA adapter, not NVIDIA hardware or RM. The separate OSTD test uses a synthetic reservation with an empty virtual area and is only lifetime logic evidence.
- Full kernel compilation/link and instruction-counted TCG boot passed, including genuine NVport clock/thread/yield callbacks, the static `/init` userspace marker and poweroff with raw QEMU debug-exit **33**. The initramfs contains no NVIDIA firmware and no GPU firmware was loaded. Evidence: `../evidence/nvidia-ownership/cpu-boot/result.json`, `serial.log`, `build.log`; reproducer: `../evidence/nvidia-ownership/run_cpu_boot.py`. This is not wall-time, performance or GPU evidence; the ordinary-TCG calibration issue is not fixed.
- **38 graphics/orchestration regression tests passed**. Targeted Rust formatting for all files changed in this phase, actionlint, shell/Python syntax, diff checks, exact incremental patch application/byte round trip and unchanged original Git index passed. The repository-wide formatting command found an already-existing import-order difference in `kernel/comps/drm/src/ioctl/gem.rs`; that unrelated previous contribution was not rewritten. Two initial compile failures (test generic inference and an attempted private IRQ-query call) remain preserved in the initial/v2 suite logs; final runs corrected them.
- Ordinary prepared Actions now explicitly names and runs the PCI resource lifecycle suite through `run_cpu_kernel_tests.sh`, including the required QEMU test device. Manual native NVIDIA/VFIO restrictions remain unchanged. No remote workflow was published or run. Patch validation: `../evidence/nvidia-ownership/patch-validation.json`.

### Acceptance remains blocked, not waived

Source audit of `kernel-open/nvidia/nv-pci.c` and `nv.c` confirms that genuine startup still requires `rm_wait_for_bar_firewall`, `rm_is_supported_pci_device`, RM private state, initial 32-bit and adjusted DMA masks/segment rules, power references, real IRQ/worker lifetimes, `nv_start_device`/`rm_init_adapter`, GSP validation/upload/handshake, and real `nv_stop_device`/`rm_shutdown_adapter` teardown. CXL ownership, ReBAR reassignment and removal need their own implementations. This patch deliberately stops before those unsupported operations, rather than returning pretend probe success.

OSTD DMA constructors currently have no PCI requester identity, device mask or per-device IOMMU-domain argument. A wrapper around global DMA allocation cannot safely supply that missing contract, so this pre-RM path does not enable GPU DMA. The NVIDIA ioctl/mmap ABI, GPU allocation/submission/fences/reset, accelerated DRM sharing/synchronization/display and native Vulkan/Wayland/GPUI stack also remain incomplete. The sandbox still has no `/dev/kvm`, `/dev/nvidia*` or `/dev/dri`; no physical NVIDIA benchmark output or GPU timing exists. The earlier incomplete-installer firmware remains unverified, untrusted CPU-test data and was not loaded/executed here. Genuine acceptance requires a complete port plus physical NVIDIA hardware booting Asterinas with authenticated matching firmware and userspace and returning correct GPU benchmark output; a Linux-host pass cannot replace it.

No saved token was accessed; no authentication, commit, push, PR, publication or remote Actions execution occurred. The next owner must retain the local-only restriction until explicit push authorization arrives.

## Next local patch: requester-bound direct DMA and real EDU transfers

Patch: `asterinas-nvidia-device-dma.patch`, applied **after** `asterinas-nvidia-resource-ownership.patch` and all its prerequisites. The complete nine-patch Asterinas order is recorded in `../evidence/nvidia-device-dma/full-stack-validation.json`; GPUI Shell patches target their separate repository. NVIDIA remains pinned to reviewed 615.78.08 commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`, not a newly researched release.

### Real implementation, not an isolated IOMMU domain

OSTD now supplies validated segment/BDF metadata, inclusive DMA masks, an explicit `DirectDmaContext` and borrowing `DirectDmaBuffer`. Actual zeroed contiguous untyped frames are allocated independently of the legacy global DMA mapping path. Zero/overflow sizes and the last address byte are checked; out-of-mask allocations are freed without truncation or silent remapping. No constrained low-memory/bounce allocator exists, so an unreachable allocation fails. Context/buffer creation rejects IRQ-disabled and interrupt contexts.

`PciMmioGuard::dma_context` derives identity from the real enumerated function; `NvidiaDeviceResources::dma_context` forwards to it. Buffers borrow their context, and the PCI context borrows the MMIO owner/reservation. Production code never enables BME, programs transfers or starts RM/GSP. Hardware quiescence remains the driver's responsibility: lifetimes alone cannot prove completion. NVIDIA initially needs a 32-bit mask until actual RM negotiation establishes otherwise. **Direct DMA has no device isolation and does not implement IOMMU map/unmap.**

The new policy rejects detected remapping, initialized IOMMU registers, DMAR/IVRS, unavailable/invalid ACPI, unsupported architectures and the currently supported TDX runtime. Review caught the dependency's `headers()` iterator silently omitting invalid SDTs. A bounded strict RSDP/RSDT/XSDT inventory now validates every immediate child and full checksum and propagates errors instead of treating corrupt DMAR as absent. This retains OSTD's existing trusted-firmware/immutable linear-map assumption; numeric bounds cannot prove arbitrary corrupt physical pointers are mapped. Parser fixtures test logic, not hardware IOMMU behavior.

### Real device execution and a VT-d boot defect fixed

Using QEMU's actual EDU **1234:11e8**, configured with a 32-bit hardware mask, eight cycles allocate real buffers, poison destinations, enable this test controller only after allocation, transfer **4096 bytes RAM→device→RAM**, check every byte, confirm completion, disable BME and release/reacquire. This is actual device DMA, not CPU copying or mocked completion. Tiny-mask rejection and unchanged command state are also checked. A timeout quarantines both buffers and reports BME readback rather than freeing potentially active DMA memory; that failure branch was reviewed, not fault-injected. Specification: https://www.qemu.org/docs/master/specs/edu.html ; implementation checked: https://raw.githubusercontent.com/qemu/qemu/v8.2.2/hw/misc/edu.c .

The separate Intel-IOMMU boot first faulted at the unmapped alias `0xffff8000fed90008`, recursively overflowing the boot stack and triple-faulting. Evidence is retained in `../evidence/nvidia-device-dma/iommu-debug/`. The legacy VT-d constructor used a physical-to-linear conversion without mapping MMIO. It now owns an actual sensitive, uncacheable `IoMem` mapping retained with its register views and validates dynamic fault/IOTLB offsets before pointer construction. Layouts beyond its one-page window are explicitly unsupported. The final boot reaches genuine `DMA remapping enabled`; the new API then rejects direct DMA without issuing a transfer. This register fix does **not** implement domain isolation or IOTLB revocation.

### Executed validation

- Final two-CPU TCG: **20 NVport, 4 platform, 5 PCI, 5 PCI-with-IOMMU and 236 reported OSTD tests passed**, with final runner markers. Three VMX/VMCS/EPT bodies explicitly skipped unavailable hardware. Logs/counts: `../evidence/nvidia-device-dma/native-suites-v4/` and `test-counts.json`.
- Two real compiler-negative cases were rejected with **E0597**: buffer outliving context and context outliving MMIO. Production source was restored byte-for-byte and rechecked. Evidence/reproducer: `../evidence/nvidia-device-dma/lifetime-checks/` and `check_lifetimes.py`. This proves borrow enforcement, not hardware completion.
- Full kernel build/link and instruction-counted TCG boot passed, with actual NVport clock/thread/yield callbacks, CPU-only `/init` and debug-exit **33**. No NVIDIA firmware was present or loaded. This is not timing/performance evidence. Logs/result/reproducer: `../evidence/nvidia-device-dma/cpu-boot/` and `run_cpu_boot.py`.
- **14 host C/Rust tests**, real formatter/libc regressions, **38 graphics tests**, targeted formatting, actionlint, syntax and diff checks passed. The incremental patch applies with exact byte equality against the preserved incoming tree; original Git index unchanged.
- All **nine** patches applied cleanly to archived HEAD `00a4842d976db6723b8224be2e57ac8105f13ee2`. Of 72 touched files, 71 match current bytes exactly. The only exception is the already-documented preexisting import-order-only difference in `kernel/comps/drm/src/ioctl/gem.rs`; exact diff retained, unrelated source not rewritten. No unexpected mismatch occurred. Do not describe the entire legacy stack as byte-identical.
- Ordinary prepared Actions now requires actual EDU transfer and separate Intel-IOMMU rejection markers, including genuine remapping initialization. Manual protected NVIDIA/VFIO acceptance stays separate. No remote run occurred.

Initial getter-type errors, a parser fixture compilation error and the VT-d boot failure remain preserved in the initial/v2/v3 logs. Final v4 runs corrected them; the earlier results are failures, not passes.

### Full NVIDIA, RM/GSP, display and GPU acceptance remain unmet

A genuine per-requester remapping backend still needs multi-DRHD segment/scope/bridge-alias resolution, RMRR policy, distinct owned domain IDs/roots, mask-constrained IOVA allocation, transactional mappings, completed context/IOTLB/device-TLB invalidation and ordered memory/domain reuse. The existing shared mapper and missing unmap revocation were not relabeled as isolation. Streaming pin/map, low-memory allocation and maximum-segment/boundary negotiation remain absent.

Supported-chip/BAR-firewall checks, adapter-private state, actual power/IRQ/worker lifetimes and the reviewed foreign-code boundary still prevent genuine `nv_start_device`/`rm_init_adapter` integration. GSP validation/upload/handshake and shutdown need authenticated matching firmware and supported hardware. No success callback bypasses these gaps. The truncated-installer firmware remains unverified CPU-test-only data and was not used here. NVIDIA ioctl/mmap, GPU memory/submission/fences/reset, GPU GEM/sharing/synchronization/atomic KMS and native Vulkan/Wayland/GPUI presentation remain unfinished engineering.

The sandbox has no `/dev/kvm`, `/dev/nvidia*` or `/dev/dri`. **No native NVIDIA benchmark, GPU timing, RM/GSP initialization or full display/GPUI acceptance exists.** Both the remaining port and an actual GPU-capable Asterinas runner are required; Linux-host compute cannot replace either. Everything remains local and uncommitted: no saved token, authentication, commit, push, publication, PR or remote Actions execution occurred.


## Décimo patch Asterinas: domínios Intel VT-d limitados com execução DMA real

Patch: `asterinas-nvidia-isolated-vtd.patch`, aplicado **depois** de `asterinas-nvidia-device-dma.patch` e seus oito predecessores Asterinas. Os patches GPUI Shell continuam separados. A sequência completa está em `../evidence/nvidia-isolated-vtd/full-stack-validation.json`. A [lista oficial de releases NVIDIA](https://github.com/NVIDIA/open-gpu-kernel-modules/releases) foi consultada novamente e marca **615.78.08**, commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`, como Latest; o pin revisado foi mantido, sem substituir fontes ou firmware.

### Implementação concreta e limite de segurança

O novo caminho possui raiz e páginas de tradução próprias por requester, DIDs exclusivos 1–15 (zero permanece com o mapper legado), alocação de RAM não tipada, IOVAs limitados pela máscara inclusiva e janela conservadora de 38 bits, sem reutilização de IOVA durante a vida do domínio. A publicação das tabelas mantém a propriedade das páginas antes de expor ponteiros ao hardware; cache lines são explicitamente descarregadas com CLFLUSH/MFENCE, inclusive no QEMU sem ECAP.C. Present é retirado antes de trocar um contexto; a palavra alta com AW/DID é publicada antes da palavra baixa com Present.

Map/unmap/detach aguardam descritores reais de context-cache e IOTLB globais, drenagem de leituras/escritas e Wait/Fence. Falha parcial de map limpa as folhas antes da invalidação; páginas intermediárias permanecem possuídas até detach concluído. `IsolatedDmaBuffer::revoke` devolve a RAM ainda possuída somente depois da revogação concluída. Timeout envenena o backend e mantém RAM, tabelas, DID e requester em quarentena; não é convertido em sucesso nem tenta liberar memória potencialmente acessível. O ramo de timeout de invalidação foi revisado, mas não foi fault-injected. A normalização do tail da fila também corrige o retorno inválido de índice 256 ao completar uma volta.

A política ACPI estrita valida inventário/checksums e aceita somente um DRHD segmento zero. Para o novo caminho, um endpoint precisa constar explicitamente em um escopo tipo 1 de um salto no barramento zero; duplicatas, bridges, caminhos de múltiplos saltos, múltiplos DRHDs, RMRR, IVRS concorrente e estruturas não suportadas são rejeitados. O escopo IOAPIC canônico do QEMU (barramento ff, caminho 00.0) é aceito sem autorizar requester PCI. `INCLUDE_PCI_ALL` pode ser reconhecido pelo controlador, mas **não autoriza implicitamente nenhum domínio novo**. Permanece a suposição existente de firmware confiável e regiões ACPI imutáveis realmente presentes no linear map; limites numéricos não contêm firmware hostil.

`PciMmioGuard::isolated_dma_context` e `NvidiaDeviceResources::isolated_dma_context` ligam o domínio ao empréstimo PCI/MMIO real. O wrapper rejeita BME ativo, funções ausentes, header não-zero/multifunção, função diferente de zero, classe bridge, PCIe e cadeias convencionais malformadas. O primitivo OSTD recebe metadata BDF e não é autenticação de dispositivo; somente mapeia armazenamento não tipado próprio. A associação de posse é feita pelo wrapper PCI seguro, sem introduzir unsafe na camada PCI. TT=00 não habilita Device-TLB/translated requests; ATS/PRI/PASID e aliases PCIe continuam fora do subconjunto revisado. O mapper compartilhado antigo continua existindo para outros drivers: não houve conversão de todo o sistema a isolamento.

**GPUs NVIDIA PCIe modernas continuam explicitamente recusadas.** Resolver PCIe/bridge/aliases e ampliar o roteamento real é requisito antes de NVIDIA DMA. Nenhum código de produção habilita BME, inicia transferências ou chama RM/GSP.

### Provas executadas no estado final

- O runner final completo terminou com **exit 0** em `../evidence/nvidia-isolated-vtd/native-suites-v4.log`: **20 NVport, 4 platform, 5 PCI, 5 PCI-com-IOMMU e 244 testes reportados OSTD**. Três corpos VMX/VMCS/EPT continuam explicitamente skipped por hardware indisponível; não são validação VMX. Contagens em `test-counts.json`.
- EDU real do QEMU: oito ciclos de **4096 bytes RAM→dispositivo→RAM** através de tabelas privadas, comparação byte a byte, máscara impossível/contexto duplicado rejeitados, BME desligado antes de liberar recursos e detach/reaquisição. Não é memcpy CPU nem mock de conclusão.
- Dois controladores EDU reais simultâneos, 00:05.0 e 00:06.0, receberam DIDs distintos e **os mesmos IOVAs 4096/8192 mapeando RAM diferente**. Transferências e padrões distintos tiveram readback integral correto, exercitando seleção efetiva por requester. A extração de duas funções do bus existe apenas no cfg de teste, sem API de produção que contorne reservas.
- Após revogar o destino mantendo a mesma página física possuída e envenenada para CPU, oito escritas EDU no IOVA antigo 0x2000 foram bloqueadas. O log contém **oito faults VT-d reais**, requester 0x28, motivo 5, página 2, e a RAM permaneceu inalterada. O runner exige esses faults, não somente um marcador de sucesso. A confirmação antiga de faults escrevia F/PFO como zero apesar de serem W1C; agora escreve um, percorre circularmente os registros e permite registrar/confirmar todos os oito faults. Evidência: `native-suites-v4/native-pci-iommu-tests.log`.
- Build/link do kernel completo e **dois boots da mesma ISO**, com e sem Intel VT-d, chegaram ao `/init` real e terminaram em debug-exit **33**, esperado como sucesso. NVport executou seus callbacks reais; não havia firmware NVIDIA. Resultados/serial/hash em `../evidence/nvidia-isolated-vtd/cpu-boot-final/` e `full-boot-final.log`. TCG instruction-counted é regressão funcional, não timing/performance.
- Dois casos reais de compilação receberam **E0597**: buffer sobrevivendo ao contexto e contexto sobrevivendo a MMIO. A fonte de produção foi restaurada e recompilada; `lifetime-checks-v2/result.json`. Isso prova empréstimos, não quiescência do hardware.
- **38 testes host gráficos/orquestração**, rustfmt, actionlint, sintaxe Bash, compilação Python e diff checks passaram. Os 20 arquivos do patch incremental reproduzem exatamente os bytes finais; índice Git inalterado. Os **dez** patches aplicam limpos sobre `00a4842d976db6723b8224be2e57ac8105f13ee2`: 79 dos 80 arquivos tocados são byte-idênticos; a única exceção continua a diferença preexistente de ordem de imports em `kernel/comps/drm/src/ioctl/gem.rs`. Não há divergência inesperada.

As falhas iniciais de compilação e o primeiro boot recusando o DRHD explícito do QEMU foram preservados. A execução v3 teve todos os testes nativos positivos, mas o shell terminou 127 porque seu arquivo foi reescrito durante a execução; **não é contada como passagem do runner**. A repetição v4 sem alterações concorrentes e os dois boots finais terminaram com código zero. Fontes finais conferem com os hashes do patch. O workflow local já chama esse runner e agora exige as duas EDU e os faults reais; **nenhum GitHub Actions remoto foi executado**.

### O alvo completo ainda não foi atingido

Além das restrições PCIe/topologia/IOMMU acima, faltam supported-chip/BAR-firewall, estado privado do adapter, power/IRQ/workers, integração real `nv_start_device`/`rm_init_adapter`, upload/validação/handshake GSP e shutdown, ioctl/mmap NVIDIA, memória/submissão/fences/reset GPU e integração GEM/compartilhamento/syncobj/atomic KMS, Vulkan/Wayland/GPUI. Não foram adicionados callbacks de sucesso fictícios. Firmware extraído anteriormente do instalador truncado continua não verificado e não foi carregado.

O sandbox continua sem `/dev/kvm`, `/dev/nvidia*` e `/dev/dri`. **Não existe benchmark NVIDIA nativo, timing de GPU, inicialização RM/GSP nem aceitação GPUI acelerada.** EDU e Linux-host Vulkan não satisfazem essa aceitação. São necessários tanto o restante do port quanto um runner Asterinas autorizado com GPU NVIDIA e versões correspondentes. Tudo permanece local e não commitado: nenhum token salvo foi acessado, nenhuma autenticação, commit, push, publicação, PR ou execução remota ocorreu.


## Décimo primeiro patch Asterinas: PCIe/DMAR/ACS com DMA real NVMe

Patch incremental: `asterinas-nvidia-pcie-topology.patch`, após os dez patches Asterinas anteriores. Fonte NVIDIA permanece fixada em **615.78.08**, commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`; não foi substituída por uma versão 360.x presumida. **Não é o driver NVIDIA completo.** Evidências e reprodutores: `../evidence/nvidia-pcie-topology/`, com índice `handoff-manifest.json`.

### Implementação e subconjunto seguro

A política DMAR preserva escopos/caminhos com limites de oito saltos e 128 escopos. Endpoint tipo 1 corresponde ao caminho completo ou ao BDF/barramento final quando o firmware emite um caminho de um salto começando no barramento do endpoint. Bridge tipo 2 enraizada no bus zero autoriza apenas descendentes estritos, nunca a própria bridge. Escopos de bridge iniciando fora do bus zero não autorizam isoladamente: esta API não recebe os barramentos intermediários. O wrapper PCI verifica a rota em configuração real; metadata OSTD não autentica hardware. Duplicatas/sobreposições incompatíveis, caminhos malformados, RMRR/ATSR e múltiplos DRHDs continuam recusados. INCLUDE_PCI_ALL não autoriza implicitamente um domínio.

A associação de segmento não presume que a primeira região ECAM seja segmento zero: quando MCFG existe, a política exige uma única tabela de 60 bytes e uma única região segmento zero iniciando no bus zero, reservados zero, alinhamento, bounds e aperture sem overflow. Múltiplas regiões/tabelas/segmentos são recusadas. MCFG ausente preserva o caminho convencional CF8; isto não implementa PCI global multi-segmento.

O wrapper admite um endpoint PCIe tipo zero, função única, devfn 00.0 imediatamente abaixo de um root port PCIe real no bus zero, com secondary=subordinate e rota única. Switches, peers extras, bridges legadas, aliases e rotas ambíguas são recusados. ECAM estendido tem leituras falíveis e validação de cabeçalhos, versão, limites/ciclos/lacunas. ATS/PRI/PASID, ARI/SR-IOV e multifunção são recusados. ACS Source Validation, P2P Request/Completion Redirect e Upstream Forwarding precisam existir e estar ativos; Direct Translated P2P ativo é recusado. Translation Blocking opcional não é exigido porque serviços de tradução do endpoint são recusados e VT-d TT=00 rejeita pedidos traduzidos. A produção só lê configuração, não programa ACS nem habilita BME. Rota/ACS precisam permanecer estáveis; hotplug não é suportado. GPUs NVIDIA multifunção comuns ainda são recusadas.

### Provas do estado final

- Runner **v4 exit 0**: **20 NVport, 4 platform, 8 PCI, 8 PCI+IOMMU e 247 testes reportados OSTD**. Três corpos VMX/VMCS/EPT explicitamente skipped; não são validação VMX. Contagens/hashes em `test-counts.json`.
- **NVMe PCIe verdadeiro** atrás de root port verdadeiro executou Identify Controller com SQ, CQ e payload privados via VT-d. CID/status/phase, SQ head/ID, vendor/serial/model foram conferidos nos dados escritos pelo controlador. CC.EN desligado, CSTS.RDY=0 confirmado e BME desligado antes de liberar filas/payload. Isto é DMA PCIe funcional, não benchmark GPU. Casos negativos reais: controles ACS desligados, root port sem ACS (`disable-acs=on`) e EDU atrás de bridge legada, todos recusados antes de DMA. Não houve alteração/spoof de IDs PCI.
- Oito ciclos EDU de página completa, dois requesters com DIDs distintos e IOVAs iguais mapeando RAM diferente, e **oito faults VT-d reais** bloqueando o IOVA revogado com a RAM ainda possuída continuam positivos.
- Kernel completo recompilado/linkado. **Três boots da mesma ISO** — sem IOMMU, com VT-d, e VT-d com root port/NVMe/bridge reais — chegaram ao `/init` estático e terminaram em debug-exit **33**. Resultados/seriais/hashes em `cpu-boot/`. TCG instruction-counted não é timing/performance; nenhum firmware NVIDIA foi carregado.
- **38 testes host** gráficos/orquestração, teste C de formatter com fonte fixada, rustfmt, actionlint, Bash/Python e diff checks passaram. Os 12 arquivos do patch reproduzem os bytes finais. Onze patches aplicam limpos sobre a base preservada: 81/82 arquivos são byte-idênticos; a única exceção continua a ordem de imports preexistente em `kernel/comps/drm/src/ioctl/gem.rs`. Nenhuma divergência inesperada; índice Git inalterado.

Falhas intermediárias foram preservadas, não contadas como passagem: teste DMAR com nome conflitante; v1 recusou corretamente BME deixado ativo pelo firmware no NVMe. O fixture passou a resetar/parar o controlador de verdade e aguardar RDY=0 **antes** de limpar BME, sem relaxar a API de produção. V2 executou Identify, mas expôs 0xffffffff no espaço estendido inexistente de EDU convencional; somente o caminho sem capability PCIe reconhece esse valor como ausência, mantendo PCIe estrito. V3 passou ambos os conjuntos PCI, mas um teste OSTD ainda esperava a antiga recusa de todo barramento downstream; foi corrigido para exigir rota no construtor legado e continuar recusando 00:00.0. V4 e os três boots passaram sem alteração concorrente de fontes. O primeiro formatter sem NVIDIA_SOURCE_DIR foi recusado; a execução com a fonte oficial fixada passou.

### RM/GSP e alvo completo continuam pendentes

A auditoria do código upstream real confirmou a ordem: `nv-pci.c` exige rm_wait_for_bar_firewall/rm_is_supported_pci_device; `nv.c::nv_start_device` prepara power refs/stacks, MSI/MSI-X/IRQ e workers antes de rm_init_adapter; `osinit.c` prepara DMA, seleciona firmware pela família real e só então inicializa GSP. Um domínio ou leitura de BAR não substitui essa sequência.

Próxima integração necessária: posse IRQ/MSI-X acoplada ao dispositivo/domínio, handler registrado antes de unmask, bottom-half/workers e teardown/quiescência sincronizados, private adapter/power/stacks e ligação ABI do RM completo. O CapabilityMsixData::new legado habilita BME por conta própria e não fornece esse contrato; não foi usado para contornar a ordem. Suporte aos peers multifunção/aliases, supported-chip/BAR-firewall, firmware autenticado correspondente, upload/handshake/shutdown GSP, ioctl/mmap NVIDIA, memória/submissão/fences/reset GPU, DRM/GEM/syncobj/atomic KMS e Vulkan/Wayland/GPUI seguem faltando. Firmware extraído do instalador truncado continua dado CPU-only não verificado, sem execução.

O sandbox segue sem `/dev/kvm`, `/dev/nvidia*` e `/dev/dri`. **Não há RM/GSP inicializado, benchmark NVIDIA nativo, timing GPU, Actions remoto ou aceitação GPUI acelerada.** NVMe/EDU/QEMU e Linux-host não satisfazem esse alvo. O runner atualizado já é chamado pelo workflow preparado, mas nada foi executado remotamente. Tudo permanece local e não commitado: nenhum token salvo, autenticação, commit, push, publicação ou PR.

## Décimo segundo patch Asterinas: grupo PCIe multifunção com domínio único e teardown IRQ corrigido

Patch incremental: `asterinas-nvidia-multifunction.patch`, após os onze patches Asterinas anteriores, terminando em `asterinas-nvidia-pcie-topology.patch`. A [lista oficial de releases NVIDIA](https://github.com/NVIDIA/open-gpu-kernel-modules/releases) foi consultada nesta continuação: **615.78.08** continua marcada Latest, commit revisado `1a82fbfda8f321b056eb3568850f9316c5e465c5`. Não se presumiu uma versão 360.x e não houve troca de fonte/firmware. Evidências/reprodutores: `../evidence/nvidia-multifunction/`.

### Posse e tradução do grupo, sem habilitar produção

`PciBus::reserve_unclaimed_group` enumera fisicamente todas as oito funções do slot, independentemente do bit multifunção, exige função zero, cabeçalhos type-0, BME desligado e disponibilidade de cada membro na fila não reivindicada. A reserva toma todas as funções ou nenhuma, revalida presença/BME e mantém o empréstimo mutável do barramento, impedindo probing simultâneo. Uma função ausente da fila, já possuída ou ativa impede a reserva inteira. A API não para hardware e não interpreta BME-off como quiescência.

`PciGroupReservation::prepare_mmio` toma um BAR de memória selecionado por função; falha em qualquer membro desfaz os mapeamentos anteriores e restaura seus comandos. O contexto DMA toma emprestado esse conjunto, que toma emprestada a reserva: buffer → contexto → MMIO → grupo → barramento. Cada função precisa passar individualmente pelo mesmo root port ACS-verificado, rota PCIe real e autorização DMAR. A lista deve corresponder ao slot completo, não a um subconjunto. A inspeção downstream também rejeita funções órfãs/peers em outros slots, mesmo sem função zero. A produção não escreve ACS, não habilita BME e não inicia RM/GSP.

O backend VT-d reserva todos os BDFs e um DID antes de publicar contextos. Todos apontam para **a mesma raiz possuída e o mesmo DID**, com revogação/invalidação concluída antes e depois da publicação. Detach limpa todos os membros e só recicla tabelas/DID/identidades após conclusão de invalidação. Erro ambíguo envenena o backend e mantém o grupo inteiro em quarentena. O timeout de invalidação não foi fault-injected; esse caminho foi revisado, não contado como teste positivo.

O subconjunto ainda é conservador: slot zero imediatamente abaixo de um único root port segmento zero com ACS correto, firmware suportado e BAR MMIO utilizável em cada função. Switches, ARI/SR-IOV, ATS/PRI/PASID, aliases e hotplug/configuração dinâmica continuam recusados. Funções do grupo compartilham uma fronteira de confiança; não há isolamento entre elas. Isso não certifica suporte a GPUs NVIDIA multifunção reais.

### DMA real e regressão IRQ reproduzida

O runner final **v3 exit 0** reportou **20 NVport, 4 platform, 9 PCI, 9 PCI+IOMMU e 249 testes OSTD**. Três corpos VMX/VMCS/EPT continuam explicitamente skipped. Contagens/hashes: `test-counts.json`; logs completos em `native-suites-v3/`.

Dois controladores NVMe QEMU genuínos, funções 0 e 1 do mesmo slot, completaram Identify Controller por DMA privado, em dois ciclos reais de attach/detach/reacquire do domínio compartilhado. Foram conferidos CID, status/phase, SQ head/ID, VID, serial/model escritos por cada controlador, DID reciclado após detach e recusa `DomainBusy` de uma segunda vinculação enquanto o grupo estava vivo. Cada controlador foi parado, RDY=0 confirmado e BME desligado antes da liberação de filas/payload. Identidades PCI não foram alteradas/spoofadas. O fixture programa/restaura ACS explicitamente; a API de produção exige controles já ativos e permanece read-only.

Casos negativos reais: função irmã retirada da fila, BME ativo preservado sem limpeza pela produção, BAR inválido no segundo membro com rollback do primeiro, caminho individual/subconjunto recusado e um segundo slot genuíno com NVMe função zero + EDU convencional função um recusado por incompatibilidade PCIe. O boot sem IOMMU recusa DMA de grupo; o boot com IOMMU exige DMA positivo. Os testes antigos de NVMe individual, ACS ausente/desligado, bridge legada, EDU e oito faults VT-d reais por IOVA revogado continuam passando.

A investigação da próxima etapa IRQ encontrou uma falha real de ordem: `IrqLine` liberava o último `InnerHandle`/número antes de destruir seus callbacks, permitindo reutilização prematura. O novo Drop remove/sincroniza callbacks pelo writer lock antes da liberação do número. Um teste real de kernel usa probes de destruição e clones: **falha com a ordem antiga** (número já realocável, resultado 2) e **passa com a correção** (ainda reservado, resultado 1). Logs/prova: `irq-regression/`. O primeiro parser do reproducer não removia ANSI; os logs originais já mostravam exit 0 e um teste passando na versão corrigida. A revalidação desses mesmos logs corrigiu apenas o parser, sem inventar execução. Isso não implementa MSI-X transacional, IRQ threadada ou workers NVIDIA.

### Integridade local e limitações

**38 testes host gráficos/orquestração** e o formatter C real da fonte NVIDIA fixada passaram. O workflow preparado `graphics_gpu.yml` passou actionlint direcionado; o actionlint global também expôs problemas preexistentes em outros workflows e não foi apresentado como passagem global. A primeira compilação ampla omitiu VDSO_LIBRARY_DIR; a execução corrigida com o vDSO local passou. A tentativa v1 das suítes omitiu NVIDIA_SOURCE_DIR e foi recusada pelo build, como esperado. V2 passou os testes individuais, mas a API recusou corretamente o novo root port com ACS desligado; o fixture v3 passou a habilitar/restaurar os controles reais, sem relaxar a produção. A formatação da nova suíte foi normalizada para a edição Rust do projeto.

Os **12 patches** aplicam limpos sobre a base preservada; 83/84 arquivos são byte-idênticos. A única diferença continua sendo a ordem de imports preexistente em `kernel/comps/drm/src/ioctl/gem.rs`, sem divergência inesperada. O patch incremental reproduz os 12 arquivos alterados exatamente e o índice Git original permanece inalterado. Procedimentos e resultados: `make_patch.py`, `patch-validation.json`, `validate_stack.py` e `full-stack-validation.json`.

A auditoria do upstream confirma que `rm_wait_for_bar_firewall` chama lógica real do GPU manager e não pode ser substituído por uma leitura de BAR. `nv_start_device` depende de power/stacks, MSI/MSI-X/IRQ e filas antes de `rm_init_adapter`. O MSI-X legado Asterinas ainda habilita BME por conta própria e não possui teardown reversível suficiente; não foi usado para contornar essa fronteira. Faltam supported-chip/firewall, estado privado/power/stacks do adapter, IRQ/workers completos, handshake/shutdown GSP com firmware autenticado, ioctl/mmap NVIDIA, memória/submissão/fences/reset, DRM/GEM/syncobj/atomic KMS e Vulkan/Wayland/GPUI nativos.

O sandbox segue sem `/dev/nvidia*`, `/dev/dri` e `/dev/kvm`. **Não existe benchmark NVIDIA nativo, timing GPU, RM/GSP inicializado ou aceitação GPUI acelerada.** Firmware do instalador truncado continua dado CPU-only não verificado e não foi carregado. NVMe/EDU/QEMU não são substitutos de uma GPU NVIDIA. O runner preparado exige as novas provas reais, mas nenhum Actions remoto foi executado.

O usuário autorizou posteriormente commit/push para o repositório privado `cybermanju-shell`, com orientação explícita nesta etapa para concluir a validação e entregar o workspace estável **sem executar commit/push aqui**. Nenhum token salvo foi acessado; não houve autenticação, commit, push, publicação ou PR nesta continuação. O coordenador recebe o estado local para decidir e executar a próxima ação autorizada.

### Fechamento da validação e workspace estável

As quatro provas negativas de compilador foram executadas contra as APIs reais: buffer sobrevivendo ao contexto, contexto sobrevivendo ao MMIO e MMIO sobrevivendo à reserva foram recusados com **E0597**; tentativa de reservar novamente o barramento com o grupo vivo foi recusada com **E0499**. A fonte foi restaurada byte a byte e recompilada com sucesso. Logs/resultados/reprodutor em `lifetime-checks/` e `check_lifetimes.py`. São provas de borrow/lifetime, não de quiescência física.

O kernel completo final foi recompilado e linkado. Três boots da mesma ISO — normal, VT-d e VT-d com dois NVMe genuínos nas funções 0/1 do mesmo slot PCIe — executaram os callbacks NVport reais e chegaram ao `/init` estático, com marcador userspace e poweroff/debug-exit **33**. Os três `result-*.json` em `cpu-boot/` registram sucesso funcional, hashes da ISO/initramfs/serial e explicitamente `hardware_gpu=false`, `rm_gsp_initialized=false`, `gpu_firmware_loaded=false`. Reprodutor: `run_cpu_boot.py`. TCG instruction-counted não é medição de desempenho.

As manifestações OSDK temporárias foram removidas pelos próprios runners. Nenhuma etapa de commit/push ou autenticação foi iniciada após a autorização do usuário. O novo patch, o índice de evidências e estas notas são a entrega local para o coordenador; nenhum resultado CPU/QEMU foi convertido em sucesso de benchmark GPU.

## Décimo terceiro patch Asterinas: identidade e teardown reais de callbacks IRQ

Patch incremental: `asterinas-nvidia-irq-callbacks.patch`, aplicado **depois** de `asterinas-nvidia-multifunction.patch` e dos onze predecessores Asterinas. GPUI Shell continua na árvore separada. Evidências: `../evidence/nvidia-irq-callbacks/`; sequência completa em `full-stack-validation.json`. A [lista oficial NVIDIA](https://github.com/NVIDIA/open-gpu-kernel-modules/releases) foi aberta novamente: **615.78.08**, commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`, continua Latest. Fonte e firmware não foram substituídos.

### Implementação nativa

`ostd/src/irq/top_half.rs` identificava callbacks pelo endereço dos dados de `Box<closure>`. Closures distintas de tamanho zero podem compartilhar o ponteiro dangling: descartar a segunda inscrição removia e destruía a primeira, deixando o callback errado registrado. Agora inscrição e handle possuem `Arc<IrqCallbackFunction>` e usam `Arc::ptr_eq`, com identidade de alocação real, sem nova dependência, unsafe adicional ou callback fictício. Os contratos oficiais de [Box zero-sized](https://doc.rust-lang.org/std/boxed/index.html) e [Arc::ptr_eq](https://doc.rust-lang.org/std/sync/struct.Arc.html) foram lidos antes da mudança.

Os captures também eram destruídos dentro do writer lock do registro IRQ; destructors que consultassem o registro podiam bloquear para sempre. A remoção agora aguarda leitores/dispatch em andamento sob o lock, mas libera as referências e os captures somente fora dele. O último proprietário do número IRQ continua vivo até terminar a destruição, preservando a correção anterior. Descartar uma linha dentro do próprio callback continua não suportado: dispatch mantém read lock. Isso não implementa mascaramento do dispositivo, drenagem física MSI/MSI-X ou workers NVIDIA.

### Provas reais desta execução

Dois testes de kernel reproduzem os defeitos na produção anterior. O primeiro usa duas closures comprovadamente zero-sized, com destructors distintos e atômicos; remover a segunda inscrição deve destruir somente o segundo capture e manter o IRQ reservado até o último clone. O segundo captura um probe cujo destructor usa `try_read()` no lock real: expõe o lock indevidamente mantido sem travar a execução negativa. Ambos os baselines antigos falharam por assertion dentro do guest, não por compilação. A implementação corrigida passou a suíte OSTD integral, que inclui os cinco testes IRQ. Reprodutor e logs: `check_regressions.py` e `regression-results.json`; fonte restaurada byte a byte após as comparações.

A matriz final `run_cpu_kernel_tests.sh` terminou **exit 0**: **20 NVport, 4 platform, 9 PCI, 9 PCI-com-IOMMU e 251 testes reportados OSTD**. Três corpos VMX/VMCS/EPT continuam explicitamente skipped por hardware ausente, sem validação VMX. As provas anteriores EDU/NVMe PCIe/multifunção e oito faults VT-d reais de IOVA revogado continuam exigidas e passaram. Contagens e hashes: `test-counts.json`; logs em `native-suites/`. **38 testes host** dos validadores gráficos/orquestração passaram; os fixtures negativos não representam execução de driver GPU.

O kernel completo foi recompilado/linkado e a mesma ISO passou três boots funcionais: normal, VT-d e VT-d com dois NVMe QEMU nas funções 0/1 do mesmo slot PCIe. Todos alcançaram `/init`, callbacks NVport reais e debug-exit **33**. `cpu-boot/result-*.json` registra explicitamente `hardware_gpu=false`, `rm_gsp_initialized=false`, `gpu_firmware_loaded=false`. TCG instruction-counted não é evidência de desempenho ou GPU.

### Integridade, falhas preservadas e continuação

O patch incremental reproduz exatamente a fonte final. Os **13 patches** aplicam limpos sobre a base preservada: **83/84 arquivos** são byte-idênticos. A única exceção permanece a ordem preexistente de imports em `kernel/comps/drm/src/ioctl/gem.rs`, sem divergência inesperada. Rustfmt direcionado, diff check, sintaxe Python e actionlint do workflow gráfico passaram. Índice Git original inalterado; manifestações OSDK temporárias removidas.

O parser inicial não reconheceu `Panicked` com maiúscula, embora o guest já demonstrasse a regressão ZST; os logs/resultados foram preservados e o parser corrigido. Um filtro inicial de módulo executou zero testes e foi corretamente recusado, não contado como passagem: o final executou toda a suíte OSTD. A chamada direta do runner foi recusada por permissão (126); executá-lo explicitamente com Bash passou. O sumarizador inicialmente consultou o diretório histórico e recusou ausência de cinco logs; o caminho foi corrigido antes das contagens finais. Nada disso foi tratado como sucesso GPU.

Próxima frente concreta: MSI-X ainda habilita BME implicitamente na aquisição e não oferece posse/teardown transacional suficiente ao adapter NVIDIA. Separar configuração mascarada, registro de handler, ativação e quiescência real requer preservar os consumidores atuais e validar em endpoint QEMU verdadeiro antes de ligar RM. A auditoria também encontrou indexação direta em `irq_mut` apesar do retorno Option; esse arquivo não foi alterado aqui. Não contornar supported-chip/BAR firewall, private state/power/stacks, IRQ/workers, `nv_start_device`/`rm_init_adapter` ou GSP com sucesso simulado.

A correção IRQ é infraestrutura comum aos drivers AMD/NVIDIA; **não foi implementado um novo backend AMD nem completado NVIDIA nesta execução**. RM/GSP com firmware autenticado correspondente, ioctl/mmap, memória/submissão/fences/reset GPU, DRM/GEM/syncobj/atomic KMS e Vulkan/Wayland/GPUI acelerados continuam incompletos. A política VT-d mantém seu subconjunto conservador; topologias não foram relaxadas sem prova.

Hardware novamente conferido: sem `/dev/nvidia*`, `/dev/dri` ou `/dev/kvm`; nenhum dispositivo PCI exposto no sysfs desta execução. **Não existe benchmark GPU físico sob Asterinas nem aceitação gráfica.** EDU/NVMe/QEMU e Vulkan Linux-host não substituem essa exigência. Nenhum firmware GPU foi carregado. A instrução local-only vigente foi mantida: nenhum token salvo, autenticação, commit, push, PR, publicação ou execução remota Actions. O agendamento existente não foi alterado.
