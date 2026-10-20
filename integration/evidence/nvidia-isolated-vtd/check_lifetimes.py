#!/usr/bin/env python3
"""Compile real PCI APIs with invalid lifetimes; restore source after each attempt."""
from pathlib import Path
import hashlib
import json
import os
import subprocess

ROOT = Path('/workspace/asterinas-gpui')
REPO = ROOT / 'asterinas'
OUT = ROOT / 'evidence/nvidia-isolated-vtd/lifetime-checks-v2'
FILE = REPO / 'kernel/core/comps/pci/src/dma.rs'
CASES = {
    'buffer-outlives-context': '''
#[cfg(ktest)]
fn negative_buffer_outlives_context(owner: &crate::mmio::PciMmioGuard<'_>) {
    let buffer = {
        let context = owner.isolated_dma_context(ostd::mm::dma::DmaMask::new(32).unwrap()).unwrap();
        context.alloc_coherent(1).unwrap()
    };
    drop(buffer);
}
''',
    'context-outlives-mmio': '''
#[cfg(ktest)]
fn negative_context_outlives_mmio(device: &mut crate::common_device::PciCommonDevice) {
    let context = {
        let owner = crate::mmio::PciMmioGuard::prepare(device, 0).unwrap();
        owner.isolated_dma_context(ostd::mm::dma::DmaMask::new(32).unwrap()).unwrap()
    };
    drop(context);
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
            result = subprocess.run(['cargo', 'osdk', 'check', '--ktests'],
                                    cwd=REPO / 'kernel/core/comps/pci', env=env,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
            text = result.stdout.decode(errors='replace')
            (OUT / f'{name}.log').write_text(text)
            expected = result.returncode != 0 and 'error[E0597]' in text and 'does not live long enough' in text
            results.append(dict(case=name, exit=result.returncode, expected_borrow_failure=expected))
            print(name, results[-1], flush=True)
            if not expected:
                raise RuntimeError('Missing actual borrow-checker rejection: ' + name)
            FILE.write_bytes(original)
        result = subprocess.run(['cargo', 'osdk', 'check', '--ktests'],
                                cwd=REPO / 'kernel/core/comps/pci', env=env,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
        (OUT / 'restored-source.log').write_bytes(result.stdout)
        if result.returncode:
            raise RuntimeError('Restored production source does not compile')
    finally:
        FILE.write_bytes(original)
        restored = FILE.read_bytes() == original
        (OUT / 'result.json').write_text(json.dumps(dict(
            cases=results, restored_source=restored, source_sha256=hashlib.sha256(original).hexdigest(),
            scope='Real compiler ownership/lifetime checks, not hardware DMA quiescence proof'), indent=2) + '\n')
    print('Production source restored and rechecked', flush=True)

if __name__ == '__main__':
    main()
