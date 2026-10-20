#!/usr/bin/env bash
# Test the ISO produced by `make iso NIXOS_TEST_SUITE=hello`.
set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root/asterinas"

case "${1:-}" in
  install)
    # Never accept a success marker left by an earlier VM invocation.
    rm -f qemu.log qemu-serial.log
    status=0
    timeout --signal=TERM --kill-after=30s 22m make run_iso || status=$?
    [[ ! -f qemu.log ]] || cp qemu.log qemu-install.log
    [[ ! -f qemu-serial.log ]] || cp qemu-serial.log qemu-install-serial.log
    if (( status != 0 )); then
      echo "ISO VM failed or timed out (exit $status)." >&2
      exit "$status"
    fi
    # The upstream auto-installer powers off even on failure, so a zero VM
    # exit status alone is insufficient. Match the actual completion message.
    grep -q '^Congratulations! Asterinas NixOS has been installed successfully!' qemu.log || {
      echo 'Installer did not report successful installation.' >&2
      exit 1
    }
    test -s target/nixos/asterinas.img
    echo 'PASS: ISO booted and installation completed.'
    ;;
  boot)
    test -s target/nixos/asterinas.img
    rm -f qemu.log qemu-serial.log
    # The existing Rust harness checks the login prompt, executes
    # hello-asterinas, verifies Hello Asterinas!, and checks clean shutdown.
    timeout --signal=TERM --kill-after=30s 12m \
      make run_nixos NIXOS_TEST_SUITE=hello NIXOS_TEST_TIMEOUT=5min
    [[ ! -f qemu.log ]] || cp qemu.log qemu-boot.log
    [[ ! -f qemu-serial.log ]] || cp qemu-serial.log qemu-boot-serial.log
    echo 'PASS: installed Asterinas booted and hello-asterinas passed.'
    ;;
  *)
    echo "Usage: $0 {install|boot}" >&2
    exit 2
    ;;
esac
