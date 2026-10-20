#!/usr/bin/env python3
"""Test real GPUI layer surfaces on nested Niri; never certifies GPU hardware."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


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
            raise RuntimeError(f"Process exited before readiness: {process.returncode}")
        time.sleep(0.2)
    raise TimeoutError(f"Readiness timed out after {seconds} seconds")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--niri", type=Path, required=True)
    parser.add_argument("--gpui", type=Path, required=True)
    parser.add_argument("--mesa", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--library-path", type=Path)
    args = parser.parse_args()
    args.niri = args.niri.resolve()
    args.gpui = args.gpui.resolve()
    args.mesa = args.mesa.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    report = {"platform": "Linux nested X11/Wayland", "hardware_acceleration_verified": False}
    xvfb = compositor = shell = None
    try:
        with tempfile.TemporaryDirectory(prefix="gpui-nested-") as temporary:
            base = Path(temporary)
            runtime = base / "runtime"
            runtime.mkdir(mode=0o700)
            env = dict(os.environ)
            for key in ("WAYLAND_DISPLAY", "NIRI_SOCKET", "NIRI_SOCKET_PATH", "HYPRLAND_INSTANCE_SIGNATURE", "MANGO_SOCKET"):
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
                "RUST_BACKTRACE": "1",
                "RUST_LOG": "info",
            })
            if args.library_path:
                env["LD_LIBRARY_PATH"] = args.library_path.read_text().strip()
            vendors = list((args.mesa / "share/glvnd/egl_vendor.d").glob("*.json"))
            icds = list((args.mesa / "share/vulkan/icd.d").glob("*lvp*.json"))
            if len(vendors) != 1 or len(icds) != 1:
                raise RuntimeError(f"Expected one Mesa EGL vendor and one lavapipe ICD, got {vendors}, {icds}")
            env["__EGL_VENDOR_LIBRARY_FILENAMES"] = str(vendors[0])
            env["VK_DRIVER_FILES"] = str(icds[0])
            env["VK_ICD_FILENAMES"] = str(icds[0])
            report["vulkan_icd"] = str(icds[0])
            with (args.output / "xvfb.log").open("w") as xvfb_log, (args.output / "niri.log").open("w") as niri_log, (args.output / "gpui.log").open("w") as gpui_log:
                display_file = base / "display"
                with display_file.open("w") as display_out:
                    xvfb = subprocess.Popen(["Xvfb", "-displayfd", str(display_out.fileno()), "-screen", "0", "1280x800x24", "-nolisten", "tcp"], pass_fds=(display_out.fileno(),), stdout=xvfb_log, stderr=subprocess.STDOUT)
                display = wait_for(lambda: display_file.read_text().strip(), xvfb)
                env["DISPLAY"] = ":" + display
                config = base / "niri.kdl"
                config.write_text('animations { off; }\nhotkey-overlay { skip-at-startup; }\n')
                compositor = subprocess.Popen([str(args.niri), "--config", str(config)], env=env, stdout=niri_log, stderr=subprocess.STDOUT)
                wayland = wait_for(lambda: next((p for p in runtime.glob("wayland-*") if p.is_socket()), None), compositor)
                niri_socket = wait_for(lambda: next((p for p in runtime.glob("niri.*.sock") if p.is_socket()), None), compositor)
                env["WAYLAND_DISPLAY"] = wayland.name
                env["NIRI_SOCKET"] = str(niri_socket)
                def query(name):
                    result = subprocess.run([str(args.niri), "msg", "--json", name], env=env, capture_output=True, text=True, timeout=10)
                    if result.returncode:
                        raise RuntimeError(result.stderr.strip())
                    return json.loads(result.stdout)
                report["outputs"] = query("outputs")
                info = subprocess.run(["wayland-info"], env=env, capture_output=True, text=True, timeout=15)
                (args.output / "wayland-info.txt").write_text(info.stdout + info.stderr)
                if info.returncode or "zwlr_layer_shell_v1" not in info.stdout:
                    raise RuntimeError("Niri did not expose layer-shell")
                shell = subprocess.Popen([str(args.gpui)], env=env, stdout=gpui_log, stderr=subprocess.STDOUT)
                def find_bar():
                    layers = query("layers")
                    (args.output / "layers.json").write_text(json.dumps(layers, indent=2) + "\n")
                    return layers if '"namespace": "bar"' in json.dumps(layers) else None
                report["layers"] = wait_for(find_bar, shell, seconds=45)
                time.sleep(5)
                if shell.poll() is not None:
                    raise RuntimeError(f"GPUI exited after creating its bar: {shell.returncode}")
                screenshot = args.output / "gpui-nested.png"
                capture = subprocess.run([str(args.niri), "msg", "action", "screenshot-screen", "--show-pointer", "false", "--path", str(screenshot)], env=env, capture_output=True, text=True, timeout=15)
                if capture.returncode:
                    raise RuntimeError("Output capture failed: " + capture.stderr)
                wait_for(lambda: screenshot.is_file() and screenshot.stat().st_size > 0, compositor, seconds=10)
                report["result"] = "PASS: real GPUI bar layer registered with nested Niri and process stayed alive for 5 seconds"
                report["screenshot"] = str(screenshot)
                report["limitations"] = "Software Vulkan only; no Asterinas kernel, physical GPU, modesetting, input interaction, or full desktop services validated."
    except Exception as error:
        report["result"] = "FAIL"
        report["error"] = str(error)
    finally:
        stop(shell)
        stop(compositor)
        stop(xvfb)
        (args.output / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report.get("result", "").startswith("PASS") else 1


if __name__ == "__main__":
    raise SystemExit(main())
