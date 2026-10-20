// SPDX-License-Identifier: MPL-2.0

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const NVIDIA_COMMIT: &str = "1a82fbfda8f321b056eb3568850f9316c5e465c5";
const NVIDIA_RELEASE: &str = "615.78.08";

macro_rules! fail {
    ($($arg:tt)*) => {{
        panic!("nvidia-nvport build: {}", format_args!($($arg)*));
    }};
}

fn output(command: &mut Command, what: &str) -> Output {
    let rendered = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|err| fail!("could not run {what} ({rendered}): {err}"));
    if !output.status.success() {
        fail!(
            "{what} failed ({rendered})\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}

fn text_output(command: &mut Command, what: &str) -> String {
    String::from_utf8(output(command, what).stdout)
        .unwrap_or_else(|err| fail!("{what} output is not UTF-8: {err}"))
        .trim()
        .to_owned()
}

fn tool(env_name: &str, default: &str) -> (String, Vec<String>) {
    let spec = env::var(env_name).unwrap_or_else(|_| default.to_owned());
    let mut words = spec.split_whitespace();
    let program = words
        .next()
        .unwrap_or_else(|| fail!("{env_name} is empty"))
        .to_owned();
    (program, words.map(str::to_owned).collect())
}

fn run_tool(program: &(String, Vec<String>), args: &[String], what: &str) {
    let mut command = Command::new(&program.0);
    command.args(&program.1).args(args);
    output(&mut command, what);
}

fn include_dirs(root: &Path) -> Vec<PathBuf> {
    // These are the NVIDIA 615.78.08 src/nvidia/Makefile include roots used
    // for the nvport memory translation unit, plus its checked-in common ABI
    // header root. Keep os-interface.h's actual Unix declaration header first.
    [
        "src/nvidia/arch/nvalloc/unix/include",
        "src/nvidia/arch/nvalloc/common/inc",
        "src/nvidia/inc",
        "src/nvidia/inc/os",
        "src/nvidia/inc/kernel",
        "src/nvidia/inc/libraries",
        "src/nvidia/src/libraries",
        "src/nvidia/src/libraries/nvport/sync",
        "src/common/sdk/nvidia/inc",
        "src/common/inc",
        "kernel-open/common/inc",
    ]
    .into_iter()
    .map(|path| root.join(path))
    .collect()
}

fn main() {
    println!("cargo:rerun-if-env-changed=NVIDIA_SOURCE_DIR");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-env-changed=TARGET");

    let source_root = env::var_os("NVIDIA_SOURCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            fail!("NVIDIA_SOURCE_DIR is required; expected the NVIDIA 615.78.08 source checkout")
        });
    let root = fs::canonicalize(&source_root).unwrap_or_else(|err| {
        fail!(
            "cannot resolve NVIDIA_SOURCE_DIR {}: {err}",
            source_root.display()
        )
    });

    let commit = text_output(
        Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["rev-parse", "HEAD"]),
        "reading NVIDIA source commit",
    );
    if commit != NVIDIA_COMMIT {
        fail!("NVIDIA source commit is {commit}, expected exactly {NVIDIA_COMMIT}");
    }
    let tracked_changes = text_output(
        Command::new("git").arg("-C").arg(&root).args([
            "status",
            "--porcelain",
            "--untracked-files=no",
        ]),
        "checking NVIDIA tracked worktree",
    );
    if !tracked_changes.is_empty() {
        fail!("NVIDIA tracked worktree is dirty:\n{tracked_changes}");
    }
    let readme_path = root.join("README.md");
    let readme = fs::read_to_string(&readme_path)
        .unwrap_or_else(|err| fail!("cannot read {}: {err}", readme_path.display()));
    if !readme.contains(&format!("version {NVIDIA_RELEASE}.")) {
        fail!("README.md does not identify NVIDIA release {NVIDIA_RELEASE}");
    }

    let target = env::var("TARGET").unwrap_or_else(|_| fail!("Cargo TARGET is required"));
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let kernel_abi = target == "x86_64-unknown-none";
    let host_test = arch == "x86_64" && target_os == "linux";
    if !kernel_abi && !host_test {
        fail!(
            "unsupported target {target}; supported: x86_64-unknown-none kernel ABI or x86_64 Linux PIC host tests"
        );
    }
    if kernel_abi && arch != "x86_64" {
        fail!("x86_64-unknown-none requires CARGO_CFG_TARGET_ARCH=x86_64 (got {arch})");
    }

    let out_dir =
        PathBuf::from(env::var_os("OUT_DIR").unwrap_or_else(|| fail!("Cargo OUT_DIR is required")));
    fs::create_dir_all(&out_dir)
        .unwrap_or_else(|err| fail!("cannot create OUT_DIR {}: {err}", out_dir.display()));

    let source = root.join("src/nvidia/src/libraries/nvport/memory/memory_unix_kernel_os.c");
    if !source.is_file() {
        fail!("upstream source file is missing: {}", source.display());
    }
    let probe = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|| fail!("CARGO_MANIFEST_DIR is required to find abi_probe.c")),
    )
    .join("abi_probe.c");
    if !probe.is_file() {
        fail!("ABI probe is missing: {}", probe.display());
    }
    let common_header = root.join("src/common/sdk/nvidia/inc/cpuopsys.h");
    let includes = include_dirs(&root);
    for path in includes.iter().chain([&common_header]) {
        println!("cargo:rerun-if-changed={}", path.display());
        if !path.exists() {
            fail!(
                "required NVIDIA include path is missing: {}",
                path.display()
            );
        }
    }

    let mut flags = vec![
        "-std=gnu11",
        "-Werror",
        "-Werror=implicit-function-declaration",
        "-Werror=incompatible-pointer-types",
        "-Wwrite-strings",
        "-Wundef",
        "-fno-common",
        "-ffunction-sections",
        "-fdata-sections",
        "-ffreestanding",
        "-fno-stack-protector",
        "-fno-builtin",
        "-D_LANGUAGE_C",
        "-D__NO_CTYPE",
        "-DNVRM",
        "-DLOCK_VAL_ENABLED=0",
        "-DPORT_ATOMIC_64_BIT_SUPPORTED=1",
        "-DPORT_IS_KERNEL_BUILD=1",
        "-DPORT_IS_CHECKED_BUILD=0",
        "-DPORT_MODULE_atomic=1",
        "-DPORT_MODULE_core=1",
        "-DPORT_MODULE_cpu=1",
        "-DPORT_MODULE_crypto=1",
        "-DPORT_MODULE_debug=1",
        "-DPORT_MODULE_memory=1",
        "-DPORT_MODULE_safe=1",
        "-DPORT_MODULE_string=1",
        "-DPORT_MODULE_sync=1",
        "-DPORT_MODULE_thread=1",
        "-DPORT_MODULE_time=1",
        "-DPORT_MODULE_util=1",
        "-DPORT_MODULE_example=0",
        "-DPORT_MODULE_mmio=0",
        "-DRS_STANDALONE=0",
        "-DRS_STANDALONE_TEST=0",
        "-DRS_COMPATABILITY_MODE=1",
        "-DRS_PROVIDES_API_STATE=0",
        "-DNV_CONTAINERS_NO_TEMPLATES",
        "-DINCLUDE_NVLINK_LIB",
        "-DINCLUDE_NVSWITCH_LIB",
        "-DNV_PRINTF_STRINGS_ALLOWED=1",
        "-DNV_ASSERT_FAILED_USES_STRINGS=1",
        "-DPORT_ASSERT_FAILED_USES_STRINGS=1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if kernel_abi {
        flags.extend(
            [
                "-fno-pic",
                "-fno-pie",
                "-mno-red-zone",
                "-mcmodel=kernel",
                "-mno-mmx",
                "-mno-sse",
                "-mno-sse2",
                "-mno-3dnow",
                "-msoft-float",
            ]
            .into_iter()
            .map(str::to_owned),
        );
    } else {
        flags.push("-fPIC".to_owned());
    }

    let mut common_args = flags.clone();
    common_args.push("-include".to_owned());
    common_args.push(common_header.display().to_string());
    for include in &includes {
        common_args.push(format!("-I{}", include.display()));
    }

    let (cc, cc_args) = tool("CC", "cc");
    let compiler = (cc, cc_args);
    let object = out_dir.join("memory_unix_kernel_os.o");
    let mut compile_args = common_args.clone();
    compile_args.extend([
        "-c".to_owned(),
        source.display().to_string(),
        "-o".to_owned(),
        object.display().to_string(),
    ]);
    run_tool(
        &compiler,
        &compile_args,
        "compiling unmodified upstream memory_unix_kernel_os.c",
    );

    let firmware_bridge =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("firmware_bridge.c");
    let firmware_object = out_dir.join("firmware_bridge.o");
    let mut firmware_args = common_args.clone();
    firmware_args.extend([
        "-c".to_owned(),
        firmware_bridge.display().to_string(),
        "-o".to_owned(),
        firmware_object.display().to_string(),
    ]);
    run_tool(
        &compiler,
        &firmware_args,
        "compiling NVIDIA firmware selector and ABI checks",
    );
    println!("cargo:rerun-if-changed={}", firmware_bridge.display());

    let mut objects = vec![object.clone(), firmware_object.clone()];
    let mut runtime_sources = vec![
        root.join("src/nvidia/src/libraries/nvport/time/time_unix_kernel_os.c"),
        root.join("src/nvidia/src/libraries/nvport/thread/thread_unix_kernel_os.c"),
    ];
    if kernel_abi {
        runtime_sources.extend([
            root.join("src/nvidia/src/libraries/nvport/memory/memory_tracking.c"),
            root.join("src/nvidia/src/libraries/nvport/sync/sync_unix_kernel_os.c"),
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("sync_bridge.c"),
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("diagnostics_format.c"),
        ]);
    }
    for runtime_source in runtime_sources {
        let filename = runtime_source.file_stem().unwrap().to_str().unwrap();
        let runtime_object = out_dir.join(format!("{filename}.o"));
        let mut args = common_args.clone();
        args.extend([
            "-c".to_owned(),
            runtime_source.display().to_string(),
            "-o".to_owned(),
            runtime_object.display().to_string(),
        ]);
        run_tool(
            &compiler,
            &args,
            &format!("compiling {}", runtime_source.display()),
        );
        println!("cargo:rerun-if-changed={}", runtime_source.display());
        objects.push(runtime_object);
    }

    let mut probe_args = common_args;
    probe_args.extend(["-fsyntax-only".to_owned(), probe.display().to_string()]);
    run_tool(
        &compiler,
        &probe_args,
        "checking NVIDIA C ABI declarations and signatures",
    );

    let archive = out_dir.join("libnvidia_nvport_memory.a");
    let (ar, ar_args) = tool("AR", "ar");
    let archiver = (ar, ar_args);
    let mut archive_args = vec!["crs".to_owned(), archive.display().to_string()];
    archive_args.extend(objects.iter().map(|object| object.display().to_string()));
    run_tool(&archiver, &archive_args, "creating nvport static archive");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=nvidia_nvport_memory");
    println!("cargo:rerun-if-changed={}", probe.display());
    println!("cargo:rerun-if-changed={}", source.display());
    println!(
        "cargo:rerun-if-changed={}",
        root.join("src/nvidia/arch/nvalloc/unix/include/os-interface.h")
            .display()
    );
    println!("cargo:rerun-if-changed={}", readme_path.display());
    println!("cargo:metadata=nvidia_commit={NVIDIA_COMMIT}");
    println!("cargo:metadata=nvidia_release={NVIDIA_RELEASE}");
}
