#!/usr/bin/env python3
"""Verify actual borrow-checker rejection; never execute device code."""
from pathlib import Path
import hashlib
import json
import os
import subprocess

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-multifunction/lifetime-checks'
FILE = REPO / 'kernel/core/comps/pci/src/dma.rs'
CASES = {
    'buffer-outlives-group-context': '''
#[cfg(ktest)]
fn negative_buffer(owner: &crate::mmio::PciGroupMmioGuard<'_>) {
    let buffer = {
        let context = owner.isolated_dma_context(DmaMask::new(32).unwrap()).unwrap();
        context.alloc_coherent(1).unwrap()
    };
    drop(buffer);
}
''',
    'context-outlives-group-mmio': '''
#[cfg(ktest)]
fn negative_mmio(group: &mut crate::reservation::PciGroupReservation<'_>) {
    let context = {
        let owner = group.prepare_mmio(&[0, 0]).unwrap();
        owner.isolated_dma_context(DmaMask::new(32).unwrap()).unwrap()
    };
    drop(context);
}
''',
    'mmio-outlives-reservation': '''
#[cfg(ktest)]
fn negative_group(bus: &mut crate::bus::PciBus, location: crate::PciDeviceLocation) {
    let owner = {
        let mut group = bus.reserve_unclaimed_group(location).unwrap();
        group.prepare_mmio(&[0, 0]).unwrap()
    };
    drop(owner);
}
''',
    'reprobe-while-group-live': '''
#[cfg(ktest)]
fn negative_reprobe(bus: &mut crate::bus::PciBus, location: crate::PciDeviceLocation) {
    let group = bus.reserve_unclaimed_group(location).unwrap();
    let other = bus.reserve_unclaimed(location);
    drop(group);
    drop(other);
}
''',
}

def main():
    OUT.mkdir(parents=True, exist_ok=False)
    original = FILE.read_bytes()
    env = os.environ.copy()
    env.update(PATH=str(ROOT / 'toolchain/bin') + ':' + env['PATH'], CARGO_NET_OFFLINE='true')
    results = []
    try:
        for name, case in CASES.items():
            FILE.write_bytes(original + case.encode())
            p = subprocess.run(['cargo', 'osdk', 'check', '--ktests'],
                cwd=REPO / 'kernel/core/comps/pci', env=env,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
            text = p.stdout.decode(errors='replace')
            (OUT / f'{name}.log').write_text(text)
            code = 'E0499' if name == 'reprobe-while-group-live' else 'E0597'
            expected = p.returncode != 0 and f'error[{code}]' in text
            results.append(dict(case=name, exit=p.returncode, expected_code=code,
                expected_borrow_failure=expected))
            print(name, results[-1], flush=True)
            if not expected:
                raise RuntimeError('Missing actual borrow-checker rejection: ' + name)
            FILE.write_bytes(original)
        p = subprocess.run(['cargo', 'osdk', 'check', '--ktests'],
            cwd=REPO / 'kernel/core/comps/pci', env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
        (OUT / 'restored-source.log').write_bytes(p.stdout)
        if p.returncode:
            raise RuntimeError('Restored production source does not compile')
    finally:
        FILE.write_bytes(original)
        (OUT / 'result.json').write_text(json.dumps(dict(cases=results,
            restored_source=FILE.read_bytes() == original,
            source_sha256=hashlib.sha256(original).hexdigest(),
            scope='Real compiler lifetime checks, not device quiescence proof'), indent=2) + '\n')
    print('Production source restored and rechecked', flush=True)

if __name__ == '__main__':
    main()
