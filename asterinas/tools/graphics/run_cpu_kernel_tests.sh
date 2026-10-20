#!/usr/bin/env bash
# Real Asterinas/OSTD tests under CPU-only QEMU TCG. Never GPU acceptance.
set -euo pipefail
ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
OUTPUT=${1:?usage: run_cpu_kernel_tests.sh OUTPUT_DIRECTORY}
mkdir -p "$OUTPUT"
OUTPUT=$(cd -- "$OUTPUT" && pwd)

for suite in nvport platform pci pci-iommu ostd; do
    case "$suite" in
        nvport) package=ostd/libs/nvidia-nvport; name=nvidia-nvport ;;
        platform) package=kernel/comps/nvidia-platform; name=nvidia-platform ;;
        pci|pci-iommu) package=kernel/core/comps/pci; name=$suite ;;
        ostd) package=ostd; name=ostd ;;
    esac
    log="$OUTPUT/native-$name-tests.log"
    (
        cd "$ROOT/$package"
        # Never overwrite a developer-provided package manifest. noclobber also
        # closes the race with another local process creating this file.
        set -o noclobber
        pci_test_device=""
        if [[ "$suite" == pci || "$suite" == pci-iommu ]]; then
            pci_test_device=" -device pci-testdev,membar=4096 -device edu,dma_mask=4294967295,addr=05.0 -device edu,dma_mask=4294967295,addr=06.0"
            pci_test_device+=" -device pcie-root-port,id=rp-good,addr=07.0,chassis=7,hotplug=off -device nvme,bus=rp-good,addr=00.0,serial=ASTER-PCIE-DMA"
            pci_test_device+=" -device pcie-root-port,id=rp-noacs,addr=08.0,chassis=8,hotplug=off,disable-acs=on -device nvme,bus=rp-noacs,addr=00.0,serial=ASTER-NO-ACS"
            pci_test_device+=" -device pci-bridge,id=legacy,addr=09.0,chassis_nr=9,shpc=off -device edu,bus=legacy,addr=01.0,dma_mask=4294967295"
            pci_test_device+=" -device pcie-root-port,id=rp-group,addr=0a.0,chassis=10,hotplug=off -device nvme,bus=rp-group,addr=00.0,multifunction=on,serial=ASTER-GROUP-F0 -device nvme,bus=rp-group,addr=00.1,serial=ASTER-GROUP-F1"
            pci_test_device+=" -device pcie-root-port,id=rp-msix,addr=0c.0,chassis=12,hotplug=off -device nvme,bus=rp-msix,addr=00.0,serial=ASTER-MSIX"
            pci_test_device+=" -device pcie-root-port,id=rp-msix-legacy,addr=0d.0,chassis=13,hotplug=off -device nvme,bus=rp-msix-legacy,addr=00.0,serial=ASTER-MSIX-LEGACY"
            pci_test_device+=" -device pcie-root-port,id=rp-msix-quarantine,addr=0e.0,chassis=14,hotplug=off -device nvme,bus=rp-msix-quarantine,addr=00.0,serial=ASTER-MSIX-QUARANTINE"
            pci_test_device+=" -device pcie-root-port,id=rp-group-bad,addr=0b.0,chassis=11,hotplug=off -device nvme,bus=rp-group-bad,addr=00.0,multifunction=on,serial=ASTER-GROUP-BAD -device edu,bus=rp-group-bad,addr=00.1,dma_mask=4294967295"
        fi
        if [[ "$suite" == pci-iommu ]]; then
            pci_test_device+=" -device intel-iommu,intremap=on"
        fi
        printf '%s\n' \
            '[boot]' \
            'method = "grub-rescue-iso"' \
            '[grub]' \
            'boot_protocol = "multiboot2"' \
            '[qemu]' \
            "args = \"-machine q35,accel=tcg -cpu max -m 2048M -smp 2 -display none -monitor none -serial stdio -no-reboot -nic none -device isa-debug-exit,iobase=0xf4,iosize=0x04${pci_test_device}\"" \
            '[test.boot]' \
            'kcmd_args = ["earlycon", "console=ttyS0"]' > OSDK.toml
        trap 'rm -f OSDK.toml' EXIT
        # No guest input is needed. Detach stdin so QEMU never attempts to
        # read the controlling terminal from this pipeline/background group.
        features=()
        if [[ "$suite" == platform ]]; then
            features=(--features nvport-runtime)
        fi
        timeout --signal=TERM --kill-after=5 600s cargo osdk test --strip-elf "${features[@]}" </dev/null
    ) 2>&1 | tee "$log"
    # An OSDK filter can exit successfully after executing zero tests. Do not
    # mistake that for a pass, or accept logs without the final runner marker.
    python3 - "$log" <<'PY'
from pathlib import Path
import re
import sys
text = Path(sys.argv[1]).read_text(errors="strict")
text = re.sub(r"\x1b\[[0-9;]*[mK]", "", text)
if not re.search(r"test result: ok\. [1-9][0-9]* passed; 0 failed;", text):
    raise SystemExit("No positive, passing kernel test count in " + sys.argv[1])
if "[ktest runner] All crates tested." not in text:
    raise SystemExit("Missing final kernel runner marker in " + sys.argv[1])
PY
    if [[ "$suite" == platform ]]; then
        grep -Fq 'NVIDIA native worker: actual task, pending coalescence, accepted-work drain and capture destruction passed; not RM/GPU' "$log"
        grep -Fq 'NVIDIA native worker: actual APIC timer IRQ to sleepable task handoff passed; not NVIDIA IRQ or GPU' "$log"
    fi
    if [[ "$suite" == pci || "$suite" == pci-iommu ]]; then
        grep -Fq 'PCI real NVMe MSI-X masked-pending IRQ and quiesced shutdown cycles passed; not GPU validation' "$log"
        grep -Fq 'PCI legacy MSI-X passed: no implicit BME, checked indices, no handler unmask, no vector replacement' "$log"
        grep -Fq 'PCI MSI-X abandonment passed: function and MMIO quarantined, no reprobe; not a hardware completion test' "$log"
    fi
    if [[ "$suite" == pci ]]; then
        grep -Fq 'PCI device DMA passed: 8 real EDU 4096-byte RAM->device->RAM cycles;' "$log"
        grep -Fq 'PCIe topology negative cases passed without IOMMU; no PCIe DMA issued' "$log"
        grep -Fq 'PCI multifunction unsupported sibling correctly rejected: real conventional PCI EDU function one lacks PCIe' "$log"
        grep -Fq 'PCI multifunction group correctly rejected without IOMMU; no group DMA issued' "$log"
        grep -Fq 'PCI multifunction group reservation passed:' "$log"
    elif [[ "$suite" == pci-iommu ]]; then
        grep -Fq 'DMA remapping enabled' "$log"
        grep -Fq 'PCI device DMA correctly rejected unsupported IOMMU/CVM platform; zero DMA issued' "$log"
        grep -Fq 'PCIe NVMe isolated DMA passed:' "$log"
        grep -Fq 'PCIe topology negative cases passed: ACS controls off, ACS absent, legacy bridge alias rejected before BME;' "$log"
        grep -Fq 'PCI isolated DMA passed: 8 real EDU 4096-byte RAM->device->RAM cycles;' "$log"
        grep -Fq 'PCI isolated DMA revocation passed:' "$log"
        grep -Fq 'PCI isolated DMA simultaneous requester isolation passed:' "$log"
        grep -Fq 'PCI multifunction unsupported sibling correctly rejected: real conventional PCI EDU function one lacks PCIe' "$log"
        grep -Fq 'PCI multifunction group reservation passed:' "$log"
        grep -Fq 'PCI multifunction isolated DMA passed: two real NVMe functions, shared requester domain, actual Identify CID/status/vendor/serial/model verified; both controllers stopped and BME off' "$log"
        # Eight actual write-permission faults at the revoked 0x2000 IOVA,
        # from EDU 00:05.0. A marker alone cannot establish DMA revocation.
        python3 - "$log" <<'PYFAULT'
from pathlib import Path
import re
import sys
text = re.sub(r"\x1b\[[0-9;]*[mK]", "", Path(sys.argv[1]).read_text())
faults = re.findall(r"Source identifier: 28, Fault Reason: 5, Fault info: 2,", text)
if len(faults) != 8:
    raise SystemExit(f"Expected 8 real revoked-IOVA VT-d write faults, saw {len(faults)}")
PYFAULT
        if grep -Fq 'PCI device DMA passed:' "$log"; then
            echo 'Unexpected direct-DMA success with IOMMU present' >&2
            exit 1
        fi
    fi
done
