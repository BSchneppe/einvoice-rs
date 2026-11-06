#!/usr/bin/env python3
"""Independent candidate artifact/accounting checks; no runtime admission API."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
from inventory import Document, load_archives

HERE = Path(__file__).resolve().parents[1]
RAW_SHA256 = '8882e1e0550e0f45db2aa6e55ec28de0cc120b51983bba79f2bb175c5b02bed6'
# Independent expected syntax for the one reviewed declarative source occurrence.
# This is test evidence, never a production expression or source-selection input.
BUYER_TREE = {
    'kind': 'call',
    'name': {'qualified': 'not', 'namespace': 'http://www.w3.org/2005/xpath-functions', 'local': 'not'},
    'arguments': [{'kind': 'step', 'axis': 'child', 'test': {'kind': 'name', 'name': {
        'qualified': 'ram:BuyerTaxRepresentativeTradeParty',
        'namespace': 'urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100',
        'local': 'BuyerTaxRepresentativeTradeParty'}}}],
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return (json.dumps(value, indent=2, ensure_ascii=False) + '\n').encode()


def files(root):
    return {str(p.relative_to(root)): {'sha256': digest(p.read_bytes()), 'size': p.stat().st_size}
            for p in sorted(root.rglob('*')) if p.is_file()}


def check(directory, other, archives):
    loaded, _ = load_archives(archives)
    assert files(directory) == files(other), 'full candidate output differs with reversed archive order'
    raw_bytes = (directory / 'raw/ir.json').read_bytes()
    assert digest(raw_bytes) == RAW_SHA256, 'protected raw IR changed'
    raw = json.loads(raw_bytes)
    candidate = json.loads((directory / 'resolved-candidate.json').read_bytes())
    emitted_manifest = (directory / 'source-resolution.lock.json').read_bytes()
    assert emitted_manifest == (HERE / 'source-resolution.lock.json').read_bytes()
    lock = json.loads(emitted_manifest)
    provenance = json.loads((directory / 'resolved-manifest.json').read_bytes())
    assert candidate['format'] == 'einvoice-resolved-candidate/1'
    assert candidate['raw_ir_sha256'] == RAW_SHA256 == provenance['raw_ir_sha256']
    assert candidate['decision_manifest_sha256'] == digest(emitted_manifest) == provenance['decision_manifest_sha256']
    assert candidate['correspondence_audit'] == 'PendingFullPredicateContextHelperMessagePolicyCorrespondence'
    assert candidate['backend'] == 'Unavailable'
    assert candidate['effective_package']['backend'] == 'Unavailable'
    assert candidate['effective_package']['policy']['execution'] == 'Unavailable'
    actual_files = files(directory)
    assert provenance['files'] == {k: v for k, v in actual_files.items() if k != 'resolved-manifest.json'}
    assert provenance['candidate_sha256'] == actual_files['resolved-candidate.json']['sha256']
    effective_bytes = (json.dumps(candidate['effective_package'], indent=2, ensure_ascii=False) + '\n').encode()
    assert provenance['effective_package_sha256'] == digest(effective_bytes)
    assert provenance['generator'] == json.loads((directory / 'raw/manifest.json').read_bytes())['generator']
    assert len(candidate['resolutions']) == 1
    resolution = candidate['resolutions'][0]
    decision, = lock['decisions']
    assert resolution['decision'] == decision['id']
    assert resolution['authority'] == decision['authority'] and resolution['reason'] == decision['reason']
    assert resolution['lineage'] == lock['nodes'] and resolution['namespaces'] == lock['namespaces']
    docs = {r: Document(r, loaded[r]) for r in lock['resources']}
    for name, g in resolution['lineage'].items():
        doc = docs[g['node']['source']['resource']]
        node = doc.root
        chain = []
        for index, step in enumerate(g['selector']):
            tag = '{' + step['namespace'] + '}' + step['local']
            matches = [n for n in ([doc.root] if index == 0 else node) if n.tag == tag]
            node = matches[step['sibling']]
            assert all(node.attrib[k] == v for k, v in step['identity'].items()), name
            chain.append(node)
        assert len(chain) == len(g['ancestors']) + 1
        for n, origin in zip(chain, [*g['ancestors'], g['node']], strict=True):
            loc = origin['source']
            doc.check_location(n, loc)
            assert digest(doc.data[loc['xml_byte_start']:loc['xml_byte_end']]) == origin['sha256']
            assert set(n.attrib) == set(origin['attributes'])
            for attr, a in origin['attributes'].items():
                loc = a['source']
                doc.check_location(n, loc, attr)
                assert digest(doc.data[loc['xml_byte_start']:loc['xml_byte_end']]) == a['sha256']
                assert digest(n.attrib[attr].encode()) == a['decoded_sha256']
    occurrences = resolution['duplicates']
    assert [o['origin'] for o in occurrences] == [lock['nodes'][key] for key in decision['duplicates']]
    assert resolution['selected'] == lock['nodes'][decision['selected']]
    selected, = [o for o in occurrences if o['origin'] == resolution['selected']]
    assert resolution['effective'] == selected['expression']
    selected_tree = selected['expression']['tree']
    assert selected_tree == BUYER_TREE, 'selected AST differs from independently authored source structure'
    assert digest(canonical(selected_tree)) == decision['effective_ast_sha256'], 'selected AST hash drift'
    assert digest(canonical(resolution['original_site']['test']['tree'])) == decision['original_ast_sha256']
    assert digest(canonical(resolution['context_parameter']['expression']['tree'])) == decision['context_ast_sha256']
    for occurrence in [*occurrences, resolution['context_parameter']]:
        e = occurrence['expression']
        doc = docs[e['source']['resource']]
        n, = [n for n in doc.root.iter() if doc.positions[n][0] == occurrence['origin']['node']['source']['xml_byte_start']]
        doc.check_location(n, e['source'], 'value')
        assert n.attrib['value'] == e['text']
    assert occurrences[1]['expression']['tree'] == resolution['original_site']['test']['tree']
    assert occurrences[0]['expression']['tree'] != occurrences[1]['expression']['tree']
    expected = copy.deepcopy(raw)
    sites = [(r, s) for schema in expected['schemas'] for p in schema['patterns'] for r in p['rules'] for s in r['sites']
             if s['identity'] == resolution['original_site']['identity']]
    rule, site = sites[0]
    assert len(sites) == 1 and site == resolution['original_site']
    assert rule['context'] == resolution['original_context']
    assert rule['context']['tree'] == resolution['context_parameter']['expression']['tree']
    assert raw['schemas'][0]['namespaces'] == resolution['namespaces']
    site['test'] = resolution['effective']
    # Only these original source documents plus the independent release comparison
    # are deliberately retained in the candidate; raw resources stay untouched.
    added = {
        'cen.zip/schematron/EN16931-CII-validation.sch',
        'cen.zip/schematron/abstract/EN16931-CII-syntax.sch',
        'cen.zip/schematron/CII/EN16931-CII-syntax.sch',
        'cen.zip/xslt/EN16931-CII-validation.xslt',
    }
    assert set(candidate['added_resources']) == added
    for resource, metadata in candidate['added_resources'].items():
        assert resource not in expected['resources']
        assert (directory / 'candidate-resources' / resource).read_bytes() == loaded[resource]
        archive, member = resource.split('/', 1)
        pin = lock['resources'][resource]
        expected_metadata = dict(archive=archive, member=member, sha256=pin['sha256'],
                                 size=pin['size'], role=pin['role'],
                                 pin='oracle-member-lock; fixed source-resolution.lock.json')
        assert metadata == expected_metadata, 'added resource metadata differs from fixed source identity'
        assert pin['sha256'] == digest(loaded[resource]) and pin['size'] == len(loaded[resource])
        expected['resources'][resource] = expected_metadata
    entry = lock['nodes']['entry']['node']['source']['resource']
    concrete = lock['nodes']['concrete_pattern']['node']['source']['resource']
    abstract = lock['nodes']['abstract_pattern']['node']['source']['resource']
    edges = [dict(from_=entry, to=abstract, relation='bounded-declarative-include', namespace=None),
             dict(from_=entry, to=concrete, relation='bounded-declarative-include', namespace=None),
             dict(from_=concrete, to=abstract, relation='bounded-is-a-parameter-linkage', namespace='http://purl.oclc.org/dsdl/schematron')]
    edges = [{('from' if k == 'from_' else k): v for k, v in e.items()} for e in edges]
    assert candidate['added_source_graph'] == edges
    expected['source_graph'] += edges
    # The selected subtree adds no construct requirements. Check all remaining
    # package fields, including every other predicate, helper, context and policy.
    assert expected == candidate['effective_package'], 'unexplained materialized package delta'
    for r, metadata in raw['resources'].items():
        assert (directory / 'raw/resources' / r).read_bytes() == loaded.get(r, (HERE.parent / 'oracle/notices/KoSIT-configuration-LICENSE').read_bytes())
        assert metadata['sha256'] == digest((directory / 'raw/resources' / r).read_bytes())
    return dict(format='einvoice-candidate-source-accounting/1', artifact_kind=provenance['artifact_kind'],
                raw_ir_sha256=RAW_SHA256, candidate_sha256=provenance['candidate_sha256'],
                decision_manifest_sha256=digest(emitted_manifest), files=actual_files)


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--one', type=Path, required=True)
    p.add_argument('--two', type=Path, required=True)
    p.add_argument('--archives', type=Path, required=True)
    p.add_argument('--summary', type=Path, required=True)
    a = p.parse_args()
    summary = check(a.one, a.two, a.archives)
    a.summary.write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n')
    print(json.dumps({k: v for k, v in summary.items() if k != 'files'}, sort_keys=True))
