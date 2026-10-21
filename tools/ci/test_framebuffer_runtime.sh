#!/usr/bin/env bash
# Disposable OSTD metadata / boot framebuffer tests, not the installer ISO.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
OUT=${1:?usage: test_framebuffer_runtime.sh OUT --kind contract|hardware}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
printf '{"status":"not_verified"}\n' > "$OUT/result.json"
[[ $# == 3 && $2 == --kind ]] || { echo 'expected OUT --kind contract|hardware' >&2; exit 2; }
KIND=$3
case "$KIND" in
    contract) CRATE=ostd; PATHCRATE=ostd; FILTER=cybermanju_framebuffer_boot_contract; CFG="$ROOT/tools/ci/framebuffer-contract.OSDK.toml" ;;
    hardware) CRATE=aster-framebuffer; PATHCRATE=kernel/core/comps/framebuffer; FILTER=cybermanju_framebuffer_runtime; CFG="$ROOT/tools/ci/framebuffer-runtime.OSDK.toml" ;;
    *) echo 'expected --kind contract|hardware' >&2; exit 2 ;;
esac
CONFIG="$ROOT/asterinas/$PATHCRATE/OSDK.toml"
if [[ -e "$CONFIG" || -L "$CONFIG" ]]; then
    echo 'Refusing to overwrite an existing OSDK.toml' >&2
    exit 2
fi
for tool in cargo rustc qemu-system-x86_64 grub-mkrescue xorriso timeout python3; do
    command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool" >&2; exit 2; }
done
# Local-dev selects this fork's OSTD and test runner, not published substitutes.
(cd "$ROOT/asterinas/osdk" && CARGO_TARGET_DIR="$ROOT/asterinas/osdk/target" OSDK_LOCAL_DEV=1 cargo build --locked -p cargo-osdk) > "$OUT/osdk-build.log" 2>&1
OSDK="$ROOT/asterinas/osdk/target/debug/cargo-osdk"
export CARGO_TARGET_DIR="$ROOT/asterinas/target"
SYSROOT=$(cd "$ROOT/asterinas" && rustc --print sysroot)
HOST=$(cd "$ROOT/asterinas" && rustc -vV | sed -n 's/^host: //p')
LLVM="$SYSROOT/lib/rustlib/$HOST/bin/llvm-objcopy"
[[ -x "$LLVM" ]] || { echo 'Missing pinned nightly llvm-tools component' >&2; exit 2; }
mkdir -p "$OUT/llvm-bin"
ln -sf "$LLVM" "$OUT/llvm-bin/rust-objcopy"
export PATH="$OUT/llvm-bin:$PATH"
(set -o noclobber; cat "$CFG" > "$CONFIG") || exit 2
trap 'if cmp -s "$CONFIG" "$CFG"; then rm -f "$CONFIG"; else echo "Changed configuration preserved: $CONFIG" >&2; fi' EXIT
# Rebuild only this generated disposable bundle, never a source tree or installed
# disk. Record the GRUB config extracted from that image, not a guessed config.
BUNDLE="$ROOT/asterinas/target/osdk/$CRATE"
[[ ! -L "$BUNDLE" ]] || { echo 'Refusing symlinked test bundle' >&2; exit 2; }
rm -rf -- "$BUNDLE"
set +e
(cd "$ROOT/asterinas/$PATHCRATE" && timeout --signal=TERM --kill-after=15s 600 "$OSDK" osdk test "$FILTER" --target-arch x86_64 </dev/null) 2>&1 | tee "$OUT/runtime.log"
statuses=("${PIPESTATUS[@]}")
set -e
rc=${statuses[0]}
if [[ "$rc" -eq 0 ]]; then rc=${statuses[1]}; fi
[[ "$rc" -eq 0 ]] || { echo "OSDK framebuffer runtime failed with exit $rc" >&2; exit "$rc"; }
cp "$ROOT/asterinas/target/osdk/$CRATE-test-base/Cargo.toml" "$OUT/harness-Cargo.toml"
cp "$ROOT/asterinas/target/osdk/$CRATE-test-base/Cargo.lock" "$OUT/harness-Cargo.lock"
ISO="$ROOT/asterinas/target/osdk/$CRATE/$CRATE-osdk-bin.iso"
rm -f "$OUT/grub.cfg"
xorriso -osirrox on -indev "$ISO" -extract /boot/grub/grub.cfg "$OUT/grub.cfg" > "$OUT/grub-extract.log" 2>&1
{
    (cd "$ROOT/asterinas" && rustc --version && cargo --version)
    qemu-system-x86_64 --version | head -1
    printf 'kind=%s; x86_64; q35; TCG; 1 CPU; 1 GiB; BIOS/GRUB multiboot2; OSDK_LOCAL_DEV=1\n' "$KIND"
    sha256sum "$ROOT/asterinas/target/osdk/$CRATE-osdk-bin.elf" "$ISO" "$OUT/grub.cfg" "$OUT/harness-Cargo.toml" "$OUT/harness-Cargo.lock"
    sha256sum "$CFG" "$ROOT/tools/ci/test_framebuffer_runtime.sh" "$ROOT/tools/ci/verify_framebuffer_runtime.py"
} > "$OUT/provenance.txt"
python3 "$ROOT/tools/ci/verify_framebuffer_runtime.py" "$OUT/runtime.log" "$KIND" "$OUT/result.json"
