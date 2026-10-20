# Integration status

## Most important limitation

The latest shared-list density code is included **without compilation or executed tests**. The earlier 205-Rust-test binary covers launcher spacing only, not shared rows. No full OS, GPU or desktop acceptance is claimed.

## Recorded passes—not rerun for this snapshot

| Area | Prior result and boundary | Evidence |
| --- | --- | --- |
| Asterinas native patch21 | 315 reported passed, zero failed; three explicit VMX/VMX-EPT skips. CPU/QEMU and PCI/NVMe lifecycle only, not an NVIDIA GPU. | [Report](../evidence/native-retired-2130/REPORT.md), [summary](../evidence/native-retired-2130/test-summary.json) |
| GPUI launcher-density baseline | 205 Rust + 36 Python tests, real binary build and all-target Clippy. Launcher semantic spacing only; no rendered density rerun. | [Summary](../evidence/appearance-density-2121/test-summary.json), [notes](../evidence/appearance-density-2121/continuation-notes.md) |
| GPUI live colors | Earlier 201 Rust + 36 Python tests, build/Clippy; real nested Niri/Xvfb/lavapipe color transition, invalid-profile warning, last-good retention and recovery. Software rendering on Linux, not Asterinas. | [Host result](../evidence/cybermanju-live-profile/host-window-smoke/result.json), [PR notes](../../local-prs/PR_NOTES.md) |
| App profile export/restore | Last recorded frontend run: 125 files / 1,207 tests, including 11 profile cases; typecheck/version/header gates. Later native-only follow-ups did not rerun frontend tests. | [Bridge contract](../../cybermanju-app/docs/ASTERINAS-PERSONALIZATION-BRIDGE.md), [worklog](../../cybermanju-app/worklog.md), [tasks](../../cybermanju-app/TASKS.md) |
| Nix module and older shell smoke | x86_64 module evaluation and flake evaluation passed; aarch64 derivation evaluated only. Earlier nested shell smoke used llvmpipe. | [Second review](../../local-prs/GPUI_SECOND_REVIEW.md), [summary](../evidence/gpui-second-review/validation-summary.json) |

Counts are separate historical checkpoints and must not be added together as a current whole-repository test result. Old failed attempts and negative regressions are retained with their later corrections. Early DRM/simpledrm compile-only or timed-out runs are not runtime passes; later PCI test successes do not retroactively validate those early framebuffer tests.

## Uncompiled / unexecuted

The shared `ListItem` extension uses density for semantic horizontal padding/gaps and default vertical spacing while preserving explicit `.spacing(...)` overrides. Its three new Rust regression tests are unexecuted. The matching app bridge contract documents the same boundary. Prior syntax/format, whitespace, in-memory byte replay and read-only reverse-applicability checks are source consistency checks only. [Notes](../evidence/list-density-lowdisk-2139/continuation-notes.md), [validation](../evidence/list-density-lowdisk-2139/patch-validation.json).

## Skipped or not performed

No heavy build, test suite, GUI launch, QEMU run, ISO generation, cache deletion, remote Actions or push was performed during consolidation. Browser export/import click-through, native Tauri/GTK/WebKit runtime, aarch64 build/runtime, live NixOS/systemd session, rendered launcher/shared-row density and full input/multi-monitor/scaling acceptance remain unverified. All three source worktrees are preserved.

## Hardware blocked and open integration

Recorded environment checks show no `/dev/dri`, `/dev/nvidia*` or `/dev/kvm`, and no usable physical display GPU. CPU software Vulkan and EDU/NVMe devices cannot prove GPU acceleration. Native NVIDIA RM/GSP/firmware/submission/reset/fence lifecycle, AMD support, accelerated DRM/Vulkan/Wayland, target GPUI/Tauri execution and matched physical-hardware benchmarks remain open. [Constraints](../evidence/list-density-lowdisk-2139/constraints.json), [app ABI baseline](../../cybermanju-app/docs/ASTERINAS-ABI-BASELINE.md).

Automatic app-to-shell transport, wallpaper/media, desktop style/docking, complete density/accessibility and restart/durable round-trips are also open. Exporting a cosmetic profile and watching an explicitly supplied host file do not provide those features.

## Preparation-only checks

`copy-check.json`, `verification.json` and `secret-scan.json` in this directory record source copying, hashes, exclusions, source-worktree preservation and redacted secret-scan review. They are packaging/security checks, not OS or GPU acceptance. The existing target remote and privacy settings are not altered; no GitHub API/visibility update is made.

## Authorized local-cache exception

The final audit initially stopped on nine pre-existing ignored `.pyc` files under `asterinas/tools/graphics/__pycache__/`. The user then explicitly instructed that they remain untouched and untracked and are not a blocker. Their exact names, sizes and hashes are recorded in `retained-local-cache.json`; they are excluded from the commit. The resumed audit verifies those hashes and rejects any such cache entry in the index. No cache or source files were deleted.
