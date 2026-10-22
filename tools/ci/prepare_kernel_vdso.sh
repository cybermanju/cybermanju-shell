#!/usr/bin/env bash
# Real upstream VDSO pinned by the kernel-dev Dockerfile; never a stub.
set -euo pipefail
[[ $# == 1 ]] || { echo 'Uso: prepare_kernel_vdso.sh DIRETORIO' >&2; exit 2; }
OUT=$1
[[ ! -L "$OUT" ]] || { echo 'Diretório VDSO simbólico recusado' >&2; exit 2; }
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
FILE="$OUT/vdso_x86_64.so"
[[ ! -L "$FILE" ]] || { echo 'Arquivo VDSO simbólico recusado' >&2; exit 2; }
readonly COMMIT=8af571775a3d3a1df3e84fc9709b9d6b160c771a
readonly SHA256=27cd7bf576a92fa48a891c05988f52fac35b7374d5b9650d6b035e477c5d93a3
readonly URL="https://raw.githubusercontent.com/asterinas/linux_vdso/$COMMIT/vdso_x86_64.so"
verify() {
    [[ -f "$1" ]] || return 1
    printf '%s  %s\n' "$SHA256" "$1" | sha256sum --check --status || return 1
    python3 - "$1" <<'PY'
from pathlib import Path
import sys
b = Path(sys.argv[1]).read_bytes()
assert len(b) >= 64 and b[:6] == b'\x7fELF\x02\x01', 'VDSO não é ELF64 little-endian'
assert int.from_bytes(b[16:18], 'little') == 3, 'VDSO não é ET_DYN'
assert int.from_bytes(b[18:20], 'little') == 62, 'VDSO não é x86-64'
PY
}
if [[ -e "$FILE" ]]; then
    verify "$FILE" || { echo 'VDSO existente divergente; arquivo preservado' >&2; exit 2; }
else
    command -v curl >/dev/null || { echo 'curl necessário para obter VDSO real' >&2; exit 2; }
    TMP=$(mktemp "$OUT/.vdso.XXXXXX")
    trap 'rm -f -- "$TMP"' EXIT
    curl --fail --location --retry 2 --connect-timeout 15 --max-time 120 --proto '=https' --proto-redir '=https' --tlsv1.2 "$URL" -o "$TMP"
    verify "$TMP" || { echo 'Download VDSO não validado; nada publicado' >&2; exit 1; }
    chmod 644 "$TMP"
    # Atomic publication, refusing a destination created by another caller.
    ln -- "$TMP" "$FILE"
fi
printf 'VDSO real validado; commit=%s\n' "$COMMIT"
sha256sum "$FILE"
