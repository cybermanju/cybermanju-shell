# Graphics CI and NVIDIA Vulkan benchmark

The workflow is [`.github/workflows/graphics_gpu.yml`](../../.github/workflows/graphics_gpu.yml). It deliberately separates ordinary source validation from access to a real GPU:

- `pull_request` and `push` run unit tests and build the Vulkan benchmark and SPIR-V shader on GitHub-hosted `ubuntu-latest`. A tiny runtime selection test checks rejection when no eligible GPU is present; the job does not request GPU hardware and is not a hardware acceptance run.
- The NVIDIA benchmark is available only through `workflow_dispatch`, and only when dispatched against the trusted `main` branch. Its job checks out the exact dispatch commit (`github.sha`) and is restricted to runners labeled `[self-hosted, linux, x64, nvidia-gpu]`. There are no PR-triggered GPU jobs or user-provided ref inputs.

## Enabling the GPU job safely

Repository administrators must create and protect the GitHub Actions environment named **`gpu-benchmark`** before using the manual benchmark. Configure required reviewers and deployment-branch restrictions for `main` in the environment settings; also protect `main` with the repository's normal review controls. A dispatch from any ref other than `main` is skipped by the workflow, and the checked-out source is fixed to that dispatch's commit rather than a moving branch tip.

Use an isolated, **ephemeral** self-hosted runner with the exact labels above. Self-hosted runners can retain credentials, files, processes, or other state between jobs; a GPU label or environment approval alone does not make an unsafe persistent runner trustworthy. Do not expose this runner to pull-request jobs or unreviewed workflow changes. Rebuild or destroy it after each job, keep its privileges minimal, and restrict who can dispatch workflows and approve the environment.

The GPU runner must provide a real NVIDIA GPU and working NVIDIA driver/Vulkan implementation, C compiler (`cc`), Vulkan development headers and loader (`libvulkan-dev`/`libvulkan1` equivalents), `glslangValidator`, and GNU `timeout`. Python 3 is required on the hosted CI runner for unit tests. The hosted job installs `build-essential`, `libvulkan-dev`, and `glslang-tools`; the self-hosted GPU runner is expected to have its prerequisites installed already. Missing tools or hardware are failures, not successful skips.

The GPU job compiles `tools/graphics/vulkan_compute_bench.c`, builds `tools/graphics/vector.comp` for Vulkan 1.2, and invokes the benchmark with `--shader tools/graphics/vector.spv --vendor nvidia`, bounded by a five-minute timeout. Its console output is uploaded as a run artifact even if build or benchmark execution fails (when artifact upload itself remains available).

## Scope and interpretation

This is an opt-in **Vulkan compute benchmark** on an NVIDIA-capable host. It is separate from GPUI's Wayland session, compositor/layer-shell, rendering, or display integration checks. It does not boot Asterinas, execute the benchmark on Asterinas, implement a GPU kernel driver, or establish native Asterinas GPU support. Neither a passing hosted compile nor a passing benchmark is an **Asterinas native pass**; benchmark evidence only describes the configured host and selected Vulkan device.

## Local run and acceptance criteria

From the repository root, with the same build prerequisites as CI:

```sh
sh tools/graphics/run_vulkan_compute_bench.sh --vendor nvidia
# Separate AMD host test; this does not port AMDGPU to Asterinas:
sh tools/graphics/run_vulkan_compute_bench.sh --vendor amd
```

The temporary runner builds both source and shader and removes its build directory on exit. The default workload has 1,048,576 output integers and ten dispatches. `--count` and `--iterations` are bounded; the GPU fence wait is limited to 30 seconds. The manual CI adds a five-minute process timeout. A hung driver call outside the fence wait can still require an external process timeout; this tool cannot guarantee recovery of a wedged GPU or kernel.

Success requires an integrated/discrete Vulkan 1.2 device, compute timestamps, suitable memory and workgroup limits, successful submission/fence completion, positive GPU/host timings, and equality of every final output with the independently computed expected value. CPU, virtual and OTHER device types are rejected. NVIDIA additionally requires its vendor ID and `VK_DRIVER_ID_NVIDIA_PROPRIETARY`; that identifies NVIDIA's Vulkan userspace driver, including installations using NVIDIA's open kernel modules. It does not identify which kernel-module flavor is loaded. Results include device, vendor, driver information, host OS, dispatch/output counts and measured nanoseconds; errors exit nonzero without a passing JSON record. Output storage is initialized to the complement of the expected values before dispatch to reject unchanged/stale data. Readback uses a shader-write-to-host-read barrier, fence wait and noncoherent invalidation following the [Khronos synchronization guide](https://docs.vulkan.org/guide/latest/synchronization_examples.html).

This small deterministic workload is a correctness/timing baseline, not a full GPU stress suite, throughput rating, per-dispatch readback, compositor frame benchmark or proof of absence of software anywhere in the desktop stack. It verifies the final output after all dispatches, not separate readback from each dispatch. Driver-reported device metadata is not independent hardware attestation.

For the source-reuse target, NVIDIA currently publishes [615.78.08](https://github.com/NVIDIA/open-gpu-kernel-modules/releases/tag/615.78.08); its [README](https://github.com/NVIDIA/open-gpu-kernel-modules) requires corresponding GSP firmware and userspace components. This benchmark reports the installed driver but does not install, upgrade, or enforce that particular version. Installing Linux packages does not make them native Asterinas drivers.

The actions are pinned to immutable upstream commits: [`actions/checkout` v4.2.2 (`11bd719…`)](https://github.com/actions/checkout/commit/11bd71901bbe5b1630ceea73d27597364c9af683) and [`actions/upload-artifact` v4.6.2 (`ea165f8…`)](https://github.com/actions/upload-artifact/commit/ea165f8d65b6e75b540449e92b4886f43607fa02).
