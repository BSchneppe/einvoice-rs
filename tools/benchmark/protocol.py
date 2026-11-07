#!/usr/bin/env python3
"""Expected frozen schedule and exact synthetic arithmetic; never execution evidence."""
import argparse
from fractions import Fraction
import importlib.util
import json
from pathlib import Path
import re
import sys

# Reuse only the existing byte/pin helpers, never its collector or executor.
HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location('_protocol_preflight', HERE / 'preflight.py')
pf = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pf)
Invalid, require = pf.Invalid, pf.require
MAX_BYTES = 16 * 1024 * 1024
MAX_NODES = 1_000_000
MAX_INT = (1 << 63) - 1
VECTOR_SHA = '1eb93aa8894db989cb2e8bacce32c86018efe7c66c2802215e28fe6ecd052f1e'
JAVA_SHA = '597561aaf4a3b1a1587cf635d4c650c838fe04788884a5878cbf8627de270817'
UNPROVEN = ('attestation', 'correctness', 'custody', 'resources', 'execution', 'timing')
TRUE_FLAGS = ('initial_boundary_valid', 'baseline_complete', 'warmup_complete',
              'timed_complete', 'document_cleanup_complete', 'prepared_state_retired',
              'terminal_flush_complete')
FALSE_FLAGS = ('worker_oom', 'timed_out')


def safety():
    return dict(release_gate_eligible=False, **dict.fromkeys(UNPROVEN, 'UNPROVEN'))


def integer(value, minimum=0, maximum=MAX_INT):
    return pf.integer(value, minimum, maximum)


def opaque(value):
    require(type(value) is str and re.fullmatch(r'[A-Za-z0-9_-]{1,64}', value) is not None)
    return value


def decode(data):
    """Budget for 41,000 rows, enforced before recursive decoding and after it."""
    require(type(data) is bytes and len(data) <= MAX_BYTES, 'E_BOUNDS')
    try:
        text = data.decode('utf-8')
        depth, quoted, escaped = 0, False, False
        for char in text:
            if quoted:
                if escaped:
                    escaped = False
                elif char == '\\':
                    escaped = True
                elif char == '"':
                    quoted = False
            elif char == '"':
                quoted = True
            elif char in '[{':
                depth += 1
                require(depth <= 8, 'E_BOUNDS')
            elif char in ']}':
                depth -= 1

        def pairs(items):
            require(len(items) <= 32, 'E_BOUNDS')
            result = {}
            for key, value in items:
                require(key not in result, 'E_JSON')
                result[key] = value
            return result

        def number(token):
            require(len(token) <= 19, 'E_BOUNDS')
            return integer(int(token))

        def bad(_):
            raise Invalid('E_JSON')

        value = json.loads(text, object_pairs_hook=pairs, parse_int=number,
                           parse_float=bad, parse_constant=bad)
        pending, count = [value], 0
        while pending:
            item = pending.pop()
            count += 1
            require(count <= MAX_NODES, 'E_BOUNDS')
            if type(item) is dict:
                pending.extend(item.keys())
                pending.extend(item.values())
            elif type(item) is list:
                require(len(item) <= 2050, 'E_BOUNDS')
                pending.extend(item)
            elif type(item) is str:
                pf.string(item, 64)
        return value
    except (UnicodeError, ValueError, RecursionError):
        raise Invalid('E_JSON') from None


def rational(value):
    """Exact rational plus six-place display; integer ties-to-even, at the end."""
    value = Fraction(value)
    sign = '-' if value < 0 else ''
    quotient, remainder = divmod(abs(value.numerator) * 1_000_000, value.denominator)
    if 2 * remainder > value.denominator or (2 * remainder == value.denominator and quotient % 2):
        quotient += 1
    return dict(numerator=str(value.numerator), denominator=str(value.denominator),
                display=f'{sign}{quotient // 1_000_000}.{quotient % 1_000_000:06d}')


def coordinates(vectors):
    return [dict(run=run, repeat=repeat, position=position, slot=slot)
            for run, vector in enumerate(vectors)
            for repeat in range(10) for position, slot in enumerate(vector)]


def frozen():
    artifacts = {filename: pf.read_file(pf.ROOT / 'tools/oracle' / name)
                 for filename, name in pf.PROTOCOL_FILES.items()}
    summary = pf.protocol(artifacts)  # Verifies all six original byte pins first.
    config = pf.decode(artifacts['protocol-0.bin'])
    manifest = [pf.decode(line) for line in artifacts['protocol-1.bin'].splitlines()]
    groups = list(config['groups'])
    selected = [(index, row) for index, row in enumerate(manifest) if row['scope'] == 'complete']
    slots = [dict(slot=slot, manifest_row=index, group_index=groups.index(row['group']),
                  input_sha256=row['sha256'], input_bytes=row['bytes'])
             for slot, (index, row) in enumerate(selected)]
    weights = [Fraction.from_float(config['groups'][group]) for group in groups]
    counts = [summary['group_counts'][group] for group in groups]
    require((len(slots), len(groups), config['warmup_passes'], config['runs'], config['repeats'])
            == (41, 19, 20, 5, 10), 'E_SOURCE_PIN')
    fixture_dir = HERE / 'tests/fixtures/schedule'
    vector_bytes = pf.read_file(fixture_dir / 'vectors.jsonl')
    java_bytes = pf.read_file(fixture_dir / 'Schedule.java')
    require(pf.sha(vector_bytes) == VECTOR_SHA and pf.sha(java_bytes) == JAVA_SHA, 'E_SOURCE_PIN')
    rows = [pf.decode(line) for line in vector_bytes.splitlines()]
    require(len(rows) == 5, 'E_SOURCE_PIN')
    for run, row in enumerate(rows):
        require(row['run'] == run and row['seed'] == 271828 + run and
                sorted(row['manifest_slots']) == list(range(41)), 'E_SOURCE_PIN')
    vectors = [row['manifest_slots'] for row in rows]
    plan = dict(format=1, kind='expected_schedule', **safety(), protocol=summary,
                identity='expected_manifest_slots_only', slots=slots,
                groups=[dict(index=i, name=group, count=counts[i], weight=rational(weights[i]))
                        for i, group in enumerate(groups)],
                vector_sha256=VECTOR_SHA, reproduction_source_sha256=JAVA_SHA,
                tool_sha256=pf.sha(pf.read_file(Path(__file__))),
                preflight_tool_sha256=pf.sha(pf.read_file(HERE / 'preflight.py')),
                baseline=[dict(position=i, slot=i) for i in range(41)],
                warmup=[dict(warmup_pass=p, position=i, slot=i) for p in range(20) for i in range(41)],
                timed=coordinates(vectors))
    plan['schedule_sha256'] = pf.sha(pf.encoded(plan))
    return plan, weights, counts


def distribution(values):
    ordered = sorted(values)
    size = len(ordered)
    require(size > 0)
    median = Fraction(ordered[size // 2]) if size % 2 else Fraction(
        ordered[size // 2 - 1] + ordered[size // 2], 2)
    return dict(count=size, sum_ns=sum(values), mean_ns=rational(Fraction(sum(values), size)),
                p50_ns=rational(median), p95_ns=ordered[min(size - 1, 95 * size // 100)])


def lane_report(lane, expected_lane, plan, weights, counts, invocation_ids):
    pf.exact(lane, ('lane', 'mode', 'worker_count', 'invocation_id', 'clock_id',
                    'envelope', 'terminal', 'samples'))
    require(lane['lane'] == expected_lane and lane['mode'] == 'primary')
    require(integer(lane['worker_count'], 1, 1) == 1)
    invocation = opaque(lane['invocation_id'])
    require(invocation not in invocation_ids)
    invocation_ids.add(invocation)
    clock = opaque(lane['clock_id'])
    terminal = lane['terminal']
    pf.exact(terminal, (*TRUE_FLAGS, *FALSE_FLAGS, 'status', 'worker_exit_code', 'supervisor_exit_code'))
    require(terminal['status'] == 'success', 'E_INCOMPARABLE')
    for key in TRUE_FLAGS:
        require(terminal[key] is True, 'E_INCOMPARABLE')
    for key in FALSE_FLAGS:
        require(terminal[key] is False, 'E_INCOMPARABLE')
    require(integer(terminal['worker_exit_code']) == 0 and
            integer(terminal['supervisor_exit_code']) == 0, 'E_INCOMPARABLE')
    envelope = lane['envelope']
    pf.exact(envelope, ('clock_id', 'start_ns', 'end_ns'))
    start, end = integer(envelope['start_ns']), integer(envelope['end_ns'])
    require(envelope['clock_id'] == clock and start <= end, 'E_INTERVAL')
    samples = lane['samples']
    require(type(samples) is list and len(samples) == 2050, 'E_SCHEDULE')
    run_groups = [[[] for _ in counts] for _ in range(5)]
    last_end, timed_sum = start, 0
    for sample, coordinate in zip(samples, plan['timed'], strict=True):
        pf.exact(sample, ('run', 'repeat', 'position', 'slot', 'clock_id',
                          'start_ns', 'end_ns', 'duration_ns'))
        for key in coordinate:
            require(integer(sample[key]) == coordinate[key], 'E_SCHEDULE')
        begin, stop, duration = (integer(sample[key]) for key in ('start_ns', 'end_ns', 'duration_ns'))
        require(sample['clock_id'] == clock and start <= begin <= stop <= end and
                last_end <= begin and duration == stop - begin, 'E_INTERVAL')
        last_end = stop
        timed_sum += duration
        group = plan['slots'][sample['slot']]['group_index']
        run_groups[sample['run']][group].append(duration)
    elapsed = end - start
    require(timed_sum <= elapsed, 'E_INTERVAL')
    runs, costs = [], []
    all_groups = [[] for _ in counts]
    for run, group_values in enumerate(run_groups):
        require([len(v) for v in group_values] == [10 * n for n in counts], 'E_SCHEDULE')
        cost = sum((weight * Fraction(sum(values), len(values))
                    for weight, values in zip(weights, group_values, strict=True)), Fraction())
        require(cost > 0, 'E_DENOMINATOR')
        costs.append(cost)
        runs.append(dict(run=run, groups=[distribution(v) for v in group_values],
                         weighted_cost_ns=rational(cost), throughput_per_second=rational(10**9 / cost)))
        for target, values in zip(all_groups, group_values, strict=True):
            target.extend(values)
    cost = sum(costs) / 5
    residual = elapsed - timed_sum
    a_max = max(w / (50 * n) for w, n in zip(weights, counts, strict=True))
    upper = cost + a_max * residual
    report = dict(lane=expected_lane, invocation_id=invocation, runs=runs,
                  groups=[distribution(v) for v in all_groups], weighted_cost_ns=rational(cost),
                  elapsed_ns=elapsed, timed_sum_ns=timed_sum, residual_ns=residual)
    if expected_lane == 'native':
        report.update(a_max=rational(a_max), upper_sensitivity_cost_ns=rational(upper))
    return report, cost, upper, all_groups


def reduce_fixture(data):
    # Retain exact bytes throughout; parsing/derived sorting never replaces this hash input.
    fixture = decode(data)
    pf.exact(fixture, ('format', 'kind', 'schedule_sha256', 'pairs'))
    require(integer(fixture['format'], 1, 1) == 1 and fixture['kind'] == 'synthetic_fixture')
    plan, weights, counts = frozen()
    require(fixture['schedule_sha256'] == plan['schedule_sha256'], 'E_BINDING')
    pairs = fixture['pairs']
    require(type(pairs) is list and 1 <= len(pairs) <= 10, 'E_BOUNDS')
    pair_ids, invocation_ids = set(), set()
    reports, costs = [], []
    pooled_groups = {lane: [[] for _ in counts] for lane in ('reference', 'native')}
    for pair in pairs:
        pf.exact(pair, ('pair_id', 'lanes'))
        pair_id = opaque(pair['pair_id'])
        require(pair_id not in pair_ids)
        pair_ids.add(pair_id)
        require(type(pair['lanes']) is list and len(pair['lanes']) == 2)
        results = [lane_report(lane, name, plan, weights, counts, invocation_ids)
                   for lane, name in zip(pair['lanes'], ('reference', 'native'), strict=True)]
        reference, native = results
        service, bound = reference[1] / native[1], reference[1] / native[2]
        group_ratios = [Fraction(sum(j), sum(n)) for j, n in zip(reference[3], native[3], strict=True)
                        if sum(n) > 0]
        require(len(group_ratios) == 19, 'E_DENOMINATOR')
        reports.append(dict(pair_id=pair_id, lanes=[r[0] for r in results],
                            service_ratio=rational(service), sufficient_only_bound=rational(bound),
                            group_service_ratios=[rational(q) for q in group_ratios]))
        costs.append((reference[1], native[1], native[2], service, bound))
        for name, result in zip(('reference', 'native'), results, strict=True):
            for target, values in zip(pooled_groups[name], result[3], strict=True):
                target.extend(values)
    j, n, u = (sum(c[i] for c in costs) for i in range(3))
    leave_one_out = [dict(omitted_pair_id=reports[i]['pair_id'],
                          service_ratio=rational((j - c[0]) / (n - c[1])),
                          sufficient_only_bound=rational((j - c[0]) / (u - c[2])))
                     for i, c in enumerate(costs) if len(costs) > 1]
    group_ratios = [Fraction(sum(a), sum(b)) for a, b in zip(
        pooled_groups['reference'], pooled_groups['native'], strict=True)]
    return dict(format=1, kind='synthetic_fixture_report', **safety(),
                assumptions='declared_fixture_only_not_verified_execution',
                bound_interpretation='conditional_sufficient_only_miss_is_inconclusive',
                sample_identity='expected_coordinates_not_observed_inputs',
                fixture_bytes=len(data), fixture_sha256=pf.sha(data),
                schedule_sha256=plan['schedule_sha256'], fixture_pair_count=len(pairs),
                future_collection_pair_count=10, pairs=reports,
                pooled_service_ratio=rational(j / n), pooled_sufficient_only_bound=rational(j / u),
                pooled_groups={name: [distribution(v) for v in values]
                               for name, values in pooled_groups.items()},
                pooled_group_service_ratios=[rational(q) for q in group_ratios],
                pair_service_ratio_range=[rational(min(c[3] for c in costs)), rational(max(c[3] for c in costs))],
                pair_bound_range=[rational(min(c[4] for c in costs)), rational(max(c[4] for c in costs))],
                leave_one_pair_out=leave_one_out)


def main():
    class Parser(argparse.ArgumentParser):
        def error(self, message):
            sys.stdout.buffer.write(pf.encoded(dict(kind='protocol_error', error='E_USAGE', **safety())))
            self.exit(2)

    parser = Parser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('schedule', help='emit expected coordinates only')
    reducer = commands.add_parser('reduce-fixture', help='reduce bounded synthetic fixture only')
    reducer.add_argument('input')
    args = parser.parse_args()
    try:
        if args.command == 'schedule':
            report = frozen()[0]
        else:
            data = pf.read_file(Path(args.input), limit=MAX_BYTES)
            report = reduce_fixture(data)
        sys.stdout.buffer.write(pf.encoded(report))
        return 0
    except (Invalid, OSError) as error:
        sys.stdout.buffer.write(pf.encoded(dict(kind='protocol_error', error=(
            error.code if isinstance(error, Invalid) else 'E_IO'), **safety())))
        return 2


if __name__ == '__main__':
    sys.exit(main())
