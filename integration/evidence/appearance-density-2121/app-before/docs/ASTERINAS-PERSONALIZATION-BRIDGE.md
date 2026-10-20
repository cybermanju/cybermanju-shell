# CyberManju appearance bridge for the Asterinas-first OS

## A concrete first connection

CyberManju keeps its existing Settings controls, appearance tokens and per-install storage. The app can export a version-1 cosmetic snapshot and explicitly preview/restore supported profiles through those existing controls. The local GPUI shell can read that file and project its **theme palette and accent** into GPUI's native theme. This does not replace the app with a second settings interface.

The active base remains **Asterinas**, as selected on 2026-10-20. Ubuntu+niri is an optional comparison environment, not a replacement kernel or the target architecture. Neither a JSON export nor a Linux-host build proves the intended Asterinas desktop works.

## Dependencies and ownership

```text
Existing Vue Settings / useTheme / custom wallpaper service
  → validated, versioned cosmetic JSON snapshot (no credentials or image bytes)
  → explicit local file transfer
  → GPUI --appearance-profile FILE validation
  → optional --watch-appearance-profile follows subsequent saves to that file
  → native GPUI Theme global + redraw, palette + accent only

Still required for the final Asterinas system:
Asterinas boot + userspace ABI
  → native per-GPU AMD/NVIDIA driver, firmware, DMA/IRQ and Vulkan
  → validated Wayland compositor, input, session services and presentation
  → GPUI shell runtime and native Tauri/GTK/WebKit app runtime
  → secure live app/shell API and complete settings/media migration
```

These are separate boundaries. The default implementation is a startup snapshot; an explicit watch flag follows saves to that one file. This is **not live app-to-shell IPC, authenticated remote synchronization or an OS settings daemon**. The app does not automatically write the watched path. It does not touch files/vault/agent/search/sync/scheduler/secrets transports or their permission/error contracts.

Work lives in this app repository and `/workspace/asterinas-gpui/gpui-shell`; the published `/workspace/cybermanju-shell` tree remains an initial snapshot and receives none of these changes automatically. No publication, Git authentication, commit or remote Actions is part of this pass.

## Supported and retained preferences

The export preserves canonical appearance settings, global/per-theme accents, density, glass, motion, follow-system, wallpaper preset and glow, plus the separate shell style and panel docking settings. Existing normalization resolves `auto` and legacy aliases; the resolved theme is a snapshot, not a replacement for the retained follow-system preference.

The native consumer supports all five current themes and global → per-theme → built-in accent precedence. It projects semantic GPUI Base16 colors rather than promising pixel-identical Vue chrome, preserves GPUI font size and does not rewrite existing configuration files.

**Not applied to the native desktop yet:** wallpaper, shell geometry/style, panel docking, density, transparency, motion, glow or live system-theme following. The consumer explicitly reports retained-but-unapplied fields rather than acknowledging successful no-ops. Keyboard bindings/KPL, gestures/JSON, locale and other app-local preferences keep their existing behavior and are outside this initial cosmetic snapshot.

Wallpaper metadata is a known preset, validated HTTP(S) URL without credentials, or bounded local image filename/MIME/size descriptor. Version 1 exports no blob, filesystem path or image contents. The app's wallpaper bytes remain per-install in IndexedDB; their migration, access permissions, missing-asset behavior and offline availability are still open work. The native importer never downloads or invents a wallpaper.

## How to inspect the bridge

Export from the existing appearance area in Settings. With the changed GPUI shell built locally:

```sh
gpuishell --validate --appearance-profile /path/to/cybermanju-appearance-v1.json
# Start only when no primary shell is already running:
gpuishell --appearance-profile /path/to/cybermanju-appearance-v1.json
# Or explicitly follow later replacements/saves of this same file:
gpuishell --appearance-profile /path/to/cybermanju-appearance-v1.json --watch-appearance-profile
```

`--validate` needs no display or GPU and makes no preference writes; even with the watch flag it starts no watcher. The profile is bounded to 64 KiB and rejects unknown schema versions, unknown fields, invalid colors/types, conflicting preferences and unsafe wallpaper metadata. A secondary shell invocation refuses imports rather than silently opening the launcher and claiming an import. Supply the file again on the next shell start; native export-back/restore are not implemented.

Opt-in live-file import loads changes off the foreground, applies a completely validated replacement to GPUI's theme and requests window redraws. Bad saves, missing files and disallowed symlinks retain the last good colors and post a separate sticky appearance error; recovery clears only that card. A stopped watcher is reported, and registration failure refuses startup. Bounded, trailing-edge inotify notifications cover rapid/atomic saves without creating a second preference store. Native theme-file reloads preserve imported colors while updating native typography. This is a real file-to-theme path, not a claim that rendered Wayland windows or Asterinas userspace have been certified.

The native file loader now rejects terminal symlinks, no-writer FIFOs, directories and Unix sockets. It opens one read-only descriptor with nonblocking/no-follow/no-controlling-terminal flags before checking its type, avoiding the earlier pathname-stat/open race. Parent directory symlinks are not sandboxed, regular-file writers are not locked out, and regular filesystem I/O is not deadline-bounded. These safeguards have actual Linux-host CLI tests, not an Asterinas userspace-runtime certification.

## Restore within the existing app

Choose a profile JSON in the Settings appearance section, inspect its preview, then explicitly apply it. Selection/preview alone does not mutate preferences. Input is validated against the same bounded v1 schema again before application. URL wallpapers go through the existing custom-wallpaper service; presets clear the current custom override through its controller before applying the profile's existing theme/shell/panel preference owners. A rejected or failed wallpaper operation leaves those preferences unapplied.

A `local-file` descriptor is refused before mutation because no image bytes are present; the existing wallpaper is preserved. Choose an image on this installation or use a URL/preset profile instead. Restore status is deliberately **session-scoped**: existing localStorage/IndexedDB availability and limits still govern persistence. This is not an atomic transaction across stores, a durable backup, a native GPUI restore, or a completed restart/media-migration guarantee.

The same fixture is tested by the app and native parser: `tests/fixtures/appearance-profile-v1.json` and GPUI's `crates/app/src/config/appearance/fixtures/appearance-profile-v1.json`. The app has normalization/export and restore-orchestration tests; GPUI has real Rust palette/parser/inotify-to-theme tests and built-binary CLI regressions with isolated directories and real Unix sockets. The final live-file follow-up passes **201 GPUI Rust tests and 36 Python tests** (13 profile CLI cases), native build and Clippy. App sources were not changed in that follow-up: their preceding frontend run passed 1,207 tests (11 profile cases), and version/header checks were repeated. Those prove prerequisites, not Asterinas runtime compatibility or physical-GPU acceleration. No browser import/export click-through was exercised.

## What still blocks acceptance

A separate actual-window check passed in nested Niri/Xvfb on the Linux host with forced Mesa lavapipe: `os-dark` changed visibly to `os-light`, invalid JSON produced the real appearance warning while retaining the last good palette, and valid `os-dark` recovered. Logs, reproducer and unmodified captures are in `/workspace/asterinas-gpui/evidence/cybermanju-live-profile/host-window-smoke/`. It validates live colors on a software-rendered host shell—not Asterinas, physical GPU performance, full app integration, input behavior or all desktop services.

[`OS-A6-PERSONAL`](../TASKS.md) stays open until supported settings propagate and round-trip after app/shell restart, imports/restores/schema migration preserve preferences, media is handled safely, and accessibility/scaling are checked on the real target. Tauri/Wry/GTK/WebKit and GPUI/compositor/desktop-service compatibility remain independent Asterinas runtime gates.

The current environment exposes no physical display-class PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. Native CPU/QEMU tests are useful prerequisite evidence but cannot certify AMD/NVIDIA rendering, Vulkan presentation or Linux parity. Acceptance still requires correct real workloads on each supported physical GPU under Asterinas, followed by end-to-end presentation/performance evidence.

See the [Asterinas-first plan](CYBERMANJU-OS-PLAN.md#active-plan--asterinas-first-2026-10-20) and [ABI baseline](ASTERINAS-ABI-BASELINE.md) for the remaining kernel, runtime, app-safety and recovery gates.
