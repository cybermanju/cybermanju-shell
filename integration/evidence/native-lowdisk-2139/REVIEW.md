# Native NVIDIA lifecycle review — low-disk run 2139

## Outcome

No source change or patch22 was created. Read-only inspection found no demonstrable bounded worker/resource lifecycle defect in the reviewed current paths that could be safely fixed without inventing NVIDIA device protocol. This is an intentional decline rather than a documentation-only patch or an unverified hardware assumption.

## Scope inspected

- `kernel/comps/nvidia-platform/src/worker.rs`: signal admission/generation accounting, task scheduling, flush and shutdown semantics, IRQ retirement, and `NvidiaInterruptWorker` prepare/register/activate/shutdown.
- `kernel/comps/nvidia-platform/src/runtime.rs`: clock callback initialization, checked time conversion, and calendar bounds.
- `kernel/comps/nvidia-platform/src/device.rs`: real PCI/MMIO ownership and borrowed resource lifetimes.
- `kernel/core/comps/pci/src/msix_transaction.rs`: activation predicate and post-IRQ drain/restore/retry behavior.
- `kernel/comps/nvidia-platform/src/worker/tests.rs`: current worker scheduling, context rejection, capture destruction, IRQ retirement, and overflow coverage.

`AGENTS.md` and the final `PR_NOTES.md` entries were reviewed. Patch21 is `local-prs/asterinas-msix-retired-activation.patch`; its reverse applicability against the current working source passed without changing files.

## Lifecycle reasoning

- `can_sleep()` itself requires enabled preemption/local IRQs, task interrupt level, and a current task; `DeviceWorker::start`, teardown validation, and NVIDIA admission checks are consistent with those requirements.
- Work admission and close serialize under one lock. Pending requests coalesce; a request while the callback is executing receives a distinct generation. Checked overflow closes admission rather than wrapping. Accepted pending work is drained before captures are destroyed.
- `shutdown_after_irq` validates before consuming the IRQ handle, then retires its callback dispatch before closing/draining. The owner documents caller responsibility for actual source quiescence and other producers.
- NVIDIA teardown passes its actual post-IRQ worker drain into the PCI transaction. A pre-drain quiesce/PBA/BME failure retains the live worker and MSI-X owner for retry. Once the drain succeeds, the worker is closed before PCI state restoration. If restoration then fails, the guard remains owned for retry and repeated worker shutdown is idempotent. Reopening the worker at that point would be unsafe because producers/callbacks have been retired.
- There is a narrow documentation imprecision in `worker.rs` around line 341: “On MSI-X shutdown failure the worker remains open” does not cover a restoration failure after the post-IRQ drain has already successfully closed it. The control flow retains resources and supports retry, so this is not a demonstrated code/resource leak or unsafe transition; changing prose alone would violate the requested source-fix criterion. No behavior change was inferred from this observation.
- Runtime calendar conversion uses checked arithmetic and validates fields/month/day/leap-year rules; no unsafe arithmetic gap was demonstrated.

## Validation and limits

- `rustfmt --check --edition 2024` on `worker.rs` and `runtime.rs`: passed.
- Targeted `git diff --check`: passed.
- Read-only `git apply --check --reverse` of canonical patch21: passed.
- Hardware check: `/dev/nvidia*`, `/dev/dri`, and `/dev/kvm` are absent. Approximately 1.1 GB remains free; no files/caches were deleted.
- No cargo build/test, OSDK, QEMU, ISO, installation, network/API lookup, or GPU test was run. No new Rust tests were added; therefore there are no executed or unexecuted new source tests to report.
- No source files were changed. Patch21 and prior canonical patches were left untouched. No credentials, staging, commits, pushes, PRs, or remote actions were used.
