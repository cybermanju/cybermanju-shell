#!/usr/bin/env bash
# CyberManju OS — build cyb .debs from cross-compiled linux binaries.
# Usage: cyb-deb.sh <version> <cli-dir> <out-dir>
#   <version>  package.json version (leading v stripped for Debian)
#   <cli-dir>  the cli/ checkout (expects build-linux-amd64/cyb etc.)
#   <out-dir>  destination for cyb_<ver>_<arch>.deb
# Needs: dpkg-deb (preinstalled on ubuntu-latest and debian images).
set -eu

VER="${1:?version required}"
CLIDIR="${2:?cli dir required}"
OUTDIR="${3:?out dir required}"
VER="${VER#v}"

mkdir -p "$OUTDIR"
for arch in amd64 arm64; do
  bin="$CLIDIR/build-linux-$arch/cyb"
  [ -f "$bin" ] || { echo "cyb-deb: missing $bin" >&2; exit 1; }
  root="$OUTDIR/debroot-$arch"
  rm -rf "$root"
  mkdir -p "$root/usr/bin" "$root/DEBIAN"
  cp "$bin" "$root/usr/bin/cyb"
  chmod 755 "$root/usr/bin/cyb"
  cat > "$root/DEBIAN/control" <<EOF
Package: cyb
Version: $VER
Architecture: $arch
Maintainer: CyberManju Team
Depends: ca-certificates
Description: Universal CyberManju OS terminal (Charm UI)
 Files, disks, sync, OAuth, cybsh and the AI agent from any terminal.
EOF
  dpkg-deb --build "$root" "$OUTDIR/cyb_${VER}_${arch}.deb"
  rm -rf "$root"
done
ls -la "$OUTDIR"/cyb_*.deb
