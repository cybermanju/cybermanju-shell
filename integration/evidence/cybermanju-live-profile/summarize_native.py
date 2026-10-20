#!/usr/bin/env python3
"""Read final real kernel logs, stripping only terminal color sequences."""
from pathlib import Path
import datetime
import glob
import hashlib
import json
import re

ROOT = Path('/workspace/asterinas-gpui')
OUT = ROOT / 'evidence/cybermanju-live-profile'
BASE = ROOT / 'evidence/native-integration-followup'
rows = {}
skipped = []
for path in sorted((BASE / 'native-suites-corrected').glob('*.log')):
    text = re.sub(r'\x1b\[[0-9;]*m', '', path.read_text(errors='replace'))
    matches = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) filtered out', text)
    assert len(matches) == 1, (path.name, matches)
    passed, failed, filtered = map(int, matches[0])
    assert failed == filtered == 0
    rows[path.name] = {'reported_passed': passed, 'failed': failed, 'filtered': filtered}
    skipped.extend(line for line in text.splitlines() if '[skipped:' in line)
assert len(rows) == 5 and sum(row['reported_passed'] for row in rows.values()) == 315
assert len(skipped) == 3
patch = BASE / 'patch19.patch'
canonical = ROOT / 'local-prs/asterinas-nvidia-interrupt-worker.patch'
assert patch.read_bytes() == canonical.read_bytes()
result = {
    'suites': rows, 'reported_passed': 315, 'explicitly_skipped_bodies': skipped,
    'new_owner_hardware_exercised': False, 'physical_gpu_benchmark': False,
    'native_runner_exit': 0, 'completion_job': 'job_s1XLuEbf',
    'canonical_patch': str(canonical), 'patch_sha256': hashlib.sha256(patch.read_bytes()).hexdigest(),
    'notes': 'Fresh masked MSI-X acquisition is enforced by resource-owner constructor. Actual device-source quiescence and RM/GSP remain unimplemented hardware-dependent work.',
}
(OUT / 'native-test-summary.json').write_text(json.dumps(result, indent=2) + '\n')
display = []
for device in Path('/sys/bus/pci/devices').glob('*'):
    try:
        if int((device / 'class').read_text().strip(), 16) >> 16 == 3:
            display.append({'bdf': device.name, **{key: (device / key).read_text().strip() for key in ['class', 'vendor', 'device']}})
    except (OSError, ValueError):
        pass
hardware = {
    'time_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'device_nodes': {'dri': glob.glob('/dev/dri*'), 'nvidia': glob.glob('/dev/nvidia*'), 'kvm': glob.glob('/dev/kvm')},
    'display_class_pci': display, 'physical_gpu_benchmark': False,
    'host_ui_renderer': 'Explicitly forced Mesa lavapipe; host Linux software rendering, not Asterinas or physical GPU',
}
(OUT / 'hardware-final.json').write_text(json.dumps(hardware, indent=2) + '\n')
print(json.dumps({'reported_passed': 315, 'failed': 0, 'explicit_skips': len(skipped), 'device_nodes': hardware['device_nodes'], 'display_class_pci': display}, indent=2))
