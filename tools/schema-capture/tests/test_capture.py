"""Genuine retained artifacts, adversarial mutations, and SYNTHETIC GRAMMAR ONLY.

No synthetic result is execution evidence or genuine positive variant coverage.
The consumer source is compiled directly from held bytes, never helper caches.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).absolute().parents[1]
SOURCE = (ROOT / 'verify.py').read_bytes()
v = types.ModuleType('capture_verifier_held_source')
v.__file__ = str(ROOT / 'verify.py')
exec(compile(SOURCE, v.__file__, 'exec'), v.__dict__)
EVIDENCE = ROOT / 'development-evidence'
PLATFORMS = ('linux', 'windows', 'macos')
SUCCESS = ('schema-valid', 'schema-producer-tests/unchanged', 'schema-producer-tests/external-change')
FAILED = ('schema-producer-tests/malformed', 'schema-producer-tests/snapshot-change')


def load(path):
    return json.loads(path.read_bytes())


def artifact(scenario='schema-producer-tests/unchanged', stage='complete'):
    path = EVIDENCE / 'linux' / scenario
    return load(path / (stage + '.json')), (path / 'input.xml').read_bytes()


def dump(path, data):
    path.write_bytes((json.dumps(data, indent=2) + '\n').encode())


class GenuineEvidence(unittest.TestCase):
    def test_exact_selected_original_bytes_and_all_417_references(self):
        descriptor = v.pinned_descriptor()
        for name, binding in descriptor['references'].items():
            self.assertEqual(v.identify((ROOT / name).read_bytes()), binding)
        selection = load(ROOT / 'selected-core.json')
        rows = sorted(selection['files'], key=lambda x: x['path'])
        digest = hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        self.assertEqual(digest, '4b8905f470ed5b38138ac2ee2f45d6ec568b90c96c87212c370cffda3529771e')
        self.assertEqual(len(rows), 141)
        self.assertEqual(sum(row['bytes'] for row in rows), 1131837)
        manifest = load(ROOT / 'artifact-file-manifest.json')['files']
        self.assertEqual(len(manifest), 417)
        refs = {row['path']: row for row in manifest}
        actual = {p.relative_to(EVIDENCE).as_posix() for p in EVIDENCE.rglob('*') if p.is_file()}
        self.assertEqual(actual, {row['path'] for row in rows})
        for row in rows:
            with self.subTest(path=row['path']):
                self.assertEqual(v.identify((EVIDENCE / row['path']).read_bytes()),
                                 {'bytes': row['bytes'], 'sha256': row['sha256']})
                self.assertEqual(row, refs[row['path']])
                if row['path'].endswith('source-inventory.json'):
                    self.assertEqual(load(EVIDENCE / row['path']), descriptor['source_members'])
        self.assertEqual(len(descriptor['source_members']), 407)
        self.assertEqual(descriptor['reviewed_full_tree_correspondence']['tree'],
                         '3e09dcfbde65cb699c5bf3d422618e059d81b8ba')
        win = (EVIDENCE / 'windows/schema-valid/source-inventory.json').read_bytes()
        self.assertEqual(win.count(b'\r\n'), 1630)
        self.assertEqual((EVIDENCE / 'linux/schema-producer-tests/malformed/complete.json').read_bytes(), b'')

    def test_all_nine_receipted_complete_and_partial(self):
        for platform in PLATFORMS:
            for scenario in SUCCESS:
                for selected in ('complete', 'partial'):
                    with self.subTest(platform=platform, scenario=scenario, selected=selected):
                        code, report = v.verify_attempt(EVIDENCE / platform / scenario, selected)
                        self.assertEqual(code, 0 if selected == 'complete' else 2, report)
                        expected = 'Incomplete' if selected == 'partial' else 'Valid' if scenario == 'schema-valid' else 'Invalid'
                        self.assertEqual(report['producer_reported_assessment'], expected)
                        self.assertEqual(report['exact_byte_bindings'], 'supplied_relations_consistent')
                        self.assertFalse(report['release_gate_eligible'])
                        self.assertEqual(report['source_witness_verification'], 'UNAVAILABLE')
                        self.assertNotIn('passed', report)

    def test_all_six_failures_and_no_partial_cherry_picking(self):
        for platform in PLATFORMS:
            for scenario in FAILED:
                for selected in ('complete', 'partial'):
                    with self.subTest(platform=platform, scenario=scenario, selected=selected):
                        code, report = v.verify_attempt(EVIDENCE / platform / scenario, selected)
                        self.assertEqual(code, 2 if scenario.endswith('malformed') else 1, report)
                        self.assertIsNotNone(report['attempt_failure'])
                        self.assertIsNone(report['producer_reported_assessment'])
                        self.assertFalse(report['release_gate_eligible'])
                        if scenario.endswith('snapshot-change'):
                            self.assertIn('input digest', report['reason'])

    def test_results_context_and_public_xml_provenance(self):
        descriptor = v.pinned_descriptor()
        for platform in PLATFORMS:
            base = EVIDENCE / platform / 'schema-producer-tests'
            results = load(base / 'results.json')
            self.assertEqual([r['scenario'] for r in results], ['unchanged', 'external-change', 'snapshot-change', 'malformed'])
            for row in results:
                attempt = base / row['scenario']
                self.assertEqual(row['failure'] is None, row['scenario'] in ('unchanged', 'external-change'))
                for child in row['actual_child_calls']:
                    obs = load(attempt / (child['stage'] + '.json'))['observation']
                    self.assertEqual(child['actual_input'], obs['input_sha256'])
                    self.assertEqual(child['command'], load(attempt / (child['stage'] + '.status.json'))['command'])
            self.assertEqual((base / 'external-change.xml').read_bytes(), b'<edited-by-caller/>')
            self.assertEqual((base / 'external-change/input.xml').read_bytes(), b'<alpha/>')
        # Immutable hosted metadata is distinct from receipt revision: the run
        # records a different workflow head. Do not relabel either value.
        metadata = load(ROOT / 'hosted-artifact-metadata.json')
        self.assertEqual({r['workflow_run']['id'] for r in metadata}, {42381413047})
        self.assertEqual({r['workflow_run']['head_sha'] for r in metadata}, {'017fb00666944b51a43c5f06c3778f6887adb3d0'})
        self.assertIn('890d1582ecb831e83c54b4e6cb817d1c3a7703d6', descriptor['supported_receipt_revisions'])

    def test_excluded_build_and_resource_claims_match_original_digest_references(self):
        # Check reference consistency only. These excluded bytes are not opened
        # or promoted to authenticated build/runtime evidence.
        refs = {r['path']: {'bytes': r['bytes'], 'sha256': r['sha256']}
                for r in load(ROOT / 'artifact-file-manifest.json')['files']}
        for platform in PLATFORMS:
            for scenario in SUCCESS:
                prefix = platform + '/' + scenario + '/'
                receipt = load(EVIDENCE / prefix / 'receipt.json')
                for field, name in [('build_stdout', 'build.jsonl'), ('build_stderr', 'build.stderr'),
                                    ('cargo_metadata', 'cargo-metadata.json')]:
                    self.assertEqual(receipt[field], refs[prefix + name])
                for member, binding in receipt['loaded_schema_bytes'].items():
                    self.assertEqual(binding, refs[prefix + 'original-resources/' + member])


class ObservationMutations(unittest.TestCase):
    def setUp(self):
        self.base, self.input = artifact()
        self.descriptor = v.pinned_descriptor()

    def reject(self, change, stage='complete'):
        data, raw = artifact(stage=stage)
        change(data)
        with self.assertRaises(v.Refusal):
            v.check_observation(data, raw, self.descriptor)

    def test_unknown_missing_nullable_and_variant_keys(self):
        changes = [lambda a: a.update(extra=True),
                   lambda a: a['observation'].pop('operational_failure'),
                   lambda a: a['observation'].update(events=None),
                   lambda a: a['observation']['events'][0]['evidence'].update(missing_reason='unknown'),
                   lambda a: a['observation']['events'][0]['evidence'].update(cause={'UndeclaredElement': None}),
                   lambda a: a['observation']['events'][0]['evidence'].update(cause='FutureCause'),
                   lambda a: a['observation']['events'][0]['evidence'].update(cause={'Value': {}, 'Facet': {}}),
                   lambda a: a['observation'].update(input_sha256=None),
                   lambda a: a['observation'].update(validated_root_reference=None)]
        for change in changes:
            with self.subTest(change=change): self.reject(change)
        with self.assertRaises(v.Refusal) as caught:
            v.check_observation({'format': 'future/2', 'new_projection': {}}, self.input, self.descriptor)
        self.assertEqual(caught.exception.code, 2)

    def test_terminal_and_phase_contradictions(self):
        changes = [lambda a: a['observation'].update(finalization_attempted=False),
                   lambda a: a['observation'].update(finalization_succeeded=False),
                   lambda a: a['observation'].update(failure='Deadline'),
                   lambda a: a['observation'].update(terminal_capture_failure='Allocation'),
                   lambda a: a['observation'].update(root_validity='NotKnown'),
                   lambda a: a['observation'].update(root_validity=None),
                   lambda a: a['observation'].update(assessment='Valid'),
                   lambda a: a.update(epoch_retired=True),
                   lambda a: a.update(epoch_remaining_requests=0),
                   lambda a: a['observation'].update(observed_callbacks=2)]
        for change in changes:
            with self.subTest(change=change): self.reject(change)
        for failure in ['EpochRetired', {'Backend': {'phase': 'invented'}}, {'Backend': {'phase': 'finalization'}}]:
            self.reject(lambda a: a['observation'].update(failure=failure), 'partial')
        self.reject(lambda a: a['observation'].update(finalization_succeeded=True), 'partial')
        self.reject(lambda a: a['observation'].update(finalization_attempted=True), 'partial')
        self.reject(lambda a: a['observation']['operational_failure'].update(ordinal=0), 'partial')
        self.reject(lambda a: a['observation']['operational_failure'].update(source=self.base['observation']['events'][0]['source']), 'partial')

    def test_order_counts_payload_and_missingness(self):
        def event(a): return a['observation']['events'][0]
        changes = [lambda a: event(a).update(ordinal=1),
                   lambda a: event(a).update(severity='Warning'),
                   lambda a: event(a).update(causal_parent=0),
                   lambda a: event(a).update(triggering_occurrence=[]),
                   lambda a: event(a).update(evidence_availability='Unavailable'),
                   lambda a: a['observation'].update(events=[]),
                   lambda a: a['observation'].update(retained_diagnostic_bytes=0),
                   lambda a: a['observation'].update(work=a['configured_limits']['work'] + 1)]
        for change in changes:
            with self.subTest(change=change): self.reject(change)
        self.reject(lambda a: a['observation']['failure']['Limit'].update(limit=1), 'partial')
        self.reject(lambda a: a['observation']['failure']['Limit'].update(dimension='mystery'), 'partial')

    def test_source_units_bounds_roles_and_schema_domain(self):
        def e(a): return a['observation']['events'][0]
        def s(a): return e(a)['source']
        changes = [lambda a: a['observation'].update(bom_bytes=3),
                   lambda a: a['observation'].update(encoding='US-ASCII'),
                   lambda a: a['observation'].update(validated_root={'UnicodeScalars': {'start': 0, 'end': 8}}),
                   lambda a: a['observation'].update(validated_root={'Utf8Bytes': {'start': 0, 'end': 9}}),
                   lambda a: s(a).update(subject={'Utf8Bytes': {'start': 1, 'end': 8}}),
                   lambda a: s(a).update(attribute_value={'Utf8Bytes': {'start': 0, 'end': 8}}),
                   lambda a: s(a).update(subject_reference={'Attribute': {'owner_preorder': 1, 'attribute': 0}}),
                   lambda a: s(a).update(owner_reference={'Node': {'preorder': 2}}),
                   lambda a: s(a).update(line=0),
                   lambda a: e(a)['path'][0]['Element'].update(position=2),
                   lambda a: e(a)['subject']['Element'].update(local='bravo'),
                   lambda a: a['schemas'][0].update(backend_uri='file:///etc/passwd'),
                   lambda a: a['schemas'].reverse()]
        for change in changes:
            with self.subTest(change=change): self.reject(change)
        for uri, span in [('file:///etc/passwd', [0, 1]),
                          (self.descriptor['schemas'][0]['backend_uri'], [0, 3079]),
                          (None, [10, 1])]:
            self.reject(lambda a: e(a)['evidence'].update(component={
                'kind': 'Element', 'name': None, 'global': None,
                'document_uri': uri, 'schema_span': span}))

    def test_same_shaped_preorder_substitution_is_explicitly_undetectable(self):
        # Necessary consistency allows a jointly replaced root/reference. This
        # is an adverse demonstration of unavailable tree truth, NOT a positive
        # execution fixture or a claim of full witness verification.
        data = copy.deepcopy(self.base)
        o = data['observation']
        o['validated_root_reference']['Node']['preorder'] = 77
        join = o['events'][0]['source']
        join['owner_reference']['Node']['preorder'] = 77
        join['subject_reference']['Node']['preorder'] = 77
        v.check_observation(data, self.input, self.descriptor)
        self.assertEqual(v.report_template('complete')['source_witness_verification'], 'UNAVAILABLE')

    def test_repeated_reference_cannot_change_name_range_or_location(self):
        for field in ('line', 'column'):
            data = copy.deepcopy(self.base)
            duplicate = copy.deepcopy(data['observation']['events'][0])
            duplicate['ordinal'] = 1
            duplicate['source'][field] += 1
            data['observation']['events'].append(duplicate)
            data['observation']['observed_callbacks'] = 2
            with self.assertRaises(v.Refusal):
                v.check_observation(data, self.input, self.descriptor)


class SyntheticGrammarOnly(unittest.TestCase):
    """Compatibility grammar/state cases, never retained native coverage."""
    def test_variant_inventory_matches_actual_reviewed_rust_source(self):
        import re
        descriptor = v.pinned_descriptor()
        revision = descriptor['reviewed_producer_revision']
        member = 'crates/einvoice-schema/src/evidence.rs'
        proc = subprocess.run(['git', 'show', revision + ':' + member], cwd=ROOT.parents[1],
                              capture_output=True, timeout=10, check=False)
        self.assertEqual(proc.returncode, 0, 'Historical producer Git object required; see README history prerequisite')
        self.assertEqual(v.identify(proc.stdout), descriptor['source_members'][member])
        source = proc.stdout.decode('utf-8')
        for typename, variants, count in [('NativeCause', v.NATIVE_VARIANTS, 14),
                                           ('ValueCause', v.VALUE_VARIANTS, 4),
                                           ('FacetCause', v.FACET_VARIANTS, 17)]:
            region = source.split('pub enum ' + typename + ' {', 1)[1].split('\n}\n', 1)[0]
            names = set(re.findall(r'^    ([A-Z][A-Za-z0-9]+)(?:[,({ ])', region, re.M))
            self.assertEqual(names, set(variants))
            self.assertEqual(len(names), count)
        region = source.split('pub enum TypeCode {', 1)[1].split('\n}\n', 1)[0]
        names = re.findall(r'^    ([A-Z][A-Za-z0-9]+),', region, re.M)
        self.assertEqual(names, v.TYPE_CODES)
        self.assertEqual(len(names), 62)

    def test_all_full_typed_payload_shapes_grammar_only(self):
        facet_payloads = {
            'LengthViolation': {'message': 'm'}, 'MinLengthViolation': {'actual': v.U64, 'min': 1},
            'MaxLengthViolation': {'actual': 0, 'max': v.U64},
            'PatternViolation': {'value': 'é𝄞\r\n', 'pattern': '[x]'}, 'EnumerationViolation': {'value': 'x'},
            'MinInclusiveViolation': {'value': '1.1', 'min': '2'},
            'MinExclusiveViolation': {'value': '-1', 'min': '0'},
            'MaxInclusiveViolation': {'value': '3', 'max': '2'},
            'MaxExclusiveViolation': {'value': '3', 'max': '2'},
            'TotalDigitsViolation': {'actual': v.U32, 'max': 0},
            'FractionDigitsViolation': {'actual': 0, 'max': v.U32},
            'ExplicitTimezoneViolation': {'message': 'timezone'}, 'InvalidPattern': {'pattern': '[', 'message': 'm'},
            'DerivationRestriction': {'message': 'm'},
            'FixedFacetViolation': {'facet_name': 'f', 'base_value': 'b', 'derived_value': 'd'},
            'ConflictingFacets': {'message': 'm'}, 'NotApplicable': {'facet': 'f', 'type_name': 't'},
        }
        for tag, payload in facet_payloads.items():
            for cause in ({'Facet': {tag: payload}}, {'Value': {'FacetViolation': {tag: payload}}}):
                before = copy.deepcopy(cause)
                v.grammar(cause, v.NATIVE)
                self.assertEqual(cause, before)
                bad = copy.deepcopy(cause)
                target = bad['Facet'] if 'Facet' in bad else bad['Value']['FacetViolation']
                target[tag]['invented'] = 'x'
                with self.assertRaises(v.Refusal): v.grammar(bad, v.NATIVE)
        values = [{'Value': {'InvalidLexical': {'value': 'x', 'type_name': 't', 'message': 'm'}}},
                  {'Value': {'RangeError': {'value': '-99999999999999999999999', 'type_name': 'int'}}}]
        name = {'namespace': None, 'local': 'x'}
        natives = ['IncompleteContent', 'ProhibitedAttribute', 'AttributeNotAllowed', 'UndeclaredElement',
                   'NilNotAllowed', 'NilWithFixedValue', 'NilWithContent',
                   {'UnexpectedChild': {'found': name}}, {'MissingRequiredAttribute': {'name': name}},
                   {'AttributeFixedValue': {'expected': 'e', 'actual': 'a'}},
                   {'XsiTypeUnresolved': {'value': 'p:n'}},
                   {'XsiTypeDerivation': {'value': 'p:n', 'blocked': False}}]
        for value in values + natives:
            v.grammar(value, v.NATIVE)
        for code in v.TYPE_CODES:
            v.grammar({'Value': {'TypeError': {'expected': code, 'actual': 'None'}}}, v.NATIVE)
        for bad in (None, 0, 'none', {'None': None}):
            with self.assertRaises(v.Refusal): v.grammar(bad, v.TYPE_CODE)
        for number in (True, -1, 1.0, str(v.U32), v.U32 + 1):
            with self.assertRaises(v.Refusal):
                v.grammar({'TotalDigitsViolation': {'actual': number, 'max': 1}}, v.FACET)

    def test_declared_failure_and_component_grammar_only(self):
        cases = ['InvalidLimits', 'ArithmeticOverflow', 'Allocation', 'Cancelled', 'Deadline', 'EpochRetired',
                 'UnsupportedEvidence', 'SourceJoin', 'Consistency', {'Limit': {'dimension': 'work', 'limit': 0}},
                 {'UnsupportedCapability': {'capability': 'future'}}, {'Backend': {'phase': 'finalization'}}]
        for value in cases: v.grammar(value, v.FAILURE)
        for global_value in (None, False, True):
            value = {'kind': 'ComplexType', 'name': None, 'global': global_value,
                     'document_uri': None, 'schema_span': [0, v.U64]}
            v.grammar(value, v.COMPONENT)
        for span in ([0], [0, 1, 2], {'start': 0, 'end': 1}):
            with self.assertRaises(v.Refusal):
                v.grammar(dict(value, schema_span=span), v.COMPONENT)

    def test_nullable_admission_and_succeeded_incomplete_grammar_only(self):
        descriptor = v.pinned_descriptor()
        for root_present in (False, True):
            data, raw = artifact('schema-valid', 'partial')
            o = data['observation']
            o.update(input_sha256=None if not root_present else v.identify(raw)['sha256'],
                     validated_root_reference=None, root_validity=None, operational_failure=None,
                     work=0 if not root_present else len(raw))
            o['failure'] = {'Limit': {'dimension': 'work', 'limit': data['configured_limits']['work']}}
            v.check_observation(data, raw, descriptor)
        for invalid in (0, 1 << 63):
            data, raw = artifact('schema-valid', 'partial')
            data['configured_limits']['diagnostic_count'] = invalid
            data['observation'].update(input_sha256=None, validated_root_reference=None, root_validity=None,
                                       operational_failure=None, work=0, failure='InvalidLimits')
            v.check_observation(data, raw, descriptor)
        data, raw = artifact('schema-valid')
        data['epoch_retired'] = True
        data['observation'].update(assessment='Incomplete', failure='Deadline')
        v.check_observation(data, raw, descriptor)
        data['observation'].update(root_validity='NotKnown', failure='Consistency')
        v.check_observation(data, raw, descriptor)
        # Captured with absent cause/origins is legal explicit missingness.
        data, raw = artifact()
        data['observation']['events'][0]['evidence']['cause'] = None
        v.check_observation(data, raw, descriptor)

    def test_failure_and_accounting_preservation_grammar_only(self):
        descriptor = v.pinned_descriptor()
        # Logical boundary only, not actual retained-layout/resource evidence.
        data, raw = artifact()
        data['observation']['retained_diagnostic_bytes'] = data['observation']['work']
        v.check_observation(data, raw, descriptor)
        # First failure remains independent of separately refused terminal capture.
        for failure in ('SourceJoin', 'ArithmeticOverflow', 'UnsupportedEvidence',
                        {'Limit': {'dimension': 'terminal_bytes', 'limit': 1048576}},
                        {'Limit': {'dimension': 'path_temporary_bytes', 'limit': 1048576}}):
            data, raw = artifact(stage='partial')
            data['observation'].update(operational_failure=None, terminal_capture_failure=failure)
            v.check_observation(data, raw, descriptor)
        data, raw = artifact(stage='partial')
        data['configured_limits']['diagnostic_count'] = 1
        data['observation'].update(observed_callbacks=2,
                                  failure={'Limit': {'dimension': 'diagnostic_count', 'limit': 1}})
        v.check_observation(data, raw, descriptor)
        # A root reference alone does not prove runtime started.
        data, raw = artifact('schema-valid', 'partial')
        data['observation'].update(root_validity=None, operational_failure=None)
        v.check_observation(data, raw, descriptor)
        # Error phase is absent: keep a possible post-finalization consistency stop.
        data, raw = artifact()
        data['epoch_retired'] = True
        data['observation'].update(assessment='Incomplete', root_validity='Valid', failure='Consistency')
        v.check_observation(data, raw, descriptor)

    def test_encodings_and_attribute_roles_grammar_only(self):
        # No generated XML is executed. Only coordinate/domain grammar is tested.
        for encoding, raw, bom, unit, end in (
                ('UTF-8', b'\xef\xbb\xbf<r>\xc3\xa9</r>', 3, 'Utf8Bytes', 12),
                ('UTF-16LE', b'\xff\xfe' + '<r>𝄞\r\n</r>'.encode('utf-16-le'), 2, 'UnicodeScalars', 10),
                ('UTF-16BE', b'\xfe\xff' + '<r>𝄞\r\n</r>'.encode('utf-16-be'), 2, 'UnicodeScalars', 10),
                ('ISO-8859-1', b'<?xml version="1.0" encoding="latin1"?><r>\xe9</r>', 0, 'UnicodeScalars', 45),
                ('US-ASCII', b'<?xml version="1.0" encoding="ascii"?><r/>', 0, 'UnicodeScalars', 39)):
            end = len(raw) if unit == 'Utf8Bytes' else len(raw[bom:].decode({'UTF-16LE': 'utf-16-le', 'UTF-16BE': 'utf-16-be', 'ISO-8859-1': 'latin1', 'US-ASCII': 'ascii'}[encoding]))
            observation = {'encoding': encoding, 'bom_bytes': bom, 'validated_root': {unit: {'start': bom if unit == 'Utf8Bytes' else 0, 'end': end}}, 'validated_root_reference': {'Node': {'preorder': 1}}}
            checks = v.SourceChecks(raw, observation)
            with self.assertRaises(v.Refusal): checks.span({'UnicodeScalars' if unit == 'Utf8Bytes' else 'Utf8Bytes': {'start': 0, 'end': 1}})
            if encoding == 'UTF-8':
                with self.assertRaises(v.Refusal): checks.span({'Utf8Bytes': {'start': 7, 'end': 8}})
        base, raw = artifact()
        d = copy.deepcopy(base['observation']['events'][0])
        name = {'namespace': None, 'local': 'a'}
        d['subject'] = {'Attribute': name}
        d['path'].append({'Attribute': {'name': name}})
        d['source'].update(subject_reference={'Attribute': {'owner_preorder': 1, 'attribute': 0}},
                           subject={'Utf8Bytes': {'start': 2, 'end': 7}},
                           attribute_value={'Utf8Bytes': {'start': 4, 'end': 5}})
        checks = v.SourceChecks(raw, base['observation'])
        checks.diagnostic(d)
        for change in (lambda s: s.update(attribute_value={'Utf8Bytes': {'start': 0, 'end': 5}}),
                       lambda s: s['subject_reference']['Attribute'].update(owner_preorder=2)):
            bad = copy.deepcopy(d);change(bad['source'])
            with self.assertRaises(v.Refusal): v.SourceChecks(raw, base['observation']).diagnostic(bad)


class StrictJSON(unittest.TestCase):
    def test_malformed_duplicate_numbers_unicode_and_trailing(self):
        for raw in (b'{"x":1,"x":2}', b'{"x":NaN}', b'{"x":Infinity}', b'1.0', b'1e0', b'-0',
                    b'184467440737095516150', b'{"x":"\xff"}', b'"\\ud800"', b'"\\udfff"',
                    b'{"x":0}garbage', b'{', b'\xef\xbb\xbf{}'):
            with self.subTest(raw=raw):
                with self.assertRaises(v.Refusal): v.strict_json(raw)
        for value in (True, -1, 1.0, v.U64 + 1, '0'):
            with self.assertRaises(v.Refusal): v.grammar(value, 'u64')
        self.assertEqual(v.strict_json(str(v.U64).encode()), v.U64)
        self.assertEqual(v.strict_json(b'"\\ud834\\udd1e"'), '𝄞')

    def test_before_decode_depth_and_materialization_limits(self):
        for raw in (b'[' * (v.DEPTH_MAX + 1) + b'0' + b']' * (v.DEPTH_MAX + 1),
                    b'"' + b'x' * (v.STRING_MAX + 1) + b'"',
                    b'[' + b'0,' * (v.COLLECTION_MAX + 1) + b'0]',
                    b' ' * (v.JSON_MAX + 1)):
            with self.assertRaises(v.Refusal) as caught: v.strict_json(raw)
            self.assertEqual(caught.exception.code, 2)
        with patch.object(v, 'NODE_MAX', 10):
            with self.assertRaises(v.Refusal): v.strict_json(b'[0,0,0,0,0,0,0,0,0,0,0]')
        with patch.object(v, 'STRINGS_MAX', 3):
            with self.assertRaises(v.Refusal): v.strict_json(b'["ab","cd"]')
        self.assertEqual(v.strict_json(b'{"s":"[\\\"{]"}'), {'s': '["{]'})


class AttemptMutations(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='schema-capture-')
        # Platform /var or /tmp aliases are canonicalized ONLY in test setup.
        # The verifier itself never resolves or follows a caller symlink.
        self.root = Path(self.temp.name).resolve()
        self.attempt = self.root / 'attempt'
        shutil.copytree(EVIDENCE / 'linux/schema-producer-tests/unchanged', self.attempt)

    def tearDown(self): self.temp.cleanup()

    def verify(self, expected=1, selected='complete'):
        code, report = v.verify_attempt(self.attempt, selected)
        self.assertEqual(code, expected, report)
        self.assertFalse(report['release_gate_eligible'])
        return report

    def mutate(self, name, change):
        path = self.attempt / name
        data = load(path);change(data);dump(path, data)

    def rebind_observation(self, stage, change):
        self.mutate(stage + '.json', lambda a: change(a['observation']))
        binding = v.identify((self.attempt / (stage + '.json')).read_bytes())
        self.mutate(stage + '.status.json', lambda s: s['stdout'].update(binding))
        self.mutate('receipt.json', lambda r: r.update({stage + '_output': binding}))

    def test_rebound_terminal_failure_and_accounting_contradictions(self):
        original = {p.name: p.read_bytes() for p in self.attempt.iterdir()}
        changes = [
            ('partial', lambda o: o.update(operational_failure=None)),
            ('partial', lambda o: o.update(root_validity=None, operational_failure=None)),
            ('partial', lambda o: o.update(finalization_attempted=True, finalization_succeeded=True)),
            ('partial', lambda o: o.update(finalization_attempted=True, finalization_succeeded=True,
                                           operational_failure=None, terminal_capture_failure='SourceJoin')),
            ('partial', lambda o: o.update(failure='Consistency')),
            ('partial', lambda o: o.update(failure={'Limit': {'dimension': 'diagnostic_count', 'limit': 1000}})),
            ('partial', lambda o: o.update(observed_callbacks=1002)),
            ('complete', lambda o: o.update(retained_diagnostic_bytes=o['work'] + 1)),
        ]
        for invalid in ('InvalidLimits', 'Cancelled', 'Deadline', 'Allocation', 'Consistency',
                        {'Limit': {'dimension': 'work', 'limit': 1938}},
                        {'Backend': {'phase': 'operational'}}):
            changes.append(('partial', lambda o, invalid=invalid: o.update(terminal_capture_failure=invalid)))
        for index, (stage, change) in enumerate(changes):
            with self.subTest(index=index):
                for name, raw in original.items(): (self.attempt / name).write_bytes(raw)
                self.rebind_observation(stage, change)
                for selection in ('complete', 'partial'): self.verify(1, selection)
        self.cli(expected=1)

    def test_pair_root_facts_without_claiming_lexical_truth(self):
        def change_root(o):
            o['validated_root_reference']['Node']['preorder'] = 77
            for event in o['events']:
                for field in ('owner_reference', 'subject_reference'):
                    event['source'][field]['Node']['preorder'] = 77
        self.rebind_observation('partial', change_root)
        self.verify(1)
        receipt = (self.attempt / 'receipt.json').read_bytes()
        (self.attempt / 'receipt.json').unlink()
        self.verify(1)  # Contradiction is observable even without a receipt.
        (self.attempt / 'receipt.json').write_bytes(receipt)
        self.rebind_observation('complete', change_root)
        self.verify(0)  # Joint substitution still cannot prove lexical truth.
        shutil.rmtree(self.attempt)
        shutil.copytree(EVIDENCE / 'linux/schema-valid', self.attempt)
        self.rebind_observation('partial', lambda o: o['validated_root']['Utf8Bytes'].update(start=1))
        self.verify(1)

    def test_present_null_optional_files_are_malformed_not_absent(self):
        receipt = (self.attempt / 'receipt.json').read_bytes()
        for name in ('failure.json', 'receipt.json'):
            with self.subTest(name=name):
                (self.attempt / name).write_bytes(b'null\n')
                self.verify(1)
                self.cli(expected=1)
                if name == 'failure.json': (self.attempt / name).unlink()
                else: (self.attempt / name).write_bytes(receipt)
        (self.attempt / 'receipt.json').unlink()
        self.verify(2)

    def test_changed_raw_output_status_input_and_source_bindings(self):
        for name, content in [('input.xml', b'<bravo/>'), ('complete.json', b'{}'),
                              ('complete.stderr', b'changed'), ('source-inventory.json', b'{}')]:
            path = self.attempt / name;before = path.read_bytes();path.write_bytes(content)
            self.verify();path.write_bytes(before)
        self.mutate('complete.status.json', lambda s: s['stdout'].update(bytes=1))
        self.verify()

    def test_unknown_producer_and_changed_source_map(self):
        self.mutate('receipt.json', lambda r: r.update(git_head='0' * 40))
        report = self.verify(2)
        self.assertIn('unreviewed', report['reason'])
        self.mutate('source-inventory.json', lambda s: s['Cargo.toml'].update(sha256='0' * 64))
        self.verify(1)  # Original receipt still binds original inventory bytes.
        (self.attempt / 'receipt.json').unlink()
        self.verify(2)  # Unsupported producer map, never draft-string acceptance.

    def test_failure_and_source_change_prevent_success_receipt(self):
        dump(self.attempt / 'failure.json', {'kind': 'development reproduction failure; not an Observation',
                                            'error_type': 'ValueError', 'message': 'source changed'})
        self.verify()
        (self.attempt / 'failure.json').unlink()
        self.mutate('receipt.json', lambda r: r.update(source_unchanged_during_build_and_run=False))
        self.verify()

    def test_failed_statuses_cannot_certify_capture(self):
        for change in (lambda s: s.update(returncode=1), lambda s: s.update(returncode=-9),
                       lambda s: s.update(timed_out=True), lambda s: s.update(truncated=[True, False]),
                       lambda s: s.update(read_errors=['read failure', None]),
                       lambda s: s.update(pipe_drain_complete=False),
                       lambda s: s.update(launch_error='launch failure', returncode=None)):
            path = self.attempt / 'complete.status.json';before = path.read_bytes()
            self.mutate(path.name, change);self.verify();path.write_bytes(before)
        (self.attempt / 'receipt.json').unlink()
        self.mutate('complete.status.json', lambda s: s.update(timed_out=True))
        report = self.verify(2)
        self.assertEqual(report['attempt_status'], 'retained_child_failure')
        self.assertIsNone(report['producer_reported_assessment'])

    def test_paired_status_input_and_receipt_commands(self):
        changes = [('partial.status.json', lambda s: s['command'].__setitem__(2, '/opaque/other/input.xml')),
                   ('receipt.json', lambda r: r['partial_output'].update(sha256='0' * 64)),
                   ('receipt.json', lambda r: r['backend_features'].append('future')),
                   ('receipt.json', lambda r: r['loaded_schema_bytes'].clear())]
        for name, change in changes:
            path = self.attempt / name;before = path.read_bytes()
            self.mutate(name, change);self.verify();path.write_bytes(before)

    def test_unselected_corruption_is_checked_before_complete_can_succeed(self):
        self.mutate('partial.json', lambda a: a['observation'].update(finalization_succeeded=True))
        report = self.verify(1)
        self.assertIsNone(report['producer_reported_assessment'])

    def test_missing_evidence_and_no_implicit_output(self):
        (self.attempt / 'receipt.json').unlink();self.verify(2)
        (self.attempt / 'partial.stderr').unlink();self.verify(1)
        (self.attempt / 'input.xml').unlink();self.verify(2)
        self.assertFalse((self.attempt / 'report.json').exists())

    def cli(self, directory=None, expected=1):
        proc = subprocess.run([sys.executable, '-I', str(ROOT / 'verify.py'), '--attempt', str(directory or self.attempt)],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=False)
        self.assertEqual(proc.returncode, expected, proc.stderr)
        self.assertEqual(proc.stderr, b'')
        self.assertLess(len(proc.stdout), 16 * 1024)
        result = json.loads(proc.stdout)
        self.assertFalse(result['release_gate_eligible'])
        return result

    def test_actual_cli_genuine_and_directory_fifo_symlink_hardlink(self):
        self.cli(expected=0)
        path = self.attempt / 'input.xml';raw = path.read_bytes();path.unlink();path.mkdir()
        self.cli();path.rmdir();path.write_bytes(raw)
        os.mkfifo(self.root / 'fifo')
        path.unlink();os.mkfifo(path);self.cli();path.unlink();path.write_bytes(raw)
        self.cli(self.root / 'fifo')
        link = self.root / 'ancestor';link.symlink_to(self.attempt, target_is_directory=True)
        self.cli(link)
        path.unlink();path.symlink_to(EVIDENCE / 'linux/schema-producer-tests/unchanged/input.xml')
        self.cli();path.unlink();path.write_bytes(raw)
        os.link(path, self.root / 'hardlink');self.cli()

    def test_resource_limits_no_read_twice_and_descriptor_integrity(self):
        with v.Files(self.attempt) as files:
            first = files.read('input.xml')
            (self.attempt / 'input.xml').write_bytes(b'changed')
            self.assertIs(files.read('input.xml'), first)
            self.assertEqual(v.identify(first), v.identify(b'<alpha/>'))
            with self.assertRaises(v.Refusal): files.read('../input.xml')
        with patch.object(v, 'INPUT_MAX', 2): self.verify(2)
        with patch.object(v, 'AGGREGATE_MAX', 3): self.verify(2)
        with patch.object(v, 'FILE_MAX', 1): self.verify(2)
        with patch.object(v, 'DESCRIPTOR_SHA256', '0' * 64): self.verify(1)
        (self.attempt / 'input.xml').write_bytes(b'<alpha/>')
        with (self.attempt / 'input.xml').open('r+b') as file:
            file.truncate(v.INPUT_MAX + 1)
        result = self.cli(expected=2)
        self.assertEqual(result['result'], 'consumer_resource_failure')

    def test_opaque_historical_paths_are_never_dereferenced(self):
        with patch.object(v.Files, 'read', autospec=True, side_effect=v.Files.read) as read:
            self.verify(0)
            self.assertTrue(all(call.args[1] in v.ATTEMPT_NAMES or call.args[1] == 'producer-2d82953.json'
                                for call in read.call_args_list))


if __name__ == '__main__':
    unittest.main()
