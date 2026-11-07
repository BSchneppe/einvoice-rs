"""Synthetic/offline protocol checks. No validator, real timers or performance evidence."""
from copy import deepcopy
from fractions import Fraction
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

MODULE = Path(__file__).resolve().parents[1] / 'protocol.py'
spec = importlib.util.spec_from_file_location('protocol', MODULE)
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
W = Fraction(948126237341157, 18014398509481984)


def exact(value):
    return Fraction(int(value['numerator']), int(value['denominator']))


class ProtocolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.plan = p.frozen()[0]

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(dir='/private/tmp' if sys.platform == 'darwin' else '/tmp')
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)

    def fixture(self, durations=((100, 10),), gap=0, clock='clock', start=0):
        pairs = []
        for i, lane_times in enumerate(durations):
            lanes = []
            for name, time in zip(('reference', 'native'), lane_times, strict=True):
                samples, cursor = [], start
                for c in self.plan['timed']:
                    duration = time(c) if callable(time) else time
                    samples.append(dict(c, clock_id=clock, start_ns=cursor,
                                        end_ns=cursor + duration, duration_ns=duration))
                    cursor += duration
                terminal = dict.fromkeys(p.TRUE_FLAGS, True)
                terminal.update(dict.fromkeys(p.FALSE_FLAGS, False))
                terminal.update(status='success', worker_exit_code=0, supervisor_exit_code=0)
                lanes.append(dict(lane=name, mode='primary', worker_count=1,
                                  invocation_id=f'invocation_{i}_{name}', clock_id=clock,
                                  envelope=dict(clock_id=clock, start_ns=start, end_ns=cursor + gap),
                                  terminal=terminal, samples=samples))
            pairs.append(dict(pair_id=f'pair_{i}', lanes=lanes))
        return dict(format=1, kind='synthetic_fixture', schedule_sha256=self.plan['schedule_sha256'], pairs=pairs)

    def reduce(self, fixture):
        report = p.reduce_fixture(p.pf.encoded(fixture))
        self.assert_safe(report)
        return report

    def assert_safe(self, report):
        self.assertIs(report['release_gate_eligible'], False)
        for name in p.UNPROVEN:
            self.assertEqual(report[name], 'UNPROVEN')
        self.assertFalse({'pass', 'gate_pass', 'target_met', 'accepted'} & report.keys())

    def reject(self, fixture, code=None):
        with self.assertRaises(p.Invalid) as raised:
            p.reduce_fixture(p.pf.encoded(fixture))
        if code:
            self.assertEqual(raised.exception.code, code)

    def cli(self, *args, code=0):
        result = subprocess.run([sys.executable, '-B', str(MODULE), *map(str, args)],
                                capture_output=True, timeout=30)
        self.assertEqual(result.returncode, code, result.stderr)
        self.assertEqual(result.stderr, b'')
        self.assertNotIn(b'secret_fixture', result.stdout)
        report = json.loads(result.stdout)
        self.assert_safe(report)
        return report

    def test_frozen_pins_counts_weights_and_expected_phases(self):
        plan = self.plan
        self.assert_safe(plan)
        self.assertEqual(len(plan['protocol']['sources']), 6)
        self.assertEqual(plan['protocol']['ordered_identity_sha256'],
                         'f3457d5ee31a12359386b20b18b892ba5b24a34e26e2e65b1dabed79e927aef7')
        self.assertEqual([g['count'] for g in plan['groups']],
                         [4, 3, 1, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 15, 3, 1])
        self.assertEqual([exact(g['weight']) for g in plan['groups']], [W] * 19)
        self.assertEqual(sum(exact(g['weight']) for g in plan['groups']),
                         Fraction(18014398509481983, 18014398509481984))
        self.assertEqual((len(plan['baseline']), len(plan['warmup']), len(plan['timed'])), (41, 820, 2050))
        self.assertEqual(plan['warmup'][-1], dict(warmup_pass=19, position=40, slot=40))
        self.assertEqual(len({s['input_sha256'] for s in plan['slots']}), 40)
        self.assertEqual([s['slot'] for s in plan['slots']], list(range(41)))
        changed = deepcopy(p.pf.PINS)
        changed['workload.json'] = (1217, '0' * 64)
        with patch.object(p.pf, 'PINS', changed), self.assertRaises(p.Invalid):
            p.frozen()

    def test_all_five_independently_reproduced_vectors(self):
        data = (MODULE.parent / 'tests/fixtures/schedule/vectors.jsonl').read_bytes()
        self.assertEqual(len(data), 980)
        self.assertEqual(hashlib.sha256(data).hexdigest(),
                         '1eb93aa8894db989cb2e8bacce32c86018efe7c66c2802215e28fe6ecd052f1e')
        for run, row in enumerate(map(json.loads, data.splitlines())):
            self.assertEqual(row['seed'], 271828 + run)
            for repeat in range(10):
                block = self.plan['timed'][410 * run + 41 * repeat:410 * run + 41 * (repeat + 1)]
                self.assertEqual([c['slot'] for c in block], row['manifest_slots'])
                self.assertEqual([c['position'] for c in block], list(range(41)))
                self.assertTrue(all(c['run'] == run and c['repeat'] == repeat for c in block))
        unsigned = {key: value for key, value in self.plan.items() if key != 'schedule_sha256'}
        self.assertEqual(hashlib.sha256(p.pf.encoded(unsigned)).hexdigest(), self.plan['schedule_sha256'])

    def test_unequal_groups_and_runs_hand_calculated(self):
        # Four-document group costs 20+10r, every other group 10+10r.
        # Weighted run sums are (200+190r)w; mean is 580w, not a raw-doc mean.
        group_zero = {s['slot'] for s in self.plan['slots'] if s['group_index'] == 0}
        def native(c):
            return (20 if c['slot'] in group_zero else 10) + 10 * c['run']
        report = self.reduce(self.fixture(((100, native),)))
        pair = report['pairs'][0]
        self.assertEqual(exact(pair['lanes'][1]['weighted_cost_ns']), 580 * W)
        self.assertEqual(exact(report['pooled_service_ratio']), Fraction(95, 29))
        for r, run in enumerate(pair['lanes'][1]['runs']):
            self.assertEqual(exact(run['weighted_cost_ns']), (200 + 190 * r) * W)
            self.assertEqual(exact(run['throughput_per_second']), Fraction(10**9, 1) / ((200 + 190 * r) * W))
            group = run['groups'][0]
            self.assertEqual((group['count'], group['sum_ns']), (40, 40 * (20 + 10 * r)))
        self.assertEqual(exact(pair['lanes'][1]['groups'][0]['mean_ns']), 40)
        self.assertEqual(pair['lanes'][1]['groups'][0]['p95_ns'], 60)
        self.assertEqual(exact(pair['lanes'][1]['a_max']), W / 50)

    def test_pooled_formula_differs_from_median_and_all_pair_criterion(self):
        # Ratios 10,10,10,1,1: median=10, pooled cost ratio=50/23.
        report = self.reduce(self.fixture(tuple((100, n) for n in (10, 10, 10, 100, 100))))
        self.assertEqual(exact(report['pooled_service_ratio']), Fraction(50, 23))
        self.assertEqual([exact(pair['service_ratio']) for pair in report['pairs']], [10, 10, 10, 1, 1])
        self.assertEqual(exact(report['leave_one_pair_out'][0]['service_ratio']), Fraction(20, 11))
        self.assertEqual([exact(v) for v in report['pair_service_ratio_range']], [1, 10])
        report = self.reduce(self.fixture(((100, 10), (100, 25))))
        self.assertEqual(exact(report['pooled_sufficient_only_bound']), Fraction(40, 7))
        self.assertEqual(exact(report['pairs'][1]['sufficient_only_bound']), 4)
        self.assertEqual(exact(report['pooled_group_service_ratios'][0]), Fraction(40, 7))

    def test_exact_five_and_display_below_five_never_authorize(self):
        report = self.reduce(self.fixture(((50, 10),)))
        self.assertEqual(exact(report['pooled_sufficient_only_bound']), 5)
        below = self.reduce(self.fixture(((49_999_999, 10_000_000),)))
        value = below['pooled_sufficient_only_bound']
        self.assertLess(exact(value), 5)
        self.assertEqual(value['display'], '5.000000')
        self.assertEqual(below['bound_interpretation'], 'conditional_sufficient_only_miss_is_inconclusive')
        self.assertEqual(p.rational(Fraction(1, 2_000_000))['display'], '0.000000')
        self.assertEqual(p.rational(Fraction(3, 2_000_000))['display'], '0.000002')

    def test_quantile_conventions(self):
        stats = p.distribution(list(range(40)))
        self.assertEqual(exact(stats['p50_ns']), Fraction(39, 2))
        self.assertEqual(stats['p95_ns'], 38)
        self.assertEqual(p.distribution([0, 1, 9])['p95_ns'], 9)

    def test_timer_to_gap_monotonicity_boundary_invariance_terminal_charge(self):
        original = self.fixture(((100, 20),), gap=60)
        base = self.reduce(original)
        upper = exact(base['pairs'][0]['lanes'][1]['upper_sensitivity_cost_ns'])
        self.assertEqual(upper, 380 * W + 60 * W / 50)
        for group, n in ((0, 4), (2, 1)):
            changed = deepcopy(original)
            sample = next(s for s in changed['pairs'][0]['lanes'][1]['samples']
                          if self.plan['slots'][s['slot']]['group_index'] == group)
            sample['end_ns'] -= 8
            sample['duration_ns'] -= 8
            result = self.reduce(changed)
            moved_upper = exact(result['pairs'][0]['lanes'][1]['upper_sensitivity_cost_ns'])
            self.assertEqual(moved_upper - upper, (W / 50 - W / (50 * n)) * 8)
            self.assertLessEqual(exact(result['pooled_sufficient_only_bound']),
                                 exact(base['pooled_sufficient_only_bound']))
        for ordinal in (0, 410, 820, 2050):
            changed = deepcopy(original)
            for s in changed['pairs'][0]['lanes'][1]['samples'][ordinal:]:
                s['start_ns'] += 60
                s['end_ns'] += 60
            self.assertEqual(exact(self.reduce(changed)['pooled_sufficient_only_bound']),
                             exact(base['pooled_sufficient_only_bound']))
        changed = deepcopy(original)
        changed['pairs'][0]['lanes'][1]['envelope']['end_ns'] += 5
        extended = self.reduce(changed)
        self.assertEqual(exact(extended['pairs'][0]['lanes'][1]['upper_sensitivity_cost_ns']) - upper, W / 10)
        changed = deepcopy(original)
        changed['pairs'][0]['lanes'][0]['envelope']['end_ns'] += 10000
        self.assertEqual(exact(self.reduce(changed)['pooled_sufficient_only_bound']),
                         exact(base['pooled_sufficient_only_bound']))

    def test_wrong_missing_duplicate_reordered_and_equal_hash_slots(self):
        fixture = self.fixture()
        rows = fixture['pairs'][0]['lanes'][0]['samples']
        for key, value in (('run', 1), ('repeat', 1), ('position', 40), ('slot', (rows[0]['slot'] + 1) % 41)):
            changed = deepcopy(fixture)
            changed['pairs'][0]['lanes'][0]['samples'][0][key] = value
            self.reject(changed, 'E_SCHEDULE')
        for action in ('missing', 'duplicate', 'reorder', 'extra'):
            changed = deepcopy(fixture)
            target = changed['pairs'][0]['lanes'][0]['samples']
            if action == 'missing':
                target.pop()
            elif action == 'duplicate':
                target[1] = deepcopy(target[0])
            elif action == 'reorder':
                target[0], target[1] = target[1], target[0]
            else:
                target.append(deepcopy(target[-1]))
            self.reject(changed)
        by_hash = {}
        for slot in self.plan['slots']:
            by_hash.setdefault(slot['input_sha256'], []).append(slot['slot'])
        a, b = next(slots for slots in by_hash.values() if len(slots) == 2)
        changed = deepcopy(fixture)
        target = changed['pairs'][0]['lanes'][0]['samples']
        for row in target:
            if row['slot'] == a:
                row['slot'] = b
            elif row['slot'] == b:
                row['slot'] = a
        self.reject(changed, 'E_SCHEDULE')

    def test_interval_conservation_containment_clock_overlap(self):
        fixture = self.fixture()
        for action in ('negative_envelope', 'short_envelope', 'outside', 'overlap', 'clock', 'env_clock', 'duration'):
            changed = deepcopy(fixture)
            lane = changed['pairs'][0]['lanes'][1]
            if action == 'negative_envelope':
                lane['envelope']['start_ns'] = lane['envelope']['end_ns'] + 1
            elif action == 'short_envelope':  # E<S as well as final sample outside E.
                lane['envelope']['end_ns'] -= 1
            elif action == 'outside':
                lane['envelope']['start_ns'] = 1
            elif action == 'overlap':
                lane['samples'][1]['start_ns'] -= 1
                lane['samples'][1]['duration_ns'] += 1
            elif action == 'clock':
                lane['samples'][0]['clock_id'] = 'wrong_clock'
            elif action == 'env_clock':
                lane['envelope']['clock_id'] = 'wrong_clock'
            else:
                lane['samples'][0]['duration_ns'] += 1
            self.reject(changed, 'E_INTERVAL')
        self.reject(self.fixture(((0, 0),)), 'E_DENOMINATOR')
        # A zero native group cannot produce a comparable group service ratio.
        self.reject(self.fixture(((100, lambda c: 0 if self.plan['slots'][c['slot']]['group_index'] == 0 else 10),)),
                    'E_DENOMINATOR')

    def test_failures_cleanup_and_actual_worker_oom_supervisor_zero(self):
        fixture = self.fixture()
        for key, value in [(k, False) for k in p.TRUE_FLAGS] + [(k, True) for k in p.FALSE_FLAGS] + [
                ('status', 'failed'), ('worker_exit_code', 137), ('supervisor_exit_code', 1)]:
            changed = deepcopy(fixture)
            changed['pairs'][0]['lanes'][1]['terminal'][key] = value
            self.reject(changed, 'E_INCOMPARABLE')
        for key in (*p.TRUE_FLAGS, *p.FALSE_FLAGS, 'worker_exit_code', 'status'):
            changed = deepcopy(fixture)
            del changed['pairs'][0]['lanes'][1]['terminal'][key]
            self.reject(changed)
        path = self.base / 'secret_fixture.json'
        fixture['pairs'][0]['lanes'][1]['terminal']['worker_oom'] = True
        path.write_bytes(p.pf.encoded(fixture))
        error = self.cli('reduce-fixture', path, code=2)
        self.assertEqual(error['error'], 'E_INCOMPARABLE')
        self.assertNotIn('pairs', error)

    def test_strict_schema_types_modes_bindings_unique_ids(self):
        base = self.fixture(((100, 10), (100, 25)))
        for key, value in (('release_gate_eligible', True), ('attestation', {}), ('extra', 0)):
            changed = deepcopy(base)
            changed[key] = value
            self.reject(changed)
        for key, value in (('kind', 'native'), ('format', 2), ('format', True), ('schedule_sha256', '0' * 64)):
            changed = deepcopy(base)
            changed[key] = value
            self.reject(changed)
        for key, value in (('worker_count', True), ('worker_count', 2), ('mode', 'full_html'),
                           ('lane', 'reference'), ('invocation_id', 'private/path')):
            changed = deepcopy(base)
            changed['pairs'][0]['lanes'][1][key] = value
            self.reject(changed)
        for value in (True, 1.0, -1, 1 << 63):
            for key in ('run', 'start_ns', 'end_ns', 'duration_ns'):
                changed = deepcopy(base)
                changed['pairs'][0]['lanes'][0]['samples'][0][key] = value
                self.reject(changed)
        changed = deepcopy(base)
        changed['pairs'][1]['pair_id'] = changed['pairs'][0]['pair_id']
        self.reject(changed)
        changed = deepcopy(base)
        changed['pairs'][1]['lanes'][1]['invocation_id'] = changed['pairs'][0]['lanes'][0]['invocation_id']
        self.reject(changed)
        for count in (0, 11):
            changed = deepcopy(base)
            changed['pairs'] = [] if count == 0 else [changed['pairs'][0]] * count
            self.reject(changed, 'E_BOUNDS')

    def test_parser_malformed_duplicate_size_depth_unicode_and_node_limits(self):
        for raw in (b'{', b'{"format":1,"format":1}', b'NaN', b'1.5', b'1e999', b'Infinity',
                    b'"\xff"', b'"\\ud800"', b'"\\u0000"', b'[' * 9 + b'0' + b']' * 9,
                    b'1' * 100, b'"' + b'a' * 65 + b'"', b' ' * (p.MAX_BYTES + 1)):
            with self.subTest(raw=raw[:40]), self.assertRaises(p.Invalid):
                p.decode(raw)
        with patch.object(p, 'MAX_NODES', 3), self.assertRaises(p.Invalid):
            p.decode(b'[0,1,2]')

    def test_maximum_ten_pair_fixture_is_within_all_parser_budgets(self):
        # Longest clocks and large valid 63-bit intervals, all 41,000 rows.
        duration = p.MAX_INT // 2050
        fixture = self.fixture(tuple((duration, duration) for _ in range(10)), clock='c' * 64)
        raw = p.pf.encoded(fixture)
        self.assertLess(len(raw), p.MAX_BYTES)
        report = p.reduce_fixture(raw)
        self.assert_safe(report)
        self.assertEqual(report['fixture_pair_count'], 10)
        self.assertEqual(exact(report['pooled_service_ratio']), 1)
        self.assertEqual(len(report['leave_one_pair_out']), 10)

    def test_cli_raw_bytes_stdout_only_and_nonregular_inputs(self):
        fixture = self.fixture()
        raw = json.dumps(fixture, indent=1).encode() + b'\n\n'
        path = self.base / 'secret_fixture.json'
        path.write_bytes(raw)
        before = sorted(self.base.iterdir())
        report = self.cli('reduce-fixture', path)
        self.assertEqual(report['fixture_sha256'], hashlib.sha256(raw).hexdigest())
        self.assertEqual(report['fixture_bytes'], len(raw))
        self.assertEqual(before, sorted(self.base.iterdir()))
        self.assertEqual(path.read_bytes(), raw)
        compact = p.reduce_fixture(p.pf.encoded(fixture))
        self.assertNotEqual(report['fixture_sha256'], compact['fixture_sha256'])
        self.assertEqual(report['pooled_service_ratio'], compact['pooled_service_ratio'])
        self.cli('schedule')
        self.cli('unknown_secret_fixture_argument', code=2)
        link = self.base / 'link'
        link.symlink_to(path)
        self.cli('reduce-fixture', link, code=2)
        directory_link = self.base / 'directory_link'
        directory_link.symlink_to(self.base, target_is_directory=True)
        self.cli('reduce-fixture', directory_link / path.name, code=2)
        self.cli('reduce-fixture', self.base, code=2)
        self.cli('reduce-fixture', self.base / 'absent', code=2)
        fifo = self.base / 'fifo'
        os.mkfifo(fifo)
        self.cli('reduce-fixture', fifo, code=2)
        path.write_bytes(b' ' * (p.MAX_BYTES + 1))
        self.assertEqual(self.cli('reduce-fixture', path, code=2)['error'], 'E_BOUNDS')


if __name__ == '__main__':
    unittest.main()
