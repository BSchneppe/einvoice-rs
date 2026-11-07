#!/usr/bin/env python3
"""Reproduce genuine schema-only complete and stopped artifacts with byte receipts.

Development harness only. It authenticates and extracts four supplied ZIP members,
checks generator accounting, builds the real crate, then invokes its same driver.
This receipt is reproducibility evidence, not a controlled execution attestation.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import threading
import zipfile

ROOT = Path(__file__).resolve().parents[3]
ARCHIVE_SHA256 = '2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8'
ARCHIVE_BYTES = 499607
INPUT_BYTES = 16 * 1024 * 1024
FILE_BYTES = 128 * 1024 * 1024
OUTPUT_BYTES = 32 * 1024 * 1024  # Per stream, retained prefix; not an RSS limit.
MEMBERS = [
    ('CrossIndustryInvoice_100pD16B.xsd', 3078, '3baf143b90f289d62c5dfb086aa51fa183490a2d0b38a40c6b31dcae0760f2ba'),
    ('CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd', 36806, '6649215914728652e883662bec1d3eab008c215e86a59f3bb417db9723ee8c5c'),
    ('CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd', 99402, '1c42244298a863bcc0b5196983ff0a5d084fcabe58aea77ef82b242e7cd0ddfd'),
    ('CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd', 6967, '04fce6f6f28521886731294be18a4e6bb5cb3003456457d99ad5c4d3e652f497'),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def identify(data):
    return {'bytes': len(data), 'sha256': digest(data)}


def read_regular(path, limit):
    """Only regular files; reject size before reading and cap growth at limit+1.

    Nonblocking Unix opens also reject a raced FIFO instead of waiting for a
    writer. Filesystem I/O itself has no hard deadline guarantee here.
    """
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError(f'regular file required: {path}')
    flags = os.O_RDONLY | getattr(os, 'O_NONBLOCK', 0) | getattr(os, 'O_NOFOLLOW', 0)
    with os.fdopen(os.open(path, flags), 'rb') as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise ValueError(f'file kind/size exceeds admission: {path}')
        data = source.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f'file grew beyond admission: {path}')
    return data


def binding(path):
    return identify(read_regular(path, FILE_BYTES))


def run(args, output, stage, *, stdout_name=None, limit=OUTPUT_BYTES, timeout=300):
    """Persist real bounded child prefixes and status before propagating failure.

    This supervises the direct development child only, not a process tree or
    production worker. Pipe-drain failure and truncation are explicit failures.
    """
    if limit <= 0 or timeout <= 0:
        raise ValueError('finite positive child limits required')
    buffers = [bytearray(), bytearray()]
    truncated = [False, False]
    read_errors = [None, None]
    timed_out = False
    launch_error = None
    proc = None
    readers = []
    try:
        proc = subprocess.Popen(args, cwd=ROOT, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)

        def capture(index, pipe):
            try:
                while True:
                    chunk = os.read(pipe.fileno(), 8192)
                    if not chunk:
                        break
                    available = limit - len(buffers[index])
                    buffers[index].extend(chunk[:available])
                    if len(chunk) > available:
                        truncated[index] = True
                        proc.kill()
                        break
            except OSError as error:
                read_errors[index] = str(error)
            finally:
                pipe.close()

        for index, pipe in enumerate([proc.stdout, proc.stderr]):
            reader = threading.Thread(target=capture, args=(index, pipe), daemon=True)
            readers.append(reader)
            reader.start()
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            proc.kill()
            proc.wait()
        for reader in readers:
            reader.join(timeout=2)
    except OSError as error:
        launch_error = str(error)
    # This boundary must precede both byte copies. A late reader completion
    # during persistence must never certify an earlier incomplete snapshot.
    pipe_drain_complete = all(not reader.is_alive() for reader in readers)
    stdout, stderr = (bytes(b) for b in buffers)
    captured_truncated, captured_read_errors = list(truncated), list(read_errors)
    stdout_path = output / (stdout_name or (stage + '.stdout'))
    stderr_path = output / (stage + '.stderr')
    stdout_path.write_bytes(stdout)
    stderr_path.write_bytes(stderr)
    status = {
        'stage': stage, 'command': [str(a) for a in args], 'cwd': str(ROOT),
        'returncode': None if proc is None else proc.returncode,
        'launch_error': launch_error, 'timeout_seconds': timeout, 'timed_out': timed_out,
        'per_stream_limit': limit, 'truncated': captured_truncated, 'read_errors': captured_read_errors,
        'pipe_drain_complete': pipe_drain_complete,
        'stdout': {'path': stdout_path.name, **identify(stdout)},
        'stderr': {'path': stderr_path.name, **identify(stderr)},
    }
    (output / (stage + '.status.json')).write_text(json.dumps(status, indent=2) + '\n')
    if (launch_error or status['returncode'] != 0 or timed_out or any(captured_truncated)
            or any(captured_read_errors) or not status['pipe_drain_complete']):
        raise RuntimeError(f'{stage} failed; retained {stage}.status.json and raw output prefixes')
    return subprocess.CompletedProcess(args, proc.returncode, stdout, stderr)


def require_input(observation, expected):
    if (observation['input_bytes'] != expected['bytes']
            or observation['input_sha256'] != expected['sha256']):
        raise ValueError('actual observation input differs from admitted snapshot')


def inventory_key(path):
    return path.relative_to(ROOT).as_posix()


def backend_identity(source_inventory):
    files = [{'path': key.removeprefix('vendor/xsd-schema/'), 'sha256': value['sha256']}
             for key, value in sorted(source_inventory.items()) if key.startswith('vendor/xsd-schema/')]
    if not files:
        raise ValueError('backend consumed-source inventory is empty')
    return digest(json.dumps({'domain': 'bsc8-maintained-xsd-consumed-source-v1', 'files': files},
                             sort_keys=True, separators=(',', ':')).encode())


def inventory():
    paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', ROOT / 'vendor/xsd-schema.provenance.json', ROOT / 'vendor/xml-rs.provenance.json']
    for relative in ['crates/einvoice-schema', 'crates/einvoice-source', 'vendor/xsd-schema', 'vendor/xml-rs']:
        paths.extend(p for p in (ROOT / relative).rglob('*') if p.is_file() and '__pycache__' not in p.parts)
    return {inventory_key(p): binding(p) for p in sorted(set(paths))}


def produce(args, output):
    archive = read_regular(args.archive, ARCHIVE_BYTES)
    if len(archive) != ARCHIVE_BYTES or digest(archive) != ARCHIVE_SHA256:
        raise ValueError('archive identity differs')
    # Both children and the receipt use the same admitted snapshot. Changes to
    # the caller's file after this read cannot silently change calibration.
    input_data = read_regular(args.input, INPUT_BYTES)
    input_binding = identify(input_data)
    snapshot = output / 'input.xml'
    snapshot.write_bytes(input_data)
    accounting_path = ROOT / 'tools/validation-generator/SOURCE-ACCOUNTING.json'
    accounting_bytes = read_regular(accounting_path, FILE_BYTES)
    accounting = json.loads(accounting_bytes)['files']
    resource_dir = output / 'original-resources'
    loaded = {}
    with zipfile.ZipFile(io.BytesIO(archive)) as z:
        for name, size, sha in MEMBERS:
            member = 'resources/cii/16b/xsd/' + name
            if z.namelist().count(member) != 1:
                raise ValueError('missing/duplicate member: ' + member)
            if z.getinfo(member).file_size != size:
                raise ValueError('resource size differs before decompression: ' + member)
            with z.open(member) as entry:
                data = entry.read(size + 1)
            actual = {'size': len(data), 'sha256': digest(data)}
            if actual != {'size': size, 'sha256': sha} or accounting['resources/config.zip/' + member] != actual:
                raise ValueError('resource/generator authority differs: ' + member)
            path = resource_dir / member
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            loaded[member] = identify(data)
    before = inventory()
    (output / 'source-inventory.json').write_text(json.dumps(before, indent=2, sort_keys=True) + '\n')
    build_command = ['cargo', '+1.83.0', 'build', '--offline', '--locked', '--all-features', '-p', 'einvoice-schema', '--example', 'cii-observe', '--message-format=json']
    built = run(build_command, output, 'build', stdout_name='build.jsonl')
    executables = [row['executable'] for line in built.stdout.splitlines() if (row := json.loads(line)).get('reason') == 'compiler-artifact' and row['target']['name'] == 'cii-observe' and row.get('executable')]
    if len(executables) != 1:
        raise ValueError('actual example executable unavailable')
    binary = Path(executables[0])
    binary_binding = binding(binary)
    backend_builds = [row for line in built.stdout.splitlines() if (row := json.loads(line)).get('reason') == 'compiler-artifact' and row.get('target', {}).get('name') == 'xsd_schema']
    if len(backend_builds) != 1 or set(backend_builds[0]['features']) != {'default', 'unicode-normalization'}:
        raise ValueError('actual backend build features differ')
    rustc = run(['rustc', '+1.83.0', '-Vv'], output, 'rustc').stdout.decode()
    host = next(line.removeprefix('host: ') for line in rustc.splitlines() if line.startswith('host: '))
    metadata = run(['cargo', '+1.83.0', 'metadata', '--offline', '--locked', '--all-features', '--format-version', '1', '--filter-platform', host], output, 'metadata', stdout_name='cargo-metadata.json')
    meta = json.loads(metadata.stdout)
    backend = next(p for p in meta['packages'] if p['name'] == 'xsd-schema')
    node = next(n for n in meta['resolve']['nodes'] if n['id'] == backend['id'])
    if set(node['features']) != {'default', 'unicode-normalization'} or Path(backend['manifest_path']) != ROOT / 'vendor/xsd-schema/Cargo.toml':
        raise ValueError('backend feature/source drift')
    command = [str(binary), str(resource_dir), str(snapshot), '1000']
    complete = run(command, output, 'complete', stdout_name='complete.json')
    complete_json = json.loads(complete.stdout)
    obs = complete_json['observation']
    require_input(obs, input_binding)
    if not obs['finalization_succeeded'] or obs['assessment'] == 'Incomplete':
        raise ValueError('input did not produce an actual completed schema run')
    # The same driver and input, with a measured lower explicit work allowance.
    # This calibration creates a development stop fixture, not a default policy.
    partial_command = command + [str(max(1, obs['work'] - 1))]
    partial = run(partial_command, output, 'partial', stdout_name='partial.json')
    partial_json = json.loads(partial.stdout)
    require_input(partial_json['observation'], input_binding)
    if binding(snapshot) != input_binding or binding(binary) != binary_binding:
        raise ValueError('retained snapshot or executable changed during run')
    if partial_json['observation']['assessment'] != 'Incomplete' or partial_json['observation']['failure'] is None:
        raise ValueError('no actual partial-stop observation')
    after = inventory()
    receipt = {
        'kind': 'schema-only-development-reproduction; no execution attestation',
        'archive': identify(archive), 'loaded_schema_bytes': loaded,
        'generator_accounting': identify(accounting_bytes),
        'input': input_binding, 'input_snapshot': snapshot.name,
        'git_head': run(['git', 'rev-parse', 'HEAD'], output, 'git-head').stdout.decode().strip(),
        'git_status_porcelain': run(['git', 'status', '--porcelain=v1', '--untracked-files=normal'], output, 'git-status').stdout.decode(),
        'source_inventory': binding(output / 'source-inventory.json'),
        'source_unchanged_during_build_and_run': before == after,
        'rustc': rustc,
        'cargo': run(['cargo', '+1.83.0', '-V'], output, 'cargo-version').stdout.decode(),
        'build_command': build_command,
         'backend_features': node['features'],
        'backend_build_artifact': backend_builds[0],
        'backend_consumed_source_identity': backend_identity(before),
        'executable': {'path': str(binary), **binary_binding},
        'build_stdout': binding(output / 'build.jsonl'),
        'build_stderr': binding(output / 'build.stderr'),
        'cargo_metadata': binding(output / 'cargo-metadata.json'),
        'complete_command': command, 'complete_output': binding(output / 'complete.json'),
        'partial_command': partial_command, 'partial_output': binding(output / 'partial.json'),
        'target_directory': os.environ.get('CARGO_TARGET_DIR'),
        'limits': 'example has explicit demonstration allowances; no RSS/hard-deadline claim',
    }
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2, sort_keys=True) + '\n')
    if before != after:
        raise ValueError('source changed during reproduction; receipt is not a stable build binding')
    print(json.dumps({'complete': obs['assessment'], 'complete_events': len(obs['events']), 'partial_events': len(partial_json['observation']['events']), 'receipt': str(output / 'receipt.json')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError('output directory must be empty; retained prior attempts cannot be overwritten')
    try:
        produce(args, output)
    except Exception as error:
        failure = {'kind': 'development reproduction failure; not an Observation',
                   'error_type': type(error).__name__, 'message': str(error)}
        (output / 'failure.json').write_text(json.dumps(failure, indent=2) + '\n')
        raise


if __name__ == '__main__':
    main()
