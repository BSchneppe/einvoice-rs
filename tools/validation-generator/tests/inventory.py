#!/usr/bin/env python3
"""Independent stdlib XML accounting. Reads upstream ZIP bytes, never Rust-derived expectations.

Run: python3 tests/inventory.py --ir OUTPUT/ir.json --archives ARCHIVE_DIRECTORY
Archive directory contains the five authoritative names. --other OUTPUT checks every output byte.
"""
import argparse
import hashlib
import json
import re
import xml.etree.ElementTree as ET
import xml.parsers.expat as expat
import zipfile
from pathlib import Path

from occurrences import check_occurrences

HERE = Path(__file__).resolve().parents[1]
SCH = '{http://purl.oclc.org/dsdl/schematron}'
XSL = '{http://www.w3.org/1999/XSL/Transform}'
DEFAULTS = {'xs': 'http://www.w3.org/2001/XMLSchema',
            'fn': 'http://www.w3.org/2005/xpath-functions',
            'xml': 'http://www.w3.org/XML/1998/namespace'}
CEN = 'cen.zip/schematron/preprocessed/EN16931-CII-validation-preprocessed.sch'
XR = 'xr.zip/schematron/cii/XRechnung-CII-validation.sch'
COMMON = 'xr.zip/schematron/common.sch'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load_archives(directory):
    lock = json.loads((HERE.parent / 'oracle/artifacts.lock.json').read_bytes())
    loaded = {}
    pins = {}
    for pin in lock['artifacts']:
        if pin['file'] not in {'cen.zip', 'xr.zip', 'config.zip', 'validator.jar', 'cen-source.zip'}:
            continue
        data = (directory / pin['file']).read_bytes()
        assert digest(data) == pin['sha256'], pin['file']
        pins[pin['file']] = pin
        with zipfile.ZipFile(directory / pin['file']) as archive:
            members = [i.filename for i in archive.infolist() if not i.is_dir()]
            assert len(members) == len(set(members)), 'duplicate ZIP member'
            if 'members' in pin:
                assert set(members) == set(pin['members']), 'exact member inventory'
            for name in members:
                value = archive.read(name)
                if 'members' in pin:
                    assert digest(value) == pin['members'][name], name
                loaded[pin['file'] + '/' + name] = value
    return loaded, pins


def opening_end(data, start):
    quote = None
    for index in range(start, len(data)):
        byte = data[index]
        if quote:
            if byte == quote:
                quote = None
        elif byte in (34, 39):
            quote = byte
        elif byte == 62:
            return index
    raise AssertionError('unterminated XML opening tag')


class Document:
    def __init__(self, resource, data):
        self.resource = resource
        self.data = data
        self.root = ET.fromstring(data)
        # Expat is independent of roxmltree, including its source position accounting.
        starts = []
        parser = expat.ParserCreate(namespace_separator='}')
        parser.StartElementHandler = lambda name, attrs: starts.append(
            (parser.CurrentByteIndex, parser.CurrentLineNumber, parser.CurrentColumnNumber + 1))
        parser.Parse(data, True)
        self.positions = dict(zip(self.root.iter(), starts, strict=True))

    def identity(self, node):
        return f'{self.resource}@{self.positions[node][0]}'

    def check_location(self, node, location, attr=None):
        start, line, column = self.positions[node]
        assert location['resource'] == self.resource
        assert location['attribute'] == attr
        if attr is None:
            assert (location['xml_byte_start'], location['line'], location['column']) == (start, line, column)
        else:
            # Locate the raw XML attribute, independently from the decoded XPath value.
            opening = self.data[start:opening_end(self.data, start) + 1]
            pattern = rb'(?<![\w:-])' + attr.encode() + rb'\s*=\s*(?:"[^"]*"|\x27[^\x27]*\x27)'
            match = re.search(pattern, opening)
            assert match is not None
            expected_start = start + match.start()
            assert location['xml_byte_start'] == expected_start, (self.resource, attr)
            assert location['xml_byte_end'] == start + match.end()
            prefix = self.data[:expected_start].decode('utf-8')
            assert location['line'] == prefix.count('\n') + 1
            assert location['column'] == len(prefix.rsplit('\n', 1)[-1]) + 1
        assert location['xml_byte_end'] <= len(self.data)


def typename(t):
    return t['name']['qualified'] + (t['occurrence'] or '') if t else None


def ir_binding(b, parameter=False):
    attrs = {'name': b['name']['qualified']}
    if b['sequence_type'] is not None:
        attrs['as'] = typename(b['sequence_type'])
    value = b['value']
    children = []
    if value['kind'] == 'select':
        attrs['select'] = value['expression']['text']
    elif value['kind'] == 'content':
        children = [ir_instruction(i) for i in value['body']]
    return {'tag': 'param' if parameter else 'variable', 'attrs': attrs, 'children': children}


def ir_instruction(i):
    if i['kind'] == 'text':
        return {'text': i['text']}
    if i['kind'] == 'variable':
        return ir_binding(i['binding'])
    if i['kind'] == 'sequence':
        return {'tag': 'sequence', 'attrs': {'select': i['expression']['text']}, 'children': []}
    if i['kind'] == 'choose':
        branches = [{'tag': 'when', 'attrs': {'test': b['test']['text']},
                     'children': [ir_instruction(c) for c in b['body']]} for b in i['branches']]
        if i['otherwise']:
            branches.append({'tag': 'otherwise', 'attrs': {},
                             'children': [ir_instruction(c) for c in i['otherwise']]})
        return {'tag': 'choose', 'attrs': {}, 'children': branches}
    raise AssertionError(i)


def xml_instruction(node):
    children = []
    if node.text and node.text.strip():
        children.append({'text': node.text})
    for c in node:
        children.append(xml_instruction(c))
        if c.tail and c.tail.strip():
            children.append({'text': c.tail})
    return {'tag': node.tag.removeprefix(XSL), 'attrs': dict(node.attrib), 'children': children}


def check_binding(doc, node, binding, owner, scope):
    assert binding['id'] == doc.identity(node)
    doc.check_location(node, binding['source'])
    assert binding['declared_owner'] == owner
    assert binding['effective_scope'] == scope
    assert binding['initialization_context'] == ('document-root' if scope == 'xslt2_schema_global' else 'lexical-dynamic-context')
    assert binding['name']['qualified'] == node.attrib['name']
    if node.tag == SCH + 'let':
        assert binding['value']['expression']['text'] == node.attrib['value']
    else:
        assert ir_binding(binding, node.tag == XSL + 'param') == xml_instruction(node)


def xml_message(node):
    out = []
    if node.text:
        out.append({'kind': 'text', 'text': node.text})
    for child in node:
        if child.tag == SCH + 'name':
            out.append({'kind': 'name'})
        else:
            assert child.tag == SCH + 'value-of'
            out.append({'kind': 'value_of', 'text': child.attrib['select']})
        if child.tail:
            out.append({'kind': 'text', 'text': child.tail})
    return out


def ir_message(parts):
    return [({'kind': 'text', 'text': p['text']} if p['kind'] == 'text' else
             {'kind': 'name'} if p['kind'] == 'name' else
             {'kind': 'value_of', 'text': p['expression']['text']}) for p in parts]


def all_expressions(value):
    if isinstance(value, dict):
        if set(value) == {'text', 'tree', 'source'}:
            yield value
        for child in value.values():
            yield from all_expressions(child)
    elif isinstance(value, list):
        for child in value:
            yield from all_expressions(child)


def walk_bindings(body):
    for instruction in body:
        if instruction['kind'] == 'variable':
            yield instruction['binding']
            if instruction['binding']['value']['kind'] == 'content':
                yield from walk_bindings(instruction['binding']['value']['body'])
        elif instruction['kind'] == 'choose':
            for branch in instruction['branches']:
                yield from walk_bindings(branch['body'])
            yield from walk_bindings(instruction['otherwise'])


def check_resolution(tree, scope, namespaces):
    kind = tree['kind']
    if kind == 'variable':
        name = tree['name']
        qualified = name['qualified']
        prefix, _, local = qualified.rpartition(':')
        expected_ns = namespaces[prefix] if prefix else ''
        assert (name['namespace'], name['local']) == (expected_ns, local)
        assert tree['binding'] == scope['{' + expected_ns + '}' + local], (qualified, tree['binding'])
        return
    if kind == 'bind':
        inner = dict(scope)
        for binding in tree['bindings']:
            check_resolution(binding['value'], inner, namespaces)
            name = binding['name']
            prefix, _, local = name['qualified'].rpartition(':')
            ns = namespaces[prefix] if prefix else ''
            assert (name['namespace'], name['local']) == (ns, local)
            assert '/expr:' in binding['id']
            inner['{' + ns + '}' + local] = binding['id']
        check_resolution(tree['result'], inner, namespaces)
        return
    if kind == 'call':
        name = tree['name']
        prefix, _, local = name['qualified'].rpartition(':')
        assert (name['namespace'], name['local']) == (namespaces[prefix] if prefix else DEFAULTS['fn'], local)
    if kind == 'step' and tree['test']['kind'] == 'name':
        name = tree['test']['name']
        prefix, _, local = name['qualified'].rpartition(':')
        assert (name['namespace'], name['local']) == (namespaces[prefix] if prefix else '', local)
    for value in tree.values():
        if isinstance(value, dict) and 'kind' in value and value is not tree.get('test'):
            check_resolution(value, scope, namespaces)
        elif isinstance(value, list):
            for child in value:
                if isinstance(child, dict) and 'kind' in child:
                    check_resolution(child, scope, namespaces)


def check_body_resolution(body, scope, namespaces):
    inner = dict(scope)
    for instruction in body:
        if instruction['kind'] == 'variable':
            binding = instruction['binding']
            check_value_resolution(binding['value'], inner, namespaces)
            name = binding['name']
            inner['{' + name['namespace'] + '}' + name['local']] = binding['id']
        elif instruction['kind'] == 'sequence':
            check_resolution(instruction['expression']['tree'], inner, namespaces)
        elif instruction['kind'] == 'choose':
            for branch in instruction['branches']:
                check_resolution(branch['test']['tree'], inner, namespaces)
                check_body_resolution(branch['body'], inner, namespaces)
            check_body_resolution(instruction['otherwise'], inner, namespaces)


def check_value_resolution(value, scope, namespaces):
    if value['kind'] == 'select':
        check_resolution(value['expression']['tree'], scope, namespaces)
    elif value['kind'] == 'content':
        check_body_resolution(value['body'], scope, namespaces)


def check_schema(docs, schema, entry, counts):
    doc = docs[entry]
    root = doc.root
    doc.check_location(root, schema['source'])
    namespaces = dict(DEFAULTS)
    declarations = root.findall(SCH + 'ns')
    namespaces.update((n.attrib['prefix'], n.attrib['uri']) for n in declarations)
    assert schema['namespaces'] == namespaces
    assert [(d['prefix'], d['uri']) for d in schema['namespace_declarations']] == [(d.attrib['prefix'], d.attrib['uri']) for d in declarations]
    assert schema['xpath_element_default_namespace'] == ''
    assert schema['xpath_function_default_namespace'] == DEFAULTS['fn']
    assert schema['title'] == root.findtext(SCH + 'title')
    assert schema['selection'] == '#ALL'
    assert schema['declared_default_phase'] == root.attrib.get('defaultPhase')
    assert schema['dispatch'] == 'DocumentTraversalFirstMatchingRulePerPatternInSourceOrder'
    phases = root.findall(SCH + 'phase')
    assert [(p['id'], p['active_patterns']) for p in schema['phases']] == [(p.attrib['id'], [a.attrib['pattern'] for a in p]) for p in phases]
    for source_phase, ir_phase in zip(phases, schema['phases'], strict=True):
        doc.check_location(source_phase, ir_phase['source'])
    globals_xml = root.findall(SCH + 'let')
    assert len(globals_xml) == len(schema['globals'])
    for node, binding in zip(globals_xml, schema['globals'], strict=True):
        check_binding(doc, node, binding, 'schema', 'xslt2_schema_global')
    expected_patterns = []
    includes = []
    for n in root:
        if n.tag == SCH + 'pattern':
            expected_patterns.append((doc, n))
        elif n.tag == SCH + 'include':
            # Exact known edge, no filesystem or URI resolution during independent walk.
            assert entry == XR and n.attrib == {'href': '../common.sch'}
            expected_patterns.append((docs[COMMON], docs[COMMON].root))
            includes.append((n, COMMON))
    global_scope = {}
    for global_doc, nodes in [(doc, globals_xml)] + [(d, p.findall(SCH + 'let')) for d, p in expected_patterns]:
        for binding in nodes:
            prefix, _, local = binding.attrib['name'].rpartition(':')
            global_scope['{' + (namespaces[prefix] if prefix else '') + '}' + local] = global_doc.identity(binding)
    for binding in schema['globals']:
        check_value_resolution(binding['value'], global_scope, namespaces)
    assert len(includes) == len(schema['includes'])
    for (n, to), edge in zip(includes, schema['includes'], strict=True):
        doc.check_location(n, edge['from'])
        assert edge['to'] == to
    assert len(expected_patterns) == len(schema['patterns'])
    overrides = {'BR-CL-23': 'warning', 'BR-CL-21': 'warning', **dict.fromkeys(['CII-SR-452', 'CII-SR-453', 'CII-SR-454', 'CII-SR-465', 'CII-SR-466'], 'error'), 'CII-SR-475': 'information', 'CII-SR-476': 'information'}
    for (pdoc, p), pattern in zip(expected_patterns, schema['patterns'], strict=True):
        assert pattern['id'] == p.attrib['id']
        pdoc.check_location(p, pattern['source'])
        assert pattern['active'] is True
        assert pattern['activation_reason'] == 'selected; predicates determine contextual applicability'
        lets = p.findall(SCH + 'let')
        assert len(lets) == len(pattern['bindings'])
        for node, binding in zip(lets, pattern['bindings'], strict=True):
            check_binding(pdoc, node, binding, 'pattern:' + p.attrib['id'], 'xslt2_schema_global')
            check_value_resolution(binding['value'], global_scope, namespaces)
        rules = p.findall(SCH + 'rule')
        assert len(rules) == len(pattern['rules'])
        counts['rules'] += len(rules)
        for r, rule in zip(rules, pattern['rules'], strict=True):
            pdoc.check_location(r, rule['source'])
            assert rule['identity'] == pdoc.identity(r)
            assert rule['flag'] == r.attrib.get('flag')
            assert rule['context']['text'] == r.attrib['context']
            check_resolution(rule['context']['tree'], global_scope, namespaces)
            rule_scope = dict(global_scope)
            locals_xml = r.findall(SCH + 'let')
            assert len(locals_xml) == len(rule['bindings'])
            for node, binding in zip(locals_xml, rule['bindings'], strict=True):
                check_binding(pdoc, node, binding, pdoc.identity(r), 'rule_local')
                check_value_resolution(binding['value'], rule_scope, namespaces)
                prefix, _, local = node.attrib['name'].rpartition(':')
                rule_scope['{' + (namespaces[prefix] if prefix else '') + '}' + local] = pdoc.identity(node)
            sites = r.findall(SCH + 'assert')
            assert len(sites) == len(rule['sites'])
            counts['sites'] += len(sites)
            for a, site in zip(sites, rule['sites'], strict=True):
                pdoc.check_location(a, site['source'])
                assert site['identity'] == pdoc.identity(a)
                assert (site['id'], site['raw_flag'], site['test']['text']) == (a.attrib['id'], a.attrib['flag'], a.attrib['test'])
                base = {'fatal': 'error', 'warning': 'warning', 'information': 'information'}[a.attrib['flag']]
                assert site['base_severity'] == base
                assert site['effective_severity'] == overrides.get(a.attrib['id'], base)
                assert ir_message(site['message']) == xml_message(a)
                check_resolution(site['test']['tree'], rule_scope, namespaces)
                for part in site['message']:
                    if part['kind'] == 'value_of':
                        check_resolution(part['expression']['tree'], rule_scope, namespaces)
                name_nodes = a.findall(SCH + 'name')
                name_parts = [part for part in site['message'] if part['kind'] == 'name']
                for n, part in zip(name_nodes, name_parts, strict=True):
                    pdoc.check_location(n, part['source'])
    helpers = root.findall(XSL + 'function')
    counts['helpers'] += len(helpers)
    assert len(helpers) == len(schema['helpers'])
    for h, helper in zip(helpers, schema['helpers'], strict=True):
        doc.check_location(h, helper['source'])
        assert helper['name']['qualified'] == h.attrib['name']
        assert helper['name']['namespace'] == namespaces[h.attrib['name'].split(':')[0]]
        assert typename(helper['result_type']) == h.attrib['as']
        params = h.findall(XSL + 'param')
        assert len(params) == len(helper['parameters'])
        for p, binding in zip(params, helper['parameters'], strict=True):
            check_binding(doc, p, binding, '{' + helper['name']['namespace'] + '}' + helper['name']['local'], 'helper_parameter')
        helper_scope = dict(global_scope)
        for param, binding in zip(params, helper['parameters'], strict=True):
            prefix, _, local = param.attrib['name'].rpartition(':')
            helper_scope['{' + (namespaces[prefix] if prefix else '') + '}' + local] = doc.identity(param)
        check_body_resolution(helper['body'], helper_scope, namespaces)
        expected_body = [xml_instruction(n) for n in h if n.tag != XSL + 'param']
        assert [ir_instruction(n) for n in helper['body']] == expected_body
        variables = [n for n in h.iter(XSL + 'variable')]
        bindings = list(walk_bindings(helper['body']))
        assert len(variables) == len(bindings)
        for n, b in zip(variables, bindings, strict=True):
            check_binding(doc, n, b, '{' + helper['name']['namespace'] + '}' + helper['name']['local'], 'helper_local')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--ir', type=Path, required=True)
    parser.add_argument('--archives', type=Path, required=True)
    parser.add_argument('--other', type=Path)
    parser.add_argument('--summary', type=Path)
    args = parser.parse_args()
    resources, pins = load_archives(args.archives)
    package = json.loads(args.ir.read_bytes())
    docs = {key: Document(key, resources[key]) for key in [CEN, XR, COMMON]}
    counts = dict(sites=0, rules=0, helpers=0)
    assert len(package['schemas']) == 2
    for entry, schema in zip([CEN, XR], package['schemas'], strict=True):
        check_schema(docs, schema, entry, counts)
    assert counts == dict(sites=880, rules=156, helpers=14), counts
    # Exact accounting of EVERY expression-bearing attribute, not only assertion tests.
    expected = {}
    for resource, doc in docs.items():
        for node in doc.root.iter():
            for attr in ['context', 'test', 'value', 'select']:
                if attr in node.attrib:
                    expected[(resource, doc.positions[node][0], attr)] = (node, node.attrib[attr])
    actual = list(all_expressions(package['schemas']))
    assert len(expected) == len(actual) == 1108
    seen = set()
    for expression in actual:
        loc = expression['source']
        doc = docs[loc['resource']]
        candidates = [(k, v) for k, v in expected.items() if k[0] == loc['resource'] and k[2] == loc['attribute'] and k not in seen and v[1] == expression['text'] and k[1] <= loc['xml_byte_start'] <= opening_end(doc.data, k[1])]
        assert len(candidates) == 1, loc
        key, (node, text) = candidates[0]
        seen.add(key)
        doc.check_location(node, loc, key[2])
        assert expression['text'] == text
        assert isinstance(expression['tree'], dict) and 'kind' in expression['tree']
        check_occurrences(text, expression['tree'], loc)
    assert seen == set(expected)
    assert package['backend'] == 'Unavailable'
    assert package['policy']['execution'] == 'Unavailable'
    # Byte-identical retained resources and original license identity.
    manifest = json.loads((args.ir.parent / 'manifest.json').read_bytes())
    assert manifest['ir_sha256'] == digest(args.ir.read_bytes())
    assert manifest['artifact_kind'] == 'non-executable-inspection; cannot install or run'
    for key, r in package['resources'].items():
        retained = (args.ir.parent / 'resources' / key).read_bytes()
        assert digest(retained) == r['sha256'] and len(retained) == r['size']
        if key in resources:
            assert retained == resources[key]
    assert len([r for r in package['resources'].values() if r['role'] == 'cii_schema']) == 4
    imports = [e for e in package['source_graph'] if e['relation'] == 'xsd-import' and e['from'].startswith('config.zip/resources/cii/')]
    assert len(imports) == 6
    # All retained XSD/report dependency edges checked against independent XML imports.
    for key, r in package['resources'].items():
        if r['role'] not in {'cii_schema', 'report_schema', 'opaque_report'}:
            continue
        root = ET.fromstring(resources[key])
        for n in root:
            if n.tag in {'{http://www.w3.org/2001/XMLSchema}import', XSL + 'import', XSL + 'include'}:
                expected_href = n.attrib.get('schemaLocation', n.attrib.get('href'))
                if expected_href:
                    import posixpath
                    target = posixpath.normpath(posixpath.join(posixpath.dirname(key), expected_href))
                else:
                    assert key == 'config.zip/resources/xsd/report.xsd'
                    target = 'config.zip/resources/xsd/scenarios.xsd'
                assert any(e['from'] == key and e['to'] == target and e['namespace'] == n.attrib.get('namespace') for e in package['source_graph'])
    # Policy selector text and overrides come directly from the pinned configuration.
    cfgdoc = Document('config.zip/scenarios.xml', resources['config.zip/scenarios.xml'])
    ns = '{http://www.xoev.de/de/validator/framework/1/scenarios}'
    scenario = next(n for n in cfgdoc.root.findall(ns + 'scenario') if n.findtext(ns + 'name') == 'EN16931 XRechnung (CII)')
    for local, field in [('match', 'selection'), ('acceptMatch', 'acceptance')]:
        node = scenario.find(ns + local)
        assert package['policy'][field]['text'] == node.text
        cfgdoc.check_location(node, package['policy'][field]['source'])
        check_occurrences(node.text, package['policy'][field]['tree'], package['policy'][field]['source'])
    custom = {n.text: n.attrib['level'] for n in scenario.find(ns + 'createReport').findall(ns + 'customLevel')}
    assert next(c['values'] for c in package['policy']['clauses'] if c['kind'] == 'overrides') == custom
    for clause in package['policy']['clauses']:
        assert clause['evidence']
        for evidence in clause['evidence']:
            assert evidence['sha256'] == digest(resources[evidence['resource']])
            assert evidence['locator']
    for archive, pin in pins.items():
        recorded = manifest['archives'][archive]
        assert recorded['sha256'] == pin['sha256'] and recorded['version'] == pin['version']
        expected_members = {key[len(archive) + 1:]: {'size': len(value), 'sha256': digest(value)} for key, value in resources.items() if key.startswith(archive + '/')}
        assert recorded['verified_regular_members'] == expected_members
    if args.other:
        def files(root):
            return {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob('*') if p.is_file()}
        assert files(args.ir.parent) == files(args.other), 'two-run byte determinism'
    result = {**counts, 'expressions': len(actual), 'ir_sha256': manifest['ir_sha256'], 'resources': len(package['resources']), 'determinism': bool(args.other)}
    if args.summary:
        file_hashes = {p.relative_to(args.ir.parent).as_posix(): {'sha256': digest(p.read_bytes()), 'size': p.stat().st_size} for p in sorted(args.ir.parent.rglob('*')) if p.is_file()}
        summary = {'format': 'einvoice-frontend-source-accounting/1', 'artifact_kind': manifest['artifact_kind'], 'target': package['target'], 'source_package': package['source_package'], 'ir_format': package['format'], **result, 'files': file_hashes}
        args.summary.write_text(json.dumps(summary, sort_keys=True, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
