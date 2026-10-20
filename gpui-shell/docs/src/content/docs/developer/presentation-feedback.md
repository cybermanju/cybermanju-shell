---
title: The missing presentation-feedback boundary
description: Why per-buffer Wayland feedback needs an upstream GPUI hook, and the tested contract for adding it safely.
---

## What works—and what is still missing

The shell measures real bar render entries and CPU-side view-building time. It **does not yet observe per-buffer presentation events**. Physical presentation FPS stays null. No app-level listener, second Wayland connection, replacement GPU device, or synthetic frame counter has been added.

The locked GPUI exposes raw display and surface handles, but not its original protocol queue or a callback at the actual buffer commit. Creating a feedback request in `Bar::render`, `Window::on_next_frame`, or after a draw cannot reliably associate it with the buffer that was displayed. The safe next change belongs upstream, at GPUI's Wayland/wgpu boundary.

Production renderer diagnostics now explain this explicitly: `presentation_feedback_status` is `unavailable_locked_gpui`, `host_monitor_scanout_verified` is false, and the existing physical-timestamp and hardware-verification fields remain false. This is a capability diagnostic, **not a new presentation API**.

## The exact stack and surface

`Cargo.lock` pins Zed/GPUI to `b5e03c7b0302894ac7a57d233be8d03dc9c81223`, wgpu to `29.0.4`, wayland-client to `0.31.14`, wayland-backend to `0.3.15`, and wayland-protocols to `0.32.13`. This audit is about that revision, not an assumed current GPUI API.

The following source links are pinned to the audited revision:

| Boundary | Evidence and consequence |
| --- | --- |
| Shell bar | `crates/app/src/bar/view.rs` opens a `WindowKind::LayerShell` with namespace `bar`. A render entry is not a buffer commit. |
| Original connection | [GPUI Wayland client](https://github.com/zed-industries/zed/blob/b5e03c7b0302894ac7a57d233be8d03dc9c81223/crates/gpui_linux/src/linux/wayland/client.rs#L636-L643) creates the connection and queue; its calloop `WaylandSource` owns dispatch at lines 849–851. `Globals.qh` and `WaylandClientState` are internal. |
| Actual bar surface | [Wayland window](https://github.com/zed-industries/zed/blob/b5e03c7b0302894ac7a57d233be8d03dc9c81223/crates/gpui_linux/src/linux/wayland/window.rs#L738-L790) creates the `wl_surface`. Lines 555–575 pass its own surface/backend pointers to wgpu, not a surface from another connection. |
| Public raw handles | [GPUI Window](https://github.com/zed-industries/zed/blob/b5e03c7b0302894ac7a57d233be8d03dc9c81223/crates/gpui/src/window.rs#L6207-L6218) delegates raw handles. Wayland window lines 1403–1427 return pointers, not a safe original `QueueHandle` or commit subscription. |
| Successful buffer presentation | [WgpuRenderer::draw](https://github.com/zed-industries/zed/blob/b5e03c7b0302894ac7a57d233be8d03dc9c81223/crates/gpui_wgpu/src/wgpu_renderer.rs#L1086-L1339) acquires a texture, renders, submits and calls `frame.present()`. There are **two** present sites: the instance-buffer overflow limit path and the normal submission path. Both need the hook. |
| No-buffer commits | Wayland window lines 1739–1749 may call `surface.commit()` when no renderer frame was presented. Initial mapping, input-region and layer configuration changes also commit state. These must not be called new rendered buffers. |
| Driver/WSI boundary | Locked wgpu-hal Vulkan `src/vulkan/swapchain/native.rs::present` calls `vkQueuePresentKHR`; GL `src/gles/egl.rs::present` calls `eglSwapBuffers`. Drivers own the actual Wayland attach/commit. A pre-present callback is necessary but still needs ordering validation for each supported WSI implementation. |

A foreign-display wrapper is technically possible, but is not the missing safe hook. The [backend FFI contract](https://docs.rs/wayland-backend/0.3.15/wayland_backend/sys/client/struct.Backend.html) requires the display to outlive every guest-backend clone and prohibits taking over a proxy already managed by someone else. It does not grant access to GPUI's queue or serialize its commits. Do not use `manage_object` on GPUI's surface, move its proxy to another queue, reconnect to the compositor, or introduce another render device.

## The smallest upstream change

1. **Own the protocol in `gpui_linux`.** Add optional `wp_presentation` binding to the existing registry/`Globals` and dispatch through the existing `WaylandClientStatePtr` queue. Negotiate `min(advertised_version, 2)`; absence is a normal unsupported state. Keep the advertised clock ID for the connection. The [locked `feedback` method](https://docs.rs/wayland-protocols/0.32.13/wayland_protocols/wp/presentation_time/client/wp_presentation/struct.WpPresentation.html) requires the original `WlSurface`, `QueueHandle<D>`, and immutable per-feedback user data.

2. **Bracket the real present attempt, not the whole draw.** Add a narrowly scoped pre-present closure to `WgpuRenderer::draw`, immediately before **each** `frame.present()` call. `WaylandWindow` supplies a closure capturing clones of the original surface/protocol/queue plus a small connection-owned tracker. Never reborrow the currently mutably borrowed window state from that closure. The hook does not run on failed acquisition, reconfiguration, recovery, or no-buffer `completed_frame` paths. Leave existing adapter/device selection and buffer production unchanged.

3. **Prove WSI ordering before enabling measurements.** The callback must issue `presentation.feedback(original_surface, original_qh, key)` before the corresponding driver buffer commit and after any earlier commit on that surface. Same-surface commits must be serialized. Confirm on the wire for Vulkan and GL separately; drivers may defer commits beyond a `present()` call. If one implementation queues asynchronously or inserts an intervening state-only commit, a GPUI pre-present closure alone is insufficient: move association into that WSI's actual attach/commit boundary or leave that backend unsupported. Do not assume `frame.present()` returning proves a successful commit or presentation. Presentation errors can leave feedback awaiting a later commit; disable that surface's observer and retire pending work without crediting it to a subsequent frame.

4. **Deliver a read-only per-window subscription.** Add a platform-neutral event type at GPUI's `PlatformWindow`/`Window` boundary, with an unavailable default for other platforms. The Wayland dispatcher forwards immutable records to the subscribed original window only, outside `RefCell` borrows. Subscribe only bar windows locally, and unsubscribe on close. This proposed API is not implemented in this checkout. Use window/surface generation and monotonically allocated commit keys; never key solely by recyclable Wayland object IDs or callbacks. Recovery/recreated surfaces receive a new generation. Allocate keys once; never reuse a completed key.

5. **Bound work independently of redraws.** Keep a connection-wide outstanding-proxy cap (64 in the executable contract), a 2-second monotonic deadline, and a bounded 4096-record ring. An event-loop timer must expire requests even when the bar is hidden or idle. Cap retained output aliases (16 here), record omitted output events/dropped samples, and avoid new proxies while the outstanding budget is exhausted. The omitted-event count does not claim to count distinct omitted outputs. Timeout is not a protocol discard.

6. **Honor terminal-object lifetime.** `presented` and `discarded` are terminal destructor events. Feedback has **no client destroy request**. Dropping a Rust handle, destroying `wp_presentation`, or timing out does not cancel its server object. Timed-out entries release payload storage but retain bounded outstanding slots until an actual terminal event or connection loss. Keep that retirement state at connection scope when a window closes; do not accumulate an orphaned tracker per recreated surface. On shutdown, unsubscribe, stop timers, destroy renderer before its surface, and release connection resources in owner order. Do not close the live shell connection merely to clear telemetry.

## What each real event must preserve

The [presentation-time protocol](https://wayland.app/protocols/presentation-time) associates one feedback object with one content submission. The [locked generated events](https://docs.rs/wayland-protocols/0.32.13/wayland_protocols/wp/presentation_time/client/wp_presentation_feedback/enum.Event.html) retain these fields:

- **Clock:** raw `clock_id`; never subtract it from Rust `Instant` or core Wayland input timestamps. Missing or changed clocks cannot establish timing. A clock change on one connection disables further requests.
- **Time:** full-width `(u64 seconds, u32 nanoseconds)`, requiring nanoseconds below 1,000,000,000. Do not overflow a `u64` by flattening arbitrary seconds to nanoseconds.
- **Output:** every related `sync_output` proxy, mapped through the original registry to a global/name and generation where available. Multiple bound aliases can mean the same physical output. Missing output association stays missing; output IDs are not DRM/PCI identity. Reset interval comparisons on output changes/hotplug.
- **Sequence and refresh:** join high/low MSC words without truncation. Zero MSC is unknown/unavailable, not a measured zero frame counter. `refresh` is a prediction (zero means unavailable), **not measured FPS**. Respect the version-1/version-2 variable-refresh distinction.
- **Flags:** retain raw bits including unknown bits, and report `vsync`, `hw_clock`, `hw_completion`, and `zero_copy` separately. Absence of `vsync` means hardware-vsync was not asserted; it is not proof of asynchronous physical scanout or tearing. Never infer flags from mailbox/FIFO choice.
- **Outcome:** distinguish presented, compositor-discarded, local-timeout, invalid protocol timestamp, connection loss, and late terminal events. Never turn discarded or timed-out work into a presented sample.

An eventual compositor presentation rate uses only valid ordered timestamps for the **same connection, clock, surface generation and output**, with duplicate/non-increasing timestamps and unresolved losses disclosed. It remains a compositor-feedback rate until host display provenance is independently verified. Do not promote hardware flags, a vendor device name, or a nested feedback event into physical GPU/scanout certification. CPU renderers stay `software_cpu`; other renderer labels remain unverified unless additional device/driver/scanout evidence exists.

## What is code-tested locally

`crates/app/src/presentation_contract.rs` is compiled **only for tests** through `#[cfg(test)]`. It consumes the actual locked generated Rust protocol event types using parser fixtures. It checks full-width fields, flags, invalid nanoseconds, missing/changing clocks, aliases, per-surface keys, out-of-order/duplicate/untracked terminal events, discard, timeouts, late completion, disconnect and storage/proxy budgets.

This is an executable parser/lifecycle contract, **not a compiled upstream GPUI patch**, protocol subscription, WSI ordering proof, or physical-display measurement. Reservation does not send a request. Production binaries contain no contract tracker. The future adapter must enforce monotonically unique keys, connection/output identity and wire ordering; fixtures cannot prove those conditions.

```sh
# In the project's normal dependency environment; cached builds here use low debug.
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --workspace --locked
python3 -m unittest discover -s scripts -p 'test_frame_telemetry.py' -v
GPUI_SHELL_TEST_BINARY=target/debug/gpuishell \
  python3 -m unittest discover -s scripts -p 'test_session_startup.py' -v
```

## Acceptance gates—not Linux parity claims

`scripts/test_nested_presentation_boundary.py` starts isolated Xvfb → Niri → GPUI sessions with software Mesa. It checks actual bar layer surfaces, original surface attach/commit trace activity, diagnostic status, passive and bounded-redraw telemetry, and strict rejection of physical-GPU certification. It also records whether the nested compositor advertises presentation-time. **Advertising the protocol is not receiving per-buffer feedback.** With today's unpatched GPUI, feedback-request count must remain zero.

```sh
# NIRI is a Niri executable; MESA is the Mesa prefix containing both the EGL
# vendor JSON and lavapipe Vulkan ICD. Run with the binary's runtime libraries.
# Optionally add --library-path FILE with a tested Niri/GPUI library-path bundle.
dbus-run-session -- python3 scripts/test_nested_presentation_boundary.py \
  --niri "$NIRI" --mesa "$MESA" --mode passive \
  --output target/presentation-review/nested-passive
dbus-run-session -- python3 scripts/test_nested_presentation_boundary.py \
  --niri "$NIRI" --mesa "$MESA" --mode probe \
  --output target/presentation-review/nested-probe
```

The private D-Bus session avoids contacting the existing desktop's services. The harness strips Nix `LD_LIBRARY_PATH`/`LD_PRELOAD` from system Xvfb and `wayland-info`, while preserving the binary's library environment for Niri and GPUI. This avoids mixing a system loader with incompatible Nix glibc libraries. The runtime bundle also needs Niri's dynamically loaded X11 libraries (such as libXcursor), not just GPUI's build dependencies. Wayland debug tracing adds overhead: these runs validate integration boundaries, not performance.

After the upstream hook exists, add genuine end-to-end checks for both present branches, Vulkan and GL WSI ordering, compositor without presentation support, hidden/occluded bars, resize/output changes, surface destruction/recreation, driver recovery, discard, event-loop timeout, and outstanding-proxy saturation. A trace assertion must bind each feedback request to the next **buffer-bearing commit on that exact original surface**, excluding state-only commits; no fixture can stand in for this test. Mocking remains limited to parser logic.

Even real `presented` events from nested Niri would describe its nested output. They are **not proof of the host monitor's physical scanout**. This workspace's CPU software renderer, Xvfb and Linux kernel are not native Asterinas performance evidence. Asterinas's accelerated display path and Linux parity remain unestablished; no Asterinas file is changed by this work.
