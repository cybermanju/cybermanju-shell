# CyberManju appearance bridge for the Asterinas-first OS

## A concrete first connection

CyberManju keeps its existing Settings controls, appearance tokens and per-install storage. The app can now export a version-1 cosmetic snapshot; the local GPUI shell can read that file and project its **theme palette and accent** into GPUI's native theme. This does not replace the app with a second settings interface.

The active base remains **Asterinas**, as selected on 2026-10-20. Ubuntu+niri is an optional comparison environment, not a replacement kernel or the target architecture. Neither a JSON export nor a Linux-host build proves the intended Asterinas desktop works.

## Dependencies and ownership

```text
Existing Vue Settings / useTheme / custom wallpaper service
  → validated, versioned cosmetic JSON snapshot (no credentials or image bytes)
  → explicit local file transfer
  → GPUI --appearance-profile FILE validation
  → native GPUI Theme global, palette + accent only

Still required for the final Asterinas system:
Asterinas boot + userspace ABI
  → native per-GPU AMD/NVIDIA driver, firmware, DMA/IRQ and Vulkan
  → validated Wayland compositor, input, session services and presentation
  → GPUI shell runtime and native Tauri/GTK/WebKit app runtime
  → secure live app/shell API and complete settings/media migration
```

These are separate boundaries. The current implementation is a startup snapshot, **not live app-to-shell IPC, authenticated remote synchronization or an OS settings daemon**. It does not touch files/vault/agent/search/sync/scheduler/secrets transports or their permission/error contracts.

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
```

`--validate` needs no display or GPU and makes no preference writes. The profile is bounded to 64 KiB and rejects unknown schema versions, unknown fields, invalid colors/types, conflicting preferences and unsafe wallpaper metadata. A secondary shell invocation refuses imports rather than silently opening the launcher and claiming an import. Supply the file again on the next shell start; live reload/export-back/restore are not implemented.

The same fixture is tested by the app and native parser: `tests/fixtures/appearance-profile-v1.json` and GPUI's `crates/app/src/config/appearance/fixtures/appearance-profile-v1.json`. The app has pure normalization/export tests; GPUI has real Rust palette/parser tests and built-binary CLI regressions with isolated directories and real Unix sockets. Those prove the adapter boundary, not runtime compatibility or graphics acceleration.

## What still blocks acceptance

[`OS-A6-PERSONAL`](../TASKS.md) stays open until supported settings propagate and round-trip after app/shell restart, imports/restores/schema migration preserve preferences, media is handled safely, and accessibility/scaling are checked on the real target. Tauri/Wry/GTK/WebKit and GPUI/compositor/desktop-service compatibility remain independent Asterinas runtime gates.

The current environment exposes no physical display-class PCI GPU, `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`. Native CPU/QEMU tests are useful prerequisite evidence but cannot certify AMD/NVIDIA rendering, Vulkan presentation or Linux parity. Acceptance still requires correct real workloads on each supported physical GPU under Asterinas, followed by end-to-end presentation/performance evidence.

See the [Asterinas-first plan](CYBERMANJU-OS-PLAN.md#active-plan--asterinas-first-2026-10-20) and [ABI baseline](ASTERINAS-ABI-BASELINE.md) for the remaining kernel, runtime, app-safety and recovery gates.
