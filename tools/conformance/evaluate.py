#!/usr/bin/env python3
"""Offline frozen-evidence consumer. Never executes a validator or blesses labels."""
import argparse
from collections import Counter
from dataclasses import dataclass
from functools import lru_cache
import hashlib
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import xml.etree.ElementTree as ET
import zipfile

from jsonschema import Draft202012Validator

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BASE = '87e0a0a0c7095412faf23efad7aa8f0710328d03'
TARGET = 'xrechnung-3.0.2-cii-d16b-kosit-2026-08-31'
WIRE_TARGET = 'xrechnung-3.0.2-cii-2026-08-31'
STAGES = ('xml_syntax', 'target_selection', 'xsd', 'cen_rules', 'xrechnung_rules', 'acceptance')
PROJECTION = {'xml_syntax': 'val-xml', 'xsd': 'val-xsd', 'cen_rules': 'val-sch.1', 'xrechnung_rules': 'val-sch.2'}
THEMES = {'b2b_service_maintenance': 2500, 'b2b_hardware_delivery': 2500,
          'municipal_epc_milestone': 1500, 'utility_energy_feedin': 1500,
          'reverse_charge_subcontractor': 1000, 'credit_note_correction': 1000}
# Independent required inventory: removing an entry from the lock cannot reduce verification.
FROZEN = '''.github/workflows/oracle.yml
tools/oracle/.gitattributes
tools/oracle/.gitignore
tools/oracle/README.md
tools/oracle/artifacts.lock.json
tools/oracle/corpus/initial.jsonl
tools/oracle/corpus/sinabis.jsonl
tools/oracle/corpus/split.json
tools/oracle/java/Oracle.java
tools/oracle/java/ScopedOracle.java
tools/oracle/java/XPathProbe.java
tools/oracle/notices/CEN-LICENSE.txt
tools/oracle/notices/KoSIT-configuration-LICENSE
tools/oracle/notices/KoSIT-jar-NOTICE
tools/oracle/notices/KoSIT-validator-LICENSE
tools/oracle/notices/Sinabis-dataset-card.md
tools/oracle/notices/XRechnung-LICENSE
tools/oracle/oracle.py
tools/oracle/outcome.schema.json
tools/oracle/probes.py
tools/oracle/probes/xpath/Cargo.lock
tools/oracle/probes/xpath/Cargo.toml
tools/oracle/probes/xpath/cases.json
tools/oracle/probes/xpath/src/main.rs
tools/oracle/probes/xsd/Cargo.lock
tools/oracle/probes/xsd/Cargo.toml
tools/oracle/probes/xsd/src/main.rs
tools/oracle/requirements-test.txt
tools/oracle/requirements.txt
tools/oracle/results/benchmark-smoke.json
tools/oracle/results/initial.jsonl
tools/oracle/results/initial.jsonl.metadata.json
tools/oracle/results/scoped.jsonl
tools/oracle/results/sinabis.jsonl
tools/oracle/results/sinabis.jsonl.metadata.json
tools/oracle/results/xpath.json
tools/oracle/results/xsd-probe.json
tools/oracle/tests/test_oracle.py
tools/oracle/workload.json'''.splitlines()
POLICY_FILES = ('evaluate.py', 'evidence.lock.json', 'native-record.schema.json',
                'run.schema.json', 'scoped.schema.json', 'CONTRACT.md', 'README.md',
                'requirements.txt', 'tests/test_evaluate.py')


class EvidenceError(ValueError):
    """Only constant codes: never disclose untrusted or held-out evidence in errors."""


def require(condition, code):
    if not condition:
        raise EvidenceError(code)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False).encode('utf-8')


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate_json_key')
        result[key] = value
    return result


def _number(value):
    number = float(value)
    require(math.isfinite(number), 'nonfinite_number')
    return number


def _constant(_):
    raise EvidenceError('nonfinite_number')


def _strings(value):
    if isinstance(value, str):
        require(not any(0xD800 <= ord(c) <= 0xDFFF for c in value), 'invalid_unicode')
    elif isinstance(value, dict):
        for key, item in value.items():
            _strings(key)
            _strings(item)
    elif isinstance(value, list):
        for item in value:
            _strings(item)


def decode(data):
    try:
        if isinstance(data, bytes):
            data = data.decode('utf-8')
        value = json.loads(data, object_pairs_hook=_pairs, parse_float=_number, parse_constant=_constant)
        _strings(value)
        return value
    except (UnicodeError, ValueError, RecursionError) as error:
        if isinstance(error, EvidenceError):
            raise
        raise EvidenceError('malformed_json') from None


def read_json(path):
    return decode(Path(path).read_bytes())


def read_jsonl(path):
    return decode_jsonl(Path(path).read_bytes())


def decode_jsonl(data):
    require(data and data.endswith(b'\n'), 'incomplete_jsonl')
    lines = data.split(b'\n')[:-1]
    require(all(line.strip() for line in lines), 'blank_jsonl_record')
    rows = [decode(line) for line in lines]
    unique(rows)
    return rows


def unique(rows):
    seen = set()
    for row in rows:
        require(isinstance(row, dict) and isinstance(row.get('id'), str) and row['id'], 'invalid_record_identity')
        require(row['id'] not in seen, 'duplicate_record_id')
        seen.add(row['id'])


def confined(root, name):
    require(isinstance(name, str) and name and '\\' not in name, 'unsafe_path')
    parts = PurePosixPath(name)
    require(not parts.is_absolute() and all(p not in ('', '.', '..') for p in name.split('/')), 'unsafe_path')
    root = Path(root).resolve()
    result = root.joinpath(*parts.parts)
    require(result.resolve().is_relative_to(root), 'unsafe_path')
    return result


@lru_cache(maxsize=8)
def schema_validator(path):
    schema = read_json(path)
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


def validate_schema(value, path):
    validator = schema_validator(path)
    # Never format jsonschema's exception: it embeds untrusted instance values.
    require(next(validator.iter_errors(value), None) is None, 'schema_contract')


# XML 1.0 NCName, rather than prefix/local-name heuristics or splitting on '/'.
_START = r'A-Z_a-z\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u02FF\u0370-\u037D\u037F-\u1FFF\u200C-\u200D\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD\U00010000-\U000EFFFF'
_REST = _START + r'\-.0-9\u00B7\u0300-\u036F\u203F-\u2040'
NAME = re.compile('[' + _START + '][' + _REST + ']*')
PI_NAME = re.compile('[:' + _START + '][:' + _REST + ']*')


def location(path):
    """Return immutable (kind, namespace, local/PI target, occurrence) steps.

    Root is (). Only element steps may have children. Index is same expanded-name
    sibling occurrence, not child ordinal. Attributes have no index.
    """
    require(isinstance(path, str) and path.startswith('/'), 'location_syntax')
    if path == '/':
        return ()
    i, steps = 0, []
    while i < len(path):
        require(path[i] == '/' and (not steps or steps[-1][0] == 'element'), 'location_syntax')
        i += 1
        attribute = path.startswith('@', i)
        if attribute:
            i += 1
        namespace, name = '', ''
        if path.startswith('Q{', i):
            end = path.find('}', i + 2)
            require(end >= 0 and '{' not in path[i+2:end], 'location_syntax')
            namespace = path[i+2:end]
            match = NAME.match(path, end+1)
            require(match is not None, 'location_syntax')
            name, i = match.group(), match.end()
            kind = 'attribute' if attribute else 'element'
        else:
            require(not attribute, 'location_syntax')
            kind = ''
            for token, node in (('text()', 'text'), ('comment()', 'comment')):
                if path.startswith(token, i):
                    kind, i = node, i + len(token)
                    break
            if not kind and path.startswith('processing-instruction("', i):
                start = i + len('processing-instruction("')
                match = PI_NAME.match(path, start)
                require(match is not None and path.startswith('")', match.end()), 'location_syntax')
                name, i, kind = match.group(), match.end()+2, 'processing_instruction'
                require(name.lower() != 'xml', 'location_syntax')
            require(kind, 'location_syntax')
        index = None
        if kind != 'attribute':
            match = re.match(r'\[([1-9][0-9]{0,19})\]', path[i:])
            require(match is not None, 'location_index')
            index = int(match.group(1))
            require(index <= 2**64-1, 'location_index')
            i += match.end()
        require(i == len(path) or path[i] == '/', 'location_syntax')
        # document children may be comments/PIs; attributes/text need an owner element.
        require(kind not in ('attribute', 'text') or steps, 'location_syntax')
        steps.append((kind, namespace, name, index))
    return tuple(steps)


def validate_record(row, native=False, validator=None):
    if validator is None:
        validate_schema(row, HERE/'native-record.schema.json' if native else ROOT/'tools/oracle/outcome.schema.json')
    else:
        require(next(validator.iter_errors(row), None) is None, 'schema_contract')
    stages = row.get('stages', {})
    require(set(stages) <= set(PROJECTION.values()), 'unknown_stage')
    require(row['schema'] == stages.get('val-xsd', 'not_evaluated'), 'schema_stage_consistency')
    both = all(k in stages for k in ('val-sch.1', 'val-sch.2'))
    require((row['business'] == 'evaluated') == both, 'business_stage_consistency')
    if 'val-xsd' in stages:
        require(stages.get('val-xml') == 'valid', 'xsd_without_xml')
    if any(k in stages for k in ('val-sch.1', 'val-sch.2')):
        require(row['schema'] == 'valid', 'business_without_xsd')
    if 'val-sch.2' in stages:
        require('val-sch.1' in stages, 'xr_without_cen')
    if row.get('target_matched'):
        require(row.get('scenario') == 'EN16931 XRechnung (CII)', 'target_scenario_consistency')
    if row.get('scenario') == 'EN16931 XRechnung (CII)':
        require(row.get('target_matched') is True, 'target_scenario_consistency')
    require((row['processing'] == 'failed') == bool(row['processing_errors']), 'processing_errors_consistency')
    if row['processing'] == 'failed':
        require(row['acceptance'] == 'indeterminate', 'failed_acceptance')
    else:
        require(stages.get('val-xml') == 'valid', 'completed_without_xml')
        if row.get('target_matched') or row.get('scenario') == 'EN16931 (CII)':
            require('val-xsd' in stages, 'missing_required_xsd')
        if row.get('scenario') == 'EN16931 (CII)' and row['schema'] == 'valid':
            require('val-sch.1' in stages, 'missing_required_cen')
        complete = row.get('target_matched') is True and row['schema'] == 'valid' and both
        accepted = complete and row['reference_acceptance'] == 'acceptable'
        require(row['acceptance'] == ('accepted' if accepted else 'rejected'), 'acceptance_consistency')
        if row.get('target_matched') and row['schema'] == 'valid':
            require(both, 'missing_business_stage')
    for array in ('findings', 'structural_findings'):
        for diagnostic in row[array]:
            stage = diagnostic['stage']
            require(stage in stages, 'diagnostic_absent_stage')
            require((stage.startswith('val-sch.')) == (array == 'findings'), 'diagnostic_array_stage')
            require(diagnostic['severity'] in ('error', 'warning', 'information', None) and
                    diagnostic['effective_severity'] in ('error', 'warning', 'information', None), 'invalid_severity')
            if diagnostic['severity'] in ('error', 'warning'):
                require(stages[stage] == 'invalid', 'base_severity_stage')
            if diagnostic['location'] is not None:
                location(diagnostic['location'])
    if row['acceptance'] == 'accepted':
        require(not any(d['effective_severity'] == 'error' for d in row['findings'] + row['structural_findings']), 'accepted_effective_error')
    for stage, verdict in stages.items():
        if verdict == 'invalid':
            require(any(d['stage'] == stage and d['severity'] in ('error', 'warning')
                        for d in row['findings'] + row['structural_findings']), 'invalid_stage_without_observation')
    return row


def identity(row):
    return {k: row[k] for k in ('id', 'sha256', 'scope', 'split')}


def ordered_records(rows, manifest, native=False, validator=None):
    unique(rows)
    require(len(rows) == len(manifest), 'record_count')
    for row, source in zip(rows, manifest):
        require(identity(row) == identity(source), 'record_order_identity')
        validate_record(row, native, validator)


def validate_inventory(dataset, initial, split):
    unique(dataset)
    unique(initial)
    require(len(dataset) == 10000 and len(initial) == 43, 'manifest_counts')
    require(sha(canonical([r['id'] for r in dataset])) == '8121e18aa2a0a880d754884b67be910769cca1b97c2bbb4c361b922b84289cb0', 'dataset_identity_inventory')
    require(Counter(r['scenario'] for r in dataset) == THEMES, 'theme_inventory')
    require(split['dataset_rows'] == len(dataset) and split['initial_cases'] == len(initial), 'split_counts')
    require(set(split['themes']) == set(THEMES), 'split_themes')
    dataset_ids = {r['id'] for r in dataset}
    merged, hashes, paths, sources, mutations = {}, {}, {}, {}, set()
    for row in dataset + initial:
        require(row['scope'] in ('complete', 'rule_test_set'), 'manifest_scope')
        confined(ROOT/'tools/oracle', row['path'])
        require(row['path'].startswith('.cache/'), 'input_cache_path')
        if row['id'] in merged:
            require(row == merged[row['id']], 'overlap_identity')
            continue
        merged[row['id']] = row
        source = row['source']
        if source == 'sinabis.parquet':
            require(row['id'] in dataset_ids and row['family'] == 'original' and
                    row['revision'] == '8d9ab205ab74f360fac344a50fcd54c35d0cd9ba' and
                    row['group'] == row['scenario'] and row['scope'] == 'complete', 'dataset_source')
            expected = split['themes'][row['scenario']]
        elif source == 'local-mutation-v1':
            require(row['family'] in split['mutation_families'], 'mutation_family')
            expected = split['mutation_families'][row['family']]
            require(row['id'] == 'mutation-' + row['family'] and row['group'] == row['id'] and
                    row['scope'] == 'complete', 'mutation_identity')
            parent = merged.get(row.get('parent'))
            require(parent is not None and parent['sha256'] == row.get('parent_sha256') and
                    parent['source'] == 'sinabis.parquet' and row['scenario'] == parent['scenario'], 'parent_identity')
            mutations.add(row['family'])
        elif source in ('cen.zip', 'cen-source.zip'):
            expected = 'held_out'
            require(row['scope'] == ('complete' if source == 'cen.zip' else 'rule_test_set') and
                    row['group'] == ('official-complete' if source == 'cen.zip' else 'rule-scoped') and
                    row['id'] == ('official-' if source == 'cen.zip' else 'scoped-') + row['family'] and
                    PurePosixPath(row['source_path']).stem == row['family'], 'official_identity')
        else:
            raise EvidenceError('unknown_source')
        require(row['split'] == expected, 'source_split')
        require(row['sha256'] not in hashes or hashes[row['sha256']] == expected, 'byte_split_conflict')
        hashes[row['sha256']] = expected
        require(row['path'] not in paths, 'duplicate_input_path')
        paths[row['path']] = row['id']
        if 'source_path' in row:
            confined(Path('/'), row['source_path'])
            key = (source, row['source_path'])
            require(key not in sources, 'duplicate_source_identity')
            sources[key] = row['id']
    require(mutations == set(split['mutation_families']), 'mutation_inventory')
    require(len(merged) == 10029 and Counter(r['scope'] for r in merged.values()) == {'complete': 10027, 'rule_test_set': 2}, 'union_inventory')
    return list(merged.values())


def verify_scoped(rows, containers, pins):
    unique(rows)
    require(len(rows) == 9 and sha(canonical([{k: r[k] for k in ('id', 'rule_ids')} for r in rows])) == '833fe1b7fe014ca0eeb89b356849f99a280244e2a58e3c884d938920e1fa6ba8', 'scoped_inventory')
    # Container bytes are pinned in archive members even without a materialized cache.
    member_hashes = next(p['members'] for p in pins if p['file'] == 'cen-source.zip')
    expected = []
    for container in containers:
        matches = [name for name, digest in member_hashes.items()
                   if name.endswith('/' + container['source_path']) and digest == container['sha256']]
        require(len(matches) == 1, 'scoped_container_source')
        indices = []
        prefix = PurePosixPath(container['source_path']).name + ':'
        for row in rows:
            if row['id'].startswith(prefix):
                require(re.fullmatch(r'[0-9]+', row['id'][len(prefix):]) is not None, 'scoped_ordinal')
                indices.append(int(row['id'][len(prefix):]))
                require(row['rule_ids'] == container['rule_ids'], 'scoped_rule_ids')
                expected.append(row['id'])
        require(indices == list(range(len(indices))) and indices, 'scoped_ordinal_inventory')
    require([r['id'] for r in rows] == expected, 'scoped_container_order')
    for row in rows:
        validate_schema(row, HERE/'scoped.schema.json')
        require(row['processing'] == 'completed', 'scoped_processing')
        for d in row['findings']:
            if d['location'] is not None:
                location(d['location'])


def audit_cache(cache, pins, inventory, scoped):
    cache = Path(cache)
    counts = Counter()
    for pin in pins:
        path = confined(cache, 'downloads/' + pin['file'])
        require(path.is_file(), 'cache_artifact_integrity')
        data = path.read_bytes()
        require(sha(data) == pin['sha256'], 'cache_artifact_integrity')
        counts['artifacts'] += 1
        if 'members' not in pin:
            continue
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            for member in archive.infolist():
                confined(cache, member.filename.rstrip('/'))
            names = [i.filename for i in archive.infolist() if not i.is_dir()]
            require(len(names) == len(set(names)) and set(names) == set(pin['members']), 'archive_inventory')
            for name, digest in pin['members'].items():
                confined(cache, name)
                require(sha(archive.read(name)) == digest, 'archive_member_integrity')
                extracted = confined(cache, pin['file'].removesuffix('.zip') + '/' + name)
                require(extracted.is_file() and sha(extracted.read_bytes()) == digest, 'extracted_member_integrity')
                counts['members'] += 1
    for row in inventory:
        path = confined(cache, row['path'].removeprefix('.cache/'))
        require(path.is_file(), 'cache_input_missing')
        data = path.read_bytes()
        require(len(data) == row['bytes'] and sha(data) == row['sha256'], 'cache_input_integrity')
        counts['inputs'] += 1
        if row['scope'] == 'rule_test_set':
            # Inspect only structural inventory; no labels or XML content is emitted.
            tree = ET.fromstring(data)
            ns = '{http://difi.no/xsd/vefa/validator/1.0}'
            fragments = tree.findall(ns+'test')
            prefix = PurePosixPath(row['source_path']).name + ':'
            observed = [r['id'] for r in scoped if r['id'].startswith(prefix)]
            require(observed == [prefix+str(i) for i in range(len(fragments))], 'scoped_fragment_inventory')
            rules = [n.text for n in tree.findall(ns+'assert/'+ns+'scope')]
            require(all(r['rule_ids'] == rules for r in scoped if r['id'].startswith(prefix)), 'scoped_container_rules')
    return dict(status='verified', **dict(counts))


def evaluator_digest(snapshots=None):
    # Domain-separated canonical list; no generated report or digest-containing file.
    snapshots = snapshots or {}
    return sha(canonical({'domain': 'bsc10-evaluator-v1', 'files': [
        {'path': name, 'sha256': sha(snapshots[name] if name in snapshots else (HERE/name).read_bytes())}
        for name in sorted(POLICY_FILES)]}))


@dataclass
class VerifiedEvidence:
    """Parsed evidence and identities derived from the same checked byte snapshots."""

    report: dict
    manifests: dict
    references: dict
    hashes: dict


def verify(lock_path=HERE/'evidence.lock.json', cache=None):
    lock_bytes = Path(lock_path).read_bytes()
    lock = decode(lock_bytes)
    require(set(lock) == {'format', 'reviewed_revision', 'files'} and type(lock['format']) is int and lock['format'] == 1 and
            lock['reviewed_revision'] == BASE, 'lock_anchor')
    entries = lock['files']
    require(isinstance(entries, list) and all(isinstance(x, dict) and set(x) == {'path', 'bytes', 'sha256'} for x in entries), 'lock_shape')
    require([x['path'] for x in entries] == sorted(FROZEN), 'lock_inventory')
    frozen = {}
    for entry in entries:
        path = confined(ROOT, entry['path'])
        require(path.is_file() and not path.is_symlink(), 'frozen_file_missing')
        data = path.read_bytes()
        require(type(entry['bytes']) is int and len(data) == entry['bytes'] and sha(data) == entry['sha256'], 'frozen_bytes')
        frozen[entry['path']] = data
    paths = [p for p in (ROOT/'tools/oracle').rglob('*') if
             '.cache' not in p.relative_to(ROOT/'tools/oracle').parts and '__pycache__' not in p.parts]
    require(not any(p.is_symlink() for p in paths), 'frozen_symlink')
    actual = {str(p.relative_to(ROOT)) for p in paths if p.is_file()}
    require(actual == {p for p in FROZEN if p.startswith('tools/oracle/')}, 'frozen_inventory')
    def frozen_json(name):
        return decode(frozen['tools/oracle/' + name])

    def frozen_rows(name):
        return decode_jsonl(frozen['tools/oracle/' + name])

    outcome_schema = frozen_json('outcome.schema.json')
    Draft202012Validator.check_schema(outcome_schema)
    outcome_validator = Draft202012Validator(outcome_schema)
    manifests = {name: frozen_rows(f'corpus/{name}.jsonl') for name in ('sinabis', 'initial')}
    inventory = validate_inventory(manifests['sinabis'], manifests['initial'], frozen_json('corpus/split.json'))
    references = {}
    for name, rows in manifests.items():
        references[name] = frozen_rows(f'results/{name}.jsonl')
        ordered_records(references[name], [r for r in rows if r['scope'] == 'complete'], validator=outcome_validator)
        meta = frozen_json(f'results/{name}.jsonl.metadata.json')
        require(meta['artifact_lock_sha256'] == sha(frozen['tools/oracle/artifacts.lock.json']) and
                meta['bridge_sha256'] == sha(frozen['tools/oracle/java/Oracle.java']), 'reference_provenance')
    pins = frozen_json('artifacts.lock.json')['artifacts']
    scoped = frozen_rows('results/scoped.jsonl')
    verify_scoped(scoped, [r for r in inventory if r['scope'] == 'rule_test_set'], pins)
    workload = frozen_json('workload.json')
    groups = {r['group'] for r in manifests['initial'] if r['scope'] == 'complete'}
    require(set(workload['groups']) == groups and all(v == 1/len(groups) for v in workload['groups'].values()), 'workload_groups')
    if cache is None:
        cache_report = {'status': 'unavailable', 'reason': 'cache_not_supplied'}
    elif not Path(cache).exists():
        cache_report = {'status': 'unavailable', 'reason': 'cache_directory_missing'}
    else:
        cache_report = audit_cache(cache, pins, inventory, scoped)
    report = base_report('verify')
    policy_snapshots = {'evidence.lock.json': lock_bytes} if Path(lock_path).resolve() == (HERE/'evidence.lock.json').resolve() else {}
    report.update(frozen_integrity='verified', frozen_files=len(entries), complete_documents=10027,
                  manifest_complete_records={'initial': 41, 'sinabis': 10000}, scoped_containers=2,
                  scoped_fragments=len(scoped), cache=cache_report,
                  integrity_complete=cache_report['status'] == 'verified', reviewed_revision=BASE,
                  evidence_lock_sha256=sha(lock_bytes), evaluator_sha256=evaluator_digest(policy_snapshots))
    return VerifiedEvidence(report, manifests, references, {path: sha(data) for path, data in frozen.items()})


def base_report(command):
    return {'format': 1, 'command': command, 'release_gate_eligible': False,
            'evidence_complete': False, 'native_execution_attested': False,
            'gaps': ['authentic_evaluator_controlled_execution_unavailable', 'runtime_coverage_unavailable',
                     'resource_evidence_unavailable', 'native_scoped_execution_unavailable',
                     'structural_cause_mapping_unavailable', 'raw_flag_role_reference_unavailable',
                     'loaded_package_backend_build_source_unverified', 'runtime_capabilities_unverified']}


def request_for(name, selection, evidence):
    rows = [r for r in evidence.manifests[name] if r['scope'] == 'complete' and
            (selection == 'full' or r['split'] == 'development')]
    require(rows, 'empty_selection')
    return {'format': 1, 'kind': 'bsc10_request', 'manifest': f'tools/oracle/corpus/{name}.jsonl',
            'release_gate_eligible': False,
            'manifest_sha256': evidence.hashes[f'tools/oracle/corpus/{name}.jsonl'],
            'selection': selection, 'requested_target': TARGET, 'wire_target': WIRE_TARGET,
            'target_projection': 'bsc10-target-v1', 'artifact_lock_sha256': evidence.hashes['tools/oracle/artifacts.lock.json'],
            'evidence_lock_sha256': evidence.report['evidence_lock_sha256'], 'evaluator_sha256': evidence.report['evaluator_sha256'],
            'records': rows}


def validate_run(meta, records, request, output_digest):
    validate_schema(meta, HERE/'run.schema.json')
    for key in ('evaluator_sha256', 'evidence_lock_sha256', 'artifact_lock_sha256',
                'requested_target', 'wire_target', 'target_projection'):
        require(meta[key] == request[key], 'sidecar_binding')
    require(meta['request_sha256'] == sha(canonical(request)), 'request_digest')
    require(meta['output_sha256'] == output_digest, 'output_digest')
    require(meta['completion']['record_count'] == len(records), 'completion_count')
    require([d['id'] for d in meta['documents']] == [r['id'] for r in records], 'sidecar_document_inventory')
    implementation = meta['implementation']
    for key in ('backend', 'generator', 'package', 'source'):
        unavailable = implementation[key] is None
        require((isinstance(implementation['unavailable_reasons'][key], str) and
                 bool(implementation['unavailable_reasons'][key])) if unavailable else
                implementation['unavailable_reasons'][key] is None, 'loaded_identity_reason')
    if implementation['source'] is not None:
        require(implementation['source']['artifact_lock_sha256'] == request['artifact_lock_sha256'], 'source_pins')
    for record, doc, source_row in zip(records, meta['documents'], request['records']):
        require([s['stage'] for s in doc['stages']] == list(STAGES), 'native_stage_inventory')
        if record['acceptance'] == 'accepted':
            require(doc['failure'] is None, 'accepted_failure')
        if any(s['status'] == 'failed' for s in doc['stages']):
            require(record['processing'] == 'failed' and doc['failure'] is not None, 'failed_stage_processing')
        projected = {}
        operational = False
        for stage in doc['stages']:
            completed = stage['status'] == 'completed'
            require((stage['reason'] is None) if completed else bool(stage['reason']), 'stage_reason')
            if stage['stage'] in PROJECTION:
                if stage['verdict'] is not None:
                    require(stage['status'] != 'skipped', 'skipped_verdict')
                    projected[PROJECTION[stage['stage']]] = stage['verdict']
                if completed:
                    require(stage['verdict'] is not None, 'completed_verdict')
            else:
                require(stage['verdict'] is None, 'nonvalidation_verdict')
            operational |= stage['status'] != 'completed' and stage['stage'] in ('xml_syntax', 'target_selection', 'acceptance')
        require(projected == record['stages'], 'native_stage_projection')
        if record['processing'] == 'completed':
            require(doc['failure'] is None or doc['failure']['kind'] == 'document', 'operational_completed')
            require(not operational, 'incomplete_processing')
            for stage in doc['stages']:
                if stage['stage'] in PROJECTION and PROJECTION[stage['stage']] in projected:
                    require(stage['status'] == 'completed', 'failed_projected_stage')
            if record['business'] == 'evaluated':
                require(all(s['status'] == 'completed' for s in doc['stages']), 'incomplete_business')
            require(all(implementation[k] is not None for k in ('backend', 'generator', 'package', 'source')), 'missing_loaded_identity')
        else:
            require(doc['failure'] is not None and any(s['status'] != 'completed' for s in doc['stages']), 'missing_failure')
        details = doc['diagnostic_details']
        expected = [(array, i) for array in ('findings', 'structural_findings') for i in range(len(record[array]))]
        require([(d['array'], d['ordinal']) for d in details] == expected, 'diagnostic_bijection')
        seen = set()
        for detail in details:
            key = (detail['array'], detail['ordinal'])
            diagnostic = record[detail['array']][detail['ordinal']]
            require(detail['raw_code'] == diagnostic['id'] and detail['raw_message'] == diagnostic['message'], 'raw_diagnostic_projection')
            for field in ('constraint', 'type', 'component', 'cause'):
                reason = detail['typed_unavailable_reasons'][field]
                require(bool(reason) if detail[field] is None else reason is None, 'typed_unavailable_reason')
            parent = detail['parent']
            if parent is not None:
                require(detail['relationship'] == 'collateral' and (parent['array'], parent['ordinal']) in seen, 'diagnostic_parent')
            else:
                require(detail['relationship'] != 'collateral', 'collateral_without_parent')
            seen.add(key)
            source = detail['source']
            if source['path'] is not None:
                location(source['path'])
            require(source['path'] == diagnostic['location'], 'source_path_projection')
            span = source['span']
            if span is not None:
                require(span['start'] <= span['end'] and source['path'] is not None, 'source_span')
                if span['unit'] == 'utf8_byte':
                    require(span['end'] <= source_row['bytes'], 'source_span_bounds')
                if span['subject'] == 'owner_element':
                    require(location(source['path']) and location(source['path'])[-1][0] == 'attribute', 'owner_span_subject')
            if source['path'] is None or span is None:
                require(bool(source['unavailable_reason']), 'source_unavailable_reason')
            else:
                require(source['unavailable_reason'] is None, 'source_available_reason')
    any_failed = any(r['processing'] == 'failed' for r in records)
    require((meta['completion']['status'] == 'completed' and meta['completion']['exit_code'] == 0) if not any_failed else
            (meta['completion']['status'] == 'failed' and meta['completion']['exit_code'] != 0), 'run_completion')


def business_projection(diagnostic):
    return {**diagnostic, 'location': location(diagnostic['location']) if diagnostic['location'] is not None else None}


def compare_records(reference, candidate, selection):
    """Strict business sequence; structural raw observations never stand in for cause mapping."""
    require(len(reference) == len(candidate), 'comparison_count')
    require(selection in ('development', 'full'), 'comparison_selection')
    if selection == 'development':
        require(all(r['split'] == 'development' for r in reference + candidate), 'development_scope')
    mismatches, observations = [], []
    counts = Counter()
    for index, (ref, native) in enumerate(zip(reference, candidate)):
        fields = [key for key in ('processing', 'schema', 'business', 'stages', 'target_matched',
                  'scenario', 'reference_acceptance', 'acceptance') if ref.get(key) != native.get(key)]
        if [business_projection(d) for d in ref['findings']] != [business_projection(d) for d in native['findings']]:
            fields.append('business_sequence')
        if fields:
            counts['mismatching_documents'] += 1
            if selection == 'development':
                mismatches.append({'request_ordinal': index, 'id': ref['id'], 'fields': fields})
        left, right = ref['structural_findings'], native['structural_findings']
        counts['reference_structural_observations'] += len(left)
        counts['candidate_structural_observations'] += len(right)
        if left or right:
            counts['structural_unresolved_documents'] += 1
            if left != right:
                counts['structural_raw_difference_documents'] += 1
            if len(left) != len(right):
                counts['structural_multiplicity_difference_documents'] += 1
            # Raw ordinal facts only: this is not a cross-engine cause alignment.
            for key in ('id', 'severity', 'effective_severity', 'location', 'line', 'column', 'message', 'stage'):
                if [d[key] for d in left] != [d[key] for d in right]:
                    counts['structural_raw_' + key + '_difference_documents'] += 1
            if selection == 'development':
                observations.append({'request_ordinal': index, 'id': ref['id'],
                                     'semantic_equivalence': 'unresolved', 'reference': left, 'candidate': right})
        if ref['processing'] != 'completed' or native['processing'] != 'completed':
            counts['processing_incomplete_documents'] += 1
    parity = False if counts['mismatching_documents'] else (None if counts['structural_unresolved_documents'] or counts['processing_incomplete_documents'] else True)
    report = {'observable_parity': parity, 'counts': dict(counts)}
    if selection == 'development':
        report.update(mismatches=mismatches, structural_observations=observations)
    return report


def compare(request, references, native_path, metadata_path):
    output_bytes = Path(native_path).read_bytes()
    records = decode_jsonl(output_bytes)
    ordered_records(records, request['records'], native=True)
    meta = read_json(metadata_path)
    validate_run(meta, records, request, sha(output_bytes))
    selected_ids = {r['id'] for r in request['records']}
    reference = [r for r in references if r['id'] in selected_ids]
    require([r['id'] for r in reference] == [r['id'] for r in request['records']], 'reference_selection_order')
    details = compare_records(reference, records, request['selection'])
    report = base_report('compare')
    report.update(details, selection=request['selection'], manifest=request['manifest'],
                  expected_records=len(request['records']), observed_records=len(records), kind=meta['kind'],
                  request_sha256=sha(canonical(request)), output_sha256=meta['output_sha256'],
                  evaluator_sha256=request['evaluator_sha256'], evidence_lock_sha256=request['evidence_lock_sha256'],
                  protocol='verified', structural_semantic_parity='unresolved' if details['counts'].get('structural_unresolved_documents') else 'not_exercised',
                  processing_semantic_parity='unresolved' if details['counts'].get('processing_incomplete_documents') else 'completed')
    # Diagnostic detail/raw processing text remains in the bound input, never held-out reports.
    if meta['kind'] == 'harness_fixture':
        report['gaps'].append('synthetic_harness_fixture_not_native_evidence')
    if details['counts'].get('processing_incomplete_documents'):
        report['gaps'].append('processing_failure_classification_unavailable')
    counts = details['counts']
    if counts.get('processing_incomplete_documents') or counts.get('structural_unresolved_documents'):
        exit_code = 2
    else:
        exit_code = 1 if details['observable_parity'] is False else 0
    return report, exit_code


def write_output(value, destination, inputs):
    data = json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + '\n'
    if destination is None:
        print(data, end='')
        return
    output = Path(destination)
    resolved = output.resolve()
    # All source/evaluator/cache inputs are read-only even when a path wasn't selected.
    require(not resolved.is_relative_to((ROOT/'tools/oracle').resolve()) and
            not resolved.is_relative_to(HERE.resolve()), 'output_overwrites_source')
    protected = [ROOT/name for name in FROZEN] + [HERE/name for name in POLICY_FILES]
    protected.append(ROOT/'.github/workflows/conformance.yml')
    for path in protected + list(inputs):
        path = Path(path)
        require(resolved != path.resolve(), 'output_overwrites_input')
        if output.exists() and path.exists():
            require(not os.path.samefile(output, path), 'output_overwrites_input')
        if path.is_dir():
            require(not resolved.is_relative_to(path.resolve()), 'output_overwrites_input')
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=output.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(data)
        os.replace(temporary, output)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=('verify', 'request', 'compare'))
    parser.add_argument('--evidence-lock', type=Path, default=HERE/'evidence.lock.json')
    parser.add_argument('--cache', type=Path, help='Explicit read-only cache; never downloads/extracts/writes')
    parser.add_argument('--manifest', choices=('initial', 'sinabis'), default='initial')
    parser.add_argument('--selection', choices=('development', 'full'), default='development')
    parser.add_argument('--native', type=Path)
    parser.add_argument('--metadata', type=Path, help='Must be adjacent <native>.metadata.json')
    parser.add_argument('--output', type=Path, help='Default stdout; file writes are atomic')
    args = parser.parse_args(argv)
    inputs = [args.evidence_lock] + ([args.cache] if args.cache else [])
    if args.native:
        inputs.extend([args.native, Path(str(args.native)+'.metadata.json')])
    if args.metadata:
        inputs.append(args.metadata)
    request = None
    try:
        if args.output is not None:
            pending = base_report(args.command)
            pending.update(protocol='incomplete', observable_parity=None, error='evaluation_not_completed')
            write_output(pending, args.output, inputs)
        evidence = verify(args.evidence_lock, args.cache)
        exit_code = 0
        if args.command == 'verify':
            report = evidence.report
        else:
            request = request_for(args.manifest, args.selection, evidence)
            if args.command == 'request':
                report = request
            else:
                require(args.native is not None, 'native_input_required')
                metadata = Path(str(args.native)+'.metadata.json')
                require(args.metadata is None or args.metadata.resolve() == metadata.resolve(), 'sidecar_must_be_adjacent')
                report, exit_code = compare(request, evidence.references[args.manifest], args.native, metadata)
                report['cache'] = evidence.report['cache']
        write_output(report, args.output, inputs)
        return exit_code
    except (OSError, ValueError, KeyError, TypeError, IndexError, RecursionError, zipfile.BadZipFile, ET.ParseError) as error:
        # No raw exception strings/paths/IDs/lines, including malformed held-out records.
        code = str(error) if isinstance(error, EvidenceError) else 'invalid_or_incomplete_evidence'
        report = base_report(args.command)
        report.update(protocol='invalid', observable_parity=None, error=code)
        if request is not None:
            report.update(selection=request['selection'], manifest=request['manifest'],
                          expected_records=len(request['records']), observed_records=None,
                          request_sha256=sha(canonical(request)), evaluator_sha256=request['evaluator_sha256'],
                          evidence_lock_sha256=request['evidence_lock_sha256'])
        # Never leave a stale successful report after any failure at a safe destination.
        try:
            write_output(report, args.output, inputs)
        except (OSError, EvidenceError):
            print(json.dumps(report), file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
