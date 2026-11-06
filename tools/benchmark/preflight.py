#!/usr/bin/env python3
"""Bounded protocol/resource preparation. No validation or timing execution."""
import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import platform
import re
import stat
import sys

BASE = '756c15089dbd8d3fe586be4486641501338fa4ab'
ROOT = Path(__file__).resolve().parents[2]
# Reviewed bytes at BASE. Updating these requires review; there is no refresh command.
PINS = {
    'workload.json': (1217, 'ffa42ec1df119033c86a6614d48b6f84aa79b2eb263d5a843e84361c2738fb71'),
    'corpus/initial.jsonl': (17867, 'ae21afa50aa6516d61b87c9f2e67088444f25c7495f111567801f5661242b315'),
    'corpus/split.json': (1014, '83dd3f9ca21c77a1eaefef6b3f44b71b4953ad44d26103f1ede85b08e7a2e4ea'),
    'oracle.py': (21031, '5de78991c1bb14b28d60afb4ec34822119b879c2e5be87608688f64dccd98a6a'),
    'java/Oracle.java': (9434, '37565bfaba63f452018596d422a65a7d969f06245bbe33a134e95f92fe2cad3e'),
    'artifacts.lock.json': (107626, '5dec502919d8ee137cd2c1453be8c65c1176e8bc707e358f646cef3fb1f55175'),
}
PROTOCOL_FILES = {f'protocol-{i}.bin': name for i, name in enumerate(PINS)}
CONTROLS = ('cgroup.controllers', 'cpu.max', 'cpuset.cpus.effective',
            'memory.max', 'memory.swap.max', 'pids.max')
IDENTITY = ('host', 'machine', 'boot', 'kernel', 'arch', 'cgroup_ns', 'mount_ns')
UNPROVEN = ('effective_budget', 'execution', 'enforcement', 'exclusivity', 'correctness', 'timing')
MAX_BYTES = 2 * 1024 * 1024
MAX_TEXT = 262144
MAX_INT = (1 << 63) - 1
MAX_CPUS = 4096


class Invalid(Exception):
    def __init__(self, code='E_INVALID'):
        self.code = code


def require(ok, code='E_INVALID'):
    if not ok:
        raise Invalid(code)


def encoded(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, allow_nan=False,
                       separators=(',', ':')) + '\n').encode('utf-8')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def exact(value, keys):
    require(type(value) is dict and set(value) == set(keys))


def integer(value, minimum=0, maximum=MAX_INT):
    require(type(value) is int and minimum <= value <= maximum)
    return value


def string(value, maximum=MAX_TEXT):
    require(type(value) is str and len(value) <= maximum)
    require(not any(0xD800 <= ord(c) <= 0xDFFF or ord(c) == 0 for c in value))
    return value


def decode(data):
    require(len(data) <= MAX_BYTES, 'E_BOUNDS')
    try:
        text = data.decode('utf-8')
        # Bound nesting before invoking the recursive JSON decoder.
        depth, quoted, escaped = 0, False, False
        for c in text:
            if quoted:
                if escaped:
                    escaped = False
                elif c == '\\':
                    escaped = True
                elif c == '"':
                    quoted = False
            elif c == '"':
                quoted = True
            elif c in '[{':
                depth += 1
                require(depth <= 12, 'E_BOUNDS')
            elif c in ']}':
                depth -= 1
        def pairs(items):
            require(len(items) <= 8192, 'E_BOUNDS')
            out = {}
            for k, v in items:
                require(k not in out, 'E_JSON')
                out[k] = v
            return out
        def number(s):
            require(len(s) <= 20, 'E_BOUNDS')
            n = int(s)
            require(abs(n) <= MAX_INT, 'E_BOUNDS')
            return n
        def bad(_):
            raise Invalid('E_JSON')
        value = json.loads(text, object_pairs_hook=pairs, parse_int=number, parse_constant=bad)
        pending, count = [value], 0
        while pending:
            v = pending.pop()
            count += 1
            require(count <= 30000, 'E_BOUNDS')
            if type(v) is dict:
                pending.extend(v.keys())
                pending.extend(v.values())
            elif type(v) is list:
                require(len(v) <= 8192, 'E_BOUNDS')
                pending.extend(v)
            elif type(v) is str:
                string(v)
            elif type(v) is float:
                require(math.isfinite(v), 'E_JSON')
        return value
    except (UnicodeError, ValueError, RecursionError):
        raise Invalid('E_JSON') from None


def read_file(path, limit=MAX_BYTES, root=None):
    """No symlink components, confined resolved path, bounded read, regular files only."""
    path = Path(path).absolute()
    if root is not None:
        root = Path(root).absolute()
        require(path.is_relative_to(root), 'E_PATH')
    for part in (path, *path.parents):
        require(not part.is_symlink(), 'E_PATH')
    resolved = path.resolve(strict=True)
    if root is not None:
        require(resolved.is_relative_to(root.resolve(strict=True)), 'E_PATH')
    fd = os.open(resolved, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as handle:
        require(stat.S_ISREG(os.fstat(handle.fileno()).st_mode), 'E_PATH')
        data = handle.read(limit + 1)
    require(len(data) <= limit, 'E_BOUNDS')
    return data


def protocol(artifacts):
    require(set(artifacts) == set(PROTOCOL_FILES), 'E_SOURCE_PIN')
    for filename, name in PROTOCOL_FILES.items():
        size, digest = PINS[name]
        require(len(artifacts[filename]) == size and sha(artifacts[filename]) == digest, 'E_SOURCE_PIN')
    config = decode(artifacts['protocol-0.bin'])
    rows = [decode(line) for line in artifacts['protocol-1.bin'].splitlines()]
    selected = [r for r in rows if r['scope'] == 'complete']
    counts = dict(sorted(Counter(r['group'] for r in selected).items()))
    identities = [[r['id'], r['sha256'], r['group'], r['split']] for r in selected]
    require(len(selected) == 41 and len({r['id'] for r in selected}) == 41 and len(counts) == 19,
            'E_SOURCE_PIN')
    return dict(base=BASE, identity_scope='protocol_bytes_only', complete_inputs=41,
                ordered_identity_sha256=sha(encoded(identities)), group_counts=counts,
                weights=config['groups'], warmup_passes=config['warmup_passes'],
                warmup_placement='once_before_all_runs', runs=config['runs'], repeats=config['repeats'],
                samples_per_run=410, samples_per_mode=2050,
                aggregation=config['aggregation'],
                sources={name: {'bytes': PINS[name][0], 'sha256': PINS[name][1]} for name in PINS})


def linux_path(value):
    string(value, 4096)
    require(value.startswith('/') and '\\' not in value and
            not any(ord(c) < 32 or ord(c) == 127 for c in value), 'E_PATH')
    parts = value.split('/')[1:]
    require(len(parts) <= 64 and all(p not in ('.', '..') for p in parts), 'E_PATH')
    require(value == '/' or all(parts), 'E_PATH')
    return PurePosixPath(value)


def unescape_mount(value):
    require(not re.search(r'\\(?!040|011|012|134)', value), 'E_LAYOUT')
    return re.sub(r'\\(040|011|012|134)', lambda m: chr(int(m[1], 8)), value)


def layout(cgroup, mounts):
    """Return visible paths only. A namespace root never proves ancestor completeness."""
    # Missing/unsupported observations cannot legalize malformed present facts.
    lines, unified = [], []
    if cgroup is not None:
        string(cgroup, 8192)
        lines = cgroup.splitlines()
        unified = [linux_path(line[3:]) for line in lines if line.startswith('0::')]
    if mounts is not None:
        require(type(mounts) is list and len(mounts) <= 16, 'E_BOUNDS')
    parsed_mounts = []
    for line in mounts or []:
        string(line, 8192)
        fields = line.split()
        require(' - ' in line and len(fields) >= 10, 'E_LAYOUT')
        sep = fields.index('-')
        require(sep >= 6 and len(fields) == sep + 4 and fields[sep + 1] == 'cgroup2', 'E_LAYOUT')
        mount_root = linux_path(unescape_mount(fields[3]))
        mount = linux_path(unescape_mount(fields[4]))
        require(mount.is_relative_to(PurePosixPath('/sys/fs/cgroup')), 'E_PATH')
        parsed_mounts.append((mount_root, mount))
    if cgroup is None or mounts is None or len(lines) != 1 or len(unified) != 1:
        return None  # unavailable or well-formed unsupported v1/hybrid layout
    member = unified[0]
    candidates = []
    for mount_root, mount in parsed_mounts:
        if member.is_relative_to(mount_root):
            leaf = mount / member.relative_to(mount_root)
            paths = [str(leaf)]
            while leaf != mount:
                leaf = leaf.parent
                paths.append(str(leaf))
            require(len(paths) <= 64, 'E_BOUNDS')
            candidates.append(paths)
    return candidates[0] if len(candidates) == 1 else None


def cpu_list(text):
    string(text, 32768)
    text = text.strip()
    if not text:
        return []
    parts = text.split(',')
    require(len(parts) <= MAX_CPUS, 'E_BOUNDS')
    result, previous = [], -1
    for item in parts:
        require(re.fullmatch(r'[0-9]{1,7}(-[0-9]{1,7})?', item) is not None)
        ends = item.split('-')
        lo, hi = int(ends[0]), int(ends[-1])
        require(previous < lo <= hi <= 1048575 and len(result) + hi - lo + 1 <= MAX_CPUS, 'E_BOUNDS')
        result.extend(range(lo, hi + 1))
        previous = hi
    return result


def limit_value(text, positive=False):
    if text == 'max':
        return 'max'
    require(re.fullmatch(r'[0-9]{1,19}', text) is not None)
    return integer(int(text), 1 if positive else 0)


def control_value(key, raw):
    if raw is None:
        return None
    string(raw, 32768)
    value = raw.strip()
    if key == 'cpu.max':
        parts = value.split()
        require(len(parts) == 2)
        return [limit_value(parts[0], True), integer(limit_value(parts[1], True), 1)]
    if key == 'cpuset.cpus.effective':
        return cpu_list(raw)
    if key == 'cgroup.controllers':
        parts = value.split()
        require(len(parts) <= 64 and len(parts) == len(set(parts)) and
                all(re.fullmatch('[a-z_]{1,32}', p) for p in parts))
        return sorted(parts)
    return limit_value(value)


def snapshot_summary(raw):
    exact(raw, ('identity', 'cgroup', 'mounts', 'affinity', 'ancestors'))
    exact(raw['identity'], IDENTITY)
    gaps = ['HIDDEN_ANCESTORS_UNPROVEN', 'NAMESPACE_SCOPE_UNPROVEN']
    for key, value in raw['identity'].items():
        if value is None:
            gaps.append('UNAVAILABLE_IDENTITY_' + key.upper())
        else:
            require(bool(string(value, 1024)))
    affinity = raw['affinity']
    if affinity is None:
        gaps.append('UNAVAILABLE_AFFINITY')
    else:
        require(type(affinity) is list and 0 < len(affinity) <= MAX_CPUS)
        for cpu in affinity:
            integer(cpu, 0, 1048575)
        require(affinity == sorted(set(affinity)))
    paths = layout(raw['cgroup'], raw['mounts'])
    ancestors = raw['ancestors']
    require(type(ancestors) is list and len(ancestors) <= 64)
    require([a.get('path') for a in ancestors if type(a) is dict] == (paths or []))
    controls = []
    if paths is None:
        gaps.append('UNAVAILABLE_CGROUP2_LAYOUT')
    for ancestor in ancestors:
        exact(ancestor, ('path', 'controls'))
        exact(ancestor['controls'], CONTROLS)
        values = {key: control_value(key, ancestor['controls'][key]) for key in CONTROLS}
        for key, value in values.items():
            if value is None:
                gaps.append('UNAVAILABLE_CONTROL_' + key.upper())
        available = values['cgroup.controllers']
        if available is not None and not {'cpu', 'cpuset', 'memory', 'pids'} <= set(available):
            gaps.append('MISSING_CONTROLLERS')
        cpus = values['cpuset.cpus.effective']
        if cpus == []:
            gaps.append('EMPTY_EFFECTIVE_CPUSET')
        if cpus is not None and affinity is not None and not set(affinity) <= set(cpus):
            gaps.append('AFFINITY_OUTSIDE_CPUSET')
        controls.append(dict(path=ancestor['path'], controls=values))
    return dict(identity=raw['identity'], cgroup=raw['cgroup'], mounts=raw['mounts'],
                affinity=affinity, ancestors=controls), sorted(set(gaps))


class LinuxReader:
    """Internal dependency seam. A supplied root is always marked fixture."""
    def __init__(self, root=None, affinity=None):
        self.root = Path(root) if root is not None else None
        self.affinity_reader = affinity or (lambda: sorted(os.sched_getaffinity(0)))

    @property
    def origin(self):
        return 'fixture' if self.root is not None else 'live_self_process'

    def read(self, absolute, limit=32768):
        if self.root is None and absolute.startswith('/proc/self/'):
            absolute = '/proc/' + str(os.getpid()) + absolute[len('/proc/self'):]
        path = Path(absolute) if self.root is None else self.root / absolute.lstrip('/')
        boundary = self.root if self.root is not None else (
            Path('/sys/fs/cgroup') if absolute.startswith('/sys/fs/cgroup/') else None)
        try:
            return read_file(path, limit, boundary).decode('utf-8')
        except (OSError, UnicodeError):
            return None

    def namespace(self, name):
        if self.root is not None:
            return self.read('/proc/self/ns/' + name, 1024)
        try:
            value = os.readlink('/proc/self/ns/' + name)
            require(re.fullmatch(name + r':\[[0-9]{1,20}\]', value) is not None)
            return value
        except OSError:
            return None

    def snapshot(self):
        identity = dict(host=self.read('/proc/sys/kernel/hostname', 1024),
                        machine=self.read('/etc/machine-id', 1024),
                        boot=self.read('/proc/sys/kernel/random/boot_id', 1024),
                        kernel=self.read('/proc/sys/kernel/osrelease', 1024),
                        arch=platform.machine(), cgroup_ns=self.namespace('cgroup'),
                        mount_ns=self.namespace('mnt'))
        # Wire uses mount_ns while the kernel namespace file is named mnt.
        mounts = self.read('/proc/self/mountinfo', MAX_TEXT)
        if mounts is not None:
            mounts = [line for line in mounts.splitlines() if ' - cgroup2 ' in line]
        cgroup = self.read('/proc/self/cgroup', 8192)
        paths = layout(cgroup, mounts)
        try:
            affinity = self.affinity_reader()
        except OSError:
            affinity = None
        ancestors = []
        for path in paths or []:
            ancestors.append(dict(path=path, controls={k: self.read(path + '/' + k) for k in CONTROLS}))
        return dict(identity=identity, cgroup=cgroup, mounts=mounts, affinity=affinity, ancestors=ancestors)


def collect(reader=None, system=None):
    system = platform.system() if system is None else system
    reader = LinuxReader() if reader is None else reader
    return dict(format=1, origin=reader.origin, scope='collector_process_only', platform=system,
                collector=dict(pid=os.getpid(), python=platform.python_version(),
                               captured_at=datetime.now(timezone.utc).isoformat(timespec='microseconds')),
                before=reader.snapshot() if system == 'Linux' else None,
                after=reader.snapshot() if system == 'Linux' else None)


def resource_summary(raw):
    exact(raw, ('format', 'origin', 'scope', 'platform', 'collector', 'before', 'after'))
    require(type(raw['format']) is int and raw['format'] == 1)
    require(raw['origin'] in ('fixture', 'live_self_process') and raw['scope'] == 'collector_process_only')
    string(raw['platform'], 64)
    exact(raw['collector'], ('pid', 'python', 'captured_at'))
    integer(raw['collector']['pid'], 1)
    require(re.fullmatch(r'3\.12\.[0-9]{1,3}', string(raw['collector']['python'], 32)) is not None)
    require(re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{6}\+00:00',
                         string(raw['collector']['captured_at'], 64)) is not None)
    gaps = ['COLLECTOR_ONLY', 'OBSERVATIONS_NOT_EXECUTION_ATTESTATION',
            'CORRECTNESS_ATTESTATION_UNAVAILABLE', 'TIMING_AUTHORIZATION_UNAVAILABLE']
    if raw['origin'] == 'fixture':
        gaps.append('FIXTURE_NOT_LIVE_EVIDENCE')
    if raw['platform'] != 'Linux':
        require(raw['before'] is None and raw['after'] is None)
        return dict(before=None, after=None, gaps=sorted(gaps + ['UNSUPPORTED_PLATFORM']))
    before, bg = snapshot_summary(raw['before'])
    after, ag = snapshot_summary(raw['after'])
    if before != after:
        gaps.append('RESOURCE_STATE_DRIFT')
    return dict(before=before, after=after, gaps=sorted(set(gaps + bg + ag)))


def safety():
    return dict(release_gate_eligible=False, **{key: 'UNPROVEN' for key in UNPROVEN})


def preparation_report(artifacts):
    require(artifacts['tool.py'] == read_file(Path(__file__)), 'E_TOOL_IDENTITY')
    return dict(format=1, kind='preparation', protocol=protocol({k: artifacts[k] for k in PROTOCOL_FILES}),
                resources=resource_summary(decode(artifacts['resources.json'])), **safety())


def new_directory(output):
    output = Path(output).absolute()
    for parent in output.parents:
        require(not parent.is_symlink(), 'E_PATH')
    try:
        output.mkdir()  # exclusive; including existing and dangling symlinks
    except FileExistsError:
        raise Invalid('E_DEST_EXISTS') from None
    return output


def write_complete(directory, kind, artifacts):
    for name, data in artifacts.items():
        require(len(data) <= MAX_BYTES, 'E_BOUNDS')
        with (directory / name).open('xb') as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
    manifest = dict(format=1, kind=kind, complete=True, release_gate_eligible=False,
                    artifacts={name: dict(bytes=len(data), sha256=sha(data)) for name, data in artifacts.items()})
    with (directory / '.completion.tmp').open('xb') as handle:
        handle.write(encoded(manifest))
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(directory / '.completion.tmp', directory / 'complete.json')


def prepare(output, *, root=ROOT, reader=None, system=None):
    directory = new_directory(output)
    artifacts = {name: read_file(root / 'tools/oracle' / source, root=root)
                 for name, source in PROTOCOL_FILES.items()}
    protocol(artifacts)  # verify before collecting any live facts
    artifacts['tool.py'] = read_file(Path(__file__))
    artifacts['resources.json'] = encoded(collect(reader, system))
    report = preparation_report(artifacts)
    artifacts['report.json'] = encoded(report)
    write_complete(directory, 'preparation', artifacts)
    return report


def load_bundle(directory):
    directory = Path(directory).absolute()
    expected = set(PROTOCOL_FILES) | {'tool.py', 'resources.json', 'report.json'}
    require(directory.is_dir() and not directory.is_symlink(), 'E_BUNDLE')
    names = set()
    with os.scandir(directory) as entries:
        for entry in entries:
            require(len(names) < len(expected) + 1, 'E_BUNDLE')
            names.add(entry.name)
    require(names == expected | {'complete.json'}, 'E_BUNDLE')
    manifest = decode(read_file(directory / 'complete.json', root=directory))
    exact(manifest, ('format', 'kind', 'complete', 'release_gate_eligible', 'artifacts'))
    require(type(manifest['format']) is int and manifest['format'] == 1 and
            manifest['kind'] == 'preparation' and manifest['complete'] is True and
            manifest['release_gate_eligible'] is False, 'E_BUNDLE')
    exact(manifest['artifacts'], expected)
    artifacts = {}
    for name in sorted(expected):
        entry = manifest['artifacts'][name]
        exact(entry, ('bytes', 'sha256'))
        integer(entry['bytes'], 0, MAX_BYTES)
        require(re.fullmatch('[0-9a-f]{64}', string(entry['sha256'], 64)) is not None)
        data = read_file(directory / name, root=directory)
        require(len(data) == entry['bytes'] and sha(data) == entry['sha256'], 'E_DIGEST')
        artifacts[name] = data
    report = preparation_report(artifacts)
    # Compare canonical bytes: Python equality would accept bools as integers.
    require(encoded(decode(artifacts['report.json'])) == encoded(report), 'E_REPORT')
    return report, sha(read_file(directory / 'complete.json', root=directory))


def differences(left, right, path=''):
    """Compare known leaves; unknown gaps never erase independent differences."""
    if left is None or right is None:
        return []
    if type(left) is dict and type(right) is dict:
        result = []
        for key in sorted(set(left) | set(right)):
            result += differences(left.get(key), right.get(key), path + '/' + key)
        return result
    if type(left) is list and type(right) is list and (left or right) and all(
            type(v) is dict for v in left + right):
        result = []
        if len(left) != len(right):
            result.append(path + '/length')
        for i, (a, b) in enumerate(zip(left, right)):
            result += differences(a, b, path + '/' + str(i))
        return result
    return [path] if left != right else []


def compare(left, right, output):
    # An output inside either input would invalidate its exact file inventory.
    # Resolve aliases for containment only; keep original paths for symlink checks.
    destination = Path(output).resolve()
    for source in (left, right):
        require(not destination.is_relative_to(Path(source).resolve()), 'E_DEST_IN_INPUT')
    directory = new_directory(output)
    a, ahash = load_bundle(left)
    b, bhash = load_bundle(right)
    mismatch = differences(a['resources']['before'], b['resources']['before'], '/before')
    mismatch += differences(a['resources']['after'], b['resources']['after'], '/after')
    # Scope/architecture includes unsupported platforms as well.
    ar = decode(read_file(Path(left) / 'resources.json'))
    br = decode(read_file(Path(right) / 'resources.json'))
    mismatch += differences(ar['platform'], br['platform'], '/platform')
    gaps = sorted(set(a['resources']['gaps'] + b['resources']['gaps']))
    report = dict(format=1, kind='visible_constraint_comparison', left_manifest_sha256=ahash,
                  right_manifest_sha256=bhash,
                  visible_constraints='different' if mismatch else (
                      'equal' if a['resources']['before'] is not None and b['resources']['before'] is not None
                      else 'unavailable'),
                  differences=sorted(set(mismatch)), gaps=gaps, **safety())
    write_complete(directory, 'comparison', {'report.json': encoded(report),
                                           'tool.py': read_file(Path(__file__))})
    return report


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Invalid('E_ARGUMENTS')


def main(argv=None):
    try:
        require(sys.version_info[:2] == (3, 12), 'E_PYTHON_VERSION')
        parser = Parser(description=__doc__)
        commands = parser.add_subparsers(dest='command', required=True, parser_class=Parser)
        p = commands.add_parser('prepare')
        p.add_argument('--output', required=True)
        p = commands.add_parser('compare')
        p.add_argument('left')
        p.add_argument('right')
        p.add_argument('--output', required=True)
        args = parser.parse_args(argv)
        report = prepare(args.output) if args.command == 'prepare' else compare(args.left, args.right, args.output)
        print(json.dumps(dict(status='completed', kind=report['kind'], **safety()), sort_keys=True))
        return 1 if report.get('visible_constraints') == 'different' else 0
    except Invalid as error:
        print(json.dumps(dict(error=error.code, **safety()), sort_keys=True), file=sys.stderr)
        return 2
    except (OSError, UnicodeError, ValueError, KeyError, TypeError, OverflowError, RecursionError):
        print(json.dumps(dict(error='E_INVALID', **safety()), sort_keys=True), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
