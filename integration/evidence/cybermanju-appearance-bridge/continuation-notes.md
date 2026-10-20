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
