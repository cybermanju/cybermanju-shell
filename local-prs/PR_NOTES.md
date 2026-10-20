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

## Continuação local das 19:00 UTC: liberar armazenamento ao aposentar IRQ

O estado de `PR_NOTES.md` encerrava no décimo terceiro patch, mas a árvore compartilhada já continha trabalho MSI-X adicional não finalizado, em `../evidence/nvidia-msix-transaction/`: `msix_transaction.rs`, testes NVMe/IRQ e `IrqLine::retire`. Havia compilação/execução concorrente nessa mesma árvore. Para não sobrescrever fontes ou invalidar as evidências da outra execução, esta continuação fez uma cópia independente em `/workspace/asterinas-gpui/asterinas-irq-retire-local`, com target e OSDK próprios. **A correção descrita abaixo está implementada e testada nessa cópia; não foi mesclada ao repositório compartilhado.** A constatação da concorrência é uma mudança operacional em relação à expectativa de workspace exclusivo.

### Código real implementado

`IrqLine::retire` retirava os callbacks com `Vec::clear()` e depois esquecia o handle para manter o número IRQ reservado como sink de mensagens atrasadas. `clear()` destrói elementos, mas mantém a capacidade alocada: essa memória de armazenamento era perdida desnecessariamente em cada aposentadoria MSI-X. A nova rotina privada `disconnect_callbacks` usa `drop(core::mem::take(&mut self.callbacks))`, chamada por `retire` e Drop. Isso remove inscrições, espera os leitores do registro, destrói captures fora do lock e devolve o armazenamento do Vec; somente a retenção conservadora do handle/ID IRQ permanece. Nenhum número aposentado é reciclado nem foi alegada drenagem APIC/IRTE física.

O contrato foi conferido na [documentação oficial Vec](https://doc.rust-lang.org/std/vec/struct.Vec.html): clear não reduz capacidade; um Vec novo vazio não aloca armazenamento para estes elementos não-zero-sized. A [lista oficial NVIDIA](https://github.com/NVIDIA/open-gpu-kernel-modules/releases) foi novamente lida e continua marcando **615.78.08**, commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`, como Latest. A [documentação MSI do kernel Linux](https://docs.kernel.org/PCI/msi-howto.html) também foi consultada como referência de ordenação; nenhuma API Linux foi apresentada como implementação Asterinas.

### Prova executada

O teste real `disconnect_releases_callback_storage_before_retiring_with_clone` aloca callbacks em dois clones, comprova capacidade inicialmente não-zero, executa a mesma rotina usada pela produção, exige capacidade zero, verifica que o callback do clone sobrevive e confirma que o número continua indisponível mesmo após aposentar a primeira linha e destruir o último clone. O baseline reproduz a operação antiga `clear()` dentro do helper extraído para permitir a observação de capacidade: falhou por assertion no guest. Não é uma fonte antiga byte-idêntica; essa adaptação está explícita no reproducer. A versão corrigida passou a suíte OSTD integral: **253 testes reportados**, com três corpos VMX/VMCS/EPT explicitamente skipped por hardware ausente, sem validação VMX. **38 testes host gráficos/orquestração** também passaram.

Logs/reprodutor: `../evidence/nvidia-msix-run-1900/check_retire_storage.py`, `retire-storage-results.json`, `legacy-clear-operation.log`, `fixed-full-ostd.log` e `graphics-tests.log`. Rustfmt direcionado e sintaxe Python passaram. O patch `asterinas-irq-retire-storage.patch` aplica sobre os bytes de entrada e reproduz exatamente a implementação testada; o check somente leitura contra a árvore compartilhada também passou no momento da geração. O índice Git original permanece inalterado.

### Falhas e integração pendente

A primeira matriz executada na árvore compartilhada falhou ao compilar os testes MSI-X ainda em edição (E0308 e macro de formatação); foi preservada em `native-suites-v1.log` e **não** é contada como passagem. A primeira tentativa na cópia isolada revelou que o cargo-osdk instalado tinha caminhos de dependências embutidos para a fonte original: compilou a fonte errada e selecionou zero testes, portanto foi rejeitada. Logs preservados em `initial-wrong-osdk-*`. O OSDK foi recompilado com `OSDK_LOCAL_DEV=1` dentro da cópia independente; o reproducer agora verifica o caminho de dependências no manifesto gerado. Somente as duas execuções finais desse OSDK isolado compõem a prova acima.

A ordem pretendida é aplicar `asterinas-irq-retire-storage.patch` **depois** da finalização de `asterinas-nvidia-msix-transaction.patch` e de seus treze predecessores. A embalagem MSI-X estava em execução concorrente e não foi tomada como pronta. Esta continuação **não validou a sequência completa com esse pré-requisito ainda pendente**, não substituiu suas fontes nem tomou emprestados seus boots/testes como evidência própria. A próxima execução deve primeiro conferir a conclusão do trabalho concorrente, integrar este patch sem descartar suas mudanças e retestar os consumidores. Não executar o antigo make_patch da etapa IRQ-callbacks contra a árvore MSI-X atual: seu snapshot tem outro pré-requisito.

A revisão também observou que o teste MSI-X inicial registrava o handler só depois da conclusão mascarada; um contador sem handler não comprova ausência de entrega enquanto mascarado. A versão lida posteriormente já verifica Table/PBA reais, mas o registro antecipado do callback ainda merece revisão pelo responsável pela transação. Essa observação não foi alterada na cópia compartilhada nem tratada como prova de hardware.

### Hardware e aceitação continuam bloqueados

Conferência desta execução: sem `/dev/nvidia*`, `/dev/dri` e `/dev/kvm`; nenhum dispositivo PCI exposto no sysfs. **Nenhum benchmark GPU físico executando Asterinas passou.** OSTD/QEMU e testes host não são substitutos. Não foi implementado novo backend AMD, nem completados RM/GSP, firmware autenticado, submissão/fences/reset GPU ou Vulkan/Wayland/GPUI acelerados. Nenhum firmware GPU foi carregado, callback de sucesso simulado ou GPU fictícia foi usado. Tudo permaneceu local: sem tokens salvos, autenticação, commit, push, PR, publicação ou Actions remoto; o agendamento não foi alterado.

## Décimo quarto patch Asterinas: transação PCI MSI-X e consumidores reais

Patch incremental: `asterinas-nvidia-msix-transaction.patch`, **depois** de `asterinas-nvidia-irq-callbacks.patch` e dos doze predecessores. Evidências e reprodutores: `../evidence/nvidia-msix-transaction/`. A lista oficial NVIDIA foi novamente aberta e ainda marca [615.78.08 como Latest](https://github.com/NVIDIA/open-gpu-kernel-modules/releases), commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`. Não foi usada uma versão presumida 360.x nem substituída a fonte/firmware.

### Transação e teardown nativos

O novo `PciMsixGuard`, exposto por `PciMmioGuard::prepare_msix` e `NvidiaDeviceResources::prepare_msix`, faz aquisição exclusiva de tabela/PBA e IRQs reais, valida BARs/limites/sobreposição e salva entradas, MSI-X Control e Command. Prepara tudo com Function Mask e máscaras individuais antes de programar mensagens; nunca habilita BME. Recusa MSI-X já ativo, PBA herdado, BME ativo, aquisição duplicada e contexto sem tarefa atual/IRQs/preempção habilitados. Não escapa clone de IRQ ou linha mutável. Cada vetor alocado precisa de handler real fornecido pelo chamador antes da ativação; não foi acrescentado ISR RM fictício ou probe NVIDIA de sucesso.

`shutdown` mascara com readback, exige parada/completion/worker-drain reais pelo driver, BME desligado e PBA vazio, sincroniza a remoção de callbacks pelo writer lock existente e verifica restauração exata de tabela/Control/Command. Falha na parada conserva os recursos para retry. Abandono ativo ou em contexto inválido retém mapeamentos e põe a função em quarentena, inclusive contra reprobe ou devolução de grupo. Guard esquecido deixa a lease ativa, também provocando quarentena. O rollback de MMIO é confirmado antes de liberar seu mapeamento. O chamador não pode segurar locks de que o handler dependa durante shutdown/Drop.

**Limite deliberado:** OSTD ainda não prova drain de MSI já postada, término de EOI ou estado APIC/IRTE em todos os CPUs. Vetores ativados são permanentemente aposentados como sinks vazios até reboot, não reciclados. Isso evita reutilização insegura, mas consome um recurso finito e não é o ciclo de recuperação reutilizável de um driver GPU maduro. A sincronização não drena trabalho assíncrono arbitrário: isso continua obrigação real do driver. O desenho de um teste SMP determinístico encontrou falta de afinidade na API TaskOptions/scheduler FIFO; não foi adicionado teste que esperasse outra tarefa no mesmo CPU com read guard não-preemptível, pois travaria em vez de provar a corrida.

### Consumidores e desvio encontrado pelo boot real

MSI-X legado não habilita mais bus mastering. NVMe e Virtio PCI moderno/legado configuram vetores mascarados, registram callbacks reais e só então habilitam os vetores roteados. A política BME fica explícita no transporte. `irq_mut` agora valida índices; substituição de uma linha existente e unmask sem handler são recusados. Drop legado mascara e aposenta IRQs, mas não é apresentado como reset/parada/drain/rollback completo do dispositivo; NVIDIA usa a transação nova.

O primeiro boot com namespaces/discos reais revelou uma limitação preexistente não exercitada pelos antigos boots sem dados: o BAR PIO usado por Virtio legado ainda era um TODO que devolvia `IoError`; `legacy.rs:108` fazia panic antes do userspace. Foi implementado `IoPortRange` real no OSTD: posse byte a byte pelo allocator existente, rollback RAII em reserva parcial, bounds de toda largura e instruções 8/16/32 bits somente dentro da posse. `IoBar` mantém Arc cacheado e restaura os flags originais do BAR. Não foram usadas reservas overlapping para contornar exclusividade. A API `PodOnce` original dos BARs MMIO, inclusive largura 64 bits, permanece compatível; PIO de largura/arquitetura não suportada falha fechado. Três novos testes OSTD verificam conflito/rollback, bounds de aquisição e acessos inválidos antes de qualquer instrução de porta. Essa pequena extensão do TCB foi necessária para testar de verdade o consumidor legado, não para simular um driver GPU.

### Provas já observadas antes do follow-up IRQ

A matriz real reportou **20 NVport, 4 platform, 13 PCI, 13 PCI-com-VT-d e 255 testes OSTD**, com três corpos VMX/VMCS/EPT explicitamente skipped. Os novos testes exigem Identify/CQ reais de NVMe QEMU, handler instalado antes de iniciar DMA, ausência de chamada enquanto mascarado, Table/PBA físicos, entrega real após unmask, PBA limpo, dois ciclos de parada/reconfiguração, RDY=0 e BME-off antes de liberar DMA. Probes de destruição de captures conservam zero destruições numa falha de quiesce e exigem uma na parada bem-sucedida. A versão final acrescenta exaustão parcial real de IRQ (um vetor livre, pedido de dois) e recuperação da primeira alocação. Os testes negativos também exercitam abandono/quarentena, duplicação, contexto atômico, índice e substituição. Identidades de dispositivos QEMU não foram alteradas.

O reproducer de regressão no guest restaurou a fonte byte a byte: reintroduzir BME implícito falhou por assertion real; reintroduzir indexação direta falhou por panic real; o código corrigido passou. Logs originais são preservados. No segundo boot de I/O (`consumer-io/run-pio-v2`), os três dispositivos completaram leitura dos padrões, escrita/flush/readback e os bytes novos foram confirmados nos três raw disks; o guest atingiu o marcador global e poweroff **33**. O parser recusou apenas os marcadores individuais, emitidos em três writes e intercalados pelos logs de syscalls. O resultado original dessa tentativa permanece `passed=false`; a emissão foi tornada uma única chamada por dispositivo para a rodada final, sem remover a exigência de marcadores ou de bytes corretos no host.

### Décimo quinto patch: integração do follow-up de armazenamento IRQ

A execução paralela documentada acima deixou `asterinas-irq-retire-storage.patch` em cópia separada e não modificou a árvore desta etapa. Após congelar e validar os **14 patches**, esse follow-up foi conferido e aplicado **depois do patch MSI-X**, mantendo a ordem solicitada. Ele libera também a capacidade do Vec de callbacks por `drop(mem::take(...))`, enquanto conserva apenas a reserva conservadora do handle/ID. Não recicla número aposentado nem afirma drain físico. Seu novo teste será contabilizado na validação conjunta final abaixo.

`source-msix-final/` contém os bytes imutáveis usados para gerar o patch 14; `make_patch.py` agora compara o round-trip com esse snapshot, não absorve o patch 15 por acidente. `validate_stack.py` aplica os quinze patches em ordem sobre `git archive HEAD` e compara com a árvore combinada. Não executar os antigos geradores contra snapshots com outro predecessor. O histórico acima que dizia 'não mesclado' descreve a execução paralela naquele momento; a integração local aconteceu nesta seção.

### Validação conjunta final e workspace estável

`run_final_validation.py` terminou **exit 0 em todas as dez fases**. A matriz combinada final (`native-suites-storage-final/`) reportou **20 NVport, 4 platform, 13 PCI, 13 PCI-com-VT-d e 256 testes OSTD**, incluindo liberação do Vec e alocação parcial real de IRQ. Três corpos VMX/VMCS/EPT permanecem explicitamente skipped. Os testes de Table/PBA físicos, callback instalado antes de DMA, destruição síncrona de captures, erro/retry e rejeição de contexto atômico passaram nos dois modos PCI. Contagens/skips/hashes: `test-counts.json`.

A comparação negativa foi repetida na fonte final: BME implícito e indexação direta falharam pelos motivos esperados no guest; a corrigida passou. `regression-results.json` confirma restauração byte a byte da fonte final. Não houve build concorrente durante essas mutações controladas.

`consumer-io/run-final/result.json` passou **sem relaxar o parser**: NVMe, Virtio moderno e Virtio legado leram 4096 bytes de padrões exclusivos, escreveram 4096 bytes em scratch, fizeram fsync e verificaram readback. Após poweroff, os três raw disks mantiveram os dados iniciais e contêm exatamente os novos padrões gravados. Marcadores individuais atômicos e global presentes; debug-exit **33**. É I/O real de dispositivos QEMU no kernel Asterinas completo, não GPU, benchmark ou prova de isolamento dos drivers legados.

O kernel completo final passou três boots da mesma ISO: normal, VT-d e VT-d com duas funções NVMe PCIe genuínas. Todos chegaram ao `/init`, selftests NVport e poweroff **33**. `cpu-boot-final/result-*.json` registra hashes e explicitamente `hardware_gpu=false`, `rm_gsp_initialized=false`, `gpu_firmware_loaded=false`. Nenhum initramfs desta etapa contém firmware GPU. TCG instruction-counted não é benchmark.

Os **38 testes host**, actionlint direcionado do workflow gráfico, diff check e round-trip do patch 14 passaram. Os **15 patches** aplicam em ordem: **96/97 arquivos** byte-idênticos; única diferença conhecida de ordem de imports em `kernel/comps/drm/src/ioctl/gem.rs`; nenhuma divergência inesperada. Índice Git original inalterado, manifestações OSDK temporárias removidas. `patch-validation.json`, `full-stack-validation.json`, `final-validation-phases.json` e `README.md` indexam a entrega.

O disco encheu somente ao salvar as notas/índice após todas as dez fases terem passado. As primeiras escritas falharam atomicamente; arquivos anteriores ficaram intactos. Foram removidos apenas os caches regeneráveis `target/x86_64-unknown-none/debug/incremental` e `target/debug/incremental`, recuperando cerca de 5,9 GB. Código, patches, logs, ISOs e discos de evidência foram preservados; nenhum teste foi reclassificado por causa disso.

### Próxima fronteira real e limitações

A auditoria `nv_pci_probe_body`/`nv_start_device` confirma que MSI-X não substitui `rm_wait_for_bar_firewall`, `rm_is_supported_pci_device`, estado/power/stacks e filas por dispositivo. Próximo passo: owner/transferência de recursos por dispositivo e callbacks DMA/stacks ISR-BH/workers reais antes de ligar `rm_isr`, `rm_isr_bh` ou `rm_init_adapter`; a abstração pré-driver emprestada não é um adapter RM completo. Não usar handles inventados ou callbacks de sucesso sem serviço real. GSP ainda precisa de firmware completo, correspondente e autenticado e boot/handshake/shutdown reais. O firmware extraído de instalador truncado permanece não verificado, CPU-only e **proibido para carregamento GPU**.

RM/GSP, ioctl/mmap NVIDIA, memória/submissão/fences/reset, DRM/KMS e Vulkan/Wayland/GPUI acelerados continuam incompletos. Nenhum backend AMD novo foi implementado. Conferência final: sem `/dev/nvidia*`, `/dev/dri` ou `/dev/kvm`. O benchmark estrito permanece negativo e não emitiu JSON de passagem. **Não existe benchmark GPU físico sob Asterinas ou aceitação de driver completo.** Workflow local exige MSI-X/NVMe/VT-d, mas nenhum Actions remoto foi executado. Nenhum token/autenticação, commit/push, PR/publicação ou mudança de agendamento. Ordem final: patch 13 → `asterinas-nvidia-msix-transaction.patch` (14) → `asterinas-irq-retire-storage.patch` (15).

## Décimo sexto patch Asterinas: worker nativo com admissão IRQ e drenagem

Patch incremental: `asterinas-nvidia-device-worker.patch`, **depois** dos quinze patches existentes, encerrando em `asterinas-irq-retire-storage.patch`. No início desta execução, o trabalho MSI-X anterior já havia terminado e integrado o follow-up IRQ à árvore principal; não foi necessário reaplicar ou sobrescrever aqueles patches. Não havia cargo/QEMU concorrente antes das mutações controladas desta rodada. Fontes, testes e evidências desta etapa estão em `../evidence/nvidia-device-worker/`; a implementação está na árvore principal, **não** somente na cópia isolada da rodada anterior.

### Implementação real de kernel

O novo módulo `kernel/comps/nvidia-platform/src/worker.rs` oferece `DeviceWorker`, proprietário exclusivo de uma tarefa OSTD real e de um callback `FnMut + Send + 'static`, com capacidade `WorkSignal` clonável para produtores. `schedule` altera pendência sob `SpinLock<State, LocalIrqDisabled>` e faz wake fora do lock; não aloca um work item por pedido, não executa serviço inline no ISR e não é NMI-safe. Solicitações já pendentes coalescem; pedido durante uma execução deixa outra execução pendente. O callback executa em tarefa sleepable, podendo usar os serviços reais de tarefa apropriados.

Fechamento e admissão usam o mesmo lock. `shutdown` exige contexto sleepable, recusa autoespera, fecha novos pedidos e drena trabalho aceito antes de destruir callback/captures e publicar completion. Sinais sobreviventes passam a retornar `Closed`. Drop só fecha/acorda, inclusive quando IRQs estão desabilitadas; a tarefa conserva os captures até drenar, sem destruição prematura em IRQ. O proprietário precisa fazer shutdown explícito antes de liberar estado externo do device, sem locks necessários ao callback/destrutores. Callback que não retorna não possui drenagem finita garantida. Completion é quiescência do callback, não join definitivo da tarefa, IRQ/MSI drain, parada DMA, reset ou sucesso RM/GSP.

A lista [oficial de releases NVIDIA](https://github.com/NVIDIA/open-gpu-kernel-modules/releases) foi aberta novamente: Latest continua **615.78.08**, commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`. A leitura do contrato em `kernel-open/common/inc/nv-kthread-q.h:130–200`, de `kernel-open/nvidia-uvm/nv-kthread-q.c` e de ISR/BH/stop em `kernel-open/nvidia/nv.c` confirma item preinicializado, coalescência e reagendamento durante callback. O serviço novo implementa um callback único, **não** toda a fila FIFO multi-item nem a ABI `os_queue_work_item`. Nenhum `rm_isr`, `rm_isr_bh_unlocked`, callback de sucesso fictício ou `rm_init_adapter` foi ligado. A posse PCI/MMIO atual continua emprestada: a API exige captures próprios e não contorna `'static` para capturar reservas temporárias.

### Testes efetivamente executados

Seis testes adicionais rodaram no kernel real OSTD: outra tarefa e contexto sleepable; coalescência enquanto callback está bloqueado; drenagem da pendência e destruição dos captures antes de retorno; rejeição de criação/shutdown em contexto atômico e de autoespera; Drop não bloqueante com trabalho ainda em execução; reagendamento dentro do callback; e sinalização originada numa **interrupção APIC timer real** para a tarefa. O teste timer toma seu único sinal e deixa um callback inerte com slot vazio, pois a API timer ainda não expõe unregister. Isso não é IRQ de NVIDIA, hardware físico, RM ou benchmark.

A comparação negativa trocou a prioridade de drenagem para sair imediatamente ao fechar, descartando trabalho aceito: o teste no guest falhou por assertion (uma execução em vez de duas). A versão corrigida passou os **10 testes de plataforma**; `negative-results.json` confirma restauração byte a byte da fonte. A primeira tentativa negativa passou o filtro após `--features`, e o OSDK o interpretou como feature inexistente: falha de compilação preservada em `initial-feature-argument*`, rejeitada como prova e corrigida colocando o filtro antes das opções. Nenhuma falha de build ou execução vazia foi contada como regressão reproduzida.

A matriz final reportou **20 NVport, 10 platform, 13 PCI, 13 PCI-com-VT-d e 256 OSTD**. Três corpos VMX/VMCS/EPT foram explicitamente skipped por indisponibilidade e não validam VMX. Os **38 testes host**, bash syntax, rustfmt direcionado, sintaxe dos reprodutores, actionlint do workflow gráfico e diff check passaram. O runner já chamado pelo workflow local exige os novos marcadores de drenagem e handoff IRQ real; nenhum Actions remoto foi disparado.

O kernel completo com a fonte final compilou e executou três boots da mesma ISO: normal, VT-d e VT-d com duas funções NVMe PCIe reais em QEMU. Todos chegaram ao `/init`, passaram os marcadores NVport e desligaram com debug-exit **33**. `cpu-boot/result-*.json` registra hashes e flags negativos para GPU/RM/GSP/firmware. O initramfs continua sem firmware GPU. Esses boots comprovam ligação/boot sem regressão; a execução do novo worker é comprovada pelos testes kernel próprios, não se presume que um device NVIDIA foi exercitado no boot.

O patch incremental reproduz exatamente seus cinco arquivos. Os **16 patches** aplicam em ordem sobre `git archive HEAD`, com zero divergência inesperada; permanece apenas a diferença preexistente conhecida de ordem de imports em `kernel/comps/drm/src/ioctl/gem.rs`. Nenhum predecessor foi regenerado. Índice original Git inalterado. Resultados: `patch-validation.json`, `full-stack-validation.json`, `test-counts.json`, `negative-results.json` e `cpu-boot/result-*.json`.

### Limitações e próxima implementação

A revisão independente não identificou defeito concreto no worker, mas os testes não forçam uma interleaving adversarial entre produtores em CPUs distintas e close/registro do waiter. A serialização depende do lock e do contrato real de WaitQueue; não foi alegada prova SMP exaustiva. Sinais não devem ser emitidos segurando locks internos do scheduler/WaitQueue. A rotina do device precisa drenar seu serviço real e os produtores IRQ precisam ser impedidos/quiescidos antes da liberação de recursos.

Próxima fronteira: owner estável por adapter que transfira recursos PCI/DMA legitimamente para os callbacks, stacks/ABI estrangeira RM revisados e acoplamento do item BH ao verdadeiro `rm_isr_bh_unlocked`, só depois de satisfazer supported-chip, BAR firewall, power e quiescência. O worker sozinho não torna o objeto pré-RM emprestado um adapter inicializado. A fila FIFO multi-item, callbacks DMA RM e bootstrap/handshake/shutdown GSP com firmware correspondente e autenticado continuam pendentes. Não carregar o firmware extraído de instalador truncado ou não verificado.

Hardware reconferido nesta execução: sem `/dev/nvidia*`, `/dev/dri` ou `/dev/kvm` e sem dispositivos PCI expostos no sysfs. **Nenhum benchmark de GPU física sob Asterinas passou.** Não foi implementado novo backend AMD nem completados NVIDIA RM/GSP, ioctl/mmap, memória/submissão/fences/reset GPU, DRM/KMS ou Vulkan/Wayland/GPUI acelerados. Não usar TCG, timer, EDU/NVMe ou Linux-host como aceitação. Tudo permaneceu local, sem tokens salvos, autenticação, commit, push, PR, publicação, Actions remoto ou alteração do cron.

## Décimo sétimo patch Asterinas: flush de snapshot do worker nativo

Patch incremental: `asterinas-nvidia-worker-flush.patch`, aplicado depois dos dezesseis patches já existentes, encerrando em `asterinas-nvidia-device-worker.patch`. A mudança fica restrita ao worker nativo, seus testes e README; nenhum predecessor foi regenerado. Cópias exatas anteriores: `../evidence/nvidia-worker-flush/before/`.

### Semântica real de flush

`DeviceWorker::flush` exige tarefa OSTD sleepable, rejeita autoespera e captura sob o mesmo lock da admissão o último generation aceito. Espera somente até essa geração completar: pedidos coalescidos enquanto pendentes representam uma invocação; trabalho aceito depois do snapshot não amplia a espera. Flush não fecha admissão nem destrói callback/captures; scheduling posterior continua possível. Contabilidade `u64` usa `checked_add`; overflow fecha admissão sem wrap. É uma barreira de snapshot para este callback único, não compatibilidade completa com a fila FIFO multi-item nem o double barrier de `kthread_q_flush`. Não foram acrescentados placeholders ou chamadas RM.

### Validação no kernel e patch stack

O teste nativo de `aster-nvidia-platform` passou **12 testes, zero falhas**, incluindo espera por execução em curso e pendência coalescida, scheduling depois de flush, recusa self/atomic, vida dos captures e overflow de geração. A matriz CPU-only completa terminou com exit 0: **20 NVport, 12 platform, 13 PCI, 13 PCI-com-IOMMU e 256 OSTD** (314 resultados positivos reportados). Três corpos foram explicitamente skipped por falta de virtualização: `multiple_vmx_guards_lifecycle` e `switch_and_reuse_vmcs` (VMX indisponível), e `gpm_space::test::guest_mapping_lifecycle` (invalidação VMX/EPT indisponível). Não são validação de VMX/EPT. Logs: `../evidence/nvidia-worker-flush/native-matrix/`; execução direta do platform: `../evidence/nvidia-worker-flush/platform-tests.log`.

O patch 17 foi aplicado em árvore temporária criada de `git archive HEAD` depois dos 16 patches ordenados; os três arquivos de entrada coincidiram byte a byte com o snapshot pré-edição e os três resultados coincidem com as fontes finais. Comparação dos **99 caminhos tocados** encontrou somente a diferença preexistente de ordenação de imports em `kernel/comps/drm/src/ioctl/gem.rs`; nenhum desvio inesperado. Índice Git permaneceu inalterado, OSDK temporários foram removidos e não houve commit/push/Actions. Reprodutibilidade/manifesto: `../evidence/nvidia-worker-flush/stack-validation.json`; patch: `asterinas-nvidia-worker-flush.patch`.

Não foi repetido build/boot do kernel completo: os testes reais do componente e a matriz nativa passaram, mas isso não comprova boot completo nem hardware NVIDIA. Sem `/dev/nvidia*`, `/dev/dri` ou `/dev/kvm`; nenhum RM/GSP, firmware GPU, IRQ NVIDIA, benchmark GPU ou aceitação física foi alegado. RM/GSP e integração do callback com o verdadeiro bottom half permanecem trabalho futuro.

## 2026-10-20 20:00 UTC run — Asterinas-first CyberManju appearance bridge

The owner's primary kernel/base remains Asterinas. Reconciled the existing app plan and task tracker with a concrete local bridge; Ubuntu+niri remains a prior/optional comparison, not the target. The source trees changed are `/workspace/cybermanju-os/cybermanju.github.io`, `/workspace/asterinas-gpui/gpui-shell` and the Asterinas kernel tree. `/workspace/cybermanju-shell` remains its old published snapshot and was not synchronized or changed.

### Implemented code, not a desktop compatibility claim

- Existing Vue Settings now exports strict `com.cybermanju.appearance` v1 JSON from current appearance/custom-wallpaper state. Keeps theme normalization, global/per-theme accents, shell style and panel docking; exports wallpaper metadata only. No credential, server-configuration, blob, object URL or filesystem-path export. No duplicate preference controls/store was added.
- GPUI now accepts explicit `--appearance-profile FILE`, validates bounded regular-file input before IPC/display, and installs the supported palette/accent projection into the actual native Theme at primary startup. Preserves native font size and makes no preference-file writes. `--validate` is renderer-free. A secondary profile invocation fails without sending a launcher command; ordinary launcher IPC still works.
- Five native theme palettes, accent precedence, unknown-field/schema/type/conflict rejection and bounded safe wallpaper metadata are tested. Wallpaper, shell geometry/style, panel docking, density, glass, motion, glow and live follow-system are explicitly retained-but-unapplied. No live app/shell API, image migration, export-back or restore was invented.
- Native NVIDIA prerequisite work advanced concurrently through patch 17: real `DeviceWorker::flush` snapshots accepted/completed generations, preserves admission/captures and later work, rejects self/atomic waits and closes admission on counter overflow. It is not the full multi-item/double-barrier RM queue contract. The preceding patch-17 section records its separate source, stack and kernel evidence.

### Tests and reproducible patches

App: Vue typecheck, version/header/whitespace checks passed; frontend suite **125 files / 1,203 tests**, including **7 profile tests** (completed frontend subtask `job_X1mvx4qY`). No local app Cargo commands, new npm installation or browser export/download was performed. Existing compatible dependencies were reused temporarily and the symlink removed.

GPUI final sources: **189 Rust tests**, binary build and **29 Python tests** passed. Python includes six real-binary profile/IPC tests with actual Unix sockets, invalid/oversized profile refusals, no preference writes and headless validation. Shared app/Rust fixture is byte-identical. `Cargo.lock` only adds the already-locked `url` dependency to the app's direct dependency list; no package version changed. Evidence: `gpui-rust-tests-final.log`, `gpui-build-final.log`, `gpui-python-tests-final.log`, `gpui-test-summary.json` in this directory.

Kernel: full native CPU-only matrix passed **20 NVport, 12 platform, 13 PCI, 13 PCI+VT-d and 256 OSTD**, 314 reported passing, including **3 explicitly skipped VMX/EPT-dependent bodies**. This is QEMU/TCG prerequisite evidence, not GPU/VMX acceptance. No full-kernel build/boot was repeated. Kernel evidence and ordered patch-17 check: `../nvidia-worker-flush/`.

`make_bridge_patches.py` generates `gpui-cybermanju-appearance.patch` and `cybermanju-appearance-export.patch` from preserved pre-edit source copies. Both pass `git apply --check`, apply to isolated baseline files and reproduce final bytes exactly; `patch-validation.json` records hashes and the unchanged GPUI index. App documentation/task/worklog edits remain in the app working tree and are deliberately not bundled into the code-only app patch. Initial patch formatting lacked new-file mode metadata; that local generator defect was corrected before successful validation.

### Blockers and next implementation boundaries

`hardware.json` confirms no exposed display-class PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. No physical GPU benchmark was run or passed. No new AMD backend or completed NVIDIA RM/GSP initialization/submission/firmware/IRQ binding was established. Native GPU memory, submission, fences, reset, accelerated Vulkan/Wayland presentation and end-to-end GPUI/Tauri on Asterinas remain incomplete. Tauri/Wry/GTK/WebKit and secure live app/shell dependencies remain independent gates; all `OS-A*` acceptance tasks stay open.

Continue from real RM bottom-half ownership and driver lifecycle gaps, and from actual Asterinas runtime/DRM/Vulkan/Wayland ABI gaps; do not equate profile parsing, timer IRQs, EDU/NVMe or a Linux-host/software renderer with GPU acceptance. Full appearance/media propagation needs real capability-backed services and restart/restore checks, not successful no-ops. Require correct physical-GPU workloads under Asterinas before acceleration or Linux-parity claims.

Updated app dependency/ownership documentation: `docs/ASTERINAS-PERSONALIZATION-BRIDGE.md`, active plan, `TASKS.md` and appended `worklog.md`. Official upstream boundaries consulted are in `upstream-sources.md`. Everything remained local: no saved tokens, authentication, staging, commit, push, PR, publication, remote Actions or schedule changes.

## 2026-10-20 20:30 UTC run — patch 18, real app restore and native file boundary

Asterinas remains the primary kernel/base. Ubuntu+niri is a prior/optional reference, not the target. Work remains local in the Asterinas, GPUI shell and CyberManju app source trees; the published `/workspace/cybermanju-shell` snapshot was not changed. No credentials, authentication, staging, commit, push, PR, publication, remote Actions or schedule changes were used.

### Patch 18: ordered IRQ callback/worker teardown

`local-prs/asterinas-nvidia-irq-worker-teardown.patch` follows all 17 existing patches without regenerating them. `DeviceWorker::shutdown_after_irq(&mut Option<IrqLine>)` checks sleepable/non-self context before consuming the supplied owner, retires that handle's OSTD callbacks before closing/draining its worker, then waits for callback-capture destruction. Invalid-context rejection preserves the IRQ owner. The new kernel test exercises callback retirement before accepted-work drain/capture release and invalid-context ownership preservation.

The caller must first mask/quiesce the device interrupt source and retire all other producers. This does not stop DMA, synchronize every in-flight APIC/IRTE message, prove a GPU IRQ path, initialize RM/GSP, or implement the complete upstream multi-item/double-barrier NVIDIA queue contract. Official pinned 615.78.08 queue source and upstream README were consulted by the native implementation subtask; vendor source was unchanged.

The absolute-path native runner completed with exit 0: **20 NVport, 13 platform, 13 PCI, 13 PCI+IOMMU, 256 OSTD**, totaling **315 reported positives**, including three explicitly skipped VMX/EPT-dependent test bodies. Skips remain `multiple_vmx_guards_lifecycle`, `switch_and_reuse_vmcs`, and `gpm_space::test::guest_mapping_lifecycle`; they are not virtualization validation. Initial relative-path invocation failed immediately with exit127 and no tests; the corrected run is the sole new passing matrix. Nonfatal compiler warnings remain, including a redundant IrqLine test import. No full-kernel boot was repeated.

Patch application after patch17 reproduced all three target files exactly, with the kernel index unchanged. Evidence: `../native-2030/patch-validation.json`, `../native-2030/native-matrix/` and this directory's `native-test-summary.json`. The native subtask handed off before the runner finished; its earlier statement that results were pending is superseded by completed job `job_u2EectqK` and the actual suite logs above.

### Real app-side appearance restore

The existing appearance section now selects bounded v1 JSON, previews it without mutation and explicitly applies supported profiles through the existing wallpaper and preference owners. URL wallpapers use the existing custom-wallpaper service; presets clear the custom override before theme/shell/panel preferences change. Input is strictly revalidated before application. A failed/refused wallpaper operation leaves those preferences unapplied. Local-file descriptors are refused before mutation because no image bytes are present; current wallpaper is preserved rather than invented or silently reset.

Success wording is session-scoped. Existing localStorage/IndexedDB availability still controls persistence; this is not an atomic durable transaction, completed media migration or native GPUI restore. App changes are limited to `src/utils/appearanceProfile.ts`, `src/components/SettingsPage.vue` and their frontend tests. Typecheck, version/header checks and frontend suite passed: **125 files / 1,207 tests**, including **11 appearance-profile tests**. Logs are retained in this directory. No app Cargo command, new dependency installation, or browser import/export click-through was performed. Temporarily reused compatible frontend dependencies were removed afterward.

### Native GPUI file-open boundary

Replaced the pathname `is_file`/open precheck with one safe read-only OpenOptions descriptor, `O_NONBLOCK | O_NOFOLLOW | O_NOCTTY`, followed by descriptor type checking and the existing 64KiB-plus-sentinel bounded read. Terminal symlinks are now refused even when they target a valid profile; direct regular files remain supported. Real CLI tests cover valid/dangling symlinks, no-writer FIFO admission under a subprocess deadline, directories and Unix sockets, alongside the existing profile/IPC cases. The old binary's symlink acceptance was reproduced as an expected failing regression before the correction.

This is not a filesystem sandbox, immutable snapshot or universal I/O deadline: parent symlinks and concurrent regular-file writers remain possible, and regular filesystem/device I/O can wait. The previously locked libc 0.2.187 is now a direct app dependency for constants only; no package versions or unsafe code were added. Asterinas flag/pipe source was mapped, but host behavior does not establish target userspace runtime compatibility. Official Rust/open(2) references and target source limitations are in `upstream-sources.md`.

GPUI workspace tests **189 Rust cases**, real binary build, **32 Python cases** (nine profile/IPC/filesystem tests) and offline workspace/all-target Clippy completed successfully. A pre-existing proc-macro-error2 future-compatibility warning remains. Logs and `gpui-test-summary.json` are retained here. App/GPUI incremental patches and exact-byte validation are generated separately in this directory; old patches are preserved.

### Blockers and next implementation boundaries

`hardware.json` reconfirms no display-class physical PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. **No physical-GPU benchmark was run or passed.** No new AMD backend, completed NVIDIA RM/GSP/firmware/submission/reset/memory/fence path, accelerated Vulkan/Wayland/GPUI, native Tauri/GTK/WebKit runtime or Linux parity is established. All full `OS-A*` hardware/runtime/personalization gates remain open.

Continue actual driver source masking/quiescence, real RM bottom-half ownership and GPU lifecycle implementation, plus target DRM/Vulkan/Wayland/runtime gaps. App restore is a bounded prerequisite, not end-to-end Asterinas desktop acceptance. Live settings propagation, native density/style/media application, restart/durable restore, accessibility/scaling and physical-GPU correctness/performance remain to be implemented and measured. CPU/QEMU, APIC timers, EDU/NVMe and Linux-host rendering must never count as physical GPU acceptance.


## 2026-10-20 21:00 UTC continuation — enforced IRQ ownership and real live-file GPUI colors

Asterinas remains the primary CyberManju OS kernel/base; Ubuntu+niri is a comparison host only. Latest hardware check (`evidence/cybermanju-live-profile/hardware-final.json`) again finds no display-class PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. No physical-GPU benchmark or full NVIDIA/desktop acceptance is claimed.

**Native implementation / patch order.** Apply `asterinas-nvidia-interrupt-worker.patch` as patch **19**, after `asterinas-nvidia-irq-worker-teardown.patch` (18); predecessors are unchanged. `NvidiaInterruptWorker::prepare` borrows the real `NvidiaDeviceResources`, preflights sleepable/current-task context, acquires a fresh masked MSI-X lease through `prepare_msix`, then starts its owned deferred worker. It cannot accept an already-activated/foreign-handler guard. Unused guard rollback is retained on worker-start failure. Registered callbacks receive this worker's real signaling capability. Teardown requires the caller's actual device-source quiesce protocol, closes the MSI-X transaction/retired IRQs, then drains the worker; failures retain the guard for retry. This is lifecycle glue, not an NVIDIA register-level source mask, RM/GSP initialization or successful GPU probe. Initial arbitrary-guard patch is preserved only as rejected historical evidence; use the corrected canonical patch and `evidence/native-integration-followup/REPORT.md`.

The corrected source passed the actual native OSDK/QEMU TCG matrix: NVport 20, platform 13, PCI 13, PCI+IOMMU 13, OSTD 256 reported passed (**315**, zero failures). Three VMX/VMX-EPT-dependent bodies explicitly skip. The new owner is compiled but NOT hardware-exercised against NVIDIA resources. Offline rerun logs are in `evidence/native-integration-followup/native-suites-corrected/`; patch replay reproduces final source and indexes remain unchanged.

**Real GPUI implementation.** `--appearance-profile FILE --watch-appearance-profile` now watches only that explicitly supplied file. Startup registers before rereading to cover setup saves; bounded trailing-edge events cover rapid/atomic replacement. Parsing happens off the foreground; complete validation/projection precedes replacement of cached colors/global Theme and real window redraw. Malformed/missing/symlink inputs retain last good colors; a separate appearance card does not clear native config/theme errors, and a stopped watcher is reported. Native theme reloads preserve the imported palette while applying native typography. No preference files, Vue stores, browser transport or new settings daemon are written/created.

Final-source checks: **201 Rust + 36 Python** tests pass, including 13 built-binary profile/IPC/filesystem cases and six real inotify tests; workspace build and all-target Clippy pass. Only pre-existing icon-test/dependency warnings remain. Negative baseline genuinely reproduces the old binary accepting `--watch-appearance-profile` with no bound profile. A first binary link hit full root disk/SIGBUS; only disposable GPUI incremental compilation caches were cleared, preserving sources/evidence/binaries, then all final checks passed. Failure and recovery records remain under `evidence/cybermanju-live-profile/`.

**Actual host-window proof, not GPU acceptance.** `host-window-smoke/result.json` and its finite reproducer run the real final binary in isolated nested Niri/Xvfb with forced Mesa lavapipe: real bar observed, atomic `os-dark`→`os-light` visibly applied, invalid JSON showed the actual appearance error without killing the process, valid `os-dark` recovered. Four unmodified screenshots, protocol/layer snapshots and logs are preserved there; all test processes were stopped. This is Linux-host software rendering only—no Asterinas kernel, physical GPU, input or full app runtime validation.

**App preservation and next boundary.** Only app documentation/worklog/TASKS changed this follow-up; version/header checks pass. The preceding 1,207 frontend test result is historical and was not rerun. Full app-to-shell transport, wallpaper/media, density, shell geometry/style, accessibility/restart round-trips and native Tauri/GTK/WebKit remain open. GPUI live-file colors do not imply these settings are applied. NVIDIA-specific source masking, matching userspace/GSP firmware/RM, native Vulkan submission and physical AMD/NVIDIA Asterinas end-to-end presentation/benchmark remain required. `/workspace/cybermanju-shell` stays an unchanged initial published snapshot. No credentials/authentication, staging, commits, push/PR/publication or remote Actions occurred.

Exact source/doc incremental patches, ordered stack validation, hashes, test summaries and host capture paths are consolidated in `evidence/cybermanju-live-profile/`; start the next run here, not from the rejected initial patch19.


## 2026-10-20 21:21 UTC continuation — post-IRQ drain and actual launcher density

**Native patch20** is `asterinas-nvidia-postirq-drain.patch`, after patch19 `asterinas-nvidia-interrupt-worker.patch`. PCI MSI-X now supports a fallible post-IRQ drain: actual caller-provided source quiescence → BME/PBA checks → synchronous callback retirement → admitted deferred-work drain while MSI-X remains masked/owned → state restoration. NVIDIA's owner drains its real worker at that boundary, retaining retry semantics. The old `shutdown` API remains compatible. This does not implement NVIDIA register masking, RM/GSP or a hardware GPU probe.

After critical disk pressure initially blocked native validation, only idle GPUI incremental compilation cache entries were removed after its compilers stopped; sources, binaries, test binaries, dependencies, evidence and the unrelated Android build were preserved. Parent then ran the actual OSDK/QEMU TCG matrix against patch20: **315 reported passed**, zero failures, **three explicit VMX/VMX-EPT skips**. QEMU NVMe verified callback retirement and enabled/function-masked MSI-X at the new drain boundary. Evidence: `evidence/native-2121/native-suites/`, `test-summary.json` and the appended REPORT.md. Ordered replay of all20 patches matches 98/99 source files; only the pre-existing DRM import-order difference remains, with no unexpected mismatch/index change. These are PCI/CPU prerequisites, NOT GPU benchmarks.

**Real personalization implementation:** `settings.density` now changes nine actual native launcher query/body/footer semantic spacing consumers. Compact=0.9 and comfortable=1.0 reuse the app's existing unit semantics; default native spacing stays unchanged without import. Density is a runtime Theme overlay: it survives palette/native-theme reload, accepted atomic/inotify saves, invalid-save retention and recovery, but is not persisted into native theme files. Native typography, icon sizes, explicit hit-target dimensions, bar/compositor geometry are not scaled. The report says **launcher spacing only**, not whole-desktop density. No frontend source, schema, dependency or app preference-store changes.

Final GPUI source passed **205 Rust + 36 Python** tests, real binary build and all-target Clippy; app version/header gates passed. Frontend and rendered host-window suites were not rerun: earlier host color captures do not validate new density visually. `evidence/appearance-density-2121/` contains preimages, exact incremental patches, binary hash, tests, source references, safe cache-recovery record and continuation notes. GPUI patch applies on the previous live-file baseline; do not substitute the unchanged published `/workspace/cybermanju-shell` snapshot.

Hardware again has no display-class PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. Full native NVIDIA RM/GSP/Vulkan, AMD drivers, Asterinas Wayland/GPUI/Tauri runtime, app automatic transport, wallpapers/media, desktop shell style/docking, full density/accessibility/restart and correct physical-GPU benchmarks remain open. No credentials/authentication, staging, commits, push/PR/publication or remote Actions occurred. The next concrete native edge is activation after an unused transaction's failed post-IRQ drain: it must not accept an empty retired IRQ set; do not infer that the successful patch20 matrix covers this new retry case.


## 2026-10-20 21:30 UTC continuation — reject activation after IRQ retirement

**Implemented native patch21:** `asterinas-msix-retired-activation.patch`, directly after patch20 `asterinas-nvidia-postirq-drain.patch`; preserve all predecessors. A prepared but never-activated MSI-X transaction can retire every callback and then fail its fallible deferred drain. Its old activation predicate accepted the now-empty IRQ set because the “any empty handler” iteration was vacuously false. `PciMsixGuard::activate` now fails closed with `AccessDenied` when no owned vectors remain, retaining the masked transaction for a real shutdown retry rather than reporting successful activation.

**Genuine regression:** the dedicated idle QEMU NVMe controller verifies RDY=0 and PCI BME-off in quiesce callbacks. The test registers an actual IRQ capture, injects a drain error, verifies capture retirement, zero live vectors and enabled/function-masked MSI-X, requires activation denial, then retries shutdown and verifies exact PCI/MSI-X restoration. With the old predicate, the actual KTest run reported **12 passed / 1 failed**, observing `Ok(())` instead of `Err(AccessDenied)`; its outer Cargo exit was not reliably captured, so do not invent that status. Fixed source passed the actual five-suite QEMU/OSTD matrix: **315 reported passed, zero failures**, with **three explicit VMX/VMX-EPT skips**, and final runner markers in every suite. This is a real PCI lifecycle/error-path test, not NVIDIA hardware or GPU benchmark success.

`evidence/native-retired-2130/` holds the negative log/status, fixed native suite logs, source report, corrected patch preimage metadata, ordered-stack results and final hashes. An initial artifact reconstructed the old predicate in multiline formatting and therefore failed canonical ordered replay; it is preserved as `initial-formatted-preimage.patch`, NOT the canonical deliverable. Only patch context metadata was repaired; the tested source did not change. **All21 canonical patches now apply in order**, with no unexpected source mismatch or Git index change; the one old DRM import-order difference remains documented.

The completed 21:21 GPUI density source remains unchanged: **205 Rust + 36 Python** tests, real binary build and all-target Clippy passed. This scheduled continuation changed no GPUI/app source. Native launcher semantic spacing is only a partial density bridge; new rendered-density, frontend, Asterinas runtime and GPU tests were not performed. Full app automatic transport, wallpaper/media, shell style/docking/accessibility, native AMD/NVIDIA RM/GSP/Vulkan and Asterinas Wayland/Tauri/GPUI remain open.

Hardware still exposes no `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`; no physical-GPU benchmark has passed and Linux parity/full GPU integration is NOT accepted. Keep monitoring scarce shared disk capacity before compiling: only idle reproducible caches may be removed, never sources/evidence/binaries or unrelated Android artifacts. No credentials/authentication, staging, commits, push/PR/publication or remote Actions were used. The published `/workspace/cybermanju-shell` remains an unchanged initial snapshot. Continue from patch21, not either preserved initial artifact.


## 2026-10-20 21:39 UTC continuation — low-disk source-only shared rows

**OWNER RESTRICTION: no additional cache/file deletions without explicit approval; no disk-heavy build/ISO attempt at current capacity. The existing NixOS image reportedly reserves 6.44 GB. Source-only work and small consistency checks only.**

### Concrete source change

Shared `ListItem` rows now consume the existing runtime profile density for semantic horizontal padding and gaps. Default vertical padding follows the same compact/comfortable unit policy as the validated launcher bridge. Explicit `.spacing(Sparse/Dense/ExtraDense)` calls remain authored vertical pixel overrides; the private field is now optional so default rows and explicit overrides cannot be conflated. No public enum variant was added. Indentation, slots, visibility, disabled/selected state, clicks, hover, tooltips, fonts and compositor/bar geometry retain their prior ownership and behavior.

Three Rust regression tests were added for unchanged comfortable defaults, compact defaults, explicit vertical overrides and constructor/builder distinction. The source capability report now names partial launcher/shared-row support. Both repositories' bridge contracts explicitly distinguish the last validated launcher-only binary from this pending source extension.

### What was actually checked

Rustfmt syntax/format checks and Git whitespace checks passed. The two small patches replayed byte-for-byte from actual preserved preimages **entirely in memory**, and Git reverse-applicability checks passed read-only against current files. Both Git indexes stayed unchanged. These are source/patch consistency checks, not type checking or runtime tests.

**No compilation, Cargo test, binary/GUI test, QEMU, OSDK, dependency install or ISO build was run. The three new Rust tests are unexecuted.** The earlier 205 Rust + 36 Python results cover the prior GPUI source/binary only; they do not validate this change. The existing built binary has not gained shared-list density support from a source edit alone.

### Native NVIDIA lifecycle

The independent low-disk audit reviewed worker admission/closing/drain, runtime arithmetic, resource lifetimes, PCI restore/retry and current tests. It found no demonstrable safe behavioral defect warranting a new native patch; no patch22 was fabricated. Canonical native order still ends at validated patch21. Read-only reverse applicability of patch21 and formatting/whitespace checks passed; NVIDIA source was unchanged.

The audit records a documentation imprecision: a pre-drain MSI-X shutdown failure leaves the worker open, but a PCI restoration failure **after successful drain** leaves it already closed with the transaction/resources retained for retry. Do not assume every shutdown error permits reopening work admission. See `/workspace/asterinas-gpui/evidence/native-lowdisk-2139/REVIEW.md` for the actual reasoning and limits.

### Durable owner restrictions and acceptance gates

Do not delete any additional cache or file without the owner's explicit approval. Do not start disk-heavy builds or another ISO/image attempt with the remaining approximately 1.1 GB; the owner reports the NixOS image already reserves 6.44 GB. This run deleted nothing and did not touch the image or unrelated Android work. Only small source/preimage/patch/evidence files were written; this directory used about 272 KiB before final notes/manifests.

No `/dev/dri`, `/dev/nvidia*` or `/dev/kvm` is available. Native RM/GSP, GPU Vulkan, the target Wayland/Tauri/GPUI desktop, automatic app transport, complete personalization and physical-GPU acceptance remain open. No GPU or Linux-parity claim is justified. No credentials, authentication, staging, commit, push, PR, publication or remote Actions were used.

### Incremental artifacts

`gpui-shared-list-density-source-only.patch` applies after the 21:21 launcher-density change; it is not part of the Asterinas kernel numbered chain. `cybermanju-shared-list-density-contract.patch` contains only the corresponding app-repository contract update. `patch-validation.json` records exact preimage/final digests and the limited checks. Preserve all earlier patches/evidence, including their historical checkpoints. The published `/workspace/cybermanju-shell` remains unchanged.
