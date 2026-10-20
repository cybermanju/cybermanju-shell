#!/usr/bin/env bash
# Tests real freestanding NVIDIA diagnostic formatting against libc on the host.
# This is a CPU-only ABI/formatting test, never GPU evidence.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
SOURCE=${NVIDIA_SOURCE_DIR:?Set NVIDIA_SOURCE_DIR to the clean official 615.78.08 checkout}
EXPECTED=1a82fbfda8f321b056eb3568850f9316c5e465c5
[[ $(git -C "$SOURCE" rev-parse HEAD) == "$EXPECTED" ]] || { echo 'Wrong NVIDIA source revision' >&2; exit 1; }
[[ -z $(git -C "$SOURCE" status --porcelain --untracked-files=no) ]] || { echo 'NVIDIA tracked sources are modified' >&2; exit 1; }
BUILD=$(mktemp -d)
trap 'rm -rf "$BUILD"' EXIT
includes=()
for directory in \
    src/nvidia/arch/nvalloc/unix/include src/nvidia/arch/nvalloc/common/inc \
    src/nvidia/inc src/nvidia/inc/os src/nvidia/inc/kernel \
    src/nvidia/inc/libraries src/nvidia/src/libraries \
    src/nvidia/src/libraries/nvport/sync src/common/sdk/nvidia/inc \
    src/common/inc kernel-open/common/inc; do
    includes+=(-I "$SOURCE/$directory")
done
"${CC:-cc}" -std=gnu11 -Wall -Wextra -Werror -O2 \
    -D_LANGUAGE_C -D__NO_CTYPE -DNVRM \
    -include "$SOURCE/src/common/sdk/nvidia/inc/cpuopsys.h" "${includes[@]}" \
    -c "$ROOT/ostd/libs/nvidia-nvport/diagnostics_format.c" -o "$BUILD/formatter.o"
# The harness intentionally asks libc about truncated output and GNU percent
# padding/ignored-flag behavior. These warnings are disabled only for fixtures.
"${CC:-cc}" -std=gnu11 -Wall -Wextra -Werror -Wno-format -Wno-format-truncation -O2 \
    -D_LANGUAGE_C -D__NO_CTYPE -DNVRM \
    -include "$SOURCE/src/common/sdk/nvidia/inc/cpuopsys.h" "${includes[@]}" \
    "$BUILD/formatter.o" \
    "$ROOT/ostd/libs/nvidia-nvport/diagnostics_format_test.c" \
    -o "$BUILD/nvport-format-tests"
"$BUILD/nvport-format-tests"
