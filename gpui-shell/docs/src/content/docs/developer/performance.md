---
title: Rendering and performance evidence
description: Observe real GPUI bar render entries without confusing them with display FPS.
---

## What the measurements mean

GPUi Shell is an event-driven Wayland layer-shell client. An idle bar should not redraw at the monitor refresh rate. Low passive render-entry frequency is not, by itself, poor performance.

The shell can observe entry into each bar's `Render::render` and the elapsed wall-clock time spent building that bar's element tree on the CPU side. This is not thread CPU time and may include descheduling. These are real lifecycle observations, **not physical presentation FPS or full frame times**. They exclude subsequent GPUI layout/prepaint/paint, GPU submission and execution, compositor composition, buffer discard and physical scanout. Other surfaces (dock, launcher, notifications and control center) are not measured by this probe.

The locked GPUI public `Window` API has no physical presentation-feedback subscription. Its `on_next_frame` is also not a scanout timestamp: the platform loop can run those callbacks before its next draw. We deliberately do not turn callback counts, timer ticks, compute dispatches, or CPU loops into display FPS.

[Wayland frame callbacks](https://wayland-book.com/surfaces-in-depth/frame-callbacks.html) pace clients; they do not certify that a particular buffer became visible. Physical presentation measurements require per-commit [presentation-time feedback](https://wayland.app/protocols/presentation-time), including discarded updates, output association, clock identity and hardware-completion/timestamp flags. A nested compositor cannot certify the host display's scanout merely by advertising that protocol.

The [presentation-feedback boundary audit](../presentation-feedback/) identifies the exact original surface/queue and both wgpu present sites, documents the smallest upstream hook and WSI ordering checks, and provides a test-only protocol parser/lifecycle contract. Raw surface handles alone do not supply a safe app-layer per-buffer observer. Production diagnostics report `presentation_feedback_status=unavailable_locked_gpui`; the contract fixtures are not display measurements.

## Run the observations

Start a **new primary shell instance** in a supported compositor session (Hyprland, Niri or Mango); a second invocation only sends IPC to the existing shell, so new environment settings will not take effect there.

```sh
# Passive: observes normal bar updates, never requests extra frames.
RUST_LOG=info GPUI_SHELL_FRAME_TELEMETRY=1 gpuishell 2>&1 | tee gpui-passive.log

# Explicit diagnostic: ask GPUI to redraw the bar for at most 5 seconds.
RUST_LOG=info GPUI_SHELL_FRAME_PROBE_SECONDS=5 gpuishell 2>&1 | tee gpui-probe.log

python3 scripts/analyze_frame_telemetry.py gpui-probe.log
python3 -m unittest discover -s scripts -p 'test_frame_telemetry.py' -v
# With a local binary built in the normal development dependency environment:
GPUI_SHELL_TEST_BINARY=target/debug/gpuishell \
  python3 -m unittest discover -s scripts -p 'test_session_startup.py' -v
```

The redraw probe accepts integer seconds from 1 through 60, automatically enables telemetry, and does not change configuration files. Stop the passive instance before trying the probe. Invalid values are warned about and ignored. No frames are requested after the probe expires, but the shell keeps running normally. `probe_finished` means that a subsequent render entry observed the deadline, not that a monitor presented a final probe frame. An occluded/throttled surface may never produce enough observations, and no fabricated results are emitted to fill that gap.

Each bar/window reports separately in `GPUI_SHELL_FRAME_TELEMETRY` JSON log records. `render_entry_rate_hz` is interval count divided by the sum of observed entry intervals; it is deliberately not named FPS. `render_entry_interval_ms` and `view_build_wall_ms` report nearest-rank p50/p95/p99 and maximum. Idle gaps are included; startup has no interval until two entries exist. Reports retain at most 4096 samples, expose dropped samples/invalid intervals, and preserve intervals across report boundaries. They are emitted at roughly one-second boundaries **only when a render happens**; there is no idle timer. An incomplete final reporting window can be lost if the process exits or is killed. Logging and statistics have overhead and are opt-in.

The bounded probe is an intentional extra redraw workload, not a representative desktop interaction benchmark. GPUI can throttle inactive/unfocused layer surfaces, and the compositor can throttle hidden surfaces; neither a 30 Hz nor a 60 Hz observation establishes hardware limits. Compare like-for-like workloads and configurations, not passive idle rates against animated rates.

## Identify the renderer actually used

Every opened bar logs `GPUI_SHELL_RENDERER` with the selected window's `GpuSpecs`: device name, driver name/info and CPU-emulation status. The backend is intentionally null in this record because `GpuSpecs` does not expose it. The upstream `Selected GPU adapter: ... (Vulkan)` or `(Gl)` log identifies the actual wgpu backend; the analyzer matches it to the selected device rather than guessing from a separate `vulkaninfo` probe.

CPU/software renderers such as llvmpipe/lavapipe/SwiftShader are explicitly labelled `software_cpu`. Recognized virtual renderers are labelled `virtual_or_remote`. Everything else is `unverified_non_cpu`, **not** verified physical hardware: the public API lacks adapter type, PCI identity and a compositor/scanout device match. `hardware_acceleration_verified` remains false, and physical presentation FPS remains null. `--require-physical-gpu` in the analyzer deliberately fails closed even for an unverified NVIDIA/AMD name. Hardware certification needs additional evidence outside this API; there is no force-pass override.

The locked upstream wgpu integration enables Vulkan and GL, tests actual surface configuration during adapter selection, and can use compositor device hints. Its `ZED_DEVICE_ID` selection filter is upstream behavior, not a guarantee of acceleration or matching compositor/render devices. We leave that policy intact rather than introducing a second Vulkan instance that might select a different GPU. GPUi Shell's compositor IPC support is narrower than general Wayland protocol support.

## Asterinas is not yet an accelerated GPUI desktop

The reviewed Asterinas DRM implementation offers dumb buffers and legacy KMS. Its simpledrm copies shmem buffers into a boot framebuffer; it is not a GPU render driver. Missing end-to-end pieces include driver-specific GPU memory/submission, a complete NVIDIA RM/GSP or AMDGPU userspace/kernel contract, usable render nodes, PRIME/dma-buf import/export, synchronization/fences and GPU-backed modesetting/presentation. Compositor IPC, Wayland userspace, Vulkan loader/ICD, fonts and desktop services also need native validation. Experimental PCI/MSI-X/DMA/IOMMU/NVport work is infrastructure, not proof of any of those complete paths.

The existing Asterinas Vulkan compute benchmark measures compute correctness/timestamps, not Wayland presentation or shell FPS. Its graphics prerequisite diagnostic checks availability, not end-to-end integration. Neither can be used as a Linux-performance-parity result.

To establish parity, run the same hardware, firmware/driver, resolution, scaling, refresh, power settings, compositor and release build on both systems. Use the same sustained interactive/animation workload after warm-up and retain per-output presentation timestamps/discard counts, CPU/GPU utilization, p50/p95/p99 timings and errors. Validate rendering correctness and input behavior too. Software-Vulkan nested tests are useful integration smoke tests but cannot replace this experiment. Do not change release optimization or claim a speedup solely because a synthetic or software-rendered test looks faster.
