# Full-kernel consumer block-I/O regression

This separate runner is a **real full-kernel QEMU functional regression** for the changed NVMe, VirtIO modern, and VirtIO legacy MSI-X consumers. It does not change repository source or the existing `run_cpu_boot.py` test.

## Asterinas device-node findings

- Asterinas has its own `devtmpfs`; for an initramfs-selected `rdinit`, the kernel intentionally does **not** mount it automatically. The first-process block registry creates block nodes in the devtmpfs singleton, named directly from each driver's `BlockDevice::name()`.
- The current driver naming convention is `/dev/nvme0n1` for the first NVMe namespace and `/dev/vda`, `/dev/vdb` for VirtIO block devices in registration order. This is **not Linux devtmpfs/devnode magic or a static initramfs mknod**: the freestanding init explicitly mounts Asterinas `devtmpfs` at `/dev` before opening those nodes.
- The runner creates one NVMe namespace fixture, then attaches the modern VirtIO disk and legacy VirtIO disk. The init validates each expected device against a distinct known 4096-byte pattern. It also writes and reads back a separate 4096-byte scratch region on every disposable raw fixture.

## Run (defer until the parent kernel tests finish)

From any working directory, after the parent confirms its currently running kernel tests are complete:

```sh
python3 /workspace/asterinas-gpui/evidence/nvidia-msix-transaction/consumer-io/run_consumer_io.py
```

The runner makes a fresh `consumer-io/run/` directory (it refuses to overwrite one), builds a static no-libc init and cpio initramfs, creates three disposable 1 MiB raw disks, temporarily creates `asterinas/kernel/OSDK.toml` with exclusive-create semantics and removes it in `finally`, builds the full kernel ISO, then boots QEMU with NVMe + `virtio-blk-pci,disable-legacy=on` + `virtio-blk-pci,disable-modern=on`. It pins the same `toolchain/bin`, NVIDIA source tag, VDSO source and offline cargo setting as the existing CPU boot runner.

Inspect `consumer-io/run/result.json`, `serial.log`, `kernel-build.log`, and `fixture-manifest.json`. Success requires the per-device markers emitted only after byte comparisons **and** scratch readback, the final PASS marker, no panic, and QEMU's expected poweroff debug-exit status. A guest validation failure prints `CONSUMER_IO FAIL`; the host runner returns nonzero and records the failure. On failure, preserve the run directory for diagnosis; move it aside before another run.

**Do not run this until the parent kernel tests are complete.** No cargo build or QEMU boot was started for this subtask. This validates CPU/TCG storage I/O only; it makes no real-GPU, firmware, or performance claim.
