#!/usr/bin/env bash
# Real OSTD frames in a disposable BIOS/TCG test VM. Never loads a Linux module.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
OUT=${1:-"$ROOT/artifacts/linux-module-runtime"}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
printf '{"status":"not_verified","linux_modules_loaded":false}\n' > "$OUT/result.json"
CONFIG="$ROOT/asterinas/ostd/OSDK.toml"
if [[ -e "$CONFIG" ]]; then
    echo "Refusing to overwrite an existing OSTD/OSDK.toml" >&2
    exit 2
fi
for tool in cargo rustc qemu-system-x86_64 grub-mkrescue xorriso timeout python3; do
    command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool" >&2; exit 2; }
done
# The compile-time flag is essential: otherwise OSDK can select published OSTD
# and runner dependencies instead of the changed local fork.
(cd "$ROOT/asterinas/osdk" && CARGO_TARGET_DIR="$ROOT/asterinas/osdk/target" OSDK_LOCAL_DEV=1 cargo build --locked -p cargo-osdk) > "$OUT/osdk-build.log" 2>&1
OSDK="$ROOT/asterinas/osdk/target/debug/cargo-osdk"
export CARGO_TARGET_DIR="$ROOT/asterinas/target"
SYSROOT=$(cd "$ROOT/asterinas" && rustc --print sysroot)
HOST=$(cd "$ROOT/asterinas" && rustc -vV | sed -n 's/^host: //p')
LLVM="$SYSROOT/lib/rustlib/$HOST/bin/llvm-objcopy"
[[ -x "$LLVM" ]] || { echo "Missing nightly llvm-tools component" >&2; exit 2; }
mkdir -p "$OUT/bin"
ln -sf "$LLVM" "$OUT/bin/rust-objcopy"
export PATH="$OUT/bin:$PATH"
cp "$ROOT/tools/ci/module-staging-runtime.OSDK.toml" "$CONFIG"
trap 'rm -f "$CONFIG"' EXIT
set +e
(cd "$ROOT/asterinas/ostd" && timeout --signal=TERM --kill-after=15s 600 "$OSDK" osdk test cybermanju_staging_runtime --target-arch x86_64 </dev/null) 2>&1 | tee "$OUT/runtime.log"
statuses=("${PIPESTATUS[@]}")
set -e
status=${statuses[0]}
if [[ "$status" -eq 0 ]]; then status=${statuses[1]}; fi
[[ "$status" -eq 0 ]] || { echo "OSDK runtime failed with exit $status" >&2; exit "$status"; }
cp "$ROOT/asterinas/target/osdk/ostd-test-base/Cargo.toml" "$OUT/harness-Cargo.toml"
cp "$ROOT/asterinas/target/osdk/ostd-test-base/Cargo.lock" "$OUT/harness-Cargo.lock"
{
    (cd "$ROOT/asterinas" && rustc --version && cargo --version)
    qemu-system-x86_64 --version | head -1
    echo 'x86_64; q35; TCG; 1 CPU; 1 GiB; BIOS/GRUB multiboot2; OSDK_LOCAL_DEV=1'
    sha256sum "$ROOT/asterinas/target/osdk/ostd-osdk-bin.elf" "$ROOT/asterinas/target/osdk/ostd/ostd-osdk-bin.iso"
} > "$OUT/provenance.txt"
python3 "$ROOT/tools/ci/verify_module_staging_runtime.py" "$OUT/runtime.log"
