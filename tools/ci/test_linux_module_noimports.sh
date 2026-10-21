#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Build and inspect only. NEVER insmod/modprobe, boot QEMU, or execute module init.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
LIB="$ROOT/asterinas/kernel/libs/linux-module-elf"
FIXTURE="$LIB/tests/kbuild-noimports"
HEADERS=${KERNEL_HEADERS:-/usr/src/linux-headers-6.8.0-147-generic}
EXPECTED_RELEASE=${EXPECTED_KERNEL_RELEASE:-6.8.0-147-generic}
CC=${CC:-x86_64-linux-gnu-gcc-13}
cd "$ROOT"
OUT=${1:-artifacts/linux-module-noimports}
mkdir -p "$OUT"
OUT=$(realpath "$OUT")
WORK=''
finish() {
    local code=$?
    [[ -z "$WORK" ]] || rm -rf "$WORK"
    printf 'exit_code=%s\nloaded=false\n' "$code" > "$OUT/status.txt"
}
trap finish EXIT
printf 'exit_code=pending\nloaded=false\n' > "$OUT/status.txt"
test -s "$HEADERS/.config"
test -s "$HEADERS/Module.symvers"
grep -qx 'CONFIG_X86_64=y' "$HEADERS/.config"
RELEASE=$(sed -n 's/^#define UTS_RELEASE "\([^"]*\)"$/\1/p' "$HEADERS/include/generated/utsrelease.h")
if [[ "$RELEASE" != "$EXPECTED_RELEASE" ]]; then
    printf 'Header release mismatch: expected %s, got %s\n' "$EXPECTED_RELEASE" "$RELEASE" >&2
    exit 1
fi
command -v "$CC" >/dev/null
command -v make >/dev/null
command -v readelf >/dev/null
command -v modinfo >/dev/null
command -v nm >/dev/null
command -v objdump >/dev/null
WORK=$(mktemp -d)
cp "$FIXTURE/Makefile" "$FIXTURE/cybermanju_noimports_fixture.c" "$WORK/"
make --no-print-directory -C "$HEADERS" M="$WORK" CC="$CC" modules 2>&1 | tee "$OUT/kbuild.log"
MODULE="$WORK/cybermanju_noimports_fixture.ko"
test -s "$MODULE"
readelf -h -S -s -r "$MODULE" > "$OUT/readelf.txt"
modinfo "$MODULE" > "$OUT/modinfo.txt"
objdump -dr "$MODULE" > "$OUT/disassembly.txt"
cp "$WORK/.cybermanju_noimports_fixture.o.cmd" "$OUT/object-command.txt"
nm -u "$MODULE" > "$OUT/undefined-symbols.txt"
if [[ -s "$OUT/undefined-symbols.txt" ]]; then
    echo 'Unexpected undefined symbols in no-import fixture:' >&2
    cat "$OUT/undefined-symbols.txt" >&2
    exit 1
fi
rustc --edition=2024 --deny warnings --crate-name linux_module_elf --crate-type rlib \
    "$LIB/src/lib.rs" -o "$WORK/liblinux_module_elf.rlib"
rustc --edition=2024 --deny warnings "$LIB/tests/noimports_probe.rs" \
    --extern "linux_module_elf=$WORK/liblinux_module_elf.rlib" -o "$WORK/noimports_probe"
"$WORK/noimports_probe" "$MODULE" | tee "$OUT/inspection.txt"
VERMAGIC=$(modinfo -F vermagic "$MODULE")
[[ "${VERMAGIC%% *}" == "$EXPECTED_RELEASE" ]]
grep -Eq 'Machine:[[:space:]]+Advanced Micro Devices X86-64' "$OUT/readelf.txt"
cp "$MODULE" "$OUT/cybermanju_noimports_fixture.ko"
{
    printf 'kernel_release=%s\nvermagic=%s\n' "$RELEASE" "$VERMAGIC"
    printf 'architecture=x86_64\nheaders=%s\ncompiler=%s\n' "$HEADERS" "$CC"
    "$CC" --version | head -n 1
    rustc --version
    printf 'kernel_config_sha256='
    sha256sum "$HEADERS/.config" | cut -d ' ' -f 1
    printf 'module_symvers_sha256='
    sha256sum "$HEADERS/Module.symvers" | cut -d ' ' -f 1
    printf 'fixture_source_sha256='
    sha256sum "$FIXTURE/cybermanju_noimports_fixture.c" | cut -d ' ' -f 1
    printf 'fixture_makefile_sha256='
    sha256sum "$FIXTURE/Makefile" | cut -d ' ' -f 1
    printf 'module_sha256='
    sha256sum "$OUT/cybermanju_noimports_fixture.ko" | cut -d ' ' -f 1
    printf 'undefined_symbols=0\n'
    printf 'object_flags=-fno-instrument-functions -mfunction-return=keep; removed=-pg -mfentry -mfunction-return=thunk-extern\n'
    printf 'scope=only cybermanju_noimports_fixture.o; OBJECT_FILES_NON_STANDARD bypasses objtool; not a default kernel configuration\n'
    printf 'limitations=no imports does not establish ABI compatibility, loadability, runtime behavior, or security; objtool return-thunk checks are bypassed for this fixture\n'
    printf 'inspection=passed\nloaded=false\n'
} | tee "$OUT/provenance.txt"
