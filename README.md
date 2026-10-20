# cybermanju-shell

A private, experimental combined source snapshot of **Asterinas**, **GPUI Shell**, and the **CyberManju app**. It includes current local source changes, tests, documentation, historical patch series and compact evidence—not just the upstream commits.

**This is not a fully integrated graphical operating system. GPUI has not been accepted running on Asterinas, and physical NVIDIA/AMD GPU acceleration, native RM/GSP/Vulkan, the target Wayland/Tauri runtime and Linux performance parity remain unproven.**

## Layout

| Directory | Contents |
| --- | --- |
| `asterinas/` | Complete current kernel/OSTD source, tests, docs and experimental graphics/PCI/DMA/NVIDIA changes. |
| `gpui-shell/` | Current Rust/GPUI Wayland shell, tests, docs, Nix integration and appearance work, including uncompiled shared-list density changes. |
| `cybermanju-app/` | Distinct CyberManju Vue/Tauri/Rust/Go app source tree, tests, docs, lockfiles and original assets. |
| `local-prs/` | Preserved canonical local patches, ordered notes and historical review. |
| `integration/evidence/` | Compact historical text logs, test summaries, reproduction scripts, incremental patches and limitation records. |
| `integration/current/` | Current file hashes, source commits/status/diffs, exact exclusions, status and preparation checks. |
| `integration/snapshot/` | Unmodified original snapshot records, retained as historical evidence. |

The original commit `0832aead043cf34d06b51dbe72c732dedefe2a28` remains in Git history; its snapshot records are not rewritten. The existing remote and repository visibility are not changed. No push or remote Actions run is part of this preparation. Component `.github/` workflows stay nested as source; none is installed at root `.github/workflows/`.

Each component retains its own build system, toolchain and dependency lockfiles. There is no unified root Cargo workspace, bootable ISO, prebuilt runtime or automatic app-to-shell transport.

## Read the status before building

[Integration status](integration/current/STATUS.md) and [machine-readable status](integration/current/status.json) distinguish prior tested baselines from uncompiled changes, skipped checks and hardware-blocked acceptance.

- **Uncompiled / untested:** the newest shared `ListItem` density implementation and its three new Rust tests are present as source. Formatting, whitespace and in-memory patch checks were recorded previously, but no compilation, test execution or GUI render validated this extension. The older built binary does not contain it.
- **Earlier GPUI baseline:** 205 Rust + 36 Python tests, binary build and all-target Clippy were recorded for launcher density only. Frontend and rendered density checks were not rerun. An earlier nested Niri/Xvfb/lavapipe session exercised live colors and error recovery, not Asterinas or physical GPU rendering.
- **Asterinas patch21 baseline:** prior CPU/QEMU records report 315 passed, zero failed, with three explicit VMX/VMX-EPT skips. EDU/NVMe/PCI lifecycle checks are not NVIDIA hardware acceptance.
- **App baseline:** prior records report 125 frontend files / 1,207 tests, including 11 appearance-profile cases, plus typecheck/version/header checks. Later native-only changes did not rerun the app frontend suite. No native Tauri/GTK/WebKit or Asterinas app runtime acceptance is established.

**No build, Cargo/kernel/frontend test, QEMU run or image generation was performed for this consolidation.** Preparation checks cover copying, hashes, symlink/artifact safety, whitespace and secret scanning only; they do not extend previous functional acceptance.

## Verify the current source snapshot

```sh
python3 tools/verify_integration.py
```

The verifier checks the included source/evidence entries against `integration/current/manifest.json`. It does not certify runtime correctness or check later additional files. The old `tools/verify_snapshot.py` intentionally remains an initial-snapshot checker; its old hashes are historical and are not expected to match updated sources.

Current provenance is in `integration/current/*-status.txt` and `*-working-tree.patch`; the hashes also cover new/untracked files absent from the Git diffs. Sources remain based on Asterinas `00a4842d976db6723b8224be2e57ac8105f13ee2`, GPUI Shell `fb4173c9c23cd8dc16732f800ba4921eb1233a50`, and app `a2f78d1e39335a2d1d0873ff99b3d9766b5aaca8`.

## Patches and historical statements

Start with [patch-order guidance](integration/current/PATCHES.md) and [local PR notes](local-prs/PR_NOTES.md). Sources already include the changes: do not reapply the patches blindly. Canonical native order ends at patch21; rejected/initial patches are retained as historical evidence, not replacements. The documented old DRM import-order difference is not silently repaired here.

Historical notes saying “nothing committed,” “published snapshot unchanged,” or using absolute workspace paths describe their original run. This consolidation updates the combined sources; it does not retroactively alter those records. Reproduction scripts may require path adjustment and locally installed dependencies.

## Exclusions and retained assets

[Exact exclusion inventory](integration/current/exclusions.json) records every omitted path or directory subtree and its reason; [plain-language policy](integration/current/EXCLUSIONS.md) explains the boundaries. Nested Git data, Rust targets, dependency installations, caches, generated disk/boot images, firmware, compiled objects/executables, credentials and temporary files are excluded. Large text logs are explicitly excerpted with the original byte count and SHA-256; omitted binary screenshots are not presented as new captures.

Original upstream/app icons, images and documentation GIFs are retained as source assets, not generated test/build outputs. Licenses, copyright files, NOTICE files and source headers are preserved. External `vendor-drivers/`, installed tools/binaries and a separate verification worktree are not imported.

## Licensing

Asterinas is primarily MPL-2.0, with exceptions described in `asterinas/.licenserc.yaml`; see [LICENSE-MPL](asterinas/LICENSE-MPL), [COPYRIGHT](asterinas/COPYRIGHT) and file headers. The app retains its [MIT license](cybermanju-app/LICENSE), copyright notices and “AS IS” disclaimer.

No top-level GPUI Shell license was found. Its inclusion does not assign it MPL, MIT or another license; clarify its terms before any public redistribution. Original experimental-development and non-affiliation notices remain in `gpui-shell/README.md`. This combined repository does not impose a single new license on all components.
