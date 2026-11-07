"""Explicit integration check with the actual producer and supplied pinned ZIP.

All builds and schema executions are real. Scheduling hooks only edit this
test's own regular input/snapshot after a real child returns. No corpus labels
or fabricated observations are used. Run separately from offline helper tests.
"""
import argparse
import json
from pathlib import Path
import sys
from unittest.mock import patch
import observe


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise ValueError('integration output must be empty')
    results = []
    original_run = observe.run
    for scenario in ['unchanged', 'external-change', 'snapshot-change', 'malformed']:
        source = root / (scenario + '.xml')
        source.write_bytes(b'<alpha>' if scenario == 'malformed' else b'<alpha/>')
        output = root / scenario
        expected = observe.identify(source.read_bytes())
        child_calls = []

        def scheduled(command, destination, stage, **kwargs):
            result = original_run(command, destination, stage, **kwargs)
            if stage in ['complete', 'partial']:
                child_calls.append({'stage': stage, 'command': command,
                                    'actual_input': json.loads(result.stdout)['observation']['input_sha256']})
                if scenario == 'external-change':
                    source.write_bytes(b'<edited-by-caller/>')
                elif scenario == 'snapshot-change' and stage == 'complete':
                    (output / 'input.xml').write_bytes(b'<bravo/>')
            return result

        failure = None
        with patch.object(observe, 'run', scheduled), patch.object(sys, 'argv', [
                str(Path(observe.__file__)), '--archive', str(args.archive),
                '--input', str(source), '--output', str(output)]):
            try:
                observe.main()
            except (ValueError, RuntimeError) as error:
                failure = str(error)
        if scenario in ['unchanged', 'external-change']:
            assert failure is None, failure
            receipt = json.loads((output / 'receipt.json').read_bytes())
            assert receipt['input'] == expected
            assert len(child_calls) == 2
            assert all(c['actual_input'] == expected['sha256'] for c in child_calls)
            assert observe.identify((output / 'input.xml').read_bytes()) == expected
        elif scenario == 'snapshot-change':
            assert failure == 'actual observation input differs from admitted snapshot', failure
            assert len(child_calls) == 2
            assert child_calls[0]['actual_input'] != child_calls[1]['actual_input']
            assert (output / 'partial.json').is_file()  # Actual contradictory output retained.
            assert not (output / 'receipt.json').exists()
            assert (output / 'failure.json').is_file()
        else:
            assert failure and 'complete failed' in failure, failure
            status = json.loads((output / 'complete.status.json').read_bytes())
            assert status['returncode'] != 0 and status['launch_error'] is None
            assert (output / 'complete.stderr').read_bytes()
            assert (output / 'complete.json').read_bytes() == b''  # No invented Observation.
            assert (output / 'failure.json').is_file()
            assert not (output / 'receipt.json').exists()
        results.append({'scenario': scenario, 'failure': failure, 'actual_child_calls': child_calls})
    (root / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print('four real producer scenarios passed; schema-only, no native acceptance')


if __name__ == '__main__':
    main()
