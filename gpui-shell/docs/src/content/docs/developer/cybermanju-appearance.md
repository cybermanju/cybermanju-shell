---
title: CyberManju appearance snapshots
description: An opt-in bridge from existing Vue preferences to GPUI color tokens, with unsupported settings disclosed.
---

## What the bridge actually does

The CyberManju app retains its existing personalization controls and stores. Its Settings appearance section can export a versioned **cosmetic snapshot**. GPUi Shell now reads that file with `--appearance-profile FILE`, validates it before IPC/display initialization, and projects its theme and accent into the real GPUI `Theme` global before opening the shell windows. No settings service, new preference database, compositor, GPU device, or browser transport is introduced.

By default this is a startup-only import. Add `--watch-appearance-profile` to follow subsequent saves to that explicitly supplied file. Supply the path again after restarting the shell. A running primary shell cannot receive new import paths through launcher IPC: a secondary invocation exits unsuccessfully and explicitly says the profile was **not applied**, without sending a launcher command. `--validate` remains available even when a shell is running, without Wayland or a renderer; it does not start a watcher.

```sh
# Export from CyberManju Settings, then inspect without a display:
gpuishell --validate --appearance-profile /path/to/cybermanju-appearance-v1.json

# With no primary shell running, on an otherwise supported Wayland session:
gpuishell --appearance-profile /path/to/cybermanju-appearance-v1.json

# Opt in to subsequent replacements/saves of that same file:
gpuishell --appearance-profile /path/to/cybermanju-appearance-v1.json --watch-appearance-profile
```

No Tauri/Vue process has been made native to Asterinas by this change. Linux-host parser/build tests are not proof that GPUI runs on Asterinas. The older `/workspace/cybermanju-shell` snapshot does not acquire these changes automatically.

## Version 1 and ownership

The only accepted schema is `com.cybermanju.appearance`, version `1`. The top-level fields are `schema`, `version`, `settings`, `shellStyle`, `panelDocked`, and `wallpaper`. Unknown fields are rejected rather than used as a general settings or credential channel.

`settings` contains the existing resolved canonical theme, optional legacy shell preference, global accent, per-theme accents, density, glass level, motion, follow-system flag, preset wallpaper identifier and glow flag. Five themes are accepted: `os-dark`, `os-light`, `os-graphite`, `plasma-dark`, and `plasma-light`. Legacy aliases/auto are resolved by the app's existing settings normalization before export; `followSystem` is retained but GPUI does not watch Vue/browser system-preference events. Both `#RGB` and `#RRGGBB` accents are supported, with the existing global → per-theme → built-in precedence.

GPUI applies a semantic native Base16 palette and accent, **not an exact reproduction of Vue chrome**. Its existing font size is preserved; old native color overrides cannot defeat the explicitly imported colors. Native `theme.toml` reloads retain the cached imported colors while adopting the new native font size. No `config.toml`, `state.toml`, `theme.toml`, localStorage or IndexedDB settings are rewritten by the importer.

## Live-file import is explicit, not an app transport

With the watch flag, startup registers the file's parent watch before rereading the initial profile, covering saves during setup. Each later file event loads the bounded profile off the GPUI foreground, validates and projects the complete replacement before changing cached/global state, then requests real window redraws. Invalid JSON, missing files, disallowed terminal symlinks and projection errors retain the last good palette and show a dedicated sticky appearance-error card. A good update clears only that card, not unrelated config/theme errors. Watch registration errors refuse startup; a stopped watcher is surfaced rather than silently claiming live synchronization.

The existing inotify watcher is reused with bounded pending notifications and trailing-edge coalescing so rapid/atomic saves do not lose the final replacement. Export or replace the same file to update it. The app still does not automatically write to that path or communicate with the shell: browser downloads, Tauri/native IPC, authenticated remote synchronization and an OS settings daemon are not provided. There is no automatic path discovery, profile write-back or preference-store migration.

In the last validated binary, density applies to semantic padding/gaps in the native `LauncherFrame` query, body and footer. `compact` uses the app's existing `DENSITY_SCALE.unit = 0.9`; `comfortable` uses `1.0`, which preserves the original native spacing when no profile is imported. The runtime Theme carries this choice across live profile updates and native theme reloads. It does not scale typography, icons, bespoke control hit targets, bar width, panel docking or compositor geometry, and it is not added to `theme.toml`. That binary's validation report says **launcher spacing only**, not whole-desktop density.

**Source-only extension, not yet built or tested:** shared `ListItem` rows now consume runtime density for horizontal padding/gaps and default vertical padding. Explicit `.spacing(Sparse/Dense/ExtraDense)` vertical overrides retain their authored pixel sizes; indentation, slots, callbacks and interaction state remain intact. Three regression tests were added but not executed. The source report now names launcher and shared list-row spacing, but the existing binary is unchanged. Only formatting, whitespace and patch checks are permitted under the current low-disk restriction; old test counts do not validate this extension.

Wallpaper, shell style, panel docking, glass, motion and glow are retained in the snapshot and explicitly reported as **not applied**. Density outside the launcher chrome and live follow-system tracking remain unavailable. Geometry, theme-owned accent variants not currently selected, app keybindings/gestures/locale, and compositor workspace policy are not silently translated into invented native controls.

## Wallpaper and bounded input

A wallpaper descriptor is one of:

- `preset`: a known preset identifier matching `settings.wallpaper`.
- `url`: an HTTP(S) URL with a host, no username/password, no control characters or leading/trailing whitespace, at most 8192 UTF-8 bytes.
- `local-file`: basename, supported image MIME type and nonzero size at most 20 MiB. No file contents, absolute path, object URL or blob is exported or loaded.

The native loader opens one read-only descriptor with `O_NOFOLLOW | O_NONBLOCK | O_NOCTTY`, checks that descriptor is a regular file, and reads at most 64 KiB plus a sentinel byte. A terminal symlink is now rejected, including one pointing to an otherwise valid profile; use the real regular file instead. There is no pathname-metadata precheck that could race replacement by a blocking FIFO. FIFOs without writers, directories and Unix sockets are refused. The descriptor is never used to create or truncate a file.

This is **not a path sandbox or a universal I/O timeout**. Parent-directory symlinks are still followed, concurrent in-place regular-file writers are not locked out, and regular filesystem/device I/O can still wait. The flags' [Rust interface](https://doc.rust-lang.org/std/os/unix/fs/trait.OpenOptionsExt.html#tymethod.custom_flags) and [Linux semantics](https://man7.org/linux/man-pages/man2/open.2.html) do not promise otherwise. The loader does not fetch URLs, read wallpaper files, migrate IndexedDB blobs or manufacture an image where one is missing. Local wallpaper migration and per-install permission checks remain separate work.

## Real tests and open dependencies

`cargo test --workspace --locked --offline` exercises the native parser, shared app-export fixture, five real palette projections, accent precedence, preference retention, unknown-field/version/type/conflict rejection and wallpaper bounds. It also preserves the existing IPC and presentation parser/lifecycle tests.

Additional tests use real files, symlinks, atomic replacements and inotify to feed actual native `Theme` values through accepted changes, rejected saves and recovery. Native font changes preserve imported colors, invalid projections cannot replace the cached profile, and appearance error recovery leaves native config/theme cards alone. These exercise the live-file data path, not rendered Wayland windows.

After building `gpuishell`, `python3 scripts/test_appearance_profile.py -v` runs the actual binary against isolated configuration/runtime directories and real Unix sockets. It verifies renderer-free validation, oversized/unsafe profile refusal, terminal-symlink rejection, no-writer FIFO refusal with a subprocess deadline, directory/socket rejection, no preference writes, secondary import refusal and unchanged ordinary launcher IPC. A negative baseline reproduces the old binary accepting a terminal symlink; the corrected binary rejects it. These are parser/CLI tests, not a compositor or GPU benchmark.

The shared fixture is `crates/app/src/config/appearance/fixtures/appearance-profile-v1.json`, copied byte-for-byte from the app repository's `tests/fixtures/appearance-profile-v1.json`; keep both synchronized when deliberately evolving the contract.

A separate real-window smoke test started the built shell under isolated nested Niri/Xvfb with forced Mesa lavapipe. The bar visibly changed from `os-dark` to `os-light` after atomic file replacement, invalid JSON showed the actual appearance-error notification without killing the shell, and valid `os-dark` recovered. Unmodified captures and the finite reproducer are in `/workspace/asterinas-gpui/evidence/cybermanju-live-profile/host-window-smoke/`. This proves host software-rendered live color updates, **not** a native Asterinas desktop, GPU acceleration, complete CyberManju app integration or a performance benchmark.

Asterinas boot/userspace ABI → native per-GPU driver and Vulkan → Wayland compositor/presentation → GPUI runtime → Tauri/GTK/WebKit runtime and secure app/API boundary remain independently unproven gates. Full appearance propagation, media migration, restart/restore and physical-GPU performance must not be inferred from successful profile validation.
