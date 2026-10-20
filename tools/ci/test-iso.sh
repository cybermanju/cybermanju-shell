#!/usr/bin/env bash
# Test the full, interactive ISO produced by `make iso AUTO_INSTALL=false`.
set -euo pipefail
repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root/asterinas"
phase=${1:-}
case "$phase" in
  install|live-usb|boot)
    vm_exports=$(python3 "$repo_root/tools/ci/vm_settings.py" shell \
      --record "$repo_root/artifacts/acceleration-${phase}.json" --phase "$phase")
    eval "$vm_exports"
    ;;
esac
case "$phase" in
  install|live-usb)
    rm -f qemu.log qemu-serial.log
    status=0
    phase_timeout=$ISO_VM_LIVE_TIMEOUT
    [[ "$phase" != install ]] || phase_timeout=$ISO_VM_INSTALL_TIMEOUT
    timeout --signal=TERM --kill-after=30s "$phase_timeout" \
      python3 "$repo_root/tools/ci/iso_vm.py" "$phase" || status=$?
    [[ ! -f qemu.log ]] || cp qemu.log "qemu-${phase}.log"
    [[ ! -f qemu-serial.log ]] || cp qemu-serial.log "qemu-${phase}-serial.log"
    if (( status != 0 )); then
      echo "ISO $phase VM failed or timed out (exit $status)." >&2
      exit "$status"
    fi
    if [[ "$phase" == install ]]; then
      grep -q '^Congratulations! Asterinas NixOS has been installed successfully!' qemu.log || {
        echo 'Installer did not report successful installation.' >&2; exit 1;
      }
      test -s target/nixos/asterinas.img
    else
      grep -q '^CYBERMANJU_LIVE_INSTALLER_READY' qemu.log
    fi
    echo "PASS: full ISO $phase gate."
    ;;
  boot)
    test -s target/nixos/asterinas.img
    rm -f qemu.log qemu-serial.log
    status=0
    # This existing Rust harness checks login, Hello Asterinas!, and shutdown.
    timeout --signal=TERM --kill-after=30s "$ISO_VM_BOOT_TIMEOUT" \
      make run_nixos OVMF=on ENABLE_KVM="$ENABLE_KVM" \
      NIXOS_TEST_SUITE=hello NIXOS_TEST_TIMEOUT="$NIXOS_TEST_TIMEOUT" || status=$?
    [[ ! -f qemu.log ]] || cp qemu.log qemu-boot.log
    [[ ! -f qemu-serial.log ]] || cp qemu-serial.log qemu-boot-serial.log
    (( status == 0 )) || exit "$status"
    echo "PASS: installed Asterinas booted and hello-asterinas passed ($QEMU_ACCEL)."
    ;;
  *)
    echo "Usage: $0 {live-usb|install|boot}" >&2
    exit 2
    ;;
esac
