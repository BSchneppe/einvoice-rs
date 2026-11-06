"""Regression tests for reviewed evidence failures; synthetic processes are tooling only."""
import copy
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from reproduce import accounting, execute, inspect_family, pin_runtime, process_ok
from common import Invalid, Report, fresh_path, json_bytes, json_lines, read_file, relative, sha, strict_json
from verify import (Bundle, archive_members, check_history, check_lineage, check_requests, classify,
                    family_receipts, occurrences, process_binding, verify)

ROOT = Path(__file__).resolve().parents[1]


class PublicationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bundle = Bundle(ROOT / 'publication.json')
        cls.requests, cls.recipes = check_lineage(cls.bundle)
        cls.freeze = cls.bundle.historical_json('family-design/freeze-v1.json')
        cls.ledger = cls.bundle.historical_json('family-design/v3/proposed-ledger.json')
        cls.records = [o['record'] for o in cls.bundle.historical_json('family-run/observations.json')['observations']]

    def test_full_offline_bundle_without_external_dependencies(self):
        report = verify(self.bundle)
        self.assertEqual('verified', report['status'])
        self.assertEqual(67, report['historical_input_streams'])
        self.assertEqual(64, report['completed_request_positions'])
        self.assertEqual('unavailable', report['external_dependency_integrity']['status'])
        self.assertFalse(report['historical_799_payload_available'])
        self.assertFalse(report['native_execution_attested'])

    def test_internally_consistent_59_requests_are_not_the_frozen_batch(self):
        rows = copy.deepcopy(self.requests[:-1])
        with self.assertRaisesRegex(Invalid, 'request_membership'):
            check_requests(rows, self.freeze, self.ledger)

    def test_reordering_requests_and_forging_matching_ordinals_rejected(self):
        rows = copy.deepcopy(self.requests)
        rows[1], rows[2] = rows[2], rows[1]
        rows[1]['ordinal'], rows[2]['ordinal'] = 1, 2
        with self.assertRaisesRegex(Invalid, 'request_freeze_order_family'):
            check_requests(rows, self.freeze, self.ledger)

    def test_family_modification_rejected(self):
        rows = copy.deepcopy(self.requests)
        rows[1]['family'] = 'F02'
        with self.assertRaisesRegex(Invalid, 'request_freeze_order_family'):
            check_requests(rows, self.freeze, self.ledger)

    def test_every_single_receipt_identity_contradiction_rejected(self):
        bundle, r = self.bundle, self.requests[0]
        prefix = 'family-run/raw/00-baseline'
        inp = bundle.historical_json(prefix + '.input-receipt.json')
        ret = bundle.historical_json(prefix + '.output-receipt.json')
        raw = bundle.data(prefix + '.bridge.json')
        args = [inp['closure_sha256'], inp['driver_sha256'], inp['freeze_sha256'], inp['ledger_sha256']]
        for field in ('ordinal', 'id', 'group', 'input_sha256', 'input_bytes', 'original_request_row',
                      'closure_sha256', 'driver_sha256', 'freeze_sha256', 'ledger_sha256'):
            with self.subTest(input=field):
                changed = copy.deepcopy(inp)
                changed[field] = -1 if type(changed[field]) is int else 'contradiction'
                with self.assertRaises(Invalid):
                    family_receipts(r, raw, changed, ret, *args)
        for field in ('id', 'ordinal', 'input_sha256', 'bridge_record_sha256', 'evaluate_returned'):
            with self.subTest(output=field):
                changed = copy.deepcopy(ret)
                changed[field] = False if field == 'evaluate_returned' else 'contradiction'
                with self.assertRaises(Invalid):
                    family_receipts(r, raw, inp, changed, *args)

    def test_impossible_command_or_stale_stream_digest_rejected(self):
        bundle = self.bundle
        prefix = 'family-run/processes/evaluate60'
        command = bundle.historical_json(prefix + '.command.json')
        meta = bundle.historical_json(prefix + '.metadata.json')
        stdout, stderr = bundle.data(prefix + '.stdout'), bundle.data(prefix + '.stderr')
        for field in ('argv', 'cwd', 'stdout_sha256', 'stderr_sha256', 'exit_code', 'status'):
            with self.subTest(field=field):
                changed = copy.deepcopy(meta)
                changed[field] = ['impossible'] if field == 'argv' else -1 if field == 'exit_code' else 'contradiction'
                with self.assertRaises(Invalid):
                    process_binding(command, changed, stdout, stderr, command['argv'], command['cwd'], 600)

    def test_rejected_records_require_each_applicable_stage(self):
        original = next(r for r in self.records if r['acceptance'] == 'rejected' and r['schema'] == 'valid')
        for stage in ('val-sch.1', 'val-sch.2'):
            record = copy.deepcopy(original)
            del record['stages'][stage]
            record['business'] = 'not_evaluated'
            record['findings'] = []
            with self.subTest(stage=stage), self.assertRaises(Invalid):
                classify(record)

    def test_unknown_missing_and_contradictory_outcomes_never_complete(self):
        for mutation in ('unknown_stage', 'undefined', 'missing_xml', 'errors', 'bad_processing', 'unmatched'):
            r = copy.deepcopy(self.records[0])
            if mutation == 'unknown_stage': r['stages']['val-xml'] = 'skipped'
            if mutation == 'undefined': r['reference_acceptance'] = 'undefined'
            if mutation == 'missing_xml': del r['stages']['val-xml']
            if mutation == 'errors': r['processing_errors'] = ['engine error']
            if mutation == 'bad_processing': r['processing'] = 'skipped'
            if mutation == 'unmatched': r['target_matched'], r['scenario'] = False, None
            with self.subTest(mutation=mutation), self.assertRaises(Invalid):
                classify(r)

    def test_real_xsd_terminal_is_completed_schema_rejection(self):
        records = [r for r in self.records if r['schema'] == 'invalid']
        self.assertTrue(records)
        for r in records:
            self.assertNotIn('val-sch.1', r['stages'])
            self.assertEqual('completed_schema_rejection', classify(r))

    def test_effective_policy_warning_can_accept_invalid_cen(self):
        records = [r for r in self.records if r['acceptance'] == 'accepted' and r['stages'].get('val-sch.1') == 'invalid']
        self.assertTrue(records)
        for r in records:
            self.assertEqual('completed_business_observation', classify(r))
            self.assertTrue(any(f['severity'] == 'error' and f['effective_severity'] == 'warning' for f in r['findings']))

    def test_synthetic_xml_terminal_stays_explicitly_unsupported(self):
        r = copy.deepcopy(self.records[0])
        r.update(stages={'val-xml': 'invalid'}, schema='not_evaluated', business='not_evaluated',
                 acceptance='rejected', reference_acceptance='reject')
        self.assertEqual('unsupported_xml_terminal', classify(r))
        self.assertFalse(any(x['stages'].get('val-xml') == 'invalid' for x in self.records))

    def test_completed_stage_verdicts_bind_base_diagnostics(self):
        for stage in ('val-xml', 'val-xsd', 'val-sch.1', 'val-sch.2'):
            bucket = 'findings' if stage.startswith('val-sch') else 'structural_findings'
            for severity in ('error', 'warning'):
                r = copy.deepcopy(self.records[0])
                r[bucket].append({'id': None, 'severity': severity, 'effective_severity': 'warning',
                    'location': None, 'line': None, 'column': None, 'message': 'synthetic', 'stage': stage})
                with self.subTest(stage=stage, severity=severity), self.assertRaisesRegex(Invalid, 'base_severity_stage'):
                    classify(r)
            for severity in (None, 'information'):
                r = copy.deepcopy(self.records[0])
                r[bucket].append({'id': None, 'severity': severity, 'effective_severity': severity,
                    'location': None, 'line': None, 'column': None, 'message': 'synthetic', 'stage': stage})
                self.assertEqual('completed_business_observation', classify(r))
        for stage in ('val-sch.1', 'val-sch.2'):
            r = copy.deepcopy(self.records[0]); r['stages'][stage] = 'invalid'
            with self.subTest(stage=stage), self.assertRaisesRegex(Invalid, 'invalid_stage_without_observation'):
                classify(r)
            r['processing'], r['processing_errors'], r['acceptance'] = 'failed', ['synthetic incomplete report'], 'indeterminate'
            self.assertEqual('processing_failed', classify(r))
        for severity in (None, 'information'):
            r = copy.deepcopy(next(r for r in self.records if r['schema'] == 'invalid'))
            for f in r['structural_findings']: f['severity'] = severity
            with self.subTest(xsd_severity=severity), self.assertRaisesRegex(Invalid, 'invalid_stage_without_observation'):
                classify(r)
            self.assertEqual('process_incomplete', classify(r, process_ok=False))

    def test_failed_processing_with_all_stage_keys_cannot_render_success(self):
        r = copy.deepcopy(self.records[0])
        r.update(processing='failed', processing_errors=['synthetic tooling fixture'], acceptance='indeterminate')
        self.assertEqual('processing_failed', classify(r))
        r['acceptance'] = 'accepted'
        with self.assertRaises(Invalid): classify(r)

    def mutated_bundle(self, original, value):
        bundle = copy.copy(self.bundle)
        bundle.files = dict(bundle.files)
        bundle.files[bundle.rows[original]['retained_path']] = json_bytes(value)
        return bundle

    def test_occurrence_duplicates_order_message_null_and_base_severity_preserved(self):
        findings = self.bundle.historical_json('family-run/finding-occurrences.json')
        for mutation in ('duplicate', 'reorder', 'message', 'null', 'base'):
            rows = copy.deepcopy(findings)
            if mutation == 'duplicate': rows.append(copy.deepcopy(rows[0]))
            if mutation == 'reorder': rows[0], rows[1] = rows[1], rows[0]
            if mutation == 'message': rows[0]['finding']['message'] = rows[0]['finding']['message'].strip()
            if mutation == 'null': rows[0]['finding']['line'] = '0'
            if mutation == 'base':
                item = next(x for x in rows if x['finding']['severity'] != x['finding']['effective_severity'])
                item['finding']['severity'] = item['finding']['effective_severity']
            with self.subTest(mutation=mutation), self.assertRaisesRegex(Invalid, 'derived_occurrences'):
                check_history(self.mutated_bundle('family-run/finding-occurrences.json', rows), self.requests, self.recipes)

    def test_scoped_record_cannot_be_submitted_as_complete(self):
        r = copy.deepcopy(self.records[0]); r['scope'] = 'scoped'
        with self.assertRaisesRegex(Invalid, 'record_shape_or_scope'): classify(r)

    def test_postrun_closure_failure_keeps_observed_per_request_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / 'raw').mkdir(); (root / 'processes').mkdir()
            for r in self.requests:
                prefix = f"raw/{r['ordinal']:02d}-{r['id']}"
                for suffix in ('.bridge.json', '.input-receipt.json', '.output-receipt.json'):
                    (root / (prefix + suffix)).write_bytes(self.bundle.data('family-run/' + prefix + suffix))
            for name in ('closure-before.json', 'closure-after.json', 'configuration-loaded.json'):
                (root / 'raw' / name).write_bytes(self.bundle.data('family-run/raw/' + name))
            after = strict_json((root / 'raw/closure-after.json').read_bytes())
            after['closure_sha256'] = '0' * 64
            (root / 'raw/closure-after.json').write_bytes(json_bytes(after))
            (root / 'runtime-identities.json').write_bytes(json_bytes(self.bundle.json('provenance.json')['runtime_pins']))
            (root / 'processes/evaluate60.stdout').write_bytes(self.bundle.data('family-run/processes/evaluate60.stdout'))
            outcome = {**self.bundle.historical_json('family-run/processes/evaluate60.metadata.json'), 'started': True}
            closure = sha(self.bundle.data('family-run/runtime-closure.tsv'))
            failed = {**outcome, 'exit_code': 7}
            partial, _ = inspect_family(root, self.requests, failed, closure, self.bundle)
            self.assertTrue(all(r['status'] == 'incomplete' and r['process_attempted'] for r in partial))
            self.assertTrue(all(r['returned_receipt_observed'] for r in partial))
            rows, differences = inspect_family(root, self.requests, outcome, closure, self.bundle)
            self.assertEqual(len(self.requests), len(rows))
            self.assertTrue(all(r['status'] == 'incomplete' for r in rows))
            self.assertTrue(all(r['input_receipt_observed'] and r['returned_receipt_observed'] and r['record_bytes_observed'] for r in rows))
            self.assertTrue(all(r['error'] == 'replay_closure_receipts' for r in rows))
            self.assertEqual(len(self.requests), len(differences))

    def test_missing_extra_and_truncated_streams_fail(self):
        for raw in (b'', b'{}', b'{}\n\n', b'{}\n{'):
            with self.subTest(raw=raw), self.assertRaises(Invalid): json_lines(raw)
        data = self.bundle.data('family-run/processes/evaluate60.stdout')
        for changed in (b'\n'.join(data.split(b'\n')[:-2]) + b'\n', data + data.splitlines(keepends=True)[0]):
            bundle = copy.copy(self.bundle); bundle.files = dict(bundle.files)
            bundle.files[bundle.rows['family-run/processes/evaluate60.stdout']['retained_path']] = changed
            # Refresh wrapper commitments: stream completeness must stand on its
            # request/raw bindings even when wrapper hashes are internally consistent.
            meta = bundle.historical_json('family-run/processes/evaluate60.metadata.json')
            meta['stdout_sha256'] = sha(changed)
            bundle.files[bundle.rows['family-run/processes/evaluate60.metadata.json']['retained_path']] = json_bytes(meta)
            run = bundle.historical_json('family-run/run-metadata.json')
            run['processes'] = [meta if x.get('name') == 'evaluate60' else x for x in run['processes']]
            bundle.files[bundle.rows['family-run/run-metadata.json']['retained_path']] = json_bytes(run)
            with self.assertRaisesRegex(Invalid, 'raw_stream_order_count'): check_history(bundle, self.requests, self.recipes)


class BoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name).resolve()
        self.addCleanup(self.temp.cleanup)

    def test_strict_json_duplicate_nonfinite_surrogates_and_bad_utf8(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":NaN}', b'Infinity', b'1e999', b'"\\ud800"', b'"\xff"'):
            with self.subTest(raw=raw), self.assertRaises(Invalid): strict_json(raw)

    def test_safe_paths_existing_outputs_symlinks_and_hardlinks(self):
        for path in ('../x', '/x', 'x/../y', './x', 'x//y', 'C:x', 'x\\y', 'x\ny'):
            with self.subTest(path=path), self.assertRaises(Invalid): relative(path)
        source = self.root / 'source'; source.write_bytes(b'evidence')
        with self.assertRaises(Invalid): fresh_path(source, [])
        alias = self.root / 'alias'; alias.symlink_to(source)
        with self.assertRaises(Invalid): read_file(alias)
        with self.assertRaises(Invalid): fresh_path(alias, [])
        dangling = self.root / 'dangling'; dangling.symlink_to(self.root / 'absent')
        with self.assertRaises(Invalid): fresh_path(dangling, [])
        hard = self.root / 'hard'; os.link(source, hard)
        with self.assertRaises(Invalid): read_file(hard)
        self.assertEqual(b'evidence', source.read_bytes())

    def test_protected_output_descendants_and_ancestors_rejected(self):
        protected = self.root / 'bundle'; protected.mkdir()
        with self.assertRaises(Invalid): fresh_path(protected / 'result.json', [protected])
        with self.assertRaises(Invalid): fresh_path(self.root / 'future', [self.root / 'future/input'])

    def test_fresh_report_incomplete_then_final_never_overwrites_existing(self):
        path = self.root / 'report.json'
        report = Report(path, [ROOT])
        self.assertEqual('incomplete', strict_json(path.read_bytes())['status'])
        report.finish({'status': 'verified'})
        with self.assertRaises(Invalid): Report(path, [ROOT])
        self.assertEqual('verified', strict_json(path.read_bytes())['status'])

    def test_archive_traversal_duplicates_symlink_extra_and_bad_member_pin(self):
        for mode in ('traversal', 'duplicate', 'symlink', 'extra', 'bad_pin'):
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, 'w') as z:
                z.writestr('safe', b'ok')
                if mode == 'traversal': z.writestr('../escape', b'bad')
                if mode == 'duplicate':
                    import warnings
                    with warnings.catch_warnings():
                        warnings.simplefilter('ignore'); z.writestr('safe', b'ok')
                if mode == 'symlink':
                    info = zipfile.ZipInfo('link'); info.external_attr = 0o120777 << 16
                    z.writestr(info, 'safe')
                if mode == 'extra': z.writestr('extra', b'bad')
            pin = {'safe': {'bytes': 2, 'sha256': sha(b'bad' if mode == 'bad_pin' else b'ok')}}
            with self.subTest(mode=mode), self.assertRaises(Invalid): archive_members(stream.getvalue(), pin, exact=True)

    def test_real_subprocess_launch_nonzero_partial_timeout_and_all_output_nonzero(self):
        # These are actual Python subprocesses, not fake Oracle/native results.
        cases = [('launch', ['/definitely/missing/development-tool'], 1, 'launch_error', b''),
                 ('nonzero', [sys.executable, '-c', "import sys; print('before failure',flush=True);sys.exit(7)"], 2, 'exited', b'before failure\n'),
                 ('timeout', [sys.executable, '-c', "import time; print('partial',flush=True);time.sleep(10)"], 1, 'timeout', b'partial\n'),
                 ('all_records_nonzero', [sys.executable, '-c', "import sys;print('all declared tooling records',flush=True);sys.exit(9)"], 2, 'exited', b'all declared tooling records\n'),
                 ('success', [sys.executable, '-c', "print('tooling success')"], 2, 'exited', b'tooling success\n')]
        for name, argv, timeout, status, stdout in cases:
            with self.subTest(name=name):
                result = execute(self.root, name, argv, self.root, timeout, [{'id': 'tooling', 'ordinal': 0}], dict(os.environ))
                self.assertEqual(status, result['status'])
                self.assertEqual(stdout, (self.root / (name + '.stdout')).read_bytes())
                self.assertEqual(sha(stdout), result['stdout_sha256'])
                self.assertEqual(name != 'launch', result['started'])
                self.assertEqual(name == 'success', process_ok(result))
                self.assertTrue((self.root / (name + '.command.json')).exists())
                self.assertTrue((self.root / (name + '.metadata.json')).exists())
                if name == 'launch': self.assertIsNone(result['exit_code'])

    def test_real_subprocess_interruption_preserves_partial_stream_and_metadata(self):
        argv = [sys.executable, '-c', "import os,signal,time;print('interrupted tooling',flush=True);os.kill(os.getppid(),signal.SIGINT);time.sleep(10)"]
        result = execute(self.root, 'interrupt', argv, self.root, 3, [], dict(os.environ))
        self.assertEqual('interrupted', result['status'])
        self.assertTrue(result['started'])
        self.assertFalse(process_ok(result))
        self.assertEqual(b'interrupted tooling\n', (self.root / 'interrupt.stdout').read_bytes())
        self.assertEqual('KeyboardInterrupt', result['exception']['kind'])

    def test_absent_receipt_never_proves_request_not_reached(self):
        row = accounting({'id': 'tooling', 'ordinal': 0, 'sha256': '0' * 64}, None, False, False, False, False)
        self.assertEqual('no_request_specific_execution_evidence', row['evidence'])
        self.assertEqual('incomplete', row['status'])
        self.assertTrue(row['absence_does_not_prove_request_not_reached'])

    def test_publication_rejects_undeclared_file_symlink_and_alias_cycles(self):
        dest = self.root / 'bundle'
        shutil.copytree(ROOT, dest, ignore=shutil.ignore_patterns('__pycache__'))
        extra = dest / 'undeclared'; extra.write_bytes(b'new')
        with self.assertRaisesRegex(Invalid, 'inventory_file_set'): Bundle(dest / 'publication.json')
        extra.unlink(); extra.symlink_to(dest / 'selection.json')
        with self.assertRaisesRegex(Invalid, 'symlink_path'): Bundle(dest / 'publication.json')
        extra.unlink()
        inventory = strict_json((dest / 'publication.json').read_bytes())
        selection = strict_json((dest / 'selection.json').read_bytes())
        aliases = [r for r in selection['files'] if r.get('alias_of_original')]
        aliases[0]['alias_of_original'] = aliases[0]['path']
        changed = json_bytes(selection); (dest / 'selection.json').write_bytes(changed)
        pin = next(r for r in inventory['files'] if r['path'] == 'selection.json')
        pin.update(bytes=len(changed), sha256=sha(changed))
        (dest / 'publication.json').write_bytes(json_bytes(inventory))
        with self.assertRaisesRegex(Invalid, 'alias_reference'): Bundle(dest / 'publication.json')

    def test_wrong_supplied_runtime_rejected_without_launch(self):
        bundle = Bundle(ROOT / 'publication.json')
        with self.assertRaises((Invalid, OSError)):
            pin_runtime(bundle, Path(sys.executable), self.root)

    def test_fifo_evidence_and_helper_fail_without_waiting_for_peer(self):
        dest = self.root / 'bundle'
        shutil.copytree(ROOT, dest, ignore=shutil.ignore_patterns('__pycache__'))
        for name in ('README.md', 'common.py'):
            target = dest / name
            original = target.read_bytes(); target.unlink(); os.mkfifo(target)
            report = self.root / (name + '.result.json')
            result = subprocess.run([sys.executable, '-B', str(dest / 'verify.py'), '--manifest',
                str(dest / 'publication.json'), '--out', str(report)], capture_output=True, timeout=5)
            self.assertNotEqual(0, result.returncode)
            if name == 'README.md':
                self.assertEqual('file_alias_or_type', strict_json(report.read_bytes())['error'])
            else:
                self.assertIn(b'bootstrap_source_type', result.stderr)
            target.unlink(); target.write_bytes(original)

    def test_direct_entrypoints_compile_held_source_with_ordinary_caches(self):
        # Cache only unchanged reviewed source. Audit compilation, not a substituted
        # checker: no altered bytecode or digest-bypass fixture is needed.
        audit_wrapper = '''import hashlib,json,pathlib,runpy,sys
entry, audit = sys.argv[1:3]
root = pathlib.Path(entry).parent
sys.path.insert(0, str(root))  # Match a directly invoked script's import path.
expected = {str(root / n) for n in ('common.py','verify.py')}
events = []
def observe(event, args):
    if event == 'compile' and args[1] in expected:
        source = args[0]
        events.append({'path': args[1], 'sha256': hashlib.sha256(source).hexdigest()})
sys.addaudithook(observe)
sys.argv = [entry] + sys.argv[3:]
try:
    runpy.run_path(entry, run_name='__main__')
finally:
    pathlib.Path(audit).write_text(json.dumps(events))
'''
        for redirected in (False, True):
            folder = self.root / str(redirected); folder.mkdir()
            dest = folder / 'bundle'
            shutil.copytree(ROOT, dest, ignore=shutil.ignore_patterns('__pycache__'))
            env = dict(os.environ); env.pop('PYTHONPYCACHEPREFIX', None)
            if redirected: env['PYTHONPYCACHEPREFIX'] = str(folder / 'cache')
            helpers = [str(dest / n) for n in ('common.py', 'verify.py')]
            subprocess.run([sys.executable, '-c',
                'import py_compile,sys; [py_compile.compile(p,doraise=True) for p in sys.argv[1:]]',
                *helpers], env=env, check=True, capture_output=True, timeout=10)
            for entry in ('verify.py', 'reproduce.py'):
                audit = folder / (entry + '.audit.json')
                out = folder / (entry + '.out')
                args = ['--manifest', str(dest / 'publication.json'), '--out', str(out)] if entry == 'verify.py' else [
                    '--bundle', str(dest), '--batch', 'both', '--python', sys.executable,
                    '--jdk-home', str(folder / 'absent-jdk'), '--validator', str(folder / 'absent-jar'),
                    '--config', str(folder / 'absent-config'), '--out', str(out)]
                result = subprocess.run([sys.executable, '-B', '-c', audit_wrapper, str(dest / entry),
                    str(audit), *args], env=env, capture_output=True, timeout=15)
                self.assertEqual(0 if entry == 'verify.py' else 2, result.returncode, result.stderr)
                events = strict_json(audit.read_bytes())
                for helper in helpers:
                    self.assertIn({'path': helper, 'sha256': sha(Path(helper).read_bytes())}, events)
            # A cached helper never supplies missing source. This is a setup error,
            # not an invoice failure, and does not launch Java.
            (dest / 'common.py').unlink()
            result = subprocess.run([sys.executable, '-B', str(dest / 'verify.py'), '--manifest',
                str(dest / 'publication.json'), '--out', str(folder / 'missing-source.json')],
                env=env, capture_output=True, timeout=5)
            self.assertNotEqual(0, result.returncode)
            self.assertIn(b'FileNotFoundError', result.stderr)


if __name__ == '__main__':
    unittest.main()
