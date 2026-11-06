#!/usr/bin/env python3
"""Generate and independently compare twice, using explicit OFFLINE archive paths."""
import argparse
import json
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--archives', type=Path, required=True)
p.add_argument('--out', type=Path, required=True, help='new/empty parent directory')
p.add_argument('--check-accounting', type=Path, help='compare full-output hashes to a reviewed accounting file')
a = p.parse_args()
for name in ['one', 'two']:
    archives = sorted(a.archives.glob('*'))
    if name == 'two':
        archives.reverse()
    arguments = []
    for archive in archives:
        if archive.name in {'cen.zip', 'xr.zip', 'config.zip', 'validator.jar', 'cen-source.zip'}:
            arguments += ['--archive', f'{archive.name}={archive}']
    subprocess.run(['cargo', '+1.83.0', 'run', '--locked', '--offline', '--manifest-path', str(HERE / 'Cargo.toml'), '--', 'inspect', *arguments, '--out', str(a.out / name)], check=True)
subprocess.run(['python3', str(HERE / 'tests/inventory.py'), '--archives', str(a.archives), '--ir', str(a.out / 'one/ir.json'), '--other', str(a.out / 'two'), '--summary', str(a.out / 'accounting.json')], check=True)
subprocess.run(['python3', str(HERE / 'tests/test_occurrences.py'), '--archives', str(a.archives), '--ir', str(a.out / 'one/ir.json')], check=True)
if a.check_accounting:
    assert json.loads((a.out / 'accounting.json').read_bytes()) == json.loads(a.check_accounting.read_bytes()), 'reviewed source accounting changed'

# The same exact package cannot become runnable by choosing a different CLI command.
run = subprocess.run(['cargo', '+1.83.0', 'run', '--locked', '--offline', '--manifest-path', str(HERE / 'Cargo.toml'), '--', 'runnable', *arguments, '--out', str(a.out / 'runnable')], capture_output=True, text=True)
assert run.returncode == 2 and 'backend unavailable' in run.stderr, run.stderr
assert not (a.out / 'runnable').exists(), 'runnable failure must emit nothing'
