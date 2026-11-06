"""Bounded independent XPath lexical accounting, not a second XPath parser.

Expected counts come only from decoded XML expressions, never Rust tokens/trees.
QName spelling and numeric lexemes are exact; strings decode doubled delimiters.
Whitespace and nested comments disappear. @, .. and implicit child steps normalize
to axes; . remains context. Wildcards and kind tests remain distinct from names,
calls and multiplication. Type occurrence suffixes are not arithmetic operators.
Operator spellings are exact (including the distinct `union` and `|` spellings).
Commas and parentheses are not counted; filter brackets are counted separately.
Binder $names followed by `in` are declarations, all other $names are references.
Group is transparent. Counts do NOT establish association, precedence, predicate
order, grouping, binding identity or execution equivalence. The independent scope
checker and authored Rust structure fixtures remain necessary.
"""
from collections import Counter
import re


NCNAME = r'[^\W\d][\w.-]*'
NAME = re.compile(rf'(?:{NCNAME}:\*|\*:{NCNAME}|{NCNAME}(?::{NCNAME})?)')
NUMBER = re.compile(r'(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?')
WORD_OPERATORS = set('or and div idiv mod to eq ne lt le gt ge is union intersect except'.split())
KIND_TESTS = {'node', 'text', 'comment', 'processing-instruction'}


def tokens(text):
    """Small lexical scanner with no AST, precedence table or Rust dependency."""
    out = []
    i = 0
    while i < len(text):
        if text[i].isspace():
            i += 1
        elif text.startswith('(:', i):
            depth = 1
            i += 2
            while depth and i < len(text):
                if text.startswith('(:', i):
                    depth += 1
                    i += 2
                elif text.startswith(':)', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            assert not depth, 'unterminated XPath comment'
        elif text[i] in "\"'":
            quote = text[i]
            value = []
            i += 1
            while True:
                assert i < len(text), 'unterminated XPath string'
                if text[i] == quote:
                    i += 1
                    if i == len(text) or text[i] != quote:
                        break
                value.append(text[i])
                i += 1
            out.append(('string', ''.join(value)))
        else:
            number = NUMBER.match(text, i)
            name = NAME.match(text, i)
            if number:
                out.append(('number', number[0]))
                i = number.end()
            elif name:
                out.append(('name', name[0]))
                i = name.end()
            else:
                symbol = next((s for s in ('//', '::', '..', '!=', '<=', '>=', '<<', '>>')
                               if text.startswith(s, i)), text[i])
                assert symbol in {'//', '::', '..', '!=', '<=', '>=', '<<', '>>',
                                  '/', '.', '@', '$', '*', '+', '-', '=', '<', '>',
                                  '|', '(', ')', '[', ']', ',', '?'}, (i, text[i:i + 20])
                out.append(('symbol', symbol))
                i += len(symbol)
    return out


def numeric_kind(lexeme):
    return 'double' if 'e' in lexeme.lower() else 'decimal' if '.' in lexeme else 'integer'


def source_occurrences(text):
    ts = tokens(text)
    counts = Counter()
    operand = True
    axis = 'child'
    i = 0
    while i < len(ts):
        kind, value = ts[i]
        following = ts[i + 1][1] if i + 1 < len(ts) else None
        if kind == 'string':
            counts['string', value] += 1
            operand = False
        elif kind == 'number':
            counts['number-' + numeric_kind(value), value] += 1
            operand = False
        elif value == '$':
            assert i + 1 < len(ts) and ts[i + 1][0] == 'name', 'variable QName missing'
            declaration = i + 2 < len(ts) and ts[i + 2] == ('name', 'in')
            counts['declaration' if declaration else 'reference', following] += 1
            i += 2 if declaration else 1
            operand = declaration
        elif kind == 'name' and following == '::':
            axis = value
            operand = True
            i += 1
        elif kind == 'name' and value in {'for', 'some', 'every'} and following == '$':
            counts['operator', value] += 1
            operand = True
        elif kind == 'name' and value in {'cast', 'castable', 'instance', 'treat'} and not operand:
            connector = 'of' if value == 'instance' else 'as'
            assert ts[i + 1] == ('name', connector) and ts[i + 2][0] == 'name'
            counts['operator', value] += 1
            i += 2
            suffix = ts[i + 1][1] if i + 1 < len(ts) and ts[i + 1][1] in {'?', '*', '+'} else ''
            counts['type', ts[i][1] + suffix] += 1
            i += bool(suffix)
            operand = False
        elif kind == 'name' and value in {'return', 'satisfies', 'then', 'else'} and not operand:
            operand = True
        elif kind == 'name' and value in WORD_OPERATORS and not operand:
            counts['operator', value] += 1
            operand = True
        elif kind == 'name' and following == '(':
            if value == 'if':
                counts['control', 'if'] += 1
            elif value in KIND_TESTS:
                counts['kind-test', value] += 1
                counts['axis', axis] += 1
                axis = 'child'
            else:
                counts['function', value] += 1
            operand = True
        elif kind == 'name' or (value == '*' and operand):
            counts['node', value] += 1
            counts['axis', axis] += 1
            axis = 'child'
            operand = False
        elif value == '@':
            axis = 'attribute'
            operand = True
        elif value == '..':
            counts['axis', 'parent'] += 1
            counts['kind-test', 'node'] += 1
            operand = False
        elif value == '.':
            counts['context', '.'] += 1
            operand = False
        elif value in {'/', '//'}:
            if operand:
                counts['root', '/'] += 1
            # A bare / is represented by Root alone, rather than a binary path.
            if value == '//' or following not in {None, ')', ']', ','}:
                counts['operator', value] += 1
            operand = True
        elif value == '[':
            counts['filter', '[]'] += 1
            operand = True
        elif value in {'(', ','}:
            operand = True
        elif value in {')', ']'}:
            operand = False
        elif value in {'+', '-', '*', '=', '!=', '<', '>', '<=', '>=', '<<', '>>', '|'}:
            counts['operator', value] += 1
            operand = True
        else:
            raise AssertionError(('unaccounted XPath token', ts[i]))
        i += 1
    return counts


def tree_occurrences(tree):
    counts = Counter()

    def visit(node):
        kind = node['kind']
        if kind == 'number':
            counts['number-' + node['lexical_kind'], node['lexeme']] += 1
        elif kind == 'string':
            counts['string', node['value']] += 1
        elif kind == 'variable':
            counts['reference', node['name']['qualified']] += 1
        elif kind in {'root', 'context'}:
            counts[kind, '/' if kind == 'root' else '.'] += 1
        elif kind == 'step':
            test = node['test']
            counts['axis', node['axis']] += 1
            if test['kind'] == 'kind':
                counts['kind-test', test['name']] += 1
            else:
                spelling = {'name': lambda: test['name']['qualified'],
                            'any': lambda: '*',
                            'namespace': lambda: test['prefix'] + ':*',
                            'local': lambda: '*:' + test['local']}[test['kind']]()
                counts['node', spelling] += 1
        elif kind == 'call':
            counts['function', node['name']['qualified']] += 1
            for argument in node['arguments']:
                visit(argument)
        elif kind in {'unary', 'binary', 'type_op', 'bind'}:
            counts['operator', node['operator']] += 1
            if kind == 'binary':
                visit(node['left'])
                visit(node['right'])
            elif kind == 'bind':
                for binding in node['bindings']:
                    counts['declaration', binding['name']['qualified']] += 1
                    visit(binding['value'])
                visit(node['result'])
            else:
                visit(node['operand'])
                if kind == 'type_op':
                    sequence_type = node['sequence_type']
                    counts['type', sequence_type['name']['qualified'] + (sequence_type['occurrence'] or '')] += 1
        elif kind == 'filter':
            counts['filter', '[]'] += 1
            visit(node['base'])
            visit(node['predicate'])
        elif kind == 'group':
            visit(node['operand'])
        elif kind == 'sequence':
            for item in node['items']:
                visit(item)
        elif kind == 'if':
            counts['control', 'if'] += 1
            for field in ('condition', 'then', 'otherwise'):
                visit(node[field])
        else:
            raise AssertionError(('unaccounted AST kind', kind))

    visit(tree)
    return counts


def check_occurrences(text, tree, location=None):
    expected = source_occurrences(text)
    actual = tree_occurrences(tree)
    assert expected == actual, (
        f'XPath occurrence mismatch at {location}: '
        f'missing={dict(expected - actual)!r}; extra={dict(actual - expected)!r}')
