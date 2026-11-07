#!/usr/bin/env python3
"""Coherent artifact corruptions: outer digests are refreshed, source lock is not."""
import argparse
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import candidate


def canonical(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + '\n').encode()


class CandidateCorruptionTests(unittest.TestCase):
    def check_mutation(self, edit, error):
        with tempfile.TemporaryDirectory(prefix='candidate-corruption-') as temp:
            one, two = Path(temp) / 'one', Path(temp) / 'two'
            shutil.copytree(self.baseline, one)
            value = json.loads((one / 'resolved-candidate.json').read_bytes())
            edit(value)
            (one / 'resolved-candidate.json').write_bytes(canonical(value))
            manifest = json.loads((one / 'resolved-manifest.json').read_bytes())
            manifest['candidate_sha256'] = candidate.digest(canonical(value))
            manifest['effective_package_sha256'] = candidate.digest(canonical(value['effective_package']))
            manifest['files'] = {k: v for k, v in candidate.files(one).items() if k != 'resolved-manifest.json'}
            (one / 'resolved-manifest.json').write_bytes(canonical(manifest))
            shutil.copytree(one, two)
            with self.assertRaisesRegex(AssertionError, error):
                candidate.check(one, two, self.archives)

    def test_positive_control(self):
        with tempfile.TemporaryDirectory(prefix='candidate-control-') as temp:
            other = Path(temp) / 'other'
            shutil.copytree(self.baseline, other)
            candidate.check(self.baseline, other, self.archives)

    def test_coherent_selected_tree_corruption(self):
        for mutation in ('axis', 'function', 'namespace'):
            with self.subTest(mutation=mutation):
                def edit(value):
                    resolution = value['resolutions'][0]
                    expression = resolution['duplicates'][0]['expression']
                    if mutation == 'axis':
                        expression['tree']['arguments'][0]['axis'] = 'attribute'
                    elif mutation == 'function':
                        expression['tree']['name'].update(qualified='exists', local='exists')
                    else:
                        expression['tree']['arguments'][0]['test']['name']['namespace'] = 'urn:wrong'
                    resolution['effective'] = copy.deepcopy(expression)
                    for schema in value['effective_package']['schemas']:
                        for pattern in schema['patterns']:
                            for rule in pattern['rules']:
                                for site in rule['sites']:
                                    if site['identity'] == resolution['original_site']['identity']:
                                        site['test'] = copy.deepcopy(expression)
                self.check_mutation(edit, 'selected AST')

    def test_coherent_added_resource_corruption(self):
        for field, wrong in [('archive', 'config.zip'), ('member', 'invented.sch'),
                             ('role', 'notice'), ('pin', 'invented authority')]:
            with self.subTest(field=field):
                def edit(value):
                    resource = 'cen.zip/schematron/CII/EN16931-CII-syntax.sch'
                    value['added_resources'][resource][field] = wrong
                    value['effective_package']['resources'][resource][field] = wrong
                self.check_mutation(edit, 'added resource metadata')

    def test_unrelated_tree_corruption(self):
        def edit(value):
            value['effective_package']['schemas'][0]['patterns'][0]['rules'][0]['sites'][0]['test']['tree'] = {'kind': 'context'}
        self.check_mutation(edit, 'unexplained materialized package delta')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--archives', type=Path, required=True)
    args = parser.parse_args()
    CandidateCorruptionTests.baseline = args.candidate
    CandidateCorruptionTests.archives = args.archives
    unittest.main(argv=['test_candidate.py'])
