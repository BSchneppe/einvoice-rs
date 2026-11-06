#!/usr/bin/env python3
"""Stdlib lexical and corruption tests; pass --ir/--archives for pinned checks.

generate.py supplies both arguments in CI, so no pinned tests are skipped there.
All corruptions are in memory or temporary directories; source pins/goldens stay
unchanged. Temporary manifests have matching hashes, isolating lexical evidence
from the independent protection afforded by reviewed output hashes.
"""
import argparse
from collections import Counter
from contextlib import redirect_stdout
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import inventory
from occurrences import check_occurrences, source_occurrences, tokens

IR_PATH = None
ARCHIVES = None


def nodes(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from nodes(child)
    elif isinstance(value, list):
        for child in value:
            yield from nodes(child)


def name(qualified):
    return {'qualified': qualified, 'namespace': '', 'local': qualified}


def integer(value):
    return {'kind': 'number', 'lexical_kind': 'integer', 'lexeme': value}


class LexicalTests(unittest.TestCase):
    def test_quotes_doubled_quotes_and_nested_comments(self):
        text = '''concat('it''s (: $fake + 9 :)', "a""b", (: outer ' (: inner :) :) 'x')'''
        self.assertEqual(source_occurrences(text), Counter({
            ('function', 'concat'): 1, ('string', "it's (: $fake + 9 :)"): 1,
            ('string', 'a"b'): 1, ('string', 'x'): 1}))
        self.assertEqual(tokens("'(:'(: 'ignored (: :) ' :)+1"),
                         [('string', '(:'), ('symbol', '+'), ('number', '1')])

    def test_numeric_lexemes_are_not_values(self):
        self.assertEqual(source_occurrences('(+001, -1.00, .5, 2., 1e+03, 1E-3)'), Counter({
            ('operator', '+'): 1, ('operator', '-'): 1,
            ('number-integer', '001'): 1, ('number-decimal', '1.00'): 1,
            ('number-decimal', '.5'): 1, ('number-decimal', '2.'): 1,
            ('number-double', '1e+03'): 1, ('number-double', '1E-3'): 1}))

    def test_qnames_keywords_wildcards_axes_and_multiplication(self):
        self.assertEqual(source_occurrences('child::and/@p:id * 2 + ../p:*/*:local'), Counter({
            ('node', 'and'): 1, ('node', 'p:id'): 1, ('node', 'p:*'): 1,
            ('node', '*:local'): 1, ('axis', 'child'): 3, ('axis', 'attribute'): 1,
            ('axis', 'parent'): 1, ('kind-test', 'node'): 1,
            ('operator', '/'): 3, ('operator', '*'): 1, ('operator', '+'): 1,
            ('number-integer', '2'): 1}))
        self.assertEqual(source_occurrences('(/, //*, ., attribute::*)'), Counter({
            ('root', '/'): 2, ('operator', '//'): 1, ('node', '*'): 2,
            ('axis', 'child'): 1, ('axis', 'attribute'): 1, ('context', '.'): 1}))

    def test_kind_tests_are_not_function_calls(self):
        self.assertEqual(source_occurrences('node() | text() | comment() | processing-instruction() | fn:node()'),
                         Counter({('kind-test', k): 1 for k in
                                  ('node', 'text', 'comment', 'processing-instruction')}) +
                         Counter({('operator', '|'): 4, ('axis', 'child'): 4,
                                  ('function', 'fn:node'): 1}))

    def test_binder_declarations_references_and_shadowing(self):
        text = 'for $x in $outer, $y in $x return (some $x in $y satisfies $x = $outer)'
        self.assertEqual(source_occurrences(text), Counter({
            ('operator', 'for'): 1, ('operator', 'some'): 1, ('operator', '='): 1,
            ('declaration', 'x'): 2, ('declaration', 'y'): 1,
            ('reference', 'x'): 2, ('reference', 'outer'): 2, ('reference', 'y'): 1}))
        # Authored tree: the inner initializer sees outer x, result sees inner x.
        ref = lambda binding: {'kind': 'variable', 'name': name('x'), 'binding': binding}
        tree = {'kind': 'bind', 'operator': 'for', 'bindings': [
            {'id': 'source/expr:outer', 'name': name('x'), 'value': integer('1')}],
            'result': {'kind': 'bind', 'operator': 'some', 'bindings': [
                {'id': 'source/expr:inner', 'name': name('x'), 'value': ref('source/expr:outer')}],
                'result': {'kind': 'binary', 'operator': '=',
                           'left': ref('source/expr:inner'), 'right': integer('1')}}}
        check_occurrences('for $x in 1 return some $x in $x satisfies $x = 1', tree)
        inventory.check_resolution(tree, {}, {})
        tree['result']['bindings'][0]['value']['binding'] = 'source/expr:inner'
        with self.assertRaises(AssertionError):
            inventory.check_resolution(tree, {}, {})

    def test_type_operators_and_suffixes_are_not_node_names_or_multiplication(self):
        self.assertEqual(source_occurrences(
            '($x cast as xs:decimal?, $x castable as xs:integer, '
            '$x instance of xs:string*, $x treat as xs:string+)'), Counter({
                ('reference', 'x'): 4, ('operator', 'cast'): 1, ('operator', 'castable'): 1,
                ('operator', 'instance'): 1, ('operator', 'treat'): 1,
                ('type', 'xs:decimal?'): 1, ('type', 'xs:integer'): 1,
                ('type', 'xs:string*'): 1, ('type', 'xs:string+'): 1}))

    def test_group_is_transparent_and_multiplicity_is_required(self):
        tree = {'kind': 'call', 'name': name('f'), 'arguments': [
            {'kind': 'group', 'operand': {'kind': 'group', 'operand': integer('01')}},
            integer('01')]}
        check_occurrences('f(((01)), 01)', tree)
        tree['arguments'].pop()
        with self.assertRaisesRegex(AssertionError, 'number-integer.*01'):
            check_occurrences('f(((01)), 01)', tree)

    def test_unterminated_lexical_constructs_and_unknown_ast_fail(self):
        for text in ["'unfinished", '(: outer (: inner :)']:
            with self.subTest(text=text), self.assertRaises(AssertionError):
                source_occurrences(text)
        with self.assertRaisesRegex(AssertionError, 'unaccounted AST kind'):
            check_occurrences('1', {'kind': 'opaque', 'text': '1'})


class PinnedCorruptionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if IR_PATH is None or ARCHIVES is None:
            raise unittest.SkipTest('supply --ir and --archives for pinned integration checks (CI does)')
        cls.package = json.loads(IR_PATH.read_bytes())
        cls.manifest = json.loads((IR_PATH.parent / 'manifest.json').read_bytes())

    def run_inventory(self, package):
        with tempfile.TemporaryDirectory(prefix='einvoice-r6-') as temporary:
            output = Path(temporary)
            data = (json.dumps(package, ensure_ascii=False) + '\n').encode()
            (output / 'ir.json').write_bytes(data)
            manifest = dict(self.manifest, ir_sha256=inventory.digest(data))
            (output / 'manifest.json').write_text(json.dumps(manifest), encoding='utf-8')
            (output / 'resources').symlink_to((IR_PATH.parent / 'resources').resolve(), target_is_directory=True)
            arguments = ['inventory.py', '--ir', str(output / 'ir.json'), '--archives', str(ARCHIVES)]
            with patch.object(sys, 'argv', arguments), redirect_stdout(io.StringIO()) as printed:
                inventory.main()
            return json.loads(printed.getvalue())

    def assert_rejected(self, mutate):
        package = copy.deepcopy(self.package)
        mutate(package)
        with self.assertRaisesRegex(AssertionError, 'XPath occurrence mismatch'):
            self.run_inventory(package)

    def test_full_unmutated_pinned_package(self):
        result = self.run_inventory(self.package)
        self.assertEqual((result['expressions'], result['sites'], result['rules'], result['helpers']),
                         (1108, 880, 156, 14))

    def test_erased_all_schema_and_policy_trees(self):
        def mutate(package):
            expressions = list(inventory.all_expressions(package))
            self.assertEqual(len(expressions), 1110)
            for expression in expressions:
                expression['tree'] = integer('1')
        self.assert_rejected(mutate)

    def mutate_first(self, predicate, mutation, select=lambda p: p['schemas']):
        def mutate(package):
            node = next(n for e in inventory.all_expressions(select(package))
                        for n in nodes(e['tree']) if predicate(n))
            mutation(node)
        self.assert_rejected(mutate)

    def test_single_reference_erased(self):
        def erase(node):
            node.clear()
            node.update(integer('1'))
        self.mutate_first(lambda n: n.get('kind') == 'variable', erase)

    def test_numeric_lexeme_and_kind_changed(self):
        for field, value in [('lexeme', '000000987654321'), ('lexical_kind', 'double')]:
            with self.subTest(field=field):
                self.mutate_first(lambda n: n.get('kind') == 'number' and n['lexical_kind'] == 'integer',
                                  lambda n: n.update({field: value}))

    def test_string_value_changed(self):
        self.mutate_first(lambda n: n.get('kind') == 'string', lambda n: n.update(value='R6 corruption'))

    def test_known_function_changed(self):
        # Both names resolve to real fn functions with one accepted argument.
        self.mutate_first(lambda n: n.get('kind') == 'call' and n['name']['qualified'] == 'normalize-space',
                          lambda n: n['name'].update(qualified='string', local='string'))

    def test_node_name_changed(self):
        self.mutate_first(lambda n: n.get('kind') == 'step' and n['test']['kind'] == 'name',
                          lambda n: n['test']['name'].update(qualified='R6Node', namespace='', local='R6Node'))

    def test_operator_changed(self):
        self.mutate_first(lambda n: n.get('kind') == 'binary' and n['operator'] == 'or',
                          lambda n: n.update(operator='and'))

    def test_axis_changed(self):
        self.mutate_first(lambda n: n.get('kind') == 'step' and n['axis'] == 'child',
                          lambda n: n.update(axis='self'))

    def test_call_wrapper_removed(self):
        def remove_call(node):
            argument = node['arguments'][0]
            node.clear()
            node.update(argument)
        self.mutate_first(lambda n: n.get('kind') == 'call' and n['name']['qualified'] == 'normalize-space',
                          remove_call)

    def test_binder_declaration_changed(self):
        def change(node):
            # Rename declaration AND its references together: scope checker still passes.
            binding = node['bindings'][0]
            original = binding['name']['qualified']
            for child in nodes(node):
                if child.get('qualified') == original:
                    child.update(qualified='R6Variable', local='R6Variable')
        self.mutate_first(lambda n: n.get('kind') == 'bind', change)

    def test_helper_expression_erased(self):
        def mutate(package):
            helper = package['schemas'][1]['helpers'][0]
            next(inventory.all_expressions(helper))['tree'] = integer('1')
        self.assert_rejected(mutate)

    def test_dynamic_message_expression_erased(self):
        def mutate(package):
            part = next(n for n in nodes(package['schemas']) if n.get('kind') == 'value_of')
            part['expression']['tree'] = integer('1')
        self.assert_rejected(mutate)

    def test_both_policy_trees_erased_individually(self):
        for field in ['selection', 'acceptance']:
            with self.subTest(field=field):
                self.assert_rejected(lambda p: p['policy'][field].update(tree=integer('1')))

    def test_policy_predicate_and_kind_test_changed(self):
        self.mutate_first(lambda n: n.get('kind') == 'number', lambda n: n.update(lexeme='2'),
                          lambda p: p['policy']['acceptance'])
        self.mutate_first(lambda n: n.get('kind') == 'step' and n['test']['kind'] == 'kind',
                          lambda n: n['test'].update(name='node'), lambda p: p['policy']['selection'])

    def test_authored_helper_and_policy_structure(self):
        helper = self.package['schemas'][1]['helpers'][0]
        self.assertEqual(helper['name']['qualified'], 'u:decimalOrZero')
        tree = helper['body'][0]['expression']['tree']
        self.assertEqual(tree['kind'], 'if')
        for field, function in [('condition', 'boolean'), ('then', 'xs:decimal')]:
            call = tree[field]
            self.assertEqual((call['kind'], call['name']['qualified'], len(call['arguments'])),
                             ('call', function, 1))
            self.assertEqual(call['arguments'][0]['name']['qualified'], 'element')
            self.assertEqual(call['arguments'][0]['binding'], helper['parameters'][0]['id'])
        self.assertEqual(tree['otherwise'], integer('0'))
        selection = self.package['policy']['selection']['tree']
        self.assertEqual((selection['kind'], selection['name']['qualified']), ('call', 'exists'))
        predicate = selection['arguments'][0]['right']['predicate']
        self.assertEqual((predicate['kind'], predicate['operator']), ('binary', '='))
        self.assertEqual(predicate['left']['right'],
                         {'kind': 'step', 'axis': 'child', 'test': {'kind': 'kind', 'name': 'text'}})
        acceptance = self.package['policy']['acceptance']['tree']
        for path in [acceptance, acceptance['left']]:
            self.assertEqual(path['right']['kind'], 'filter')
            self.assertEqual(path['right']['predicate'], integer('1'))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ir', type=Path)
    parser.add_argument('--archives', type=Path)
    arguments, remaining = parser.parse_known_args()
    if bool(arguments.ir) != bool(arguments.archives):
        parser.error('--ir and --archives must be supplied together')
    IR_PATH, ARCHIVES = arguments.ir, arguments.archives
    unittest.main(argv=[sys.argv[0], *remaining])
