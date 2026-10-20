---
title: CyberManju appearance snapshots
description: An opt-in bridge from existing Vue preferences to GPUI color tokens, with unsupported settings disclosed.
---

## What the bridge actually does

The CyberManju app retains its existing personalization controls and stores. Its Settings appearance section can export a versioned **cosmetic snapshot**. GPUi Shell now reads that file with `--appearance-profile FILE`, validates it before IPC/display initialization, and projects its theme and accent into the real GPUI `Theme` global before opening the shell windows. No settings service, new preference database, compositor, GPU device, or browser transport is introduced.

This is a startup-only import. Supply the file again after restarting the shell. A running primary shell cannot receive profiles through the launcher IPC: the secondary invocation exits unsuccessfully and explicitly says the profile was **not applied**, without sending a launcher command. `--validate` remains available even when a shell is running, without Wayland or a renderer.

```sh
# Export from CyberManju Settings, then inspect without a display:
gpuishell --validate --appearance-profile /path/to/cybermanju-appearance-v1.json

# With no primary shell running, on an otherwise supported Wayland session:
gpuishell --appearance-profile /path/to/cybermanju-appearance-v1.json
```

No Tauri/Vue process has been made native to Asterinas by this change. Linux-host parser/build tests are not proof that GPUI runs on Asterinas. The older `/workspace/cybermanju-shell` snapshot does not acquire these changes automatically.

## Version 1 and ownership

The only accepted schema is `com.cybermanju.appearance`, version `1`. The top-level fields are `schema`, `version`, `settings`, `shellStyle`, `panelDocked`, and `wallpaper`. Unknown fields are rejected rather than used as a general settings or credential channel.

`settings` contains the existing resolved canonical theme, optional legacy shell preference, global accent, per-theme accents, density, glass level, motion, follow-system flag, preset wallpaper identifier and glow flag. Five themes are accepted: `os-dark`, `os-light`, `os-graphite`, `plasma-dark`, and `plasma-light`. Legacy aliases/auto are resolved by the app's existing settings normalization before export; `followSystem` is retained but GPUI does not watch Vue/browser system-preference events. Both `#RGB` and `#RRGGBB` accents are supported, with the existing global → per-theme → built-in precedence.

GPUI applies a semantic native Base16 palette and accent, **not an exact reproduction of Vue chrome**. Its existing font size is preserved; old native color overrides cannot defeat the explicitly imported colors. No `config.toml`, `state.toml`, `theme.toml`, localStorage or IndexedDB settings are rewritten by the importer. Later deliberate native theme/config changes follow the existing shell behavior; there is no live synchronization with the snapshot file or the CyberManju app.

Wallpaper, shell style, panel docking, density, glass, motion and glow are retained in the snapshot and explicitly reported as **not applied**. Live follow-system tracking is also unavailable. Geometry, theme-owned accent variants not currently selected, app keybindings/gestures/locale, and compositor workspace policy are not silently translated into invented native controls.

## Wallpaper and bounded input

A wallpaper descriptor is one of:

- `preset`: a known preset identifier matching `settings.wallpaper`.
- `url`: an HTTP(S) URL with a host, no username/password, no control characters or leading/trailing whitespace, at most 8192 UTF-8 bytes.
- `local-file`: basename, supported image MIME type and nonzero size at most 20 MiB. No file contents, absolute path, object URL or blob is exported or loaded.

The native loader requires a regular file and reads at most 64 KiB plus a sentinel byte before rejecting oversized input. It does not fetch URLs, read wallpaper files, migrate IndexedDB blobs or manufacture an image where one is missing. Local wallpaper migration and per-install permission checks remain separate work.

## Real tests and open dependencies

`cargo test --workspace --locked --offline` exercises the native parser, shared app-export fixture, five real palette projections, accent precedence, preference retention, unknown-field/version/type/conflict rejection and wallpaper bounds. It also preserves the existing IPC and presentation parser/lifecycle tests.

After building `gpuishell`, `python3 scripts/test_appearance_profile.py -v` runs the actual binary against isolated configuration/runtime directories and real Unix sockets. It verifies renderer-free validation, oversized/unsafe profile refusal, no preference writes, secondary import refusal and unchanged ordinary launcher IPC. These are parser/CLI tests, not a compositor or GPU benchmark.

The shared fixture is `crates/app/src/config/appearance/fixtures/appearance-profile-v1.json`, copied byte-for-byte from the app repository's `tests/fixtures/appearance-profile-v1.json`; keep both synchronized when deliberately evolving the contract.

Asterinas boot/userspace ABI → native per-GPU driver and Vulkan → Wayland compositor/presentation → GPUI runtime → Tauri/GTK/WebKit runtime and secure app/API boundary remain independently unproven gates. Full appearance propagation, media migration, restart/restore and physical-GPU performance must not be inferred from successful profile validation.
