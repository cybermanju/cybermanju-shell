#!/bin/sh

# SPDX-License-Identifier: MPL-2.0

# Run a NixOS installation or NixOS ISO installer image built by the root Makefile inside a VM
#
# Usage: ./run.sh [nixos | iso]

set -e

usage() {
    echo "Usage: $0 [nixos | iso]"
    exit 1
}

if [ "$#" -ne 1 ]; then
    usage
fi

MODE=$1
TARGET_ARCH=${TARGET_ARCH:-x86_64}
SCRIPT_DIR=$(dirname "$0")
ASTERINAS_DIR=$(realpath "${SCRIPT_DIR}/../..")

# tools/qemu_args.sh currently emits x86_64-specific arguments.
# Reject other architectures to avoid invoking non-x86 QEMU with incompatible args.
if [ "${TARGET_ARCH}" != "x86_64" ]; then
    echo "Error: tools/nixos/run.sh currently supports only TARGET_ARCH=x86_64; got ${TARGET_ARCH}" >&2
    exit 1
fi

# Change to Asterinas root directory to ensure all scripts run from the correct location.
cd "${ASTERINAS_DIR}"

# Do not depend on nested virtualization being exposed by the hosted runner.
# Recheck access here as well as CI preflight, before choosing the CPU model.
if [ "${ENABLE_KVM:-auto}" != "0" ] && [ -c /dev/kvm ] && [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    ENABLE_KVM=1
else
    ENABLE_KVM=0
fi
export ENABLE_KVM

# No auxiliary test disks: the install target must be unambiguously /dev/vda.
# This scheme uses UEFI explicitly, matching the installed removable EFI GRUB.
QEMU_ARGS=$(${ASTERINAS_DIR}/tools/qemu_args.sh nixos 2>/dev/null)

# Add mode-specific disk and device arguments
case "$MODE" in
    nixos)
        NIXOS_DIR="${ASTERINAS_DIR}/target/nixos"
        QEMU_ARGS="${QEMU_ARGS} \
            -drive if=none,format=raw,id=u0,file=${NIXOS_DIR}/asterinas.img \
            -device virtio-blk-pci,drive=u0,disable-legacy=on,disable-modern=off \
        "
        ;;
    iso)
        ASTER_IMAGE_PATH=${ASTERINAS_DIR}/target/nixos/asterinas.img
        NIXOS_DISK_SIZE_IN_MB=${NIXOS_DISK_SIZE_IN_MB:-16384}
        ISO_IMAGE_PATH=$(find "${ASTERINAS_DIR}/target/nixos/iso_image/iso" -name "*.iso" | head -n 1)

        if [ ! -f "$ISO_IMAGE_PATH" ]; then
            echo "Error: ISO_IMAGE not found!"
            exit 1
        fi

        rm -f "${ASTER_IMAGE_PATH}"
        echo "Creating image at ${ASTER_IMAGE_PATH} of size ${NIXOS_DISK_SIZE_IN_MB}MB......"
        # Keep guest capacity without eagerly allocating it on the host.
        truncate -s "${NIXOS_DISK_SIZE_IN_MB}M" "${ASTER_IMAGE_PATH}"
        echo "Image created successfully!"

        case "${ISO_BOOT_MODE:-cdrom}" in
            cdrom)
                ISO_BOOT_ARGS="-cdrom ${ISO_IMAGE_PATH} -boot d"
                ;;
            usb)
                # Boot the identical hybrid ISO, not an extracted kernel/initrd.
                ISO_BOOT_ARGS="-device qemu-xhci,id=iso-xhci \
                    -drive if=none,format=raw,readonly=on,id=iso-usb,file=${ISO_IMAGE_PATH} \
                    -device usb-storage,bus=iso-xhci.0,drive=iso-usb,bootindex=1"
                ;;
            *)
                echo "Error: ISO_BOOT_MODE must be cdrom or usb" >&2
                exit 1
                ;;
        esac
        QEMU_ARGS="${QEMU_ARGS} \
            ${ISO_BOOT_ARGS} \
            -drive if=none,format=raw,id=u0,file=${ASTER_IMAGE_PATH} \
            -device virtio-blk-pci,drive=u0,disable-legacy=on,disable-modern=off \
        "
        ;;
    *)
        usage
        ;;
esac

if [ "${ENABLE_KVM}" = "1" ]; then
    QEMU_ARGS="${QEMU_ARGS} -accel kvm"
    echo "QEMU acceleration: kvm (/dev/kvm available)"
else
    QEMU_ARGS="${QEMU_ARGS} -accel tcg,thread=multi"
    echo "QEMU acceleration: tcg (software emulation; KVM absent, inaccessible or disabled)"
fi

QEMU_BIN=${QEMU_BIN:-qemu-system-${TARGET_ARCH}}

# The kernel uses a specific value to signal a successful shutdown via the
# isa-debug-exit device.
KERNEL_SUCCESS_EXIT_CODE=16 # 0x10 in hexadecimal
# QEMU translates the value written to the isa-debug-exit device into a final
# process exit code using following formula.
QEMU_SUCCESS_EXIT_CODE=$(((KERNEL_SUCCESS_EXIT_CODE << 1) | 1))

# Execute QEMU
# shellcheck disable=SC2086
${QEMU_BIN} ${QEMU_ARGS} || exit_code=$?
exit_code=${exit_code:-0}

# Check if the execution was successful:
# - Exit code 0: Normal successful exit (e.g., ACPI shutdown or clean termination)
# - Exit code $QEMU_SUCCESS_EXIT_CODE: Kernel signaled success via isa-debug-exit device
if [ ${exit_code} -eq 0 ] || [ ${exit_code} -eq ${QEMU_SUCCESS_EXIT_CODE} ]; then
    exit 0
fi

exit ${exit_code}
