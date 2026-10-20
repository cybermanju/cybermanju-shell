#!/usr/bin/env python3
"""Read-only preflight; never builds or starts QEMU. Writes only beside this file."""
import datetime
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess

ROOT = Path('/workspace/asterinas-gpui/asterinas')
OUT = Path(__file__).resolve().parent
OUT.mkdir(parents=True, exist_ok=True)
LOG = OUT / 'stdout.log'
LOG.write_text('NixOS hello preflight only. No build or guest execution.\n')
commands = []

def run(args):
    commands.append(shlex.join(args))
    p = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    with LOG.open('a') as f:
        f.write('\n$ ' + shlex.join(args) + '\n' + p.stdout + '\nexit_code=' + str(p.returncode) + '\n')
    return p.stdout

def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda: f.read(1048576), b''):
            h.update(chunk)
    return h.hexdigest()

stamp = datetime.datetime.now(datetime.timezone.utc).isoformat()
head = run(['git', 'rev-parse', 'HEAD']).strip()
status = run(['git', 'status', '--short', '--untracked-files=all'])
(OUT / 'git-status-before.txt').write_text(status)
run(['git', 'diff', '--stat'])
run(['df', '-B1', str(ROOT)])
run(['df', '-i', str(ROOT)])
run(['du', '-sh', 'target', '/nix/store'])
versions = {}
for name, args in [('nix', ['nix', '--version']), ('qemu', ['qemu-system-x86_64', '--version']), ('make', ['make', '--version'])]:
    versions[name] = run(args).splitlines()[0]
run(['git', 'diff', '--quiet', '--', 'Makefile', 'tools/nixos', 'test/nixos', 'distro', 'rust-toolchain.toml'])
tracked_changes = subprocess.check_output(['git', 'diff', '--name-only', '-z'], cwd=ROOT).split(b'\0')
untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard', '-z'], cwd=ROOT).split(b'\0')
inputs = [ROOT / p for p in ['Makefile', 'AGENTS.md', 'Cargo.toml', 'Cargo.lock', 'Components.toml', 'rust-toolchain.toml', '.github/workflows/test_nixos_minimal.yml', 'tools/qemu_args.sh']]
for d in ['tools/nixos', 'test/nixos', 'distro']:
    inputs.extend(p for p in (ROOT / d).rglob('*') if p.is_file() and not p.is_symlink() and 'target' not in p.relative_to(ROOT / d).parts)
inputs.extend(ROOT / p.decode() for p in tracked_changes + untracked if p)
input_hashes = {str(p.relative_to(ROOT)): sha(p) for p in sorted(set(inputs)) if p.is_file() and not p.is_symlink()}
(OUT / 'input-sha256.json').write_text(json.dumps(input_hashes, indent=2) + '\n')
image = ROOT / 'target/nixos/asterinas.img'
artifacts = []
for p in sorted((ROOT / 'target/osdk').glob('asterinas-osdk-bin*')):
    if p.is_file():
        artifacts.append({'path': str(p.relative_to(ROOT)), 'bytes': p.stat().st_size, 'sha256': sha(p), 'reused': False, 'validation': 'existence and hash only; not validated for NixOS boot'})
(OUT / 'existing-artifacts.json').write_text(json.dumps(artifacts, indent=2) + '\n')
free = shutil.disk_usage(ROOT).free
required = 6144 * 1024 * 1024
summary = {
    'timestamp_utc': stamp,
    'worktree': str(ROOT), 'head': head, 'dirty': bool(status.strip()),
    'result': 'FAIL', 'status': 'BLOCKED_DISK_CAPACITY', 'pass': False,
    'failure_kind': 'host preflight; NOT a guest test failure',
    'build_executed': False, 'qemu_executed': False,
    'intended_suite': 'hello', 'intended_guest_command': 'hello-asterinas',
    'expected_guest_output': 'Hello Asterinas!',
    'guest_program_reached': None, 'guest_clean_exit_verified': None,
    'serial_capture_status': 'NOT_CREATED_GUEST_NEVER_STARTED',
    'scope': {'cpu_qemu_only': True, 'architecture': 'x86_64', 'intended_acceleration': 'TCG (ENABLE_KVM=0; /dev/kvm absent)', 'executed_acceleration': None, 'gpu_validated': False, 'cybermanju_app_compatibility_validated': False},
    'disk': {'available_bytes': free, 'ci_image_mib': 6144, 'image_preallocation_bytes': required, 'image_only_shortfall_bytes': max(0, required-free), 'additional_build_and_nix_space': 'required but not measured; image alone exceeds available space', 'allocation_mechanism': 'tools/nixos/build_nixos.sh:41 fallocate; not sparse'},
    'expected_image_path': str(image), 'expected_image_exists': image.exists(),
    'existing_artifacts_reused': False,
    'kvm_device_exists': Path('/dev/kvm').exists(),
    'docker_executable_present': shutil.which('docker') is not None,
    'project_environment': 'AGENTS.md specifies project Docker; Docker executable absent; not attempted because disk gate already blocks',
    'versions': versions,
    'os_a1': 'OPEN',
    'not_proven': ['libc compatibility requirements', 'GTK', 'WebKit', 'Tauri', 'Wayland', 'GPU', 'CyberManju application compatibility'],
    'source_edits_by_probe': False, 'cache_deletion': False, 'git_staging_commit_push': False, 'remote_actions': False,
}
(OUT / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
(OUT / 'commands-executed.txt').write_text('# Read-only collection commands (Python hash/stat operations also described in collect_preflight.py)\n' + '\n'.join(commands) + '\n')
(OUT / 'commands-not-executed.txt').write_text('''# NOT EXECUTED: disk capacity guard prevented build and guest run.
# Official CI baseline, from the actual worktree root:
make nixos NIXOS_TEST_SUITE=hello NIXOS_DISK_SIZE_IN_MB=6144
make run_nixos NIXOS_TEST_SUITE=hello

# Intended headless CPU/QEMU-only adaptation using existing supported options;
# NOT EXECUTED or validated. Requires sufficient space and project environment first.
make nixos NIXOS_TEST_SUITE=hello NIXOS_DISK_SIZE_IN_MB=6144 ENABLE_KVM=0 QEMU_DISPLAY=none
make run_nixos NIXOS_TEST_SUITE=hello ENABLE_KVM=0 QEMU_DISPLAY=none NIXOS_TEST_TIMEOUT=10min

# Root target internally invokes make kernel and the test/nixos wrapper.
# No direct cargo/nix-build/QEMU build/run commands were invoked by this probe.
# No sparse-image substitution, smaller image, configuration patch, or cache cleanup.
''')
with LOG.open('a') as f:
    f.write('\nDECISION: BLOCKED_DISK_CAPACITY. Build and QEMU NOT RUN.\n')
status_after = subprocess.check_output(['git', 'status', '--short', '--untracked-files=all'], cwd=ROOT, text=True)
(OUT / 'git-status-after.txt').write_text(status_after)
unchanged = status == status_after and all(sha(ROOT / name) == digest for name, digest in input_hashes.items())
(OUT / 'preservation.json').write_text(json.dumps({'status_before_equals_after': status == status_after, 'all_recorded_input_hashes_unchanged': unchanged, 'files_hashed': len(input_hashes)}, indent=2) + '\n')
print(json.dumps(summary, indent=2))
print('Preservation verified:', unchanged)
if not unchanged:
    raise SystemExit('Unexpected worktree change detected')
