#!/usr/bin/env python3
"""Generate and independently compare twice, using explicit OFFLINE archive paths."""
import argparse
import json
import os
import subprocess
from pathlib import Path


def require(condition, reason):
    if not condition:  # explicit, so python -O cannot skip reviewed checks
        raise SystemExit(reason)


HERE = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--archives', type=Path, required=True)
p.add_argument('--out', type=Path, required=True, help='new/empty parent directory')
p.add_argument('--check-candidate-accounting', type=Path, help='compare separate candidate output hashes')
p.add_argument('--check-correspondence-accounting', type=Path, help='compare complete independent correspondence inventory')
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
    subprocess.run(['cargo', '+1.83.0', 'run', '--locked', '--offline', '--manifest-path', str(HERE / 'Cargo.toml'), '--', 'resolve-candidate', *arguments, '--out', str(a.out / ('candidate-' + name))], check=True)
    raw_files = {str(p.relative_to(a.out / name)): p.read_bytes() for p in (a.out / name).rglob('*') if p.is_file()}
    candidate_raw = a.out / ('candidate-' + name) / 'raw'
    require(raw_files == {str(p.relative_to(candidate_raw)): p.read_bytes() for p in candidate_raw.rglob('*') if p.is_file()}, 'candidate changed raw inspection bytes')
# Full real-source compiler/serde/provenance roundtrip, explicitly enabled after acquisition.
subprocess.run(['cargo', '+1.83.0', 'test', '--locked', '--offline', '--manifest-path', str(HERE / 'Cargo.toml'), '--lib', 'resolution::tests::pinned_source_candidate_roundtrip', '--', '--ignored', '--exact'], check=True,
               env={**os.environ, 'VALIDATION_GENERATOR_ARCHIVES': str(a.archives.resolve())})
subprocess.run(['python3', str(HERE / 'tests/candidate.py'), '--archives', str(a.archives), '--one', str(a.out / 'candidate-one'), '--two', str(a.out / 'candidate-two'), '--summary', str(a.out / 'candidate-accounting.json')], check=True)
subprocess.run(['python3', str(HERE / 'tests/test_candidate.py'), '--archives', str(a.archives), '--candidate', str(a.out / 'candidate-one')], check=True)
if a.check_candidate_accounting:
    require(json.loads((a.out / 'candidate-accounting.json').read_bytes()) == json.loads(a.check_candidate_accounting.read_bytes()), 'reviewed candidate accounting changed')
for name in ['one', 'two']:
    subprocess.run(['python3', str(HERE / 'tests/correspondence.py'), '--archives', str(a.archives),
                    '--candidate', str(a.out / ('candidate-' + name)),
                    '--summary', str(a.out / ('correspondence-' + name + '.json'))], check=True)
require((a.out / 'correspondence-one.json').read_bytes() == (a.out / 'correspondence-two.json').read_bytes(), 'correspondence differs with reversed archive order')
subprocess.run(['python3', str(HERE / 'tests/test_correspondence.py'), '--archives', str(a.archives),
                '--candidate', str(a.out / 'candidate-one')], check=True)
if a.check_correspondence_accounting:
    require(json.loads((a.out / 'correspondence-one.json').read_bytes()) == json.loads(a.check_correspondence_accounting.read_bytes()), 'reviewed correspondence accounting changed')
subprocess.run(['python3', str(HERE / 'tests/inventory.py'), '--archives', str(a.archives), '--ir', str(a.out / 'one/ir.json'), '--other', str(a.out / 'two'), '--summary', str(a.out / 'accounting.json')], check=True)
subprocess.run(['python3', str(HERE / 'tests/test_occurrences.py'), '--archives', str(a.archives), '--ir', str(a.out / 'one/ir.json')], check=True)
if a.check_accounting:
    require(json.loads((a.out / 'accounting.json').read_bytes()) == json.loads(a.check_accounting.read_bytes()), 'reviewed source accounting changed')

# The same exact package cannot become runnable by choosing a different CLI command.
run = subprocess.run(['cargo', '+1.83.0', 'run', '--locked', '--offline', '--manifest-path', str(HERE / 'Cargo.toml'), '--', 'runnable', *arguments, '--out', str(a.out / 'runnable')], capture_output=True, text=True)
require(run.returncode == 2 and 'backend unavailable' in run.stderr, run.stderr)
require(not (a.out / 'runnable').exists(), 'runnable failure must emit nothing')
