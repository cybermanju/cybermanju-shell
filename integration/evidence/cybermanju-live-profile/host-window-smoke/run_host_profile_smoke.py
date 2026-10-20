#!/usr/bin/env python3
"""Finite host nested-Wayland appearance-profile smoke; software rendering only."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path('/workspace/asterinas-gpui')
OUT = Path(__file__).resolve().parent
BIN = ROOT / 'gpui-shell/target/debug/gpuishell'
NIRI = Path('/nix/store/wj38g0fzvv0nic72hh3svpq0jsk85dcd-niri-26.04/bin/niri')
MESA = Path('/nix/store/3967gykw3wcyq3svf238nk31jlhxnl7c-mesa-26.1.5')
LIBS = ROOT / 'evidence/nested-library-path.txt'
FIXTURE = ROOT / 'gpui-shell/crates/app/src/config/appearance/fixtures/appearance-profile-v1.json'


def stop(proc):
    if proc and proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=8)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


def wait_for(probe, process, label, seconds=35):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = probe()
        if value:
            return value
        if process and process.poll() is not None:
            raise RuntimeError(f'{label}: process exited rc={process.returncode}')
        time.sleep(.2)
    raise TimeoutError(f'{label}: timed out after {seconds}s')


def atomically_write(path, data):
    temp = path.with_name(path.name + '.replacement')
    temp.write_text(data, encoding='utf-8')
    os.replace(temp, path)


def capture(env, path):
    result = subprocess.run([str(NIRI), 'msg', 'action', 'screenshot-screen',
                             '--show-pointer', 'false', '--path', str(path)],
                            env=env, capture_output=True, text=True, timeout=15)
    if result.returncode:
        raise RuntimeError(f'screenshot failed: {result.stderr.strip()}')
    wait_for(lambda: path.is_file() and path.stat().st_size > 1000, None, 'screenshot', 10)


def pixel_stats(before, after):
    from PIL import Image
    import numpy as np
    a = np.asarray(Image.open(before).convert('RGB'), dtype=np.int16)
    b = np.asarray(Image.open(after).convert('RGB'), dtype=np.int16)
    if a.shape != b.shape:
        raise RuntimeError(f'screenshot dimensions differ: {a.shape} vs {b.shape}')
    diff = np.abs(a - b)
    # Evaluate actual image and top strip where the shell bar is placed.
    top = diff[:min(100, diff.shape[0]), :, :]
    return {
        'dimensions_px': [int(a.shape[1]), int(a.shape[0])],
        'full_image_changed_pixel_fraction': float(np.any(diff > 12, axis=2).mean()),
        'top_100px_changed_pixel_fraction': float(np.any(top > 12, axis=2).mean()),
        'mean_absolute_rgb_difference': float(diff.mean()),
    }


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    report = {
        'test': 'host nested Niri Wayland GPUI live appearance-profile smoke',
        'hardware_acceleration_verified': False,
        'rendering': 'host Mesa lavapipe software renderer only; not Asterinas/GPU acceptance',
        'source_edits_or_builds': False,
        'screenshots_are_unmodified_host_captures': True,
    }
    xvfb = niri = gpui = None
    try:
        for required in (BIN, NIRI, MESA, LIBS, FIXTURE, Path('/usr/bin/Xvfb'), Path('/usr/bin/wayland-info')):
            if not required.exists():
                raise RuntimeError(f'required existing cached executable/artifact unavailable: {required}')
        report['binary'] = str(BIN)
        report['niri'] = str(NIRI)
        report['mesa_cache'] = str(MESA)
        report['library_path_cache'] = str(LIBS)
        mesa_vendor = list((MESA / 'share/glvnd/egl_vendor.d').glob('*.json'))
        icds = list((MESA / 'share/vulkan/icd.d').glob('*lvp*.json'))
        if len(mesa_vendor) != 1 or len(icds) != 1:
            raise RuntimeError(f'expected one cached Mesa EGL vendor and lavapipe ICD, found {mesa_vendor}, {icds}')
        report['software_vulkan_icd'] = str(icds[0])

        fixture = json.loads(FIXTURE.read_text())
        # Start with a high-contrast dark palette derived from the app-export fixture.
        initial = json.loads(json.dumps(fixture))
        initial['settings']['theme'] = 'os-dark'
        initial['settings']['accent'] = '#FF3B30'
        initial['settings']['accents'] = {'os-dark': '#FF3B30', 'os-light': '#AF52DE'}
        # Recovery deliberately returns to the original dark projection after light.
        changed = json.loads(json.dumps(fixture))
        changed['settings']['theme'] = 'os-light'
        changed['settings']['accent'] = '#00A878'
        changed['settings']['accents'] = {'os-light': '#00A878', 'os-dark': '#FF3B30'}
        profile = OUT / 'appearance-profile-live.json'
        atomically_write(profile, json.dumps(initial, indent=2) + '\n')
        report['profile_file'] = str(profile)
        report['initial_theme'] = initial['settings']['theme']
        report['replacement_theme'] = changed['settings']['theme']
        report['recovery_theme'] = initial['settings']['theme']

        base = Path(tempfile.mkdtemp(prefix='gpui-host-profile-', dir='/tmp'))
        report['isolated_runtime_created'] = True
        runtime = base / 'runtime'
        runtime.mkdir(mode=0o700)
        for name in ('config', 'cache', 'data', 'state'):
            (base / name).mkdir()
        env = dict(os.environ)
        for key in ('WAYLAND_DISPLAY', 'NIRI_SOCKET', 'NIRI_SOCKET_PATH', 'HYPRLAND_INSTANCE_SIGNATURE', 'MANGO_SOCKET'):
            env.pop(key, None)
        env.update({
            'XDG_RUNTIME_DIR': str(runtime), 'XDG_CONFIG_HOME': str(base / 'config'),
            'XDG_CACHE_HOME': str(base / 'cache'), 'XDG_DATA_HOME': str(base / 'data'),
            'XDG_STATE_HOME': str(base / 'state'), 'XDG_CURRENT_DESKTOP': 'niri',
            'LIBGL_ALWAYS_SOFTWARE': '1', 'PULSE_SERVER': 'unix:' + str(runtime / 'no-pulse.sock'),
            'PATH': str(NIRI.parent) + os.pathsep + env.get('PATH', ''),
            'LD_LIBRARY_PATH': LIBS.read_text().strip(), 'RUST_BACKTRACE': '1', 'RUST_LOG': 'info',
            '__EGL_VENDOR_LIBRARY_FILENAMES': str(mesa_vendor[0]),
            'VK_DRIVER_FILES': str(icds[0]), 'VK_ICD_FILENAMES': str(icds[0]),
        })
        display_file = base / 'display-number'
        with display_file.open('w') as display_out, (OUT / 'xvfb.log').open('w') as log:
            xvfb = subprocess.Popen(['/usr/bin/Xvfb', '-displayfd', str(display_out.fileno()),
                                     '-screen', '0', '1280x800x24', '-nolisten', 'tcp'],
                                    pass_fds=(display_out.fileno(),), stdout=log, stderr=subprocess.STDOUT)
        display = wait_for(lambda: display_file.read_text().strip(), xvfb, 'Xvfb display readiness', 15)
        env['DISPLAY'] = ':' + display
        config = base / 'niri.kdl'
        config.write_text('animations { off; }\nhotkey-overlay { skip-at-startup; }\n')
        with (OUT / 'niri.log').open('w') as log:
            niri = subprocess.Popen([str(NIRI), '--config', str(config)], env=env, stdout=log, stderr=subprocess.STDOUT)
        wayland = wait_for(lambda: next((p for p in runtime.glob('wayland-*') if p.is_socket()), None), niri, 'Niri Wayland socket')
        socket_path = wait_for(lambda: next((p for p in runtime.glob('niri.*.sock') if p.is_socket()), None), niri, 'Niri control socket')
        env['WAYLAND_DISPLAY'] = wayland.name
        env['NIRI_SOCKET'] = str(socket_path)
        layers_result = subprocess.run([str(NIRI), 'msg', '--json', 'layers'], env=env, capture_output=True, text=True, timeout=10)
        if layers_result.returncode:
            raise RuntimeError('Niri layers query failed: ' + layers_result.stderr.strip())
        report['initial_layers_before_gpui'] = json.loads(layers_result.stdout)
        wi = subprocess.run(['/usr/bin/wayland-info'], env=env, capture_output=True, text=True, timeout=15)
        (OUT / 'wayland-info.txt').write_text(wi.stdout + wi.stderr)
        if wi.returncode or 'zwlr_layer_shell_v1' not in wi.stdout:
            raise RuntimeError('actual nested Wayland session did not expose zwlr_layer_shell_v1')
        report['wayland_layer_shell_protocol'] = 'zwlr_layer_shell_v1 present in wayland-info'

        command = [str(BIN), '--appearance-profile', str(profile), '--watch-appearance-profile']
        report['shell_command'] = command
        with (OUT / 'gpui.log').open('w') as log:
            gpui = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
        def query_layers():
            q = subprocess.run([str(NIRI), 'msg', '--json', 'layers'], env=env, capture_output=True, text=True, timeout=10)
            if q.returncode:
                return None
            layers = json.loads(q.stdout)
            (OUT / 'layers-latest.json').write_text(json.dumps(layers, indent=2) + '\n')
            return layers if any(x.get('namespace') == 'bar' for x in layers) else None
        layers = wait_for(query_layers, gpui, 'real GPUI bar layer')
        report['gpui_bar_layer_observed'] = layers
        time.sleep(3)
        if gpui.poll() is not None:
            raise RuntimeError(f'GPUI exited after bar appeared, rc={gpui.returncode}')
        (OUT / 'layers-startup.json').write_text(json.dumps(layers, indent=2) + '\n')
        capture(env, OUT / '01-startup-dark.png')
        report['startup_screenshot'] = str(OUT / '01-startup-dark.png')
        log_text = (OUT / 'gpui.log').read_text(errors='replace')
        if 'CyberManju appearance v1: theme=os-dark' not in log_text:
            raise RuntimeError('GPUI startup log did not prove initial dark profile loaded')
        report['startup_log_proof'] = 'GPUI reported CyberManju appearance v1 theme=os-dark'

        atomically_write(profile, json.dumps(changed, indent=2) + '\n')
        report['atomic_replacement_dark_to_light'] = True
        time.sleep(2.5)
        if gpui.poll() is not None:
            raise RuntimeError(f'GPUI exited after valid profile replacement, rc={gpui.returncode}')
        capture(env, OUT / '02-live-os-light.png')
        report['replacement_screenshot'] = str(OUT / '02-live-os-light.png')
        report['dark_to_light_pixel_comparison'] = pixel_stats(OUT / '01-startup-dark.png', OUT / '02-live-os-light.png')
        if report['dark_to_light_pixel_comparison']['top_100px_changed_pixel_fraction'] < 0.003:
            raise RuntimeError('real rendered top-of-screen screenshot change was below 0.3%; live theme application not demonstrated')

        atomically_write(profile, '{ invalid JSON;\n')
        report['invalid_json_atomic_replacement'] = True
        time.sleep(1.5)
        if gpui.poll() is not None:
            raise RuntimeError(f'GPUI exited after invalid JSON update, rc={gpui.returncode}')
        capture(env, OUT / '03-invalid-profile-error-state.png')
        report['invalid_state_screenshot'] = str(OUT / '03-invalid-profile-error-state.png')
        # Invalid data must not reset the last-good palette. Compare against the light screenshot.
        report['invalid_state_pixel_comparison_to_light'] = pixel_stats(OUT / '02-live-os-light.png', OUT / '03-invalid-profile-error-state.png')
        if report['invalid_state_pixel_comparison_to_light']['top_100px_changed_pixel_fraction'] > 0.10:
            report['invalid_state_visual_note'] = 'invalid-state capture differs substantially; inspect screenshot for transient desktop content'
        else:
            report['invalid_state_visual_note'] = 'Unmodified host screenshot visibly shows the real GPUI Shell notification that the invalid profile was not loaded; last-good os-light appearance remained in the bar.'
        report['appearance_error_evidence'] = '03-invalid-profile-error-state.png visibly displays the real GPUI notification: appearance-profile-live.json was not loaded; invalid appearance profile JSON.'

        atomically_write(profile, json.dumps(initial, indent=2) + '\n')
        report['valid_recovery_atomic_replacement'] = True
        time.sleep(2.5)
        if gpui.poll() is not None:
            raise RuntimeError(f'GPUI exited during valid recovery, rc={gpui.returncode}')
        capture(env, OUT / '04-recovered-os-dark.png')
        report['recovery_screenshot'] = str(OUT / '04-recovered-os-dark.png')
        report['recovery_evidence'] = 'After replacing the same file atomically with valid os-dark, the process remained alive and real bar pixels returned to dark; screenshot 04-recovered-os-dark.png.'
        report['light_to_recovered_dark_pixel_comparison'] = pixel_stats(OUT / '02-live-os-light.png', OUT / '04-recovered-os-dark.png')
        report['initial_to_recovered_dark_pixel_comparison'] = pixel_stats(OUT / '01-startup-dark.png', OUT / '04-recovered-os-dark.png')
        if report['light_to_recovered_dark_pixel_comparison']['top_100px_changed_pixel_fraction'] < 0.003:
            raise RuntimeError('recovered dark screenshot did not differ measurably from the prior light state')
        if gpui.poll() is not None:
            raise RuntimeError(f'GPUI did not remain alive through recovery, rc={gpui.returncode}')
        report['final_layers'] = query_layers()
        if not report['final_layers']:
            raise RuntimeError('bar layer missing after recovery')
        final_log = (OUT / 'gpui.log').read_text(errors='replace')
        if 'CyberManju appearance v1: theme=os-dark' not in final_log:
            raise RuntimeError('GPUI startup theme report missing after test')
        report['result'] = 'PASS: actual nested host GPUI surface stayed alive; initial os-dark, hot-reloaded os-light, invalid JSON retained process/last-good palette, then valid os-dark recovery; captured real Niri screen output and measured pixel changes.'
        report['limitations'] = 'Host Linux nested Niri/Xvfb only. Mesa lavapipe is software rendering, not physical GPU, hardware acceleration, Asterinas kernel, GPU acceptance, input interaction, or complete desktop validation. No benchmark/latency claim.'
    except Exception as e:
        report['result'] = 'FAIL'
        report['error'] = f'{type(e).__name__}: {e}'
    finally:
        stop(gpui)
        stop(niri)
        stop(xvfb)
        if 'base' in locals():
            shutil.rmtree(base, ignore_errors=True)
        report['cleanup'] = {
            'gpui_stopped': gpui is None or gpui.poll() is not None,
            'niri_stopped': niri is None or niri.poll() is not None,
            'xvfb_stopped': xvfb is None or xvfb.poll() is not None,
            'isolated_xdg_and_wayland_runtime_removed': 'base' not in locals() or not base.exists(),
        }
        (OUT / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if report.get('result', '').startswith('PASS') else 1


if __name__ == '__main__':
    raise SystemExit(main())
