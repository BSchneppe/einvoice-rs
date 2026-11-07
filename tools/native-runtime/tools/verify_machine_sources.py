#!/usr/bin/env python3
"""Verify exact read-dependency identities; never modify source or the manifest."""
import hashlib
import json
from pathlib import Path
root = Path(__file__).resolve().parents[3]
manifest = root / 'tools/native-runtime/MACHINE-SOURCES.json'
entries = json.loads(manifest.read_text())['files']
for entry in entries:
    data = (root / entry['path']).read_bytes()
    assert len(data) == entry['bytes'], entry['path']
    assert hashlib.sha256(data).hexdigest() == entry['sha256'], entry['path']
print(f'{len(entries)} source/IR/numeric/parser/scalar evidence identities verified')
