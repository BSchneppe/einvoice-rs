#!/usr/bin/env python3
"""Explicit verified acquisition, separate from offline compiler and consumer builds.

Only five URLs from the authoritative oracle lock; no discovery or mutable "latest".
"""
import argparse
import hashlib
import json
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
NEEDED = {'cen.zip', 'xr.zip', 'config.zip', 'validator.jar', 'cen-source.zip'}
p = argparse.ArgumentParser()
p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
lock = json.loads((HERE.parent / 'oracle/artifacts.lock.json').read_bytes())
for pin in lock['artifacts']:
    if pin['file'] not in NEEDED:
        continue
    dest = a.out / pin['file']
    if dest.exists():
        if hashlib.sha256(dest.read_bytes()).hexdigest() != pin['sha256']:
            raise SystemExit('existing archive has wrong hash: ' + pin['file'])
        continue
    partial = dest.with_suffix(dest.suffix + '.partial')
    try:
        sha = hashlib.sha256()
        size = 0
        with urllib.request.urlopen(pin['url'], timeout=120) as source, partial.open('xb') as output:
            while chunk := source.read(1024 * 1024):
                size += len(chunk)
                if size > 128 * 1024 * 1024:
                    raise ValueError('archive download budget exceeded')
                sha.update(chunk)
                output.write(chunk)
        if sha.hexdigest() != pin['sha256']:
            raise ValueError('download hash mismatch: ' + pin['file'])
        partial.replace(dest)
    finally:
        partial.unlink(missing_ok=True)
    print('verified ' + pin['file'])
