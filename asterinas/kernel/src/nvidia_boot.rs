// SPDX-License-Identifier: MPL-2.0
//! Real RM firmware file service backed exclusively by the guest boot archive.

use ostd_nvidia_nvport::firmware::{self, ChipFamily, FirmwareError, FirmwareKind, FirmwareLease};

pub(super) fn initialize_firmware_archive() {
    let Some(bytes) = ostd::boot::boot_info().initramfs else {
        ostd::early_println!(
            "NVIDIA RM firmware unavailable: no guest boot archive (GSP not started)"
        );
        return;
    };
    if let Err(error) = firmware::install_boot_archive(bytes) {
        ostd::early_println!(
            "NVIDIA RM firmware archive rejected: {:?} (GSP not started)",
            error
        );
        return;
    }
    let mut acquired = 0;
    // These are file-family inventories only. Real RM must select the hardware
    // family, validate firmware, establish GPU DMA and execute the GSP handshake.
    for family in [ChipFamily::Tu10x, ChipFamily::Ga10x] {
        for kind in [
            FirmwareKind::Gsp,
            FirmwareKind::GspLog,
            FirmwareKind::Ucodes,
        ] {
            match FirmwareLease::acquire(kind, family) {
                Ok(lease) => {
                    // CPU-only readback evidence, not signature validation or a
                    // GPU workload. The recorded host SHA-256 remains separate.
                    let checksum = lease
                        .as_bytes()
                        .iter()
                        .fold(0xcbf29ce484222325u64, |hash, byte| {
                            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
                        });
                    ostd::early_println!(
                        "NVIDIA RM firmware acquired from guest boot archive: {} bytes={} cpu_fnv1a64={:016x} (GSP not started)",
                        firmware::filename(kind, family),
                        lease.as_bytes().len(),
                        checksum,
                    );
                    acquired += 1;
                    // The owning handle is genuinely released here.
                }
                Err(FirmwareError::NotFound) => {}
                Err(error) => ostd::early_println!(
                    "NVIDIA RM firmware acquisition rejected: {}: {:?} (GSP not started)",
                    firmware::filename(kind, family),
                    error,
                ),
            }
        }
    }
    ostd::early_println!(
        "NVIDIA RM firmware file service: {} images acquired/released; GPU initialization not attempted",
        acquired,
    );
}
