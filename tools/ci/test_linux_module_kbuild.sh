#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Build and inspect a real .ko; NEVER insmod/modprobe, boot or execute its code.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
LIB="$ROOT/asterinas/kernel/libs/linux-module-elf"
HEADERS=${KERNEL_HEADERS:-/usr/src/linux-headers-6.8.0-147-generic}
EXPECTED_RELEASE=${EXPECTED_KERNEL_RELEASE:-6.8.0-147-generic}
CC=${CC:-x86_64-linux-gnu-gcc-13}
cd "$ROOT"
OUT=${1:-artifacts/linux-module-kbuild}
mkdir -p "$OUT"
OUT=$(realpath "$OUT")
WORK=''
# A failed rerun must not let an older provenance file imply current success.
finish() {
    local code=$?
    [[ -z "$WORK" ]] || rm -rf "$WORK"
    printf 'exit_code=%s\nloaded=false\n' "$code" > "$OUT/status.txt"
}
trap finish EXIT
printf 'exit_code=pending\nloaded=false\n' > "$OUT/status.txt"
# Missing/mismatched headers fail, rather than silently skipping or using uname -r.
test -s "$HEADERS/.config"
test -s "$HEADERS/Module.symvers"
grep -qx 'CONFIG_X86_64=y' "$HEADERS/.config"
# Ubuntu's headers can report a source release from `make kernelrelease` that
# differs from the packaged ABI release compiled into generated/utsrelease.h.
RELEASE=$(sed -n 's/^#define UTS_RELEASE "\([^"]*\)"$/\1/p' "$HEADERS/include/generated/utsrelease.h")
if [[ "$RELEASE" != "$EXPECTED_RELEASE" ]]; then
    printf 'Header release mismatch: expected %s, got %s\n' "$EXPECTED_RELEASE" "$RELEASE" >&2
    exit 1
fi
WORK=$(mktemp -d)
cp "$LIB/tests/kbuild/Makefile" "$LIB/tests/kbuild/cybermanju_fixture.c" "$WORK/"
make --no-print-directory -C "$HEADERS" M="$WORK" CC="$CC" modules 2>&1 | tee "$OUT/kbuild.log"
MODULE="$WORK/cybermanju_fixture.ko"
test -s "$MODULE"
readelf -h -S -s -r "$MODULE" > "$OUT/readelf.txt"
modinfo "$MODULE" > "$OUT/modinfo.txt"
nm -u "$MODULE" > "$OUT/undefined-symbols.txt"
VERMAGIC=$(modinfo -F vermagic "$MODULE")
[[ "${VERMAGIC%% *}" == "$EXPECTED_RELEASE" ]]
rustc --edition=2024 --deny warnings --crate-name linux_module_elf --crate-type rlib \
    "$LIB/src/lib.rs" -o "$WORK/liblinux_module_elf.rlib"
rustc --edition=2024 --deny warnings "$LIB/tests/probe.rs" \
    --extern "linux_module_elf=$WORK/liblinux_module_elf.rlib" -o "$WORK/probe"
"$WORK/probe" "$MODULE" | tee "$OUT/inspection.txt"
cp "$MODULE" "$OUT/cybermanju_fixture.ko"
# Record provenance, not compatibility: this module has NOT been loaded anywhere.
{
    printf 'kernel_release=%s\nvermagic=%s\n' "$RELEASE" "$VERMAGIC"
    "$CC" --version | head -n 1
    rustc --version
    printf 'kernel_config_sha256='
    sha256sum "$HEADERS/.config" | cut -d ' ' -f 1
    printf 'module_symvers_sha256='
    sha256sum "$HEADERS/Module.symvers" | cut -d ' ' -f 1
    printf 'fixture_source_sha256='
    sha256sum "$LIB/tests/kbuild/cybermanju_fixture.c" | cut -d ' ' -f 1
    printf 'module_sha256='
    sha256sum "$OUT/cybermanju_fixture.ko" | cut -d ' ' -f 1
    printf 'relocation_types='
    grep -oE 'R_X86_64_[A-Z0-9_]+' "$OUT/readelf.txt" | sort -u | paste -sd ',' -
    printf 'inspection=passed\nloaded=false\nabi_compatible=unproven\n'
} | tee "$OUT/provenance.txt"
