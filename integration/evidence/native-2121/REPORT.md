# Native NVIDIA lifecycle follow-up — patch 20

## Change

Canonical incremental patch: `/workspace/asterinas-gpui/local-prs/asterinas-nvidia-postirq-drain.patch`, applied after patch 19 (`asterinas-nvidia-interrupt-worker.patch`). It changes only the PCI MSI-X transaction API/real QEMU NVMe lifecycle test and the NVIDIA interrupt-worker owner.

The verified ordering gap was that `PciMsixGuard::shutdown` retired MSI-X callbacks and restored MSI-X state before `NvidiaInterruptWorker` drained its already-accepted bottom-half callback. That ordering was awkward against the transaction's own deferred-work quiescence contract and could restore/release MSI-X transaction state while the owner's callback was still running. Added `shutdown_with_post_irq`: mask MSI-X, run the mandatory real device-source quiesce, verify BME/PBA conditions, synchronously retire IRQ callbacks, run a fallible deferred-work drain while the transaction is still masked and owned, then restore. The existing `shutdown` API remains source-compatible and delegates with a no-op drain. `NvidiaInterruptWorker::shutdown` now closes/drains its worker in that post-IRQ phase and preserves retry/error behavior. It still requires actual caller-provided device-source quiescence; no source protocol is fabricated.

The native QEMU NVMe MSI-X cycle test now asserts at the drain boundary that the installed IRQ capture has already been destroyed and MSI-X remains enabled/function-masked; successful teardown later verifies prior MSI-X state restoration. This is an OSTD/QEMU regression test plan in source, not a claimed test pass.

## Validation and limits

- `rustfmt --check --edition 2024` on the three changed Rust files: passed.
- `git diff --check`: passed.
- Patch20 replayed from preserved patch19 source snapshots in a temporary tree; all three affected files matched the final sources byte-for-byte.
- Native OSDK/QEMU tests were **not run**. At check time the root filesystem had about 418 MiB free (100% reported utilization), and a concurrent parent-owned Android Cargo release build was active. To avoid risking parent build artifacts or filling the disk, the native suite was skipped. Prior patch19 test logs are historical only and do not validate this change.
- Hardware reconfirmed absent: no `/dev/dri`, `/dev/nvidia*`, or `/dev/kvm`. No NVIDIA hardware, GPU driver acceptance, full integration, or GPU success is claimed.
- Official Linux kernel MSI/MSI-X guide reviewed: https://www.kernel.org/doc/html/v6.17/PCI/msi-howto.html. This change adds a lifecycle sequencing hook, not NVIDIA register/API semantics.

No credentials/authentication, staging, commit, push, PR, or remote Actions were used. `PR_NOTES.md` was not modified; parent owns aggregation.

## Parent validation after safe cache recovery

After GPUI compilation completed, the parent removed only idle reproducible GPUI incremental caches and recovered ~1.2 GiB. With the existing local OSDK/toolchain, offline full native validation then passed all five suites: 315 reported passed, zero failures, three explicit VMX/VMX-EPT skips. Logs are in `native-suites/`, summary in `test-summary.json`. The actual QEMU NVMe cycle verified callback retirement before the drain and MSI-X enabled/function-masked at that boundary. This supersedes the initial disk-blocked status, not its historical record. It remains CPU/QEMU PCI lifecycle evidence, not NVIDIA hardware or GPU acceptance. All 20 canonical patches applied in order; 98/99 affected files match byte-for-byte and the sole difference is the previously documented DRM import-order formatting. No unexpected mismatch or Git index change.
