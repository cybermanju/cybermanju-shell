# Native MSI-X retired-vector activation regression (patch 21)

Canonical incremental patch (after patch 20): `/workspace/asterinas-gpui/local-prs/asterinas-msix-retired-activation.patch`.

## Change

`PciMsixGuard::activate` now rejects an empty owned-IRQ vector set with `Error::AccessDenied`, alongside the existing closed/activated/empty-handler checks. The real QEMU NVMe MSI-X test exercises an initially unactivated guard against the dedicated `ASTER-MSIX` endpoint, whose controller was stopped and whose PCI Bus Master Enable bit is clear. The empty source-quiesce callback verifies `CSTS.RDY=0` and BME-off; an installed handler capture proves IRQ retirement occurs before the post-IRQ drain. The test injects a drain error, verifies the transaction remains enabled-but-function-masked and all IRQs are retired, requires activation to return `AccessDenied`, then retries shutdown successfully and checks exact MSI-X-control and PCI-command restoration. This is an error-path regression test, not a success benchmark, and does not invent a device-source protocol.

## Evidence

- **Old-code red reproduction:** `/workspace/asterinas-gpui/evidence/native-retired-2130/negative-old-activation.log` is an actual QEMU/KTest run with the new regression and old activation predicate. Result: **12 passed, 1 failed**; the regression observed `Ok(())` instead of `Err(AccessDenied)`. Details: `negative-old-activation.status`.
- **Fixed full native matrix:** `/workspace/asterinas-gpui/asterinas/tools/graphics/run_cpu_kernel_tests.sh /workspace/asterinas-gpui/evidence/native-retired-2130/native-suites` passed all five suites with **315 passed, 0 failed**, and the final runner marker in every log. Counts: nvidia-nvport 20; nvidia-platform 13; PCI without IOMMU 13; PCI with IOMMU 13; OSTD 256. OSTD reported **3 explicit skips** (VMX unavailable twice and VMX/EPT invalidation unavailable). The two real PCI QEMU runs report the NVMe MSI-X masked-pending/quiesced-shutdown test marker.
- Detailed suite logs: `/workspace/asterinas-gpui/evidence/native-retired-2130/native-suites/`; runner transcript: `/workspace/asterinas-gpui/evidence/native-retired-2130/full-matrix-run.log`.
- `rustfmt --check --edition 2024` and `git diff --check`: passed. Patch replayed cleanly against reconstructed post-patch20 source; both resulting files were byte-identical to final sources. Patch size: **3,265 bytes**; SHA-256: `325cdf94e067943a78e436f8e7bb68f6d8ff25c4de74ee4bc99df1e1740431f5`.
- Hardware absence reconfirmed: no `/dev/dri`, `/dev/nvidia*`, or `/dev/kvm`. This is CPU/QEMU native kernel evidence only; **no GPU or NVIDIA hardware validation is claimed**.

No GPUI/app/docs/PR notes were changed. No credentials, staging, commit, push, PR, or remote actions were used. The temporary OSDK test manifest was removed. No unrelated Android build process or artifact was touched.


## Canonical preimage correction and ordered-stack verification

The first patch replayed against a reconstructed, multiline-formatted baseline, but failed against canonical patch20, whose old predicate is single-line. Only patch metadata/preimage formatting was corrected; no tested kernel source changed. The initial artifact remains `initial-formatted-preimage.patch`, and `repair_patch.py` reproduces the correction. Use the canonical patch at /workspace/asterinas-gpui/local-prs/asterinas-msix-retired-activation.patch; current SHA-256 **12843ef3d57c61872b06d1ae7967a48edc33ecf2a2f6edd29cf984de97ade3dd** (3774 bytes), superseding the initial digest above. All21 patches now apply in order. The sole final-source difference is the previously documented DRM import-order formatting; no unexpected mismatch or index change. Fixed native matrix results remain valid because tested source bytes were unchanged.
