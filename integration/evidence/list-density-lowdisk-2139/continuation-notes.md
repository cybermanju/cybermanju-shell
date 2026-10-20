# Low-disk continuation: shared native list-row density

## Concrete source change

Shared `ListItem` rows now consume the existing runtime profile density for semantic horizontal padding and gaps. Default vertical padding follows the same compact/comfortable unit policy as the validated launcher bridge. Explicit `.spacing(Sparse/Dense/ExtraDense)` calls remain authored vertical pixel overrides; the private field is now optional so default rows and explicit overrides cannot be conflated. No public enum variant was added. Indentation, slots, visibility, disabled/selected state, clicks, hover, tooltips, fonts and compositor/bar geometry retain their prior ownership and behavior.

Three Rust regression tests were added for unchanged comfortable defaults, compact defaults, explicit vertical overrides and constructor/builder distinction. The source capability report now names partial launcher/shared-row support. Both repositories' bridge contracts explicitly distinguish the last validated launcher-only binary from this pending source extension.

## What was actually checked

Rustfmt syntax/format checks and Git whitespace checks passed. The two small patches replayed byte-for-byte from actual preserved preimages **entirely in memory**, and Git reverse-applicability checks passed read-only against current files. Both Git indexes stayed unchanged. These are source/patch consistency checks, not type checking or runtime tests.

**No compilation, Cargo test, binary/GUI test, QEMU, OSDK, dependency install or ISO build was run. The three new Rust tests are unexecuted.** The earlier 205 Rust + 36 Python results cover the prior GPUI source/binary only; they do not validate this change. The existing built binary has not gained shared-list density support from a source edit alone.

## Native NVIDIA lifecycle

The independent low-disk audit reviewed worker admission/closing/drain, runtime arithmetic, resource lifetimes, PCI restore/retry and current tests. It found no demonstrable safe behavioral defect warranting a new native patch; no patch22 was fabricated. Canonical native order still ends at validated patch21. Read-only reverse applicability of patch21 and formatting/whitespace checks passed; NVIDIA source was unchanged.

The audit records a documentation imprecision: a pre-drain MSI-X shutdown failure leaves the worker open, but a PCI restoration failure **after successful drain** leaves it already closed with the transaction/resources retained for retry. Do not assume every shutdown error permits reopening work admission. See `/workspace/asterinas-gpui/evidence/native-lowdisk-2139/REVIEW.md` for the actual reasoning and limits.

## Durable owner restrictions and acceptance gates

Do not delete any additional cache or file without the owner's explicit approval. Do not start disk-heavy builds or another ISO/image attempt with the remaining approximately 1.1 GB; the owner reports the NixOS image already reserves 6.44 GB. This run deleted nothing and did not touch the image or unrelated Android work. Only small source/preimage/patch/evidence files were written; this directory used about 272 KiB before final notes/manifests.

No `/dev/dri`, `/dev/nvidia*` or `/dev/kvm` is available. Native RM/GSP, GPU Vulkan, the target Wayland/Tauri/GPUI desktop, automatic app transport, complete personalization and physical-GPU acceptance remain open. No GPU or Linux-parity claim is justified. No credentials, authentication, staging, commit, push, PR, publication or remote Actions were used.

## Incremental artifacts

`gpui-shared-list-density-source-only.patch` applies after the 21:21 launcher-density change; it is not part of the Asterinas kernel numbered chain. `cybermanju-shared-list-density-contract.patch` contains only the corresponding app-repository contract update. `patch-validation.json` records exact preimage/final digests and the limited checks. Preserve all earlier patches/evidence, including their historical checkpoints. The published `/workspace/cybermanju-shell` remains unchanged.
