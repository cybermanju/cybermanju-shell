# Experimental NVIDIA NVport and RM OS-service integration

This opt-in crate connects unchanged NVIDIA **615.78.08** source to real Rust/OSTD services. The exact source pin is the official release tag's peeled commit, `1a82fbfda8f321b056eb3568850f9316c5e465c5`. The earlier snapshot `ca2d03e6fafd4d4b56b383497d931c25b283bfcf` has the identical source tree, `e121e9c407c996e4aaca25a801232a53f7dc1b5b`; the pin now matches the public tag. This is working source-port infrastructure, **not a complete NVIDIA GPU driver**.

Cargo requires a trusted clean checkout at `NVIDIA_SOURCE_DIR`, rejects another commit or modified tracked sources, and checks the release and actual NVIDIA ABI headers. The compiler and checkout remain trusted; this is not a hermetic build. Cargo downloads neither sources nor firmware. Prepared CI fetches the exact public commit without credentials.

## Real services

CPU memory allocation/free uses the real allocator, checked layouts, private aligned headers and cleared outputs on failure. Interrupt-context allocation is rejected. This is resident heap storage, not pinned GPU/DMA memory. The safe `Buffer` invokes original NVport copy, fill, compare, pattern and overlapping-move routines with bounds capped to the upstream u32 memory ABI.

Native usercopy validates ranges and context and uses OSTD's fault-aware `VmReader`/`VmWriter`; complete transfer and initialized address-space/fault handling are required. Host tests explicitly reject nonempty native usercopy rather than using unchecked memcpy.

The firmware provider replaces Linux `request_firmware` with immutable guest boot storage. It validates the complete uncompressed `070701` newc archive, including checked fields, names, alignment and trailer. It selects exact filenames using NVIDIA's own release-specific selector. Malformed/truncated archives, accepted-form duplicate paths, symlinks/hardlinks, missing/wrong-release files, empty images and images above 128 MiB fail closed. Interior dot/repeated-separator names are ignored, not normalized or followed. No host-file fallback or decompression is advertised.

`nv_get_firmware` allocates distinct handles over static boot bytes; `nv_put_firmware` releases them. Safe `FirmwareLease` exercises a real Rust-to-C-to-Rust acquire/release round trip. The optional boot inventory reads all image bytes on the CPU and reports a reproducible noncryptographic checksum. This does **not** authenticate firmware, establish GPU DMA, or boot GSP.

Clock callbacks install once from initialized Asterinas time services. They provide monotonic/realtime values and nominal counter-tick resolution, not an absolute-accuracy guarantee. Realtime preserves Asterinas' RTC policy, including its epoch dummy when RTC is unavailable. Safe wrappers check readiness. Since upstream time code ignores callback status, unguarded foreign calls with unavailable/out-of-range clocks halt instead of fabricating timestamps. Task identities are opaque tokens unique among simultaneously live OSTD tasks, not POSIX TIDs. Yielding enters the real scheduler only in a sleepable task context. Host builds explicitly lack native task identity/yielding.

Native mutex, semaphore and reader-writer callbacks use atomic state and OSTD wait queues. Contention tests use actual OSTD tasks, blocking and wakeups, not a host scheduler mock. IRQ-save spinlocks retain and restore the real CPU-local IRQ guard. Destruction of still-held state leaks that state rather than dropping a forgotten guard on another CPU. Safe semaphore release reports overflow. Sleepability uses a tested OSTD preemption/IRQ/task predicate, not a constant compatibility answer. These primitives do not guarantee fairness or real-time latency.

The native link also uses unchanged upstream allocator-tracking source instead of dummy allocator dispatch symbols. `nv_printf` has a real freestanding integer/pointer/string formatter, bounded output with explicit truncation, width/precision/length handling and libc-comparison tests. Floating conversions, `%n` and custom unknown conversions (including the LibOS-only `%a` form) report formatting failure, not false success; this is not a full libc or every-platform NVIDIA formatting implementation. Diagnostic strings/bytes are actually logged, including escaped non-UTF8 data. An upstream debug breakpoint halts through the kernel panic path rather than silently continuing.

## Build and validation

Use the pinned toolchain, matching vDSO and standard Asterinas prerequisites:

```sh
export NVIDIA_SOURCE_DIR=/absolute/path/to/official-615.78.08-checkout
cargo test -p ostd-nvidia-nvport --target x86_64-unknown-linux-gnu
bash tools/graphics/run_nvport_format_tests.sh
bash tools/graphics/run_cpu_kernel_tests.sh /absolute/path/to/evidence
cargo osdk build --features nvidia-nvport-selftest,nvidia-pci-diagnostics \
  --boot-method grub-rescue-iso --grub-boot-protocol multiboot2 \
  --kcmd-args='earlycon console=ttyS0' --strip-elf
```

Host tests execute real memory/time code and firmware-ABI ownership with explicitly CPU-only fixtures. Native scheduler tests execute under QEMU TCG. The runner rejects zero-test success logs. A successful compile/check is not execution evidence, and CPU boot markers are not GPU evidence.

Native C uses the x86-64 kernel code model, no red zone, no PIC/PIE, no SSE/MMX or hosted runtime. Host C uses PIC. Other targets are rejected. The optional platform runtime is enabled when both kernel NVIDIA features are selected; standalone PCI diagnostics remains independent of NVIDIA source compilation.

## Trusted boundary and unfinished driver work

C sources, bridges and unsafe ownership contracts extend OSTD's trusted computing base. The kernel assembler retains `deny(unsafe_code)`; safe wrappers do not make C memory-safe. Integration remains opt-in.

Full RM/modeset is not linked. GPU/BAR ownership, device-bound DMA/IOMMU mappings, interrupts, user-page pinning, GSP boot, NVIDIA device ioctls/mmap, GPU allocation/submission/fences/reset/teardown and accelerated DRM/KMS/Wayland interoperability remain unfinished. Successful CPU tests, device names or a Linux-host benchmark supply none of those. Hardware acceptance requires the protected native VFIO runner and genuine guest GPU evidence; compute alone still does not establish full GPUI/display acceptance. See [the native acceptance path](../../../tools/graphics/NATIVE_NVIDIA.md).
