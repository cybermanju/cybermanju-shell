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
