# Patch order and review boundaries

Current component sources already incorporate the local changes. Patches are preserved for review/provenance, not as commands to run again on this checkout.

## Native Asterinas

The canonical native sequence in `local-prs/PR_NOTES.md` ends at **21**, with predecessors preserved in `local-prs/` and the ordered replay evidence retained. Final steps:

19. `local-prs/asterinas-nvidia-interrupt-worker.patch` — corrected interrupt-worker ownership.
20. `local-prs/asterinas-nvidia-postirq-drain.patch` — post-IRQ deferred drain before restore.
21. `local-prs/asterinas-msix-retired-activation.patch` — reject activation after callback retirement.

See `integration/evidence/native-retired-2130/stack-validation/full-stack-validation.json` and its report for the complete canonical order and the single documented old DRM import-order difference. Initial arbitrary-guard/initial-formatted-preimage patches are rejected or historical artifacts, **not** canonical replacements. No patch22 is invented. The low-disk lifecycle audit is retained in `integration/evidence/native-lowdisk-2139/REVIEW.md`.

## GPUI and app bridge

Keep the earlier NixOS/audio patches and subsequent incremental evidence patches in chronological dependency order described in `local-prs/PR_NOTES.md`. In particular:

1. The earlier appearance, safe file-open and live-file watcher baselines, including `integration/evidence/cybermanju-live-profile/`.
2. `integration/evidence/appearance-density-2121/gpui-cybermanju-launcher-density.patch` and matching `cybermanju-launcher-density-docs.patch` — last tested launcher-only density baseline.
3. `integration/evidence/list-density-lowdisk-2139/gpui-shared-list-density-source-only.patch` and matching `cybermanju-shared-list-density-contract.patch` — **uncompiled, untested** shared-list source extension.

Step 3 applies after step 2, not against the original published combined snapshot. Its `patch-validation.json` records preimage/final hashes and limited checks. The app patch contains bridge documentation only. The new three Rust tests have not executed. Do not equate source replay/format checks with binary, type, GUI or GPU acceptance.
