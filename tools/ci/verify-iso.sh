#!/usr/bin/env bash
# Inspect the real ISO without extracting its entire (large) Nix store.
set -Eeuo pipefail
trap 'printf "ISO verification failed at line %s: %s\n" "$LINENO" "$BASH_COMMAND" >&2' ERR
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root/asterinas"
mapfile -t images < <(find -L target/nixos/iso_image/iso -maxdepth 1 -type f -name '*.iso')
[[ ${#images[@]} -eq 1 ]] || { echo 'Expected exactly one ISO' >&2; exit 1; }
iso=$(realpath "${images[0]}")
# A conservative deliverable size gate, not a claim that the build fits 14 GB.
size=$(stat -c %s "$iso")
(( size > 0 && size <= 4 * 1024 * 1024 * 1024 )) || {
    echo "ISO is empty or exceeds the small-ISO target of 4 GiB: $size bytes" >&2
    exit 1
}
report="$root/artifacts"
mkdir -p "$report"
xorriso -indev "$iso" -report_el_torito plain -report_system_area plain \
    > "$report/iso-boot-layout.log" 2>&1
cat "$report/iso-boot-layout.log"
grep -Eq 'El Torito boot img.*BIOS' "$report/iso-boot-layout.log"
grep -Eq 'El Torito boot img.*UEFI' "$report/iso-boot-layout.log"
grep -Eq 'System area summary:.*(MBR|GPT)' "$report/iso-boot-layout.log"
# Only small configuration/boot files are extracted. The squashfs is loop-mounted
# read-only so it is not copied or unpacked onto the runner filesystem.
work=$(mktemp -d)
cleanup() {
    mountpoint -q "$work/iso" && umount "$work/iso" || true
    rmdir "$work/iso" 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT
mkdir "$work/iso"
mount -o loop,ro "$iso" "$work/iso"
find "$work/iso" -maxdepth 3 -type f ! -name nix-store.squashfs \
    -printf '%P %s bytes\n' | sort | tee "$report/iso-boot-files.log"
if [[ -f "$work/iso/EFI/BOOT/grub.cfg" ]]; then
    cp "$work/iso/EFI/BOOT/grub.cfg" "$report/iso-grub.cfg"
    cat "$report/iso-grub.cfg"
fi
test -s "$work/iso/EFI/BOOT/BOOTX64.EFI"
test -s "$work/iso/EFI/BOOT/grub.cfg"
test -s "$work/iso/boot/efi.img"
test -s "$work/iso/isolinux/isolinux.cfg"
grep -q menuentry "$work/iso/EFI/BOOT/grub.cfg"
grep -q 'Installer' "$work/iso/EFI/BOOT/grub.cfg"
grep -Eq '^[[:space:]]*linux /boot/' "$work/iso/EFI/BOOT/grub.cfg"
grep -Eq '^[[:space:]]*initrd /boot/' "$work/iso/EFI/BOOT/grub.cfg"
# Validate every referenced GRUB boot payload, not merely filenames in a listing.
while read -r path; do test -s "$work/iso$path"; done < <(
    awk '$1 == "linux" || $1 == "initrd" { print $2 }' "$work/iso/EFI/BOOT/grub.cfg" | sort -u
)
test -s "$work/iso/nix-store.squashfs"
# Listing alone reads metadata; no full squashfs extraction or second ISO copy.
# squashfsTools is already a build dependency of this exact pinned ISO module.
squashfs_tools=$(nix-build distro/nixpkgs.nix -A squashfsTools --no-out-link)
"$squashfs_tools/bin/unsquashfs" -ll "$work/iso/nix-store.squashfs" > "$report/iso-store-list.log"
for payload in '/bin/aster-nixos-install' '/etc_nixos/configuration.nix' '/bin/nixos-install' '/bin/nix-build' 'asterinas-osdk-bin'; do
    grep -Fq "$payload" "$report/iso-store-list.log" || {
        echo "Missing full installer payload: $payload" >&2; exit 1;
    }
done
printf 'PASS: %s bytes; BIOS/UEFI hybrid metadata, EFI GRUB and full installer payload.\n' "$size"
