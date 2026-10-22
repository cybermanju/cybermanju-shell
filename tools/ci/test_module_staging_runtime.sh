#!/usr/bin/env bash
# Real OSTD frames in a disposable BIOS/TCG VM. Never executes module entries.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
OUT=${1:-"$ROOT/artifacts/linux-module-runtime"}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
printf '{"status":"not_verified","linux_modules_executed":false}\n' > "$OUT/result.json"
if (( $# > 2 )); then echo 'Runtime modes are mutually exclusive' >&2; exit 2; fi
CONFIG_TEMPLATE="$ROOT/tools/ci/module-staging-runtime.OSDK.toml"
CPUS=1
case ${2:-} in
    '')
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_staging_runtime
        VERIFY=verify_module_staging_runtime.py
        ;;
    --prepared-module)
        CRATE=linux-module-staging
        CRATE_PATH=kernel/libs/linux-module-staging
        FILTER=cybermanju_prepared_module_runtime
        VERIFY=verify_prepared_module_runtime.py
        [[ -s ${CYBERMANJU_STAGING_FIXTURE:-} ]] || { echo 'Missing prepared-module fixture' >&2; exit 2; }
        ;;
    --near-text-prepared)
        CRATE=linux-module-staging
        CRATE_PATH=kernel/libs/linux-module-staging
        FILTER=cybermanju_near_text_prepared_runtime
        VERIFY=verify_near_text_prepared_runtime.py
        [[ -s ${CYBERMANJU_STAGING_FIXTURE:-} ]] || { echo 'Missing prepared-module fixture' >&2; exit 2; }
        ;;
    --signed32-prepared)
        CRATE=linux-module-staging
        CRATE_PATH=kernel/libs/linux-module-staging
        FILTER=cybermanju_signed32_prepared_runtime
        VERIFY=verify_signed32_prepared_runtime.py
        [[ -s ${CYBERMANJU_STAGING_FIXTURE:-} ]] || { echo 'Missing prepared-module fixture' >&2; exit 2; }
        ;;
    --address-reservation)
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_module_address_runtime
        VERIFY=verify_module_address_runtime.py
        ;;
    --near-text-collision)
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_near_text_collision_runtime
        VERIFY=verify_near_text_collision_runtime.py
        ;;
    --smp-rejection)
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_module_mapping_smp_rejection
        VERIFY=verify_module_smp_rejection_runtime.py
        CONFIG_TEMPLATE="$ROOT/tools/ci/module-mapping-smp-rejection.OSDK.toml"
        CPUS=2
        ;;
    --tlb-global)
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_tlb_global_runtime
        VERIFY=verify_tlb_global_runtime.py
        ;;
    --tlb-remote)
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_tlb_remote_runtime
        VERIFY=verify_tlb_remote_runtime.py
        CONFIG_TEMPLATE="$ROOT/tools/ci/tlb-remote-runtime.OSDK.toml"
        CPUS=2
        ;;
    --tlb-remote-readback)
        CRATE=ostd
        CRATE_PATH=ostd
        FILTER=cybermanju_tlb_remote_readback_runtime
        VERIFY=verify_tlb_remote_readback.py
        CONFIG_TEMPLATE="$ROOT/tools/ci/tlb-remote-readback.OSDK.toml"
        CPUS=2
        ;;
    *) echo 'Unknown runtime mode' >&2; exit 2 ;;
esac
CONFIG="$ROOT/asterinas/$CRATE_PATH/OSDK.toml"
if [[ -e "$CONFIG" || -L "$CONFIG" ]]; then
    echo 'Refusing to overwrite an existing OSDK.toml' >&2
    exit 2
fi
for tool in cargo rustc qemu-system-x86_64 grub-mkrescue xorriso timeout python3; do
    command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool" >&2; exit 2; }
done
# The compile-time flag selects the changed fork and local runner dependencies.
(cd "$ROOT/asterinas/osdk" && CARGO_TARGET_DIR="$ROOT/asterinas/osdk/target" OSDK_LOCAL_DEV=1 cargo build --locked -p cargo-osdk) > "$OUT/osdk-build.log" 2>&1
OSDK="$ROOT/asterinas/osdk/target/debug/cargo-osdk"
export CARGO_TARGET_DIR="$ROOT/asterinas/target"
SYSROOT=$(cd "$ROOT/asterinas" && rustc --print sysroot)
HOST=$(cd "$ROOT/asterinas" && rustc -vV | sed -n 's/^host: //p')
LLVM="$SYSROOT/lib/rustlib/$HOST/bin/llvm-objcopy"
[[ -x "$LLVM" ]] || { echo 'Missing nightly llvm-tools component' >&2; exit 2; }
mkdir -p "$OUT/bin"
ln -sf "$LLVM" "$OUT/bin/rust-objcopy"
export PATH="$OUT/bin:$PATH"
# noclobber also closes the race while building OSDK after the initial check.
(set -o noclobber; cat "$CONFIG_TEMPLATE" > "$CONFIG") || exit 2
trap 'if cmp -s "$CONFIG" "$CONFIG_TEMPLATE"; then rm -f "$CONFIG"; else echo "Changed configuration preserved: $CONFIG" >&2; fi' EXIT
set +e
(cd "$ROOT/asterinas/$CRATE_PATH" && timeout --signal=TERM --kill-after=15s 600 "$OSDK" osdk test "$FILTER" --target-arch x86_64 </dev/null) 2>&1 | tee "$OUT/runtime.log"
statuses=("${PIPESTATUS[@]}")
set -e
status=${statuses[0]}
if [[ "$status" -eq 0 ]]; then status=${statuses[1]}; fi
[[ "$status" -eq 0 ]] || { echo "OSDK runtime failed with exit $status" >&2; exit "$status"; }
cp "$ROOT/asterinas/target/osdk/$CRATE-test-base/Cargo.toml" "$OUT/harness-Cargo.toml"
cp "$ROOT/asterinas/target/osdk/$CRATE-test-base/Cargo.lock" "$OUT/harness-Cargo.lock"
{
    (cd "$ROOT/asterinas" && rustc --version && cargo --version)
    qemu-system-x86_64 --version | head -1
    echo "x86_64; q35; TCG; $CPUS CPU(s); 1 GiB; BIOS/GRUB multiboot2; OSDK_LOCAL_DEV=1"
    sha256sum "$ROOT/asterinas/target/osdk/$CRATE-osdk-bin.elf" "$ROOT/asterinas/target/osdk/$CRATE/$CRATE-osdk-bin.iso"
    if [[ -n ${CYBERMANJU_STAGING_FIXTURE:-} ]]; then sha256sum "$CYBERMANJU_STAGING_FIXTURE"; fi
} > "$OUT/provenance.txt"
python3 "$ROOT/tools/ci/$VERIFY" "$OUT/runtime.log"
