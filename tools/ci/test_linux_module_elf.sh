#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Host-only structural tests. Does not build, boot, or load a Linux module.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
LIB="$ROOT/asterinas/kernel/libs/linux-module-elf"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cd "$ROOT"
rustc --version
gcc --version | head -n 1
rustc --edition=2024 --deny warnings --test "$LIB/src/lib.rs" -o "$WORK/tests"
"$WORK/tests"
rustc --edition=2024 --deny warnings --crate-name linux_module_elf --crate-type rlib \
    "$LIB/src/lib.rs" -o "$WORK/liblinux_module_elf.rlib"
gcc -c -O0 -fno-pic -fno-pie "$LIB/tests/relocatable.c" -o "$WORK/relocatable.o"
readelf -h -S -r "$WORK/relocatable.o"
rustc --edition=2024 --deny warnings "$LIB/tests/probe.rs" \
    --extern "linux_module_elf=$WORK/liblinux_module_elf.rlib" -o "$WORK/probe"
"$WORK/probe" "$WORK/relocatable.o"
