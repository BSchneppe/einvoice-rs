#!/usr/bin/env python3
"""Read-only pinned reference replay. Writes observations/logs only to --output.
The committed queries/XML are sufficient; no scratch, Rust build or test parser.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys

JDK = Path('/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home/bin')
JAR = Path('/tmp/einvoice-validator-1.6.3.jar')
PIN = '799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
assert sys.version_info[:3] == (3, 12, 6), sys.version
assert hashlib.sha256(JAR.read_bytes()).hexdigest() == PIN
script = Path(__file__).resolve()
root = script.parents[1]
fixture = root / 'tests/evaluator-fixtures'
out = args.output.resolve()
assert not out.is_relative_to(fixture.resolve())
out.mkdir(parents=True, exist_ok=True)
commands = []
def run(command):
    result = subprocess.run([str(x) for x in command], capture_output=True, text=True, timeout=90)
    commands.append(dict(command=[str(x) for x in command], exit=result.returncode, stderr=result.stderr))
    (out / 'commands.json').write_text(json.dumps(commands, indent=2) + '\n')
    result.check_returncode()
    return result
version = run([JDK / 'java', '-version'])
assert '27.0.1' in version.stderr and 'Corretto-27.0.1.0.11' in version.stderr
lines = []
for line in (fixture / 'queries.tsv').read_text().splitlines():
    identity, query_hex = line.split('\t')
    lines.append(identity + '\txpath2\t' + base64.b64encode(bytes.fromhex(query_hex)).decode())
for line in (fixture / 'select-bindings.tsv').read_text().splitlines():
    identity, source_hex = line.split('\t')
    lines.append(identity + '\txslt2\t' + base64.b64encode(bytes.fromhex(source_hex)).decode())
listing = out / 'inputs.tsv'
listing.write_text('\n'.join(lines) + '\n')
classes = out / 'classes'
classes.mkdir(exist_ok=True)
run([JDK / 'javac', '-cp', JAR, '-d', classes, root / 'tools/MachineProbe.java'])
result = run([JDK / 'java', '-Xmx256m', '-XX:ActiveProcessorCount=1', '-cp', str(classes) + ':' + str(JAR), 'MachineProbe', listing, fixture / 'document.xml'])
assert 'Saxon=HE 12.9' in result.stderr
(out / 'raw.tsv').write_text(result.stdout)
observations = []
seen = set()
for line in result.stdout.splitlines():
    row = line.split('\t')
    identity, status = row[:2]
    seen.add(identity)
    if status == 'OK' and row[2] == '0':
        observations.append(identity + '\tEMPTY')
    elif status == 'ITEM':
        observations.append(identity + '\tITEM\t' + base64.b64decode(row[2]).hex() + '\t' + base64.b64decode(row[3]).hex())
    elif status == 'TEXT':
        observations.append(identity + '\tTEXT\t' + base64.b64decode(row[2]).hex())
    elif status == 'ERROR':
        assert row[2] in ('compile', 'evaluate'), row
        code = base64.b64decode(row[3]).decode().split('}')[-1]
        observations.append(identity + '\tERROR\t' + code)
assert len(seen) == len(lines)
observed = '\n'.join(observations) + '\n'
(out / 'observations.tsv').write_text(observed)
identities = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [fixture / 'queries.tsv', fixture / 'select-bindings.tsv', fixture / 'document.xml', root / 'tools/MachineProbe.java', script]}
identities.update(jar=PIN, python=sys.version, java=version.stderr)
(out / 'identities.json').write_text(json.dumps(identities, indent=2) + '\n')
expected = fixture / 'observations.tsv'
if expected.exists():
    assert observed == expected.read_text(), 'Fresh pinned observations differ from committed fixture'
print(f'{len(seen)} selected reference queries observed; {len(observations)} item/text/error/empty rows')
