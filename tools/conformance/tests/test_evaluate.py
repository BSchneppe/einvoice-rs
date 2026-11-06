"""Synthetic algorithm/protocol fixtures, never native corpus execution evidence."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import warnings
from unittest.mock import patch
import zipfile

HERE = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('bsc10_evaluate', HERE/'evaluate.py')
e = importlib.util.module_from_spec(spec)
spec.loader.exec_module(e)


def fixture_record(identifier='synthetic', split='development'):
    return dict(format=1, id=identifier, sha256='a'*64, scope='complete', split=split,
                engine='native', target=e.WIRE_TARGET, processing='completed', processing_errors=[],
                schema='valid', business='evaluated', stages={v: 'valid' for v in e.PROJECTION.values()},
                target_matched=True, scenario='EN16931 XRechnung (CII)', findings=[], structural_findings=[],
                reference_acceptance='acceptable', acceptance='accepted')


def finding(path='/Q{urn:synthetic/uri}r[1]/Q{}line[1]', message=' untrimmed message \n'):
    return dict(id='SYNTHETIC-RULE', severity='error', effective_severity='warning',
                location=path, line=None, column=None, message=message, stage='val-sch.1')


def fixture_request(records):
    # Authored identities, not an adapter or label generator. All runs using these
    # constructors are explicitly harness_fixture in their mandatory sidecar.
    return dict(format=1, kind='bsc10_request', release_gate_eligible=False, manifest='synthetic', manifest_sha256='a'*64,
                selection='development', requested_target=e.TARGET, wire_target=e.WIRE_TARGET,
                target_projection='bsc10-target-v1', artifact_lock_sha256='b'*64,
                evidence_lock_sha256='c'*64, evaluator_sha256='d'*64,
                records=[{**e.identity(r), 'bytes': 1000, 'source': 'synthetic'} for r in records])


def fixture_sidecar(request, records, path):
    identity = dict(name='synthetic-only', version='0', sha256='a'*64)
    cap = {**identity, 'capabilities_sha256': 'e'*64}
    docs = []
    for row in records:
        details = []
        for array in ('findings', 'structural_findings'):
            for i, d in enumerate(row[array]):
                details.append(dict(array=array, ordinal=i, raw_code=d['id'], raw_message=d['message'],
                                    raw_flag='fatal', raw_role=None, constraint=None, type=None, component=None,
                                    cause=None, relationship='unclassified', parent=None,
                                    typed_unavailable_reasons={k: 'synthetic_not_exposed' for k in ('constraint', 'type', 'component', 'cause')},
                                    source=dict(path=d['location'], span=None, unavailable_reason='synthetic_no_span')))
        docs.append(dict(id=row['id'], failure=None, stages=[dict(stage=s, status='completed',
            verdict=row['stages'].get(e.PROJECTION.get(s)), reason=None) for s in e.STAGES],
            diagnostic_details=details, backend_notices=[]))
    return dict(format=1, contract='bsc10-native-v1', release_gate_eligible=False, kind='harness_fixture', run_id='synthetic-only',
                output_sha256=e.sha(path.read_bytes()), request_sha256=e.sha(e.canonical(request)),
                **{k: request[k] for k in ('evaluator_sha256', 'evidence_lock_sha256', 'artifact_lock_sha256',
                                           'requested_target', 'wire_target', 'target_projection')},
                implementation=dict(facade=identity, serializer=identity, backend=cap, generator=identity,
                    package=cap, source=dict(revision='synthetic', tree_sha256='f'*64,
                    artifact_lock_sha256=request['artifact_lock_sha256']), build=dict(revision='synthetic',
                    dirty=True, profile='test', rustc='not-executed', target='synthetic', executable_sha256='a'*64),
                    unavailable_reasons={k: None for k in ('backend', 'generator', 'package', 'source')}),
                completion=dict(status='completed', exit_code=0, record_count=len(records)), documents=docs,
                coverage=dict(status='unavailable', reason='synthetic_test'),
                resources=dict(status='unavailable', reason='synthetic_test'))


def write_rows(path, records):
    path.write_text(''.join(json.dumps(r, ensure_ascii=False)+'\n' for r in records))


class StrictInput(unittest.TestCase):
    def test_reject_duplicate_keys_nonfinite_and_invalid_unicode(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":{"b":1,"b":2}}', b'NaN', b'Infinity',
                    b'-Infinity', b'1e9999', b'"\\ud800"', b'\xff', b'{"secret-heldout":',
                    '{"a":1}'.encode('utf-16'), b'\xef\xbb\xbf{"a":1}'):
            with self.subTest(raw=raw), self.assertRaises(e.EvidenceError):
                e.decode(raw)

    def test_jsonl_duplicate_id_blank_and_truncation(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d)/'test.jsonl'
            for raw in ('', '{"id":"x"}', '{"id":"x"}\n\n', '{"id":"x"}\n{"id":"x"}\n',
                        '{"id":"x"}\n{"id":\n', '[]\n'):
                path.write_text(raw)
                with self.subTest(raw=raw), self.assertRaises(e.EvidenceError):
                    e.read_jsonl(path)
            path.write_text('{"id":"x"}\r\n')
            self.assertEqual(e.read_jsonl(path), [{'id': 'x'}])

    def test_path_traversal_and_symlink_escape(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)/'root'
            root.mkdir()
            (root/'alias').symlink_to(Path(d), target_is_directory=True)
            for name in ('../outside', '/absolute', 'a/../../x', 'a//b', './x', 'a\\b', 'alias/outside'):
                with self.subTest(name=name), self.assertRaises(e.EvidenceError):
                    e.confined(root, name)

    def test_output_aliases_source_cache_and_hardlinks(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            source = root/'input'
            source.write_text('unchanged')
            alias = root/'alias'
            alias.symlink_to(source)
            hard = root/'hard'
            os.link(source, hard)
            for out in (source, alias, hard):
                with self.assertRaises(e.EvidenceError):
                    e.write_output({}, out, [source])
            with self.assertRaises(e.EvidenceError):
                e.write_output({}, HERE/'README.md', [])
            with self.assertRaises(e.EvidenceError):
                e.write_output({}, root/'new', [root])
            self.assertEqual(source.read_text(), 'unchanged')

    def test_interrupted_evaluation_replaces_old_success_with_incomplete_marker(self):
        with tempfile.TemporaryDirectory() as d:
            output = Path(d)/'report.json'
            output.write_text('{"observable_parity":true}')
            with patch.object(e, 'verify', side_effect=SystemExit(9)), self.assertRaises(SystemExit):
                e.main(['verify', '--output', str(output)])
            result = json.loads(output.read_text())
            self.assertEqual(result['protocol'], 'incomplete')
            self.assertIsNone(result['observable_parity'])
            self.assertFalse(result['release_gate_eligible'])

    def test_workflow_outputs_and_aliases_rejected_before_pending_write(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            workflows = root/'.github/workflows'
            workflows.mkdir(parents=True)
            alias = root/'workflow-alias'
            alias.symlink_to(workflows, target_is_directory=True)
            for name in ('oracle.yml', 'conformance.yml'):
                source = workflows/name
                source.write_bytes(b'original workflow')
                hard = root/('hard-' + name)
                os.link(source, hard)
                for output in (source, alias/name, hard):
                    with self.subTest(name=name, output=output), patch.object(e, 'ROOT', root), \
                            patch.object(e, 'verify') as verify, patch('sys.stderr'):
                        self.assertEqual(e.main(['verify', '--output', str(output)]), 2)
                        verify.assert_not_called()
                        self.assertEqual(source.read_bytes(), b'original workflow')
                        self.assertEqual(output.read_bytes(), b'original workflow')


class Locations(unittest.TestCase):
    def test_occurrences_namespaces_and_node_kinds(self):
        self.assertEqual(e.location('/'), ())
        base = '/Q{http://example/a/b}r[1]/Q{}x[2]'
        self.assertEqual(e.location(base)[1], ('element', '', 'x', 2))
        self.assertNotEqual(e.location(base), e.location(base.replace('[2]', '[1]')))
        self.assertNotEqual(e.location(base), e.location(base.replace('Q{}x', 'Q{urn:other}x')))
        for tail, kind in (('/@Q{urn:a/b}a', 'attribute'), ('/text()[2]', 'text'),
                           ('/comment()[1]', 'comment'), ('/processing-instruction("a:b")[3]', 'processing_instruction')):
            self.assertEqual(e.location(base+tail)[-1][0], kind)
        self.assertEqual(e.location('/comment()[1]')[0][0], 'comment')
        self.assertEqual(e.location('/Q{}é[1]')[0][2], 'é')

    def test_reject_unknown_malformed_indices_and_terminal_children(self):
        for path in ('', 'r', '//', '/Q{}x', '/Q{}x[0]', '/Q{}x[-1]', '/Q{}x[01]',
                     '/Q{}x[18446744073709551616]', '/Q{}x[9999999999999999999999999999999]',
                     '/p:x[1]', '/*[1]', '/Q{bad{uri}x[1]', '/Q{}x[1]/', '/Q{}x[1][2]',
                     '/@Q{}a', '/text()[1]', '/Q{}x[1]/@Q{}a[1]',
                     '/Q{}x[1]/text()[1]/Q{}x[1]', '/Q{}x[1]/@Q{}a/Q{}b[1]',
                     '/processing-instruction("xml")[1]', '/Q{}x[1]/processing-instruction(\'a\')[1]'):
            with self.subTest(path=path), self.assertRaises(e.EvidenceError):
                e.location(path)


class RecordSemantics(unittest.TestCase):
    def test_record_loss_extra_reorder_duplicate_and_identity(self):
        rows = [fixture_record('a'), fixture_record('b')]
        e.ordered_records(rows, rows, native=True)
        variants = [rows[:1], rows+[fixture_record('c')], rows[::-1], [rows[0], rows[0]]]
        for key, value in (('sha256', 'b'*64), ('split', 'held_out'), ('scope', 'rule_test_fragment')):
            altered = copy.deepcopy(rows)
            altered[0][key] = value
            variants.append(altered)
        for altered in variants:
            with self.assertRaises(e.EvidenceError):
                e.ordered_records(altered, rows, native=True)

    def test_base_error_effective_warning_acceptance_preserves_invalid_cen(self):
        row = fixture_record()
        row['findings'] = [finding()]
        row['stages']['val-sch.1'] = 'invalid'
        e.validate_record(row, native=True)
        # Raw fatal is a sidecar observation, not the full-record base severity.
        wrong = copy.deepcopy(row)
        wrong['findings'][0]['severity'] = 'fatal'
        with self.assertRaises(e.EvidenceError):
            e.validate_record(wrong, native=True)
        wrong = copy.deepcopy(row)
        wrong['findings'][0]['effective_severity'] = 'error'
        with self.assertRaises(e.EvidenceError):
            e.validate_record(wrong, native=True)
        wrong['acceptance'], wrong['reference_acceptance'] = 'rejected', 'reject'
        e.validate_record(wrong, native=True)

    def test_cross_field_stage_and_processing_invariants(self):
        variants = []
        for changes in ({'schema': 'invalid'}, {'business': 'not_evaluated'},
                        {'processing': 'failed'}, {'acceptance': 'rejected'},
                        {'scenario': 'EN16931 (CII)'}, {'processing_errors': ['unexpected']},
                        {'reference_acceptance': 'reject'}):
            variants.append({**fixture_record(), **changes})
        wrong = fixture_record()
        wrong['stages']['unexpected'] = 'valid'
        variants.append(wrong)
        wrong = fixture_record()
        wrong['stages']['val-sch.1'] = 'invalid'
        variants.append(wrong)
        wrong = fixture_record()
        del wrong['stages']['val-sch.2']
        wrong['business'] = 'not_evaluated'
        wrong['acceptance'] = 'rejected'
        variants.append(wrong)
        for row in variants:
            with self.subTest(row=row), self.assertRaises(e.EvidenceError):
                e.validate_record(row, native=True)

    def test_ordered_business_duplicates_untrimmed_message_and_positions(self):
        ref = fixture_record()
        ref['findings'] = [finding(), finding(), finding('/Q{}r[1]/Q{}line[2]')]
        ref['stages']['val-sch.1'] = 'invalid'
        e.validate_record(ref, native=True)
        self.assertTrue(e.compare_records([ref], [copy.deepcopy(ref)], 'development')['observable_parity'])
        variants = []
        lost = copy.deepcopy(ref)
        lost['findings'].pop(0)
        variants.append(lost)
        reordered = copy.deepcopy(ref)
        reordered['findings'].reverse()
        variants.append(reordered)
        for key, value in (('message', 'untrimmed message'), ('line', '0'), ('column', '1'),
                           ('location', '/Q{other}r[1]/Q{}line[1]'), ('effective_severity', 'information')):
            changed = copy.deepcopy(ref)
            changed['findings'][0][key] = value
            variants.append(changed)
        for row in variants:
            self.assertFalse(e.compare_records([ref], [row], 'development')['observable_parity'])

    def test_structural_equal_or_different_counts_unresolved_all_raw_preserved(self):
        ref = fixture_record()
        diagnostic = {**finding(), 'id': 'synthetic-cvc', 'stage': 'val-xsd',
                      'effective_severity': 'error', 'location': None, 'message': 'source XSD error'}
        ref.update(schema='invalid', business='not_evaluated', acceptance='rejected', reference_acceptance='reject',
                   stages={'val-xml': 'valid', 'val-xsd': 'invalid'}, structural_findings=[diagnostic, diagnostic.copy()])
        e.validate_record(ref, native=True)
        same = e.compare_records([ref], [ref], 'development')
        self.assertIsNone(same['observable_parity'])
        native = copy.deepcopy(ref)
        native['structural_findings'].pop()
        native['structural_findings'][0]['message'] = 'different backend wording'
        native['structural_findings'][0]['id'] = 'native-code'
        result = e.compare_records([ref], [native], 'development')
        self.assertIsNone(result['observable_parity'])
        self.assertEqual(result['counts']['structural_multiplicity_difference_documents'], 1)
        self.assertEqual(result['structural_observations'][0]['reference'], ref['structural_findings'])
        self.assertEqual(result['structural_observations'][0]['candidate'], native['structural_findings'])

    def test_processing_failure_never_parity_success_and_full_redaction(self):
        row = fixture_record('HELD_OUT_SECRET_ID', 'held_out')
        row.update(processing='failed', processing_errors=['HELD_OUT_SECRET_ERROR'], acceptance='indeterminate',
                   schema='not_evaluated', business='not_evaluated', stages={}, scenario=None, target_matched=False,
                   reference_acceptance='undefined')
        e.validate_record(row, native=True)
        result = e.compare_records([row], [row], 'full')
        self.assertIsNone(result['observable_parity'])
        self.assertNotIn('HELD_OUT_SECRET', json.dumps(result))
        with self.assertRaises(e.EvidenceError):
            e.compare_records([row], [row], 'development')


class RunProtocol(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)/'native.jsonl'
        self.rows = [fixture_record()]
        self.request = fixture_request(self.rows)
        write_rows(self.path, self.rows)
        self.meta = fixture_sidecar(self.request, self.rows, self.path)

    def validate(self, meta=None):
        e.validate_run(self.meta if meta is None else meta, self.rows, self.request, e.sha(self.path.read_bytes()))

    def test_harness_fixture_valid_protocol_never_native_release(self):
        self.validate()
        sidecar = Path(str(self.path)+'.metadata.json')
        sidecar.write_text(json.dumps(self.meta))
        report, code = e.compare(self.request, self.rows, self.path, sidecar)
        self.assertEqual(code, 0)
        self.assertTrue(report['observable_parity'])
        self.assertFalse(report['release_gate_eligible'])
        self.assertFalse(report['evidence_complete'])
        self.assertFalse(report['native_execution_attested'])
        self.assertIn('synthetic_harness_fixture_not_native_evidence', report['gaps'])
        # An adversarial self-relabel cannot promote the synthetic evidence.
        self.meta['kind'] = 'native'
        sidecar.write_text(json.dumps(self.meta))
        forged, _ = e.compare(self.request, self.rows, self.path, sidecar)
        self.assertFalse(forged['release_gate_eligible'])
        self.assertFalse(forged['evidence_complete'])
        self.assertFalse(forged['native_execution_attested'])
        self.assertIn('loaded_package_backend_build_source_unverified', forged['gaps'])

    def test_stale_bindings_completion_source_and_loaded_identities(self):
        for key in ('request_sha256', 'output_sha256', 'evaluator_sha256', 'evidence_lock_sha256',
                    'artifact_lock_sha256', 'target_projection'):
            meta = copy.deepcopy(self.meta)
            meta[key] = '0'*64
            with self.subTest(key=key), self.assertRaises(e.EvidenceError):
                self.validate(meta)
        for key, value in (('status', 'failed'), ('exit_code', 2), ('record_count', 0)):
            meta = copy.deepcopy(self.meta)
            meta['completion'][key] = value
            with self.assertRaises(e.EvidenceError):
                self.validate(meta)
        for key in ('backend', 'generator', 'package', 'source'):
            meta = copy.deepcopy(self.meta)
            meta['implementation'][key] = None
            meta['implementation']['unavailable_reasons'][key] = 'not loaded'
            with self.assertRaises(e.EvidenceError):
                self.validate(meta)
        meta = copy.deepcopy(self.meta)
        meta['implementation']['source']['artifact_lock_sha256'] = '0'*64
        with self.assertRaises(e.EvidenceError):
            self.validate(meta)

    def test_native_stage_inventory_failure_skip_and_coverage(self):
        for mode in ('missing', 'duplicate', 'reorder', 'skipped', 'failed', 'reason', 'verdict', 'coverage'):
            meta = copy.deepcopy(self.meta)
            stages = meta['documents'][0]['stages']
            if mode == 'missing': stages.pop()
            if mode == 'duplicate': stages.append(stages[0])
            if mode == 'reorder': stages.reverse()
            if mode in ('skipped', 'failed'):
                stages[2].update(status=mode, reason='synthetic failure')
            if mode == 'reason': stages[0]['reason'] = 'unexpected'
            if mode == 'verdict': stages[2]['verdict'] = 'invalid'
            if mode == 'coverage': meta['coverage'] = {'status': 'available', 'compiled_count': 99}
            with self.subTest(mode=mode), self.assertRaises(e.EvidenceError):
                self.validate(meta)

    def test_unobserved_failed_stages_and_required_skips_cannot_complete(self):
        for failed_stage in ('xsd', 'cen_rules', 'xrechnung_rules'):
            for scenario in ('EN16931 XRechnung (CII)', 'EN16931 (CII)', None):
                if scenario is None and failed_stage != 'xsd':
                    continue  # Unmatched input never reaches XSD/business execution.
                for status in ('failed', 'skipped'):
                    row = fixture_record()
                    observed = {'val-xml': 'valid'}
                    if failed_stage != 'xsd':
                        observed['val-xsd'] = 'valid'
                    if failed_stage == 'xrechnung_rules':
                        observed['val-sch.1'] = 'valid'
                    row.update(stages=observed, schema=observed.get('val-xsd', 'not_evaluated'),
                               business='not_evaluated', acceptance='rejected', reference_acceptance='undefined',
                               scenario=scenario, target_matched=scenario == 'EN16931 XRechnung (CII)')
                    request = fixture_request([row])
                    write_rows(self.path, [row])
                    meta = fixture_sidecar(request, [row], self.path)
                    for stage in meta['documents'][0]['stages']:
                        if stage['stage'] in e.PROJECTION and e.PROJECTION[stage['stage']] not in observed:
                            stage.update(status='skipped', verdict=None, reason='dependency absent')
                        if stage['stage'] == failed_stage:
                            stage.update(status=status, verdict=None, reason='synthetic incomplete stage')
                    sidecar = Path(str(self.path)+'.metadata.json')
                    sidecar.write_text(json.dumps(meta))
                    legitimate_skip = status == 'skipped' and (
                        scenario is None or (scenario == 'EN16931 (CII)' and failed_stage == 'xrechnung_rules'))
                    with self.subTest(stage=failed_stage, scenario=scenario, status=status):
                        if legitimate_skip:
                            result, code = e.compare(request, [row], self.path, sidecar)
                            self.assertEqual(code, 0)
                            self.assertEqual(result['protocol'], 'verified')
                        else:
                            with self.assertRaises(e.EvidenceError):
                                e.compare(request, [row], self.path, sidecar)

    def test_accepted_document_failure_rejected_but_negative_evidence_preserved(self):
        doc = self.meta['documents'][0]
        doc['failure'] = dict(kind='document', code='synthetic', message='source fault')
        with self.assertRaisesRegex(e.EvidenceError, '^accepted_failure$'):
            self.validate()
        # A real observed business rejection may carry the document-fault classification.
        row = self.rows[0]
        row.update(acceptance='rejected', reference_acceptance='reject', findings=[finding()])
        row['findings'][0]['effective_severity'] = 'error'
        row['stages']['val-sch.1'] = 'invalid'
        write_rows(self.path, self.rows)
        self.meta = fixture_sidecar(self.request, self.rows, self.path)
        self.meta['documents'][0]['failure'] = doc['failure']
        self.validate()
        # Malformed source can be a document fault AND failed processing.
        row.update(processing='failed', processing_errors=['malformed source'], schema='not_evaluated',
                   business='not_evaluated', acceptance='indeterminate', reference_acceptance='reject',
                   target_matched=False, scenario=None, stages={'val-xml': 'invalid'}, findings=[],
                   structural_findings=[{**finding(), 'stage': 'val-xml', 'effective_severity': 'error'}])
        write_rows(self.path, self.rows)
        self.meta = fixture_sidecar(self.request, self.rows, self.path)
        self.meta['documents'][0]['failure'] = doc['failure']
        self.meta['completion'].update(status='failed', exit_code=2)
        for stage in self.meta['documents'][0]['stages']:
            stage.update(status='failed' if stage['stage'] == 'xml_syntax' else 'skipped', reason='malformed source')
        e.validate_record(row, native=True)
        self.validate()

    def test_compared_native_snapshot_cannot_bind_replacement_stream(self):
        first = self.path.read_bytes()
        replacement = copy.deepcopy(self.rows)
        replacement[0]['findings'] = [finding(message='replacement observation')]
        replacement[0]['stages']['val-sch.1'] = 'invalid'
        write_rows(self.path, replacement)
        second = self.path.read_bytes()
        sidecar = Path(str(self.path)+'.metadata.json')
        sidecar.write_text(json.dumps(fixture_sidecar(self.request, replacement, self.path)))
        self.path.write_bytes(first)
        validate = e.ordered_records

        def replace_after_parse(*args, **kwargs):
            validate(*args, **kwargs)
            self.path.write_bytes(second)

        with patch.object(e, 'ordered_records', side_effect=replace_after_parse):
            with self.assertRaisesRegex(e.EvidenceError, '^output_digest$'):
                e.compare(self.request, self.rows, self.path, sidecar)
        # Stable replacement remains an actual mismatch, not a passing first snapshot.
        result, code = e.compare(self.request, self.rows, self.path, sidecar)
        self.assertEqual(code, 1)
        self.assertFalse(result['observable_parity'])

    def test_diagnostic_bijection_raw_data_source_and_cause_parent(self):
        self.rows[0]['findings'] = [finding(), finding()]
        self.rows[0]['stages']['val-sch.1'] = 'invalid'
        write_rows(self.path, self.rows)
        self.meta = fixture_sidecar(self.request, self.rows, self.path)
        self.validate()
        for mode in ('missing', 'duplicate', 'ordinal', 'raw_message', 'raw_code', 'path', 'reason', 'span', 'parent'):
            meta = copy.deepcopy(self.meta)
            ds = meta['documents'][0]['diagnostic_details']
            if mode == 'missing': ds.pop()
            if mode == 'duplicate': ds.append(ds[0])
            if mode == 'ordinal': ds[0]['ordinal'] = 1
            if mode == 'raw_message': ds[0]['raw_message'] = 'trimmed'
            if mode == 'raw_code': ds[0]['raw_code'] = 'invented'
            if mode == 'path': ds[0]['source']['path'] = '/Q{}other[1]'
            if mode == 'reason': ds[0]['source']['unavailable_reason'] = None
            if mode == 'span':
                ds[0]['source'].update(span=dict(unit='utf8_byte', start=0, end=1001, subject='exact_node', line=1, column=1), unavailable_reason=None)
            if mode == 'parent':
                ds[0].update(relationship='collateral', parent=dict(array='findings', ordinal=1))
            with self.subTest(mode=mode), self.assertRaises(e.EvidenceError):
                self.validate(meta)
        self.meta['documents'][0]['diagnostic_details'][1].update(
            relationship='collateral', parent=dict(array='findings', ordinal=0))
        self.validate()

    def test_all_operational_failure_kinds_cannot_pass(self):
        for kind in ('engine', 'timeout', 'crash', 'unsupported', 'limit'):
            rows = [fixture_record()]
            rows[0].update(processing='failed', processing_errors=['synthetic '+kind], schema='not_evaluated',
                           business='not_evaluated', stages={}, acceptance='indeterminate', reference_acceptance='undefined',
                           target_matched=False, scenario=None)
            request = fixture_request(rows)
            write_rows(self.path, rows)
            meta = fixture_sidecar(request, rows, self.path)
            meta['completion'].update(status='failed', exit_code=2)
            doc = meta['documents'][0]
            doc['failure'] = dict(kind=kind, code='synthetic', message='synthetic failure')
            for stage in doc['stages']:
                stage.update(status='skipped', verdict=None, reason='synthetic not executed')
            doc['stages'][0]['status'] = 'failed'
            sidecar = Path(str(self.path)+'.metadata.json')
            sidecar.write_text(json.dumps(meta))
            report, code = e.compare(request, rows, self.path, sidecar)
            self.assertEqual(code, 2)
            self.assertIsNone(report['observable_parity'])

    def test_absent_sidecar_and_corrupt_stream_never_stale_success(self):
        sidecar = Path(str(self.path)+'.metadata.json')
        with self.assertRaises(OSError):
            e.compare(self.request, self.rows, self.path, sidecar)
        sidecar.write_text(json.dumps(self.meta))
        self.path.write_text(self.path.read_text().rstrip('\n'))
        with self.assertRaises(e.EvidenceError):
            e.compare(self.request, self.rows, self.path, sidecar)

    def test_xsd_invalidity_is_completed_document_fault_with_rule_short_circuit(self):
        row = self.rows[0]
        row.update(schema='invalid', business='not_evaluated', acceptance='rejected', reference_acceptance='reject',
                   stages={'val-xml': 'valid', 'val-xsd': 'invalid'}, structural_findings=[
                       {**finding(), 'stage': 'val-xsd', 'severity': 'error', 'effective_severity': 'error'}])
        write_rows(self.path, self.rows)
        self.meta = fixture_sidecar(self.request, self.rows, self.path)
        doc = self.meta['documents'][0]
        doc['failure'] = dict(kind='document', code='synthetic_xsd', message='source fault')
        for stage in doc['stages']:
            if stage['stage'] in ('cen_rules', 'xrechnung_rules'):
                stage.update(status='skipped', verdict=None, reason='xsd_invalid')
        e.validate_record(row, native=True)
        self.validate()
        bad = copy.deepcopy(row)
        bad['stages'].update({'val-sch.1': 'valid', 'val-sch.2': 'valid'})
        bad['business'] = 'evaluated'
        with self.assertRaises(e.EvidenceError):
            e.validate_record(bad, native=True)
        # Raw structural evidence, even identical, is not mapped semantic parity.
        sidecar = Path(str(self.path)+'.metadata.json')
        sidecar.write_text(json.dumps(self.meta))
        result, code = e.compare(self.request, self.rows, self.path, sidecar)
        self.assertEqual(code, 2)
        self.assertIsNone(result['observable_parity'])

    def test_full_report_omits_all_heldout_findings_and_identifiers(self):
        row = fixture_record('HELD_OUT_SECRET_ID', 'held_out')
        row['findings'] = [finding(message='HELD_OUT_SECRET_PROSE')]
        row['stages']['val-sch.1'] = 'invalid'
        rows = [row]
        request = fixture_request(rows)
        request['selection'] = 'full'
        write_rows(self.path, rows)
        meta = fixture_sidecar(request, rows, self.path)
        sidecar = Path(str(self.path)+'.metadata.json')
        sidecar.write_text(json.dumps(meta))
        ref = copy.deepcopy(row)
        ref['findings'][0]['message'] = 'HELD_OUT_SECRET_REFERENCE'
        result, code = e.compare(request, [ref], self.path, sidecar)
        self.assertEqual(code, 1)
        self.assertFalse(result['observable_parity'])
        self.assertNotIn('HELD_OUT_SECRET', json.dumps(result))


class CacheAudit(unittest.TestCase):
    def test_archive_consumption_uses_the_verified_byte_snapshot(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root/'downloads').mkdir()
            path = root/'downloads/test.zip'
            with zipfile.ZipFile(path, 'w') as z:
                z.writestr('a', b'original')
            original = path.read_bytes()
            pin = dict(file='test.zip', sha256=e.sha(original), members={'a': e.sha(b'original')})
            (root/'test').mkdir()
            (root/'test/a').write_bytes(b'original')
            read = Path.read_bytes

            def replace_after_read(p):
                data = read(p)
                if p.resolve() == path.resolve():
                    p.write_bytes(b'not the verified archive')
                return data

            with patch.object(Path, 'read_bytes', replace_after_read):
                result = e.audit_cache(root, [pin], [], [])
            self.assertEqual(result['members'], 1)
            self.assertNotEqual(path.read_bytes(), original)

    def test_artifact_members_duplicates_escape_and_inputs(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root/'downloads').mkdir()
            archive_path = root/'downloads/test.zip'
            pin = dict(file='test.zip', sha256='', members={'a': e.sha(b'original')})
            for mode in ('valid', 'duplicate', 'escape', 'changed', 'extra', 'input_missing'):
                with warnings.catch_warnings():
                    warnings.simplefilter('ignore', UserWarning)
                    with zipfile.ZipFile(archive_path, 'w') as archive:
                        archive.writestr('a', b'original' if mode != 'changed' else b'changed')
                        if mode == 'duplicate': archive.writestr('a', b'original')
                        if mode == 'escape': archive.writestr('../escape/', b'')
                        if mode == 'extra': archive.writestr('extra', b'extra')
                pin['sha256'] = e.sha(archive_path.read_bytes())
                (root/'test').mkdir(exist_ok=True)
                (root/'test/a').write_bytes(b'original')
                inventory = [dict(path='.cache/input.xml', bytes=1, sha256=e.sha(b'x'), scope='complete')]
                if mode == 'input_missing':
                    (root/'input.xml').unlink(missing_ok=True)
                else:
                    (root/'input.xml').write_bytes(b'x')
                if mode == 'valid':
                    result = e.audit_cache(root, [pin], inventory, [])
                    self.assertEqual(result, dict(status='verified', artifacts=1, members=1, inputs=1))
                else:
                    with self.subTest(mode=mode), self.assertRaises(e.EvidenceError):
                        e.audit_cache(root, [pin], inventory, [])

    def test_extracted_and_input_corruption_or_alias_escape(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root/'downloads').mkdir()
            archive = root/'downloads/test.zip'
            with zipfile.ZipFile(archive, 'w') as z:
                z.writestr('a', b'original')
            pin = dict(file='test.zip', sha256=e.sha(archive.read_bytes()), members={'a':e.sha(b'original')})
            (root/'test').mkdir()
            member = root/'test/a'
            member.write_bytes(b'changed')
            with self.assertRaisesRegex(e.EvidenceError, '^extracted_member_integrity$'):
                e.audit_cache(root, [pin], [], [])
            member.unlink()
            member.symlink_to('/private/tmp')
            with self.assertRaisesRegex(e.EvidenceError, '^unsafe_path$'):
                e.audit_cache(root, [pin], [], [])


class FrozenEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.evidence = e.verify()
        cls.integrity = cls.evidence.report
        cls.manifests = cls.evidence.manifests
        cls.references = cls.evidence.references

    def test_real_offline_frozen_integrity_and_declared_denominators(self):
        self.assertEqual(self.integrity['frozen_integrity'], 'verified')
        self.assertEqual(self.integrity['complete_documents'], 10027)
        self.assertEqual(self.integrity['scoped_fragments'], 9)
        self.assertEqual(self.integrity['cache']['status'], 'unavailable')
        self.assertFalse(self.integrity['integrity_complete'])
        self.assertFalse(self.integrity['release_gate_eligible'])
        for name, dev, full in (('initial', 13, 41), ('sinabis', 6500, 10000)):
            for mode, count in (('development', dev), ('full', full)):
                request = e.request_for(name, mode, self.evidence)
                self.assertEqual(len(request['records']), count)
                if mode == 'development':
                    self.assertTrue(all(r['split'] == 'development' for r in request['records']))

    def test_evaluator_digest_policy_and_no_self_cycle(self):
        before = e.evaluator_digest()
        self.assertEqual(before, e.evaluator_digest())
        self.assertEqual(len(before), 64)
        self.assertIn('tests/test_evaluate.py', e.POLICY_FILES)
        self.assertNotIn('output.json', e.POLICY_FILES)

    def test_verified_frozen_snapshots_and_request_never_reopen_evidence(self):
        # A second path read would get different bytes. No real file is changed;
        # labels are inspected only in aggregate and never printed.
        protected = {e.ROOT/name for name in e.FROZEN} | {HERE/'evidence.lock.json'}
        reads = {}
        read = Path.read_bytes

        def changing_read(path):
            if path in protected:
                reads[path] = reads.get(path, 0) + 1
                if reads[path] > 1:
                    return b'{"different_snapshot":true}'
            return read(path)

        with patch.object(Path, 'read_bytes', changing_read):
            evidence = e.verify()
            request = e.request_for('initial', 'development', evidence)
            self.assertTrue(all(count == 1 for count in reads.values()))
        self.assertEqual(set(reads), protected)
        self.assertEqual(request['records'], e.request_for('initial', 'development', self.evidence)['records'])
        self.assertEqual(request['manifest_sha256'], self.evidence.hashes['tools/oracle/corpus/initial.jsonl'])
        self.assertEqual(request['evidence_lock_sha256'], e.sha(read(HERE/'evidence.lock.json')))
        # Request construction works even if all file reads are unavailable now.
        with patch.object(Path, 'read_bytes', side_effect=AssertionError('request reopened evidence')):
            self.assertEqual(e.request_for('initial', 'development', evidence), request)

    def test_duplicate_manifest_identity_split_parent_and_scope(self):
        split = e.read_json(e.ROOT/'tools/oracle/corpus/split.json')
        for mode in ('duplicate', 'split', 'parent', 'source', 'scope', 'theme'):
            dataset = list(self.manifests['sinabis'])
            initial = copy.deepcopy(self.manifests['initial'])
            if mode == 'duplicate': initial.append(initial[0])
            if mode == 'split': dataset[0] = {**dataset[0], 'split': 'held_out'}
            if mode == 'theme': dataset[0] = {**dataset[0], 'scenario': 'unknown'}
            if mode in ('parent', 'source', 'scope'):
                row = next(r for r in initial if r['source'] == 'local-mutation-v1')
                row[{'parent':'parent_sha256', 'source':'source', 'scope':'scope'}[mode]] = {
                    'parent':'0'*64, 'source':'unknown', 'scope':'rule_test_fragment'}[mode]
            with self.subTest(mode=mode), self.assertRaises(e.EvidenceError):
                e.validate_inventory(dataset, initial, split)

    def test_lock_missing_extra_duplicate_path_escape_and_byte_tamper(self):
        original = e.read_json(HERE/'evidence.lock.json')
        with tempfile.TemporaryDirectory() as d:
            path = Path(d)/'lock.json'
            for mode in ('missing', 'extra', 'duplicate', 'path', 'revision'):
                lock = copy.deepcopy(original)
                if mode == 'missing': lock['files'].pop()
                if mode == 'extra': lock['files'].append(dict(path='extra', bytes=0, sha256='0'*64))
                if mode == 'duplicate': lock['files'].append(lock['files'][0])
                if mode == 'path': lock['files'][0]['path'] = '../escape'
                if mode == 'revision': lock['reviewed_revision'] = '0'*40
                path.write_text(json.dumps(lock))
                with self.subTest(mode=mode), self.assertRaises(e.EvidenceError):
                    e.verify(path)
            path.write_text(json.dumps(original))
            fake_root = Path(d)/'repo'
            first = fake_root/original['files'][0]['path']
            first.parent.mkdir(parents=True)
            first.write_bytes(b'one changed byte')
            with patch.object(e, 'ROOT', fake_root), self.assertRaisesRegex(e.EvidenceError, '^frozen_bytes$'):
                e.verify(path)

    def test_scoped_scope_fragment_inventory_and_severity_separate(self):
        containers = [r for r in self.manifests['initial'] if r['scope'] == 'rule_test_set']
        pins = e.read_json(e.ROOT/'tools/oracle/artifacts.lock.json')['artifacts']
        scoped = e.read_jsonl(e.ROOT/'tools/oracle/results/scoped.jsonl')
        for mode in ('missing', 'extra', 'scope', 'ordinal', 'acceptance'):
            rows = copy.deepcopy(scoped)
            if mode == 'missing': rows.pop()
            if mode == 'extra': rows.append(rows[0])
            if mode == 'scope': rows[0]['scope'] = 'complete'
            if mode == 'ordinal': rows[0]['id'] = rows[0]['id'].split(':')[0]+':99'
            if mode == 'acceptance': rows[0]['acceptance'] = 'accepted'
            with self.subTest(mode=mode), self.assertRaises(e.EvidenceError):
                e.verify_scoped(rows, containers, pins)
        # Synthetic scoped semantics permit raw flag fatal and trimmed message;
        # they do not enter full-record acceptance/base severity comparisons.
        row = dict(format=1, id='synthetic.xml:0', scope='rule_test_fragment', rule_ids=['synthetic'],
                   schema='not_evaluated', acceptance='not_applicable', processing='completed',
                   expectations_met=True, findings=[dict(id='synthetic', severity='fatal', location='/', message='trimmed')])
        e.validate_schema(row, HERE/'scoped.schema.json')
        with self.assertRaises(e.EvidenceError):
            e.validate_record(row, native=True)

    def test_cli_redacts_malformed_heldout_and_replaces_stale_report(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d)/'heldout.jsonl'
            path.write_text('{"id":"HELD_OUT_SECRET", "message":"SECRET_DIAGNOSTIC"\n')
            report = Path(d)/'report.json'
            report.write_text('{"observable_parity":true}')
            run = subprocess.run([sys.executable, '-B', str(HERE/'evaluate.py'), 'compare', '--selection', 'full',
                                  '--native', str(path), '--output', str(report)], capture_output=True, text=True)
            self.assertEqual(run.returncode, 2)
            result = report.read_text() + run.stdout + run.stderr
            self.assertNotIn('SECRET', result)
            self.assertEqual(json.loads(report.read_text())['protocol'], 'invalid')
            self.assertIsNone(json.loads(report.read_text())['observable_parity'])


if __name__ == '__main__':
    unittest.main()
