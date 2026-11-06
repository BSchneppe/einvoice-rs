"""Behavior tests against the CLI and real pinned reference, not a mock validator.
Run `oracle.py fetch` and `oracle.py corpus` before this test suite.
"""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT/'oracle.py'


def rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


class ReferenceBehavior(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.output = Path(cls.temp.name)/'outcomes.jsonl'
        run = subprocess.run([sys.executable,str(CLI),'label','--output',str(cls.output)],capture_output=True,text=True)
        # The corpus intentionally contains processing failures; status 2 preserves
        # their observations and must not be mistaken for a passing conformance run.
        if run.returncode != 2:
            raise AssertionError((run.returncode,run.stdout,run.stderr))
        cls.outcomes = {r['id']:r for r in rows(cls.output)}

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_twelve_discovery_inputs_are_accepted(self):
        for index in range(12):
            outcome = self.outcomes[f'sinabis-{index:05d}']
            self.assertEqual(outcome['processing'],'completed')
            self.assertEqual(outcome['schema'],'valid')
            self.assertEqual(outcome['business'],'evaluated')
            self.assertEqual(outcome['acceptance'],'accepted')

    def test_structural_mutations_fail_schema_before_business_rules(self):
        for family in ['extra-attribute','unknown-element','duplicate-context','invalid-decimal']:
            outcome = self.outcomes['mutation-'+family]
            self.assertEqual(outcome['schema'],'invalid',family)
            self.assertEqual(outcome['business'],'not_evaluated',family)
            self.assertEqual(outcome['acceptance'],'rejected',family)
            self.assertTrue(outcome['structural_findings'],family)

    def test_missing_line_total_preserves_all_collateral_findings(self):
        outcome = self.outcomes['mutation-missing-line-total']
        self.assertEqual(outcome['schema'],'valid')
        self.assertEqual(outcome['acceptance'],'rejected')
        self.assertEqual({f['id'] for f in outcome['findings']},
            {'BR-24','BR-S-08','BR-CO-10','PEPPOL-EN16931-R120','PEPPOL-EN16931-R008'})
        self.assertTrue(all(f['location'] and f['message'] and f['effective_severity'] for f in outcome['findings']))

    def test_equivalent_prefix_and_wrong_namespace_are_distinguished(self):
        self.assertEqual(self.outcomes['mutation-equivalent-prefix']['acceptance'],'accepted')
        wrong = self.outcomes['mutation-wrong-namespace']
        self.assertFalse(wrong['target_matched'])
        self.assertNotEqual(wrong['acceptance'],'accepted')

    def test_policy_can_accept_a_raw_rule_error(self):
        outcome = self.outcomes['mutation-severity-override']
        finding = next(f for f in outcome['findings'] if f['id']=='BR-CL-23')
        self.assertEqual(finding['severity'],'error')
        self.assertEqual(finding['effective_severity'],'warning')
        self.assertEqual(outcome['acceptance'],'accepted')
        self.assertEqual(outcome['stages']['val-sch.1'],'invalid')

    def test_policy_can_escalate_a_warning(self):
        outcome = self.outcomes['mutation-severity-escalation']
        finding = next(f for f in outcome['findings'] if f['id']=='CII-SR-452')
        self.assertEqual(finding['severity'],'warning')
        self.assertEqual(finding['effective_severity'],'error')
        self.assertEqual(outcome['acceptance'],'rejected')

    def test_processing_failures_never_imply_validity(self):
        for family in ['malformed-xml','deep-unknown']:
            outcome = self.outcomes['mutation-'+family]
            self.assertEqual(outcome['processing'],'failed')
            self.assertTrue(outcome['processing_errors'])
            self.assertEqual(outcome['acceptance'],'indeterminate')
            self.assertEqual(outcome['schema'],'not_evaluated')

    def test_full_html_and_no_html_have_identical_diagnostics_and_acceptance(self):
        output = Path(self.temp.name)/'html.jsonl'
        run = subprocess.run([sys.executable,str(CLI),'label','--html','--output',str(output)],capture_output=True,text=True)
        self.assertEqual(run.returncode,2,run.stderr)
        self.assertEqual(rows(self.output),rows(output))
        primary_meta=json.loads(Path(str(self.output)+'.metadata.json').read_text())
        html_meta=json.loads(Path(str(output)+'.metadata.json').read_text())
        self.assertEqual(set(primary_meta['report_html_element_counts']),set(self.outcomes))
        self.assertTrue(all(count==0 for count in primary_meta['report_html_element_counts'].values()))
        # Unmatched and malformed inputs use KoSIT's separate fallback report.
        self.assertEqual(html_meta['report_html_element_counts']['mutation-wrong-namespace'],1)
        self.assertEqual(primary_meta['report_html_element_counts']['mutation-wrong-namespace'],0)

    def test_scoped_success_is_never_global_validity(self):
        output=Path(self.temp.name)/'scoped.jsonl'
        run=subprocess.run([sys.executable,str(CLI),'scoped','--output',str(output)],capture_output=True,text=True)
        self.assertEqual(run.returncode,0,run.stderr)
        outcomes=rows(output)
        self.assertEqual(outcomes,rows(ROOT/'results/scoped.jsonl'))
        self.assertGreater(len(outcomes),2)
        self.assertTrue(all(o['scope']=='rule_test_fragment' and o['acceptance']=='not_applicable' and
                            o['schema']=='not_evaluated' and o['expectations_met'] for o in outcomes))
        self.assertTrue(any(o['findings'] for o in outcomes))
        self.assertTrue(all(not key.startswith('scoped-') for key in self.outcomes))

    def test_timeout_replaces_stale_success_with_machine_readable_failure(self):
        output=Path(self.temp.name)/'timeout.jsonl'
        output.write_text('{"acceptance":"accepted"}\n')
        run=subprocess.run([sys.executable,str(CLI),'label','--ids','sinabis-00000',
            '--timeout','0','--output',str(output)],capture_output=True,text=True)
        self.assertEqual(run.returncode,2)
        outcome=rows(output)[0]
        self.assertEqual(outcome['processing'],'failed')
        self.assertEqual(outcome['acceptance'],'indeterminate')
        self.assertEqual(outcome['business'],'not_evaluated')

    def test_corrupt_input_hash_stops_the_run(self):
        manifest=Path(self.temp.name)/'corrupt.jsonl'
        original=rows(ROOT/'corpus/initial.jsonl')[0]
        original['sha256']='0'*64
        manifest.write_text(json.dumps(original)+'\n')
        output=Path(self.temp.name)/'corrupt-output.jsonl'
        run=subprocess.run([sys.executable,str(CLI),'label','--manifest',str(manifest),
            '--output',str(output)],capture_output=True,text=True)
        self.assertEqual(run.returncode,2)
        self.assertIn('Corpus hash mismatch',run.stderr)
        self.assertFalse(output.exists())

    def test_all_dataset_xml_hashes_and_whole_theme_split(self):
        manifest=rows(ROOT/'corpus/sinabis.jsonl')
        self.assertEqual(len(manifest),10000)
        themes={}
        for row in manifest:
            themes.setdefault(row['scenario'],set()).add(row['split'])
            self.assertEqual(hashlib.sha256((ROOT/row['path']).read_bytes()).hexdigest(),row['sha256'])
            self.assertEqual(row['revision'],'8d9ab205ab74f360fac344a50fcd54c35d0cd9ba')
        self.assertEqual(len(themes),6)
        self.assertTrue(all(len(splits)==1 for splits in themes.values()))
        initial=rows(ROOT/'corpus/initial.jsonl')
        families={}
        for row in initial:
            if row['source']=='local-mutation-v1':
                families.setdefault(row['family'],set()).add(row['split'])
        self.assertTrue(all(len(splits)==1 for splits in families.values()))

    def test_fresh_initial_outcomes_match_frozen_reference_exactly(self):
        self.assertEqual(rows(self.output),rows(ROOT/'results/initial.jsonl'))

    def test_frozen_full_document_outcomes_have_schema_coverage_and_provenance(self):
        schema=json.loads((ROOT/'outcome.schema.json').read_text())
        Draft202012Validator.check_schema(schema)
        validator=Draft202012Validator(schema)
        for name in ['initial','sinabis']:
            manifest={r['id']:r for r in rows(ROOT/f'corpus/{name}.jsonl') if r['scope']=='complete'}
            outcomes=rows(ROOT/f'results/{name}.jsonl')
            self.assertEqual(len(outcomes),len({r['id'] for r in outcomes}))
            self.assertEqual({r['id'] for r in outcomes},set(manifest))
            for outcome in outcomes:
                validator.validate(outcome)
                original=manifest[outcome['id']]
                self.assertEqual(outcome['sha256'],original['sha256'])
                self.assertEqual(outcome['split'],original['split'])
                self.assertEqual(outcome['scope'],original['scope'])
            metadata=json.loads((ROOT/f'results/{name}.jsonl.metadata.json').read_text())
            self.assertEqual(metadata['artifact_lock_sha256'],hashlib.sha256((ROOT/'artifacts.lock.json').read_bytes()).hexdigest())
            self.assertEqual(metadata['bridge_sha256'],hashlib.sha256((ROOT/'java/Oracle.java').read_bytes()).hexdigest())

    def test_repeated_benchmark_includes_every_declared_group(self):
        output=Path(self.temp.name)/'benchmark.json'
        run=subprocess.run([sys.executable,str(CLI),'benchmark','--output',str(output),
            '--machine','test-host','--cpu-allocation','single worker; smoke test, no OS affinity',
            '--warmup','1','--runs','2','--repeats','1'],capture_output=True,text=True)
        self.assertEqual(run.returncode,0,run.stderr)
        benchmark=json.loads(output.read_text())
        self.assertFalse(benchmark['release_gate_eligible'])
        self.assertEqual(len(benchmark['primary']),2)
        self.assertEqual(len(benchmark['full_html']),2)
        for run in benchmark['primary']:
            self.assertEqual(set(run['groups']),set(benchmark['weights']))
            self.assertGreater(run['weighted_documents_per_second'],0)
        self.assertEqual(benchmark['metadata']['processors'],1)
        self.assertEqual(benchmark['metadata']['warmup_passes'],1)
        self.assertGreater(benchmark['metadata']['input_io_ns'],0)
        self.assertEqual(benchmark['observed_outcomes']['mutation-malformed-xml']['processing'],'failed')


if __name__=='__main__':
    unittest.main()
