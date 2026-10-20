#!/usr/bin/env python3
"""Read NVIDIA's .run as a Zstandard/tar data archive; never execute its code.

Embedded MD5 detects corruption only, not authenticity. HTTPS source and observed
SHA-256 are recorded. No license acceptance, installation or GPU initialization.
"""
import hashlib
import json
import argparse
from pathlib import Path
import re
import tarfile
import zstandard

ROOT = Path('/workspace/asterinas-gpui')
SOURCE = ROOT / 'vendor-drivers/NVIDIA-Linux-x86_64-615.78.08.run'
OUTPUT = ROOT / 'vendor-drivers/firmware-615.78.08'
REPORT = ROOT / 'evidence/nvport-lifecycle/firmware-provenance.json'
EXPECTED_SIZE = 529834675
WANTED = {
    'gsp_tu10x.bin', 'gsp_ga10x.bin', 'ucodes_tu10x.bin', 'ucodes_ga10x.bin',
    'gsp_log_tu10x.bin', 'gsp_log_ga10x.bin',
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--partial-archive-cpu-only', action='store_true',
                        help='Permit complete entries from a truncated prefix; no archive integrity or GPU use claim')
    args = parser.parse_args()
    complete = SOURCE.stat().st_size == EXPECTED_SIZE
    if not complete and not args.partial_archive_cpu_only:
        raise RuntimeError('Installer is incomplete or differs from the observed official size')
    if SOURCE.stat().st_size > EXPECTED_SIZE:
        raise RuntimeError('Installer exceeds the observed official size')
    with SOURCE.open('rb') as raw:
        header = b''.join(raw.readline() for _ in range(125))
        if b'version_string=615.78.08\n' not in header:
            raise RuntimeError('Unexpected driver release')
        skip = int(re.search(rb'^skip=(\d+)$', header, re.M)[1])
        declared_md5 = re.search(rb'^MD5=([0-9a-f]{32})$', header, re.M)[1].decode()
        raw.seek(0)
        sha = hashlib.sha256()
        for block in iter(lambda: raw.read(1024 * 1024), b''):
            sha.update(block)
        raw.seek(0)
        for _ in range(3):
            raw.readline()
        md5 = hashlib.md5()
        for block in iter(lambda: raw.read(1024 * 1024), b''):
            md5.update(block)
        if complete and md5.hexdigest() != declared_md5:
            raise RuntimeError('Embedded corruption check failed')
        raw.seek(0)
        for _ in range(skip - 1):
            raw.readline()
        offset = raw.tell()
        if raw.read(4) != b'\x28\xb5\x2f\xfd':
            raise RuntimeError('Payload is not a standard Zstandard frame')
        raw.seek(offset)
        OUTPUT.mkdir(parents=True, exist_ok=True)
        files = []
        seen = set()
        try:
          with zstandard.ZstdDecompressor().stream_reader(raw) as decompressed:
            with tarfile.open(fileobj=decompressed, mode='r|') as archive:
                for member in archive:
                    name = Path(member.name).name
                    if name not in WANTED:
                        continue
                    if name in seen or not member.isfile() or not 0 < member.size <= 128 * 1024 * 1024:
                        raise RuntimeError(f'Invalid or duplicate firmware entry: {member.name}')
                    seen.add(name)
                    source = archive.extractfile(member)
                    target = OUTPUT / name
                    digest = hashlib.sha256()
                    with target.open('xb') as dest:
                        remaining = member.size
                        while remaining:
                            block = source.read(min(remaining, 1024 * 1024))
                            if not block:
                                raise RuntimeError('Truncated firmware entry')
                            digest.update(block)
                            dest.write(block)
                            remaining -= len(block)
                    files.append({'archive_entry': member.name, 'path': str(target),
                                  'size': member.size, 'sha256': digest.hexdigest()})
                    print(f'Extracted {name}: {member.size} bytes', flush=True)
        except (tarfile.ReadError, zstandard.ZstdError) as error:
            if complete or not args.partial_archive_cpu_only:
                raise
            print(f'Expected truncated archive tail in explicit CPU-only mode: {error}', flush=True)
        if not {'gsp_tu10x.bin', 'gsp_ga10x.bin'}.issubset(seen):
            raise RuntimeError('Required real GSP images not present')
        report = {'source_url': 'https://us.download.nvidia.com/XFree86/Linux-x86_64/615.78.08/NVIDIA-Linux-x86_64-615.78.08.run',
                  'release': '615.78.08', 'installer_sha256': sha.hexdigest() if complete else None,
                  'downloaded_prefix_sha256': sha.hexdigest(), 'downloaded_bytes': SOURCE.stat().st_size,
                  'expected_installer_bytes': EXPECTED_SIZE, 'installer_download_complete': complete,
                  'partial_archive_cpu_only': not complete,
                  'embedded_md5_corruption_check': complete, 'vendor_signature_verified': False,
                  'installer_executed': False, 'installed_on_host': False,
                  'gpu_firmware_executed': False, 'files': files}
        REPORT.write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
