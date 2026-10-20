# GPUI + Asterinas: second local integration review

The GPUI integration is improved, but an accelerated Asterinas desktop and Linux performance parity are **not established**. All new implementation changes are local to `gpui-shell`; Asterinas was reviewed read-only to avoid concurrent PCI/MSI-X/IRQ work. No credentials, commits, pushes, publications or remote Actions were used. The previously published combined monorepo was not modified.

## Fixed locally

- `nix/build.nix`: replace bare `builtins.path` with `lib.cleanSourceWith`, retaining the original source/filter metadata Crane needs to inspect manifests during read-only evaluation. Full `flake check --no-build` reproduced an invalid filtered source-store-path error before this change, then passed with it. The existing source filter and dependency policy are preserved.
- `crates/app/src/main.rs`: primary startup now rejects absent/empty `WAYLAND_DISPLAY` clearly; compositor connection errors return status 1 rather than triggering the release panic/abort path. Both failures explicitly release the IPC socket. Configuration validation and secondary IPC-only calls remain usable outside a Wayland session.
- `crates/app/src/bar/view.rs`, `frame_metrics.rs`, `frame_telemetry.rs`: log each bar's actual selected GPUI device/driver; opt-in, bounded render-entry observations and element-tree build wall time, with p50/p95/p99/max. Default startup does not drive extra frames. An explicitly requested 1–60 second redraw probe uses GPUI animation invalidation and stops requesting redraws at its deadline. No CPU loop or timer supplies fake frame samples.
- `scripts/analyze_frame_telemetry.py`: validate observations and associate each window's device with an unambiguous upstream selected-backend log. CPU/software and virtual devices are labelled; even an apparently physical GPU name remains unverified because public `GpuSpecs` lacks adapter type/PCI identity. The strict physical-GPU gate fails closed.
- Developer documentation covers telemetry limits and NixOS user-manager environment inheritance, including Niri's `NIRI_SOCKET` / `NIRI_SOCKET_PATH` requirement.

## Validation that passed

Evidence root: `/workspace/asterinas-gpui/evidence/gpui-second-review/`.

- `cargo test --workspace --locked`: **166 passed**, including the new eight statistics/probe/classification regressions and one asset doctest. `rust-workspace-final.log`.
- Telemetry analyzer: **15 passed**. `analyzer-tests.log`.
- Executable startup/IPC regressions: **5 passed**, with private runtime directories and an explicitly labelled IPC receiver fixture. `session-startup-tests.log`.
- Existing Asterinas graphics host tests: **38 passed** in the read-only review. These include fixtures and do not execute the Asterinas kernel.
- Full Nix flake evaluation and x86_64 module evaluation pass; the aarch64 module check derivation also evaluates, without an aarch64 build/runtime. `flake-check-fixed.log`, `nix-module-final.log`, `nix-aarch64-module-eval.log`. A separate later Nix invocation initially inherited incompatible Rust-shell `LD_LIBRARY_PATH`; the final check runs Nix with that variable unset. This environment failure is retained separately, not labelled a source/test failure.
- Rust formatting, Nix build-expression formatting and whitespace checks pass.
- The new shell executable builds in the **unoptimized dev profile**, with debug information disabled for disk-bounded validation. This is not a release performance benchmark. `shell-build.log`.
- Real nested Niri sessions pass with passive telemetry and a three-second redraw probe, surface creation and five-second process survival. A stronger final-state check confirms the passive session's bar settles on **Top** with keyboard interactivity **None** (and a dock is also present). The earlier Background layer was an initial, pre-settled observation—not a composition defect. `nested-passive/result.json`, `nested-passive/final-layers.json`, `nested-probe/result.json`.
- Live-log analysis succeeds, while `--require-physical-gpu` exits **3** as intended. `nested-probe/telemetry-analysis.json`, `nested-probe/strict-gpu-analysis.json`, `nested-passive/telemetry-analysis.json`.

Both nested sessions select **llvmpipe (LLVM 21.1.8, 256 bits)**, **Vulkan**, driver **llvmpipe / Mesa 26.1.5 (LLVM 21.1.8)**. This is CPU software rendering. There is no `/dev/dri`, `/dev/nvidia*` or `/dev/kvm` here. The probe observes its deadline and stops driven redraws; later passive updates/idle gaps remain observable. Physical presentation FPS is deliberately null. Consolidated machine-readable facts: `validation-summary.json`.

Screenshots retained as smoke evidence: `nested-probe/gpui-nested.png` and `nested-passive/gpui-nested.png`. They are not evidence of physical GPU acceleration.

## What still prevents complete integration

Asterinas currently exposes dumb buffers and legacy KMS through a boot-framebuffer/simpledrm path. PRIME/dma-buf sharing is unsupported; complete GPU submission/memory management, render-node functionality, synchronization/fence operations and accelerated modesetting/presentation remain missing. Experimental NVport/PCI/DMA/IOMMU/interrupt infrastructure is not a complete NVIDIA RM/GSP or Vulkan driver. These findings must be coordinated with the concurrent Asterinas work, not patched over in the shell.

On GPUI, the public API does not expose physical presentation timestamps, adapter type or PCI identity. `on_next_frame` does not solve that: the locked platform loop can execute it before its next draw. Accurate display FPS needs real per-commit presentation feedback, discarded-frame/output/clock tracking and credible hardware timestamp/completion flags. [Frame callback pacing](https://wayland-book.com/surfaces-in-depth/frame-callbacks.html) is distinct from [presentation-time feedback](https://wayland.app/protocols/presentation-time).

The Niri backend still supplies zero generic monitor geometry; fractional-scale sizing falls back to GPUI's display bounds. Multi-monitor hotplug, fractional scaling, compositor/renderer device matching, real input interactions, all desktop services, native Asterinas Wayland/Vulkan execution and a live NixOS/systemd session were not validated. No rebuilt Nix release package or performance speedup is claimed. The existing release size optimization was not changed speculatively.

Real Linux parity requires matched physical hardware, driver/firmware, resolution/scaling/refresh, power settings, compositor/workload and release builds on both systems, with actual presentation timing and rendering correctness. Neither nested llvmpipe observations nor the existing Vulkan compute benchmark supplies that evidence.

Developer reproduction guide: `/workspace/asterinas-gpui/gpui-shell/docs/src/content/docs/developer/performance.md`. The strengthened smoke runner is `evidence/gpui-second-review/run_nested_review_smoke.py`, with the same CLI as the prior smoke runner. No shared PR notes were overwritten.
