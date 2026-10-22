// SPDX-License-Identifier: MPL-2.0
use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=CYBERMANJU_ABI_CC");
    println!("cargo:rerun-if-env-changed=CYBERMANJU_ABI_AR");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_KTEST");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_OS");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-changed=memcmp_c_probe.c");

    let ktest = env::var_os("CARGO_CFG_KTEST").is_some();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();
    if !(ktest && arch == "x86_64" && target == "x86_64-unknown-none") {
        return;
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR missing"));
    let object = out.join("memcmp_c_probe.o");
    let archive = out.join("libcybermanju_memcmp_probe.a");
    let cc = env::var("CYBERMANJU_ABI_CC").unwrap_or_else(|_| "cc".into());
    let ar = env::var("CYBERMANJU_ABI_AR").unwrap_or_else(|_| "ar".into());
    let flags = [
        "-std=gnu11",
        "-O2",
        "-ffreestanding",
        "-fno-builtin",
        "-fno-stack-protector",
        "-fno-pic",
        "-fno-pie",
        "-mcmodel=kernel",
        "-mno-red-zone",
        "-mno-sse",
        "-mno-sse2",
        "-mno-mmx",
        "-msoft-float",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-c",
        "memcmp_c_probe.c",
    ];
    let command_text = format!(
        "{} {} -o {}\n{} crs {} {}\n",
        cc,
        flags.join(" "),
        object.display(),
        ar,
        archive.display(),
        object.display()
    );
    std::fs::write(out.join("cmp-probe.command.txt"), command_text).expect("write command record");

    let status = Command::new(&cc)
        .args(&flags)
        .arg("-o")
        .arg(&object)
        .status()
        .expect("failed to execute C compiler");
    assert!(status.success(), "C probe compilation failed");
    let status = Command::new(&ar)
        .arg("crs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("failed to execute archiver");
    assert!(status.success(), "C probe archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=cybermanju_memcmp_probe");
}
