#!/usr/bin/env python3
"""Real nested integration smoke; not a physical scanout/performance benchmark.

Requires a built gpuishell, Niri, Xvfb, wayland-info and software Mesa.
No mocked compositor, synthetic presentation events or user session is used.
Artifacts remain in --output. No screenshots or host session are required.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[1]


def stop(process):
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def wait_for(probe, process, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = probe()
        if result:
            return result
        if process.poll() is not None:
            raise RuntimeError(f"process exited before readiness: {process.returncode}")
        time.sleep(0.2)
    raise TimeoutError(f"readiness timed out after {seconds} seconds")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--niri", type=Path, required=True)
    parser.add_argument("--gpui", type=Path, default=ROOT / "target/debug/gpuishell")
    parser.add_argument("--mesa", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mode", choices=("passive", "probe"), required=True)
    parser.add_argument("--library-path", type=Path, help="file containing Niri/GPUI runtime library path")
    args = parser.parse_args()
    for name in ("niri", "gpui", "mesa", "output"):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    report = {
        "result": "FAIL", "mode": args.mode,
        "platform": "Linux / Xvfb / nested Niri / software Mesa",
        "physical_presentation_fps": None,
        "hardware_acceleration_verified": False,
        "host_monitor_scanout_verified": False,
        "limitations": "Nested compositor capabilities and client buffer commits are not host monitor scanout or Asterinas/Linux performance parity.",
    }
    xvfb = compositor = shell = None
    try:
        with tempfile.TemporaryDirectory(prefix="gpui-boundary-") as temporary:
            base = Path(temporary)
            runtime = base / "runtime"
            runtime.mkdir(mode=0o700)
            env = dict(os.environ)
            for key in (
                "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "WAYLAND_DEBUG", "NIRI_SOCKET",
                "NIRI_SOCKET_PATH", "HYPRLAND_INSTANCE_SIGNATURE", "MANGO_SOCKET",
                "MANGO_SOCKET_PATH", "GPUI_SHELL_FRAME_TELEMETRY",
                "GPUI_SHELL_FRAME_PROBE_SECONDS",
            ):
                env.pop(key, None)
            env.update({
                "XDG_RUNTIME_DIR": str(runtime),
                "XDG_CONFIG_HOME": str(base / "config"),
                "XDG_CACHE_HOME": str(base / "cache"),
                "XDG_DATA_HOME": str(base / "data"),
                "XDG_STATE_HOME": str(base / "state"),
                "XDG_CURRENT_DESKTOP": "niri",
                "LIBGL_ALWAYS_SOFTWARE": "1",
                "PULSE_SERVER": "unix:" + str(runtime / "absent-pulse.sock"),
                "PATH": str(args.niri.parent) + os.pathsep + env["PATH"],
                "RUST_BACKTRACE": "1", "RUST_LOG": "info",
            })
            if args.library_path:
                env["LD_LIBRARY_PATH"] = args.library_path.read_text().strip()
            vendors = list((args.mesa / "share/glvnd/egl_vendor.d").glob("*.json"))
            icds = list((args.mesa / "share/vulkan/icd.d").glob("*lvp*.json"))
            if len(vendors) != 1 or len(icds) != 1:
                raise RuntimeError("expected exactly one Mesa EGL vendor and lavapipe ICD")
            env["__EGL_VENDOR_LIBRARY_FILENAMES"] = str(vendors[0])
            env["VK_DRIVER_FILES"] = env["VK_ICD_FILENAMES"] = str(icds[0])
            with (args.output / "xvfb.log").open("w") as xvfb_log, \
                 (args.output / "niri.log").open("w") as niri_log, \
                 (args.output / "gpui.log").open("w") as gpui_log:
                display_file = base / "display"
                with display_file.open("w") as display_out:
                    host_env = dict(os.environ)
                    # System tools use the system loader; Nix-built Niri/GPUI
                    # retain their separate library environment below.
                    host_env.pop("LD_LIBRARY_PATH", None)
                    host_env.pop("LD_PRELOAD", None)
                    xvfb = subprocess.Popen([
                        "Xvfb", "-displayfd", str(display_out.fileno()), "-screen", "0",
                        "1280x800x24", "-nolisten", "tcp",
                    ], env=host_env, pass_fds=(display_out.fileno(),), stdout=xvfb_log, stderr=subprocess.STDOUT)
                env["DISPLAY"] = ":" + wait_for(lambda: display_file.read_text().strip(), xvfb)
                config = base / "niri.kdl"
                config.write_text('animations { off; }\nhotkey-overlay { skip-at-startup; }\n')
                compositor = subprocess.Popen([
                    str(args.niri), "--config", str(config),
                ], env=env, stdout=niri_log, stderr=subprocess.STDOUT)
                wayland = wait_for(lambda: next((p for p in runtime.glob("wayland-*") if p.is_socket()), None), compositor)
                niri_socket = wait_for(lambda: next((p for p in runtime.glob("niri.*.sock") if p.is_socket()), None), compositor)
                env["WAYLAND_DISPLAY"] = wayland.name
                env["NIRI_SOCKET"] = str(niri_socket)

                def query(name):
                    result = subprocess.run([
                        str(args.niri), "msg", "--json", name,
                    ], env=env, capture_output=True, text=True, timeout=10)
                    if result.returncode:
                        raise RuntimeError(result.stderr.strip())
                    return json.loads(result.stdout)

                info_env = dict(env)
                info_env.pop("LD_LIBRARY_PATH", None)
                info_env.pop("LD_PRELOAD", None)
                info = subprocess.run(["wayland-info"], env=info_env, capture_output=True, text=True, timeout=15)
                (args.output / "wayland-info.txt").write_text(info.stdout + info.stderr)
                if info.returncode or "zwlr_layer_shell_v1" not in info.stdout:
                    raise RuntimeError("nested compositor did not expose layer-shell")
                report["presentation_global_advertised"] = "wp_presentation" in info.stdout
                report["outputs"] = query("outputs")
                env["WAYLAND_DEBUG"] = "client"
                if args.mode == "probe":
                    env["GPUI_SHELL_FRAME_PROBE_SECONDS"] = "2"
                else:
                    env["GPUI_SHELL_FRAME_TELEMETRY"] = "1"
                shell = subprocess.Popen([str(args.gpui)], env=env, stdout=gpui_log, stderr=subprocess.STDOUT)
                wait_for(lambda: any(layer.get("namespace") == "bar" for layer in query("layers")), shell, seconds=45)
                time.sleep(5)
                if shell.poll() is not None:
                    raise RuntimeError(f"shell exited after mapping bar: {shell.returncode}")
                layers = query("layers")
                bars = [layer for layer in layers if layer.get("namespace") == "bar"]
                if not bars or any(layer.get("layer") != "Top" or layer.get("keyboard_interactivity") != "None" for layer in bars):
                    raise RuntimeError(f"incorrect settled bar layer: {bars}")
                report["layers"] = layers
                stop(shell)
                gpui_log.flush()
            text = (args.output / "gpui.log").read_text()
            if "panicked at" in text:
                raise RuntimeError("shell panicked")
            surfaces = set()
            for line in text.splitlines():
                if ".get_layer_surface(" in line and '"bar"' in line:
                    surfaces.update(re.findall(r"wl_surface[@#](\d+)", line))
            if not surfaces:
                raise RuntimeError("trace did not identify the actual bar wl_surface")
            activity = {}
            for surface in sorted(surfaces):
                prefix = rf"wl_surface[@#]{surface}\."
                activity[surface] = {
                    "attach_requests": len(re.findall(prefix + r"attach\(", text)),
                    "commit_requests": len(re.findall(prefix + r"commit\(", text)),
                }
            if any(item["attach_requests"] == 0 or item["commit_requests"] == 0 for item in activity.values()):
                raise RuntimeError("bar trace has no real buffer attach/commit activity")
            report["bar_surface_trace_activity"] = activity
            feedback_requests = len(re.findall(r"wp_presentation[@#]\d+\.feedback\(", text))
            report["feedback_requests"] = feedback_requests
            if feedback_requests:
                raise RuntimeError("unexpected feedback requests: re-audit upstream hook before interpreting")
            renderers = [json.loads(line.split("GPUI_SHELL_RENDERER ", 1)[1])
                         for line in text.splitlines() if "GPUI_SHELL_RENDERER " in line]
            if not renderers or any(r.get("presentation_feedback_status") != "unavailable_locked_gpui"
                                    or r.get("host_monitor_scanout_verified") is not False for r in renderers):
                raise RuntimeError("missing/incorrect unavailable-presentation diagnostic")
            analysis = subprocess.run([
                "python3", str(ROOT / "scripts/analyze_frame_telemetry.py"),
                str(args.output / "gpui.log"), "--require-physical-gpu",
            ], capture_output=True, text=True, timeout=15)
            (args.output / "analysis.json").write_text(analysis.stdout)
            (args.output / "analysis.stderr").write_text(analysis.stderr)
            if analysis.returncode != 3:
                raise RuntimeError(f"expected strict physical rejection (3), got {analysis.returncode}: {analysis.stderr}")
            summary = json.loads(analysis.stdout)
            if summary["physical_gpu_certification"]["classification"] != "software_cpu":
                raise RuntimeError("forced software Vulkan was not classified as CPU")
            observations = summary["telemetry"]["observations"]
            if args.mode == "probe" and not any(o["probe_finished"] for o in observations):
                raise RuntimeError("bounded redraw probe did not observe its deadline")
            expected_mode = "bounded_redraw_probe" if args.mode == "probe" else "passive"
            if any(o["mode"] != expected_mode for o in observations):
                raise RuntimeError("incorrect telemetry mode")
            report["observation_count"] = len(observations)
            report["renderer_classification"] = "software_cpu"
            report["strict_physical_rejection_exit"] = analysis.returncode
            report["result"] = "PASS"
    except Exception as error:
        report["error"] = str(error)
    finally:
        stop(shell)
        stop(compositor)
        stop(xvfb)
        (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["result"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
