"""Offline harness fixtures only; no fixture is validation evidence."""
from copy import deepcopy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

MODULE = Path(__file__).resolve().parents[1] / 'preflight.py'
spec = importlib.util.spec_from_file_location('preflight', MODULE)
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


class PreflightTests(unittest.TestCase):
    def setUp(self):
        # /tmp and /var are symlinks on Darwin; use the canonical writable root.
        self.tmp = tempfile.TemporaryDirectory(dir='/private/tmp' if sys.platform == 'darwin' else '/tmp')
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.fixture = self.base / 'fixture'
        facts = {
            '/proc/self/cgroup': '0::/job\n',
            '/proc/self/mountinfo': '29 23 0:26 / /sys/fs/cgroup rw,nosuid,nodev,noexec,relatime - cgroup2 cgroup rw\n',
            '/proc/sys/kernel/hostname': 'fixture-host\n',
            '/etc/machine-id': 'fixture-machine\n',
            '/proc/sys/kernel/random/boot_id': 'fixture-boot\n',
            '/proc/sys/kernel/osrelease': 'fixture-kernel\n',
            '/proc/self/ns/cgroup': 'cgroup:[17]',
            '/proc/self/ns/mnt': 'mnt:[18]',
        }
        for ancestor in ('', '/job'):
            for key, value in {
                'cgroup.controllers': 'cpu cpuset memory pids\n',
                'cpu.max': '25000 100000\n',
                'cpuset.cpus.effective': '0-1\n',
                'memory.max': '67108864\n',
                'memory.swap.max': '0\n',
                'pids.max': '32\n',
            }.items():
                facts['/sys/fs/cgroup' + ancestor + '/' + key] = value
        for path, value in facts.items():
            self.fact(path, value)

    def fact(self, path, text):
        target = self.fixture / path.lstrip('/')
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding='utf-8')

    def reader(self, affinity=None):
        return p.LinuxReader(self.fixture, affinity=affinity or (lambda: [0]))

    def prepare(self, name='left', **kwargs):
        output = self.base / name
        report = p.prepare(output, reader=self.reader(), system='Linux', **kwargs)
        self.assert_safe(report)
        return output

    def assert_safe(self, report):
        self.assertIs(report['release_gate_eligible'], False)
        for key in p.UNPROVEN:
            self.assertEqual(report[key], 'UNPROVEN')

    def cli(self, *args, code=0):
        result = subprocess.run([sys.executable, '-B', str(MODULE), *map(str, args)],
                                capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        self.assertNotIn('secret-held-out', result.stdout + result.stderr)
        message = json.loads(result.stderr if code == 2 else result.stdout)
        self.assert_safe(message)
        if code == 2:
            self.assertRegex(message['error'], '^E_[A-Z_]+$')
        return message

    def rewrite(self, bundle, name, value):
        data = p.encoded(value) if not isinstance(value, bytes) else value
        (bundle / name).write_bytes(data)
        manifest = json.loads((bundle / 'complete.json').read_bytes())
        manifest['artifacts'][name] = dict(bytes=len(data), sha256=p.sha(data))
        (bundle / 'complete.json').write_bytes(p.encoded(manifest))

    def raw(self, bundle):
        return json.loads((bundle / 'resources.json').read_bytes())

    def test_protocol_summary_and_fixture_never_authorize(self):
        bundle = self.prepare()
        report, _ = p.load_bundle(bundle)
        protocol = report['protocol']
        self.assertEqual((protocol['complete_inputs'], len(protocol['weights']), protocol['warmup_passes'],
                          protocol['runs'], protocol['repeats']), (41, 19, 20, 5, 10))
        self.assertEqual(sum(protocol['group_counts'].values()), 41)
        self.assertEqual((protocol['samples_per_run'], protocol['samples_per_mode']), (410, 2050))
        self.assertEqual(len(protocol['ordered_identity_sha256']), 64)
        self.assertNotIn('sinabis-', json.dumps(protocol))
        self.assertIn('FIXTURE_NOT_LIVE_EVIDENCE', report['resources']['gaps'])
        result = p.compare(bundle, bundle, self.base / 'comparison')
        self.assertEqual(result['visible_constraints'], 'equal')
        self.assertIn('HIDDEN_ANCESTORS_UNPROVEN', result['gaps'])
        self.assert_safe(result)
        # Comparison outputs cannot be consumed as preparation evidence.
        self.cli('compare', self.base / 'comparison', bundle, '--output', self.base / 'bad', code=2)

    def test_cli_prepare_compare_and_destinations(self):
        bundle = self.base / 'live'
        self.cli('prepare', '--output', bundle)
        p.load_bundle(bundle)
        self.cli('compare', bundle, bundle, '--output', self.base / 'equal')
        self.cli('prepare', '--output', bundle, code=2)
        self.assertEqual(len(list(bundle.glob('complete.json'))), 1)
        for name, target in [('symlink', bundle), ('dangling', self.base / 'absent')]:
            dest = self.base / name
            dest.symlink_to(target)
            self.cli('prepare', '--output', dest, code=2)
        self.cli('prepare', '--output', self.base / 'override', '--allow-incomplete', code=2)
        self.cli('prepare', '--output', self.base / 'execute', '--command', 'secret-held-out', code=2)
        self.assertFalse((self.base / 'execute').exists())

    def test_darwin_and_unknown_platform_are_unavailable(self):
        for system in ('Darwin', 'Other'):
            bundle = self.base / system
            report = p.prepare(bundle, reader=self.reader(), system=system)
            self.assertEqual(report['resources']['gaps'][-1], 'UNSUPPORTED_PLATFORM')
            self.assertIsNone(report['resources']['before'])
            self.assert_safe(report)
            result = p.compare(bundle, bundle, self.base / (system + '-compare'))
            self.assertEqual(result['visible_constraints'], 'unavailable')
            self.assert_safe(result)

    def test_comparison_preserves_both_inputs_for_nested_destinations(self):
        left, right = self.prepare('left'), self.prepare('right')
        original = {bundle: {f.name: f.read_bytes() for f in bundle.iterdir()}
                    for bundle in (left, right)}
        cases = [
            (left, right, left / 'comparison'),
            (left, right, right / 'comparison'),
            (left, right, left / '..' / 'left' / 'comparison'),
            (left / '..' / 'left', right, left / 'comparison'),
            (left, right, Path(os.path.relpath(left / 'comparison'))),
        ]
        for a, b, destination in cases:
            with self.subTest(destination=str(destination)):
                self.cli('compare', a, b, '--output', destination, code=2)
                for bundle in (left, right):
                    self.assertEqual(set(f.name for f in bundle.iterdir()), set(original[bundle]))
                    self.assertEqual({f.name: f.read_bytes() for f in bundle.iterdir()}, original[bundle])
                    p.load_bundle(bundle)
        # A textual prefix is not filesystem containment.
        self.cli('compare', left, right, '--output', self.base / 'left-comparison')
        for bundle in (left, right):
            p.load_bundle(bundle)

    def test_present_layout_fields_are_validated_with_unavailable_counterparts(self):
        mount = '29 23 0:26 / /sys/fs/cgroup rw - cgroup2 cgroup rw'
        hybrid = '1:cpu:/job\n0::/job\n'
        cases = [
            (True, None), (1, None), ({'unexpected': 'object'}, None),
            (None, {'unexpected': 'object'}), ('0::/../outside\n', None),
            (hybrid, [False]), ('x' * 8193, None), (None, [mount] * 17),
            (None, ['x' * 8193]), (hybrid, ['x' * 8193]),
            (None, [mount.replace('/sys/fs/cgroup', '/sys/fs/cgroup/../outside')]),
        ]
        for i, (cgroup, mounts) in enumerate(cases):
            with self.subTest(case=i):
                # Build a valid unavailable-layout producer report independently
                # of validation of the subsequently supplied malformed facts.
                reader = self.reader()
                snapshot = reader.snapshot()
                snapshot.update(cgroup=None, mounts=None, ancestors=[])
                bundle = self.base / ('malformed-layout-' + str(i))
                with patch.object(reader, 'snapshot', return_value=snapshot):
                    p.prepare(bundle, reader=reader, system='Linux')
                raw = self.raw(bundle)
                report = json.loads((bundle / 'report.json').read_bytes())
                for side in ('before', 'after'):
                    raw[side].update(cgroup=cgroup, mounts=mounts)
                    report['resources'][side].update(cgroup=cgroup, mounts=mounts)
                self.rewrite(bundle, 'resources.json', raw)
                self.rewrite(bundle, 'report.json', report)
                error = self.cli('compare', bundle, bundle, '--output',
                                 self.base / ('bad-layout-output-' + str(i)), code=2)
                self.assertIn(error['error'], ('E_INVALID', 'E_BOUNDS', 'E_PATH'))

    def test_well_formed_unavailable_layouts_remain_observations(self):
        mount = '29 23 0:26 / /sys/fs/cgroup rw - cgroup2 cgroup rw'
        cases = [(None, None), ('0::/job\n', None), (None, [mount]),
                 ('0::/job\n', []), ('1:cpu:/job\n', [mount]),
                 ('1:cpu:/job\n0::/job\n', [mount]), ('0::/job\n', [mount, mount])]
        for i, (cgroup, mounts) in enumerate(cases):
            with self.subTest(case=i):
                reader = self.reader()
                snapshot = reader.snapshot()
                snapshot.update(cgroup=cgroup, mounts=mounts, ancestors=[])
                bundle = self.base / ('unavailable-layout-' + str(i))
                with patch.object(reader, 'snapshot', return_value=snapshot):
                    p.prepare(bundle, reader=reader, system='Linux')
                report, _ = p.load_bundle(bundle)
                self.assertIn('UNAVAILABLE_CGROUP2_LAYOUT', report['resources']['gaps'])
                self.cli('compare', bundle, bundle, '--output',
                         self.base / ('unavailable-layout-output-' + str(i)))

    def test_raw_mismatches_include_period_swap_identity_and_ancestors(self):
        left = self.prepare()
        cases = [
            ('period', '/sys/fs/cgroup/job/cpu.max', '25000 200000\n', 'cpu.max'),
            ('ratio-equal', '/sys/fs/cgroup/job/cpu.max', '50000 200000\n', 'cpu.max'),
            ('ancestor', '/sys/fs/cgroup/cpu.max', '10000 100000\n', 'cpu.max'),
            ('memory', '/sys/fs/cgroup/job/memory.max', '33554432\n', 'memory.max'),
            ('swap', '/sys/fs/cgroup/job/memory.swap.max', 'max\n', 'memory.swap.max'),
            ('pid', '/sys/fs/cgroup/job/pids.max', '16\n', 'pids.max'),
            ('cpuset', '/sys/fs/cgroup/job/cpuset.cpus.effective', '0\n', 'cpuset.cpus.effective'),
            ('host', '/proc/sys/kernel/hostname', 'other\n', 'host'),
            ('boot', '/proc/sys/kernel/random/boot_id', 'other\n', 'boot'),
            ('kernel', '/proc/sys/kernel/osrelease', 'other\n', 'kernel'),
            ('machine', '/etc/machine-id', 'other\n', 'machine'),
            ('namespace', '/proc/self/ns/cgroup', 'cgroup:[99]', 'cgroup_ns'),
        ]
        for name, path, value, field in cases:
            with self.subTest(name=name):
                target = self.fixture / path.lstrip('/')
                old = target.read_text()
                self.fact(path, value)
                right = self.prepare(name)
                self.fact(path, old)
                out = self.base / (name + '-compare')
                self.cli('compare', left, right, '--output', out, code=1)
                report = json.loads((out / 'report.json').read_bytes())
                self.assertEqual(report['visible_constraints'], 'different')
                self.assertTrue(any(field in d for d in report['differences']), report)
                self.assertIn('HIDDEN_ANCESTORS_UNPROVEN', report['gaps'])
                self.assert_safe(report)
        right = self.base / 'affinity'
        p.prepare(right, reader=self.reader(lambda: [1]), system='Linux')
        report = p.compare(left, right, self.base / 'affinity-compare')
        self.assertIn('/before/affinity', report['differences'])
        with patch.object(p.platform, 'machine', return_value='other-architecture'):
            right = self.prepare('architecture')
        report = p.compare(left, right, self.base / 'architecture-compare')
        self.assertIn('/before/identity/arch', report['differences'])
        self.assert_safe(report)

    def test_gaps_do_not_erase_mismatches(self):
        left = self.prepare()
        (self.fixture / 'sys/fs/cgroup/job/memory.max').unlink()
        self.fact('/sys/fs/cgroup/job/cpu.max', 'max 100000\n')
        right = self.prepare('right')
        report = p.compare(left, right, self.base / 'comparison')
        self.assertEqual(report['visible_constraints'], 'different')
        self.assertIn('UNAVAILABLE_CONTROL_MEMORY.MAX', report['gaps'])
        self.assertTrue(any('cpu.max' in d for d in report['differences']))
        self.assert_safe(report)

    def test_hidden_namespace_root_missing_controllers_unreadable_ancestor(self):
        self.fact('/proc/self/cgroup', '0::/\n')
        self.fact('/sys/fs/cgroup/cgroup.controllers', 'cpu\n')
        (self.fixture / 'sys/fs/cgroup/memory.max').unlink()
        bundle = self.prepare()
        report, _ = p.load_bundle(bundle)
        gaps = report['resources']['gaps']
        for gap in ('HIDDEN_ANCESTORS_UNPROVEN', 'NAMESPACE_SCOPE_UNPROVEN',
                    'MISSING_CONTROLLERS', 'UNAVAILABLE_CONTROL_MEMORY.MAX'):
            self.assertIn(gap, gaps)
        result = p.compare(bundle, bundle, self.base / 'comparison')
        self.assert_safe(result)

    def test_cgroup_affinity_controller_and_limit_drift(self):
        for field in ('affinity', 'cgroup', 'controller', 'quota'):
            with self.subTest(field=field):
                reader = self.reader()
                before = reader.snapshot()
                after = deepcopy(before)
                if field == 'affinity':
                    after['affinity'] = [1]
                elif field == 'cgroup':
                    after['cgroup'] = '0::/\n'
                    after['ancestors'] = after['ancestors'][-1:]
                elif field == 'controller':
                    after['ancestors'][0]['controls']['cgroup.controllers'] = 'cpu'
                else:
                    after['ancestors'][0]['controls']['cpu.max'] = 'max 100000'
                with patch.object(reader, 'snapshot', side_effect=[before, after]):
                    report = p.prepare(self.base / field, reader=reader, system='Linux')
                self.assertIn('RESOURCE_STATE_DRIFT', report['resources']['gaps'])
                self.assert_safe(report)

    def test_unknown_unreadable_ambiguous_layout_and_affinity(self):
        for name, cgroup, mounts in [
            ('hybrid', '1:cpu:/job\n0::/job\n', None),
            ('missing', None, None),
            ('ambiguous', '0::/job\n', '29 23 0:26 / /sys/fs/cgroup rw - cgroup2 cgroup rw\n' * 2),
        ]:
            with self.subTest(name=name):
                if cgroup is None:
                    (self.fixture / 'proc/self/cgroup').unlink()
                else:
                    self.fact('/proc/self/cgroup', cgroup)
                if mounts:
                    self.fact('/proc/self/mountinfo', mounts)
                reader = self.reader(lambda: (_ for _ in ()).throw(OSError()))
                report = p.prepare(self.base / name, reader=reader, system='Linux')
                self.assertIn('UNAVAILABLE_CGROUP2_LAYOUT', report['resources']['gaps'])
                self.assertIn('UNAVAILABLE_AFFINITY', report['resources']['gaps'])
                self.assert_safe(report)

    def test_source_pins_reject_defaults_groups_order_and_identity_changes(self):
        for i, name in enumerate(p.PINS):
            with self.subTest(name=name):
                root = self.base / ('repo' + str(i))
                for source in p.PINS:
                    target = root / 'tools/oracle' / source
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(p.ROOT / 'tools/oracle' / source, target)
                target = root / 'tools/oracle' / name
                target.write_bytes(target.read_bytes() + b'\n')
                with self.assertRaises(p.Invalid) as error:
                    p.prepare(self.base / ('bad' + str(i)), root=root, reader=self.reader(), system='Linux')
                self.assertEqual(error.exception.code, 'E_SOURCE_PIN')
                self.assertFalse((self.base / ('bad' + str(i)) / 'complete.json').exists())
        for change in ('default', 'group', 'order', 'identity'):
            bundle = self.prepare(change)
            if change in ('default', 'group'):
                config = json.loads((bundle / 'protocol-0.bin').read_bytes())
                if change == 'default':
                    config['runs'] = 4
                else:
                    config['groups'].pop(next(iter(config['groups'])))
                self.rewrite(bundle, 'protocol-0.bin', config)
            else:
                rows = (bundle / 'protocol-1.bin').read_bytes().splitlines()
                if change == 'order':
                    rows.reverse()
                else:
                    rows[0] = rows[1]
                self.rewrite(bundle, 'protocol-1.bin', b'\n'.join(rows) + b'\n')
            self.assertEqual(self.cli('compare', bundle, bundle, '--output', self.base / (change + '-out'), code=2)['error'], 'E_SOURCE_PIN')

    def test_digest_forged_report_promotion_and_tool_identity(self):
        for name in ('stale', 'summary', 'bool', 'promotion', 'tool', 'raw'):
            bundle = self.prepare(name)
            if name == 'stale':
                (bundle / 'report.json').write_bytes(b'{}')
                code = 'E_DIGEST'
            elif name == 'tool':
                self.rewrite(bundle, 'tool.py', b'# fabricated collector\n')
                code = 'E_TOOL_IDENTITY'
            elif name == 'raw':
                raw = self.raw(bundle)
                raw['before']['ancestors'][0]['controls']['cpu.max'] = '50000 100000'
                self.rewrite(bundle, 'resources.json', raw)
                code = 'E_REPORT'
            else:
                report = json.loads((bundle / 'report.json').read_bytes())
                if name == 'summary':
                    report['protocol']['runs'] = 4
                elif name == 'bool':
                    report['format'] = True
                else:
                    report['release_gate_eligible'] = True
                    report['same_effective_budget'] = True
                self.rewrite(bundle, 'report.json', report)
                code = 'E_REPORT'
            message = self.cli('compare', bundle, bundle, '--output', self.base / (name + '-out'), code=2)
            self.assertEqual(message['error'], code)
            self.assertFalse((self.base / (name + '-out') / 'complete.json').exists())

    def test_partial_extra_missing_symlink_and_manifest_path_escape(self):
        for name in ('partial', 'extra', 'missing', 'symlink', 'escape', 'manifest-promotion', 'manifest-bool'):
            bundle = self.prepare(name)
            if name == 'partial':
                (bundle / 'complete.json').unlink()
            elif name == 'extra':
                (bundle / 'unexpected').write_text('x')
            elif name == 'missing':
                (bundle / 'resources.json').unlink()
            elif name == 'symlink':
                data = (bundle / 'resources.json').read_bytes()
                target = self.base / 'outside.json'
                target.write_bytes(data)
                (bundle / 'resources.json').unlink()
                (bundle / 'resources.json').symlink_to(target)
            else:
                manifest = json.loads((bundle / 'complete.json').read_bytes())
                if name == 'escape':
                    manifest['artifacts']['../outside'] = manifest['artifacts'].pop('resources.json')
                elif name == 'manifest-promotion':
                    manifest['enforcement'] = True
                else:
                    manifest['format'] = True
                (bundle / 'complete.json').write_bytes(p.encoded(manifest))
            self.cli('compare', bundle, bundle, '--output', self.base / (name + '-out'), code=2)

    def test_interrupted_write_never_completes(self):
        bundle = self.base / 'interrupted'
        real_open = Path.open
        def interrupted(path, *args, **kwargs):
            if path.name == 'resources.json' and args == ('xb',):
                raise OSError('simulated interrupted write')
            return real_open(path, *args, **kwargs)
        with patch.object(Path, 'open', interrupted):
            with self.assertRaises(OSError):
                p.prepare(bundle, reader=self.reader(), system='Linux')
        self.assertTrue(bundle.is_dir())
        self.assertFalse((bundle / 'complete.json').exists())
        self.cli('compare', bundle, bundle, '--output', self.base / 'bad', code=2)
        self.cli('prepare', '--output', bundle, code=2)

    def test_strict_json_and_resource_types_at_cli_boundary(self):
        cases = [b'{"format":1,"format":1}', b'{"x":NaN}', b'{"x":Infinity}',
                 b'{"x":1e999}', b'{"x":"\\ud800"}', b'\xff', b'{',
                 b'[' * 13 + b']' * 13, b'{"x":' + b'9' * 100 + b'}',
                 b'[' + b'0,' * 8192 + b'0]', b'"' + b'x' * (p.MAX_TEXT + 1) + b'"',
                 b' ' * (p.MAX_BYTES + 1)]
        for i, value in enumerate(cases):
            bundle = self.prepare('json' + str(i))
            self.rewrite(bundle, 'resources.json', value)
            self.cli('compare', bundle, bundle, '--output', self.base / ('jout' + str(i)), code=2)
        for i, field in enumerate(('format', 'pid', 'affinity', 'promotion', 'scope', 'origin')):
            bundle = self.prepare('type' + str(i))
            raw = self.raw(bundle)
            if field == 'format':
                raw['format'] = True
            elif field == 'pid':
                raw['collector']['pid'] = True
            elif field == 'affinity':
                raw['before']['affinity'] = [True]
            elif field == 'promotion':
                raw['before']['enforced'] = True
            else:
                raw[field] = 'native'
            self.rewrite(bundle, 'resources.json', raw)
            self.cli('compare', bundle, bundle, '--output', self.base / ('tout' + str(i)), code=2)

    def test_resource_numeric_and_cpu_list_pathological_bounds(self):
        invalid = {
            'cpu.max': ['0 100000', '25000 0', '-1 100000', '1 max', '1 2 3', '9' * 100 + ' 1'],
            'memory.max': ['-1', '1.5', 'true', str(1 << 63), '9' * 100],
            'memory.swap.max': ['-1'],
            'pids.max': ['1e2'],
            'cpuset.cpus.effective': ['0-1000000', '0-4096', '2-1', '0,0', '2,1', '0,,1',
                                      '9' * 100, ','.join(map(str, range(4097))), '0' * 32769],
        }
        i = 0
        for key, values in invalid.items():
            for value in values:
                with self.subTest(key=key, value=value[:40]):
                    bundle = self.prepare('numeric' + str(i))
                    raw = self.raw(bundle)
                    raw['before']['ancestors'][0]['controls'][key] = value
                    self.rewrite(bundle, 'resources.json', raw)
                    self.cli('compare', bundle, bundle, '--output', self.base / ('nout' + str(i)), code=2)
                    i += 1

    def test_proc_paths_mount_roots_and_symlink_confinement(self):
        for i, member in enumerate(('/../outside', '/job/../outside', '/job//x', '/' + '/'.join(['x'] * 65))):
            self.fact('/proc/self/cgroup', '0::' + member + '\n')
            with self.assertRaises(p.Invalid):
                p.prepare(self.base / ('path' + str(i)), reader=self.reader(), system='Linux')
        self.fact('/proc/self/cgroup', '0::/job\n')
        for i, mount in enumerate(('/tmp', '/sys/fs/cgroup/../other', '/sys/fs/cgroup/\\057escape')):
            self.fact('/proc/self/mountinfo', '29 23 0:26 / ' + mount + ' rw - cgroup2 cgroup rw\n')
            with self.assertRaises(p.Invalid):
                p.prepare(self.base / ('mount' + str(i)), reader=self.reader(), system='Linux')
        self.fact('/proc/self/mountinfo', '29 23 0:26 / /sys/fs/cgroup rw - cgroup2 cgroup rw\n')
        target = self.fixture / 'sys/fs/cgroup/job/memory.max'
        target.unlink()
        target.symlink_to(self.fixture / 'sys/fs/cgroup/memory.max')
        with self.assertRaises(p.Invalid) as error:
            p.prepare(self.base / 'link', reader=self.reader(), system='Linux')
        self.assertEqual(error.exception.code, 'E_PATH')

    def test_nonroot_mount_mapping_is_visible_scope_only(self):
        self.fact('/proc/self/cgroup', '0::/tenant/job\n')
        self.fact('/proc/self/mountinfo', '29 23 0:26 /tenant /sys/fs/cgroup rw - cgroup2 cgroup rw\n')
        bundle = self.prepare()
        report, _ = p.load_bundle(bundle)
        self.assertEqual([a['path'] for a in report['resources']['before']['ancestors']],
                         ['/sys/fs/cgroup/job', '/sys/fs/cgroup'])
        self.assertIn('HIDDEN_ANCESTORS_UNPROVEN', report['resources']['gaps'])
        self.assert_safe(report)

    def test_import_does_not_trust_forged_visible_resource_summary(self):
        bundle = self.prepare()
        report = json.loads((bundle / 'report.json').read_bytes())
        report['resources']['before']['ancestors'][0]['controls']['memory.swap.max'] = 'max'
        report['resources']['gaps'] = []
        self.rewrite(bundle, 'report.json', report)
        message = self.cli('compare', bundle, bundle, '--output', self.base / 'bad', code=2)
        self.assertEqual(message['error'], 'E_REPORT')


if __name__ == '__main__':
    unittest.main()
