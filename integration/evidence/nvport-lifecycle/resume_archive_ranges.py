#!/usr/bin/env python3
"""Recover only missing bytes of the official .run data file, never execute it."""
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
import os
import time
import urllib.request

ROOT = Path('/workspace/asterinas-gpui')
TARGET = ROOT / 'vendor-drivers/NVIDIA-Linux-x86_64-615.78.08.run'
URL = 'https://us.download.nvidia.com/XFree86/Linux-x86_64/615.78.08/NVIDIA-Linux-x86_64-615.78.08.run'
TOTAL = 529834675
CHUNK = 1024 * 1024
PARTS = ROOT / 'vendor-drivers/firmware-archive-tail-parts'


def main():
    start = TARGET.stat().st_size
    if start > TOTAL:
        raise RuntimeError('Target already exceeds official size')
    PARTS.mkdir(exist_ok=True)
    ranges = [(offset, min(offset + CHUNK, TOTAL) - 1) for offset in range(start, TOTAL, CHUNK)]
    print(f'Recovering {TOTAL-start} missing bytes in {len(ranges)} verified ranges', flush=True)

    def recover(bounds):
        begin, end = bounds
        part = PARTS / f'{begin}-{end}.part'
        if part.exists() and part.stat().st_size == end - begin + 1:
            return begin
        last = None
        for attempt in range(3):
            try:
                request = urllib.request.Request(f'{URL}?data_range={begin}-{end}', headers={'Range': f'bytes={begin}-{end}'})
                with urllib.request.urlopen(request, timeout=90) as response:
                    if response.status != 206 or response.headers.get('Content-Range') != f'bytes {begin}-{end}/{TOTAL}':
                        raise RuntimeError('Server did not honor the exact byte range')
                    data = response.read(end - begin + 2)
                    if len(data) != end - begin + 1:
                        raise RuntimeError(f'Incomplete range: {len(data)} bytes')
                part.write_bytes(data)
                return begin
            except Exception as error:
                last = error
                time.sleep(attempt + 1)
        raise RuntimeError(f'Failed range {begin}-{end}: {last}')

    with ThreadPoolExecutor(max_workers=16) as pool:
        pending = [pool.submit(recover, bounds) for bounds in ranges]
        for count, result in enumerate(as_completed(pending), 1):
            result.result()
            print(f'Verified tail range {count}/{len(ranges)}', flush=True)
    if TARGET.stat().st_size != start:
        raise RuntimeError('Target changed concurrently; refusing to append')
    with TARGET.open('ab') as output:
        for begin, end in ranges:
            data = (PARTS / f'{begin}-{end}.part').read_bytes()
            if len(data) != end - begin + 1:
                raise RuntimeError('Saved range size changed')
            output.write(data)
        output.flush()
        os.fsync(output.fileno())
    if TARGET.stat().st_size != TOTAL:
        raise RuntimeError('Recovered file has the wrong final size')
    print('Archive transfer complete; extraction must still verify its embedded corruption checksum', flush=True)


if __name__ == '__main__':
    main()
