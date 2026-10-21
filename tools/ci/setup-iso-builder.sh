#!/usr/bin/env bash
# Run only inside the disposable Actions builder, never on the local sandbox.
set -euo pipefail
[[ ${ISO_CI_BUILDER:-} == 1 && $EUID == 0 ]] || {
    echo 'This bootstrap requires the disposable ISO CI container.' >&2
    exit 2
}
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
    build-essential ca-certificates curl git python3 pkg-config libssl-dev \
    nix-bin xorriso mtools grub-pc-bin grub-efi-amd64-bin grub2-common \
    cpio dosfstools exfatprogs e2fsprogs parted util-linux xz-utils file
# Use local-store mode: this container has no systemd to start nix-daemon.
mkdir -p /etc/nix
cat >> /etc/nix/nix.conf <<'EOF'
build-users-group =
max-jobs = 1
cores = 2
experimental-features = nix-command flakes
extra-substituters = https://aster-nixos-release.cachix.org https://aster-nixos-dev.cachix.org
extra-trusted-public-keys = aster-nixos-release.cachix.org-1:xB6U/f5ck5vGDJZ04kPp3zGpZ4Nro9X4+TSSMAETVFE= aster-nixos-dev.cachix.org-1:xrCbE2flfliFTQCY/2HeJoT2tCO+5kMTZeLIUH9lnIA=
EOF
export NIX_REMOTE=local
# Share the exact source-pinned nixpkgs used by the ISO. Binary substitutes avoid
# rebuilding QEMU; Ubuntu 24.04's QEMU 8 has known Asterinas incompatibilities.
qemu=$(nix-build distro/nixpkgs.nix -A qemu_kvm --out-link /opt/iso-qemu)
ovmf=$(nix-build distro/nixpkgs.nix -A OVMF.fd --out-link /opt/iso-ovmf)
mkdir -p /opt/iso-bin /root/ovmf/release
ln -s "$qemu/bin/qemu-system-x86_64" /opt/iso-bin/qemu-system-x86_64
ln -s "$qemu/bin/qemu-img" /opt/iso-bin/qemu-img
ln -s "$ovmf/FV/OVMF.fd" /root/ovmf/release/OVMF.fd
ln -s "$ovmf/FV/OVMF_VARS.fd" /root/ovmf/release/OVMF_VARS.fd
# Install only the toolchain required by this checkout, not documentation tools,
# benchmark packages, other-architecture Nix caches, or gVisor test binaries.
curl --fail --location --retry 3 https://sh.rustup.rs -o /opt/rustup-init.sh
sh /opt/rustup-init.sh -y --profile minimal --default-toolchain none
export PATH="/root/.cargo/bin:/opt/iso-bin:$PATH"
rust_channel=$(sed -n 's/^channel = "\([^"]*\)"/\1/p' rust-toolchain.toml)
[[ -n "$rust_channel" ]]
rustup toolchain install "$rust_channel" --profile minimal \
    --component rust-src,rustc-dev,llvm-tools-preview --target x86_64-unknown-none
cargo install --locked cargo-binutils --version 0.4.0
# Required Linux VDSO data, pinned identically to kernel-dev/Dockerfile.
git init /opt/linux_vdso
git -C /opt/linux_vdso remote add origin https://github.com/asterinas/linux_vdso.git
git -C /opt/linux_vdso fetch --depth 1 origin 8af571775a3d3a1df3e84fc9709b9d6b160c771a
git -C /opt/linux_vdso checkout FETCH_HEAD
printf '%s\n' 'export PATH="/root/.cargo/bin:/opt/iso-bin:$PATH"' \
    'export NIX_REMOTE=local' 'export VDSO_LIBRARY_DIR=/opt/linux_vdso' \
    > /etc/profile.d/iso-ci.sh
source /etc/profile.d/iso-ci.sh
rustc --version
nix-build --version
qemu-system-x86_64 --version
test -s /root/ovmf/release/OVMF.fd
command -v rust-objcopy xorriso grub-mkrescue
