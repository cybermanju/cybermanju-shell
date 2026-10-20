# Verified upstream references

- https://github.com/NVIDIA/open-gpu-kernel-modules/releases — fetched October 20, 2026; latest release listed is 615.78.08 (October 7), not 360.x. Releases extraction showed a different abbreviated tag target (`1a82fbf`) from the existing pinned snapshot; do not silently equate them or change the reviewed source pin based on this alone.
- https://raw.githubusercontent.com/NVIDIA/open-gpu-kernel-modules/615.78.08/README.md — official source README identifies release 615.78.08; matching GSP firmware and NVIDIA userspace release are required. Open modules target Turing or newer. RM/modeset have OS-agnostic components, DRM/UVM do not.
- https://github.com/NVIDIA/open-gpu-kernel-modules/commit/ca2d03e6fafd4d4b56b383497d931c25b283bfcf — public source page identifies the existing locally pinned commit as 615.78.08. Existing pin remains unchanged and tracked vendor sources remain unmodified.
- https://us.download.nvidia.com/XFree86/Linux-x86_64/615.78.08/NVIDIA-Linux-x86_64-615.78.08.run — official matching release archive. HEAD returned content-length 529834675 and last-modified October 6, 2026. Download is local data only; no installer execution, host driver install, license acceptance or GPU startup is authorized by extraction. Embedded MD5 is a corruption check, not vendor signature verification. Extraction provenance must record actual observed SHA-256.

Direct unauthenticated GitHub API tag-reference request returned HTTP 403. No credentials or saved tokens were used or requested.

## Exact current tag resolved without credentials

Public `git ls-remote --tags` returned annotated tag object `4bfe30bb78e7364959e59c16cfefd88e8c14670e`, peeled commit `1a82fbfda8f321b056eb3568850f9316c5e465c5`. The tag was fetched without credentials into the existing vendor repository and checked out in a separate detached worktree; the old checkout remains unchanged. Both old and official commits resolve to the identical Git tree `e121e9c407c996e4aaca25a801232a53f7dc1b5b`, and their source diff is empty. Build/CI/provenance pins now use the exact official commit. The annotated tag contains an SSH signature, but no trusted-signers verification was performed; source identity is pinned, not claimed independently authenticated. Machine-readable evidence: `source-pin.json`.
