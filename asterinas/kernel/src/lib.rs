// SPDX-License-Identifier: MPL-2.0

//! The assembler crate for the Asterinas kernel.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

// High-level components as external crates.
//
// Keep this list in sync with the high-level component dependencies in `Cargo.toml`.
// Each high-level component crate must have an explicit `extern crate` declaration
// to ensure that its component registration and initialization code are linked into
// the kernel, because the assembler does not otherwise reference symbols from that crate.
extern crate aster_drm as _;
#[cfg(target_arch = "x86_64")]
extern crate aster_i8042 as _;
#[cfg(feature = "nvidia-pci-diagnostics")]
extern crate aster_nvidia_platform as _;
extern crate aster_simpledrm as _;
extern crate aster_uart as _;

#[cfg(feature = "nvidia-nvport-selftest")]
mod nvidia_boot;

#[ostd::main]
fn main() {
    #[cfg(feature = "native-gpu-ci")]
    ostd::early_println!(
        "ASTERINAS_NATIVE_GPU_BOOT commit={} nonce={}",
        env!("ASTER_NATIVE_GPU_CI_COMMIT"),
        env!("ASTER_NATIVE_GPU_CI_NONCE"),
    );
    #[cfg(feature = "nvidia-nvport-selftest")]
    {
        ostd_nvidia_nvport::self_test().expect("NVIDIA NVport CPU memory integration failed");
        ostd::early_println!("NVIDIA NVport CPU memory self-test passed (not a GPU test)");
        nvidia_boot::initialize_firmware_archive();
    }
    aster_core::boot();
}
