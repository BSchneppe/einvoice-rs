#!/usr/bin/env python3
"""Offline verification of the reviewed, bounded DEVELOPMENT publication."""
import argparse
import base64
import collections
import io
import os
from pathlib import Path
import sys
import stat
import types
import zipfile

def _load_local_source(name):
    """Bootstrap fixed local dependencies from held source, never bytecode."""
    path = Path(os.path.abspath(__file__)).with_name(name + '.py')
    directory = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parts[1:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory)
            os.close(directory)
            directory = child
        leaf = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        with os.fdopen(leaf, 'rb') as stream:
            before = os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
                raise ValueError('bootstrap_source_type')
            source = stream.read()
            after = os.fstat(stream.fileno())
            identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
            if identity(before) != identity(after) or len(source) != before.st_size:
                raise ValueError('bootstrap_source_changed')
    finally:
        os.close(directory)
    module = types.ModuleType(name)
    module.__file__ = str(path)
    module.__package__ = ''
    module.__loaded_source__ = source
    previous = sys.modules.get(name)
    sys.modules[name] = module
    try:
        exec(compile(source, str(path), 'exec'), module.__dict__)
    except BaseException:
        if previous is None:
            sys.modules.pop(name, None)
        else:
            sys.modules[name] = previous
        raise
    return module

_common_source = _load_local_source('common')

from common import (Invalid, Report, check_bytes, json_lines, no_symlinks,
                    read_file, relative, require, sha, strict_json)

HISTORY_SHA = '34924b22c0d9be1f6fe3d78fe6a2ee496793e466c27bddea7e20cd7946b68a26'
TARGET = 'xrechnung-3.0.2-cii-2026-08-31'
JVM_FLAGS = ['-Xms256m', '-Xmx1024m', '-XX:ActiveProcessorCount=1', '-XX:+UseSerialGC',
             '-Dorg.slf4j.simpleLogger.defaultLogLevel=error']
LIMITS = {'historical_799_payload_available': False, 'native_execution_attested': False,
          'release_gate_eligible': False, 'timing_eligible': False}


def indexed(rows, key='path'):
    result = {}
    for row in rows:
        require(row[key] not in result, 'duplicate_identity')
        result[row[key]] = row
    return result


class Bundle:
    def __init__(self, manifest):
        manifest = no_symlinks(manifest)
        require(manifest.name == 'publication.json', 'inventory_name')
        self.root = manifest.parent
        raw = read_file(manifest)
        self.inventory_sha256 = sha(raw)
        inventory = strict_json(raw)
        require(inventory['format'] == 'bsc10-development-publication-v1', 'inventory_format')
        rows = indexed(inventory['files'])
        require('publication.json' not in rows, 'inventory_self_reference')
        actual = set()
        for directory, dirs, files in os.walk(self.root, followlinks=False):
            for name in dirs + files:
                require(not (Path(directory) / name).is_symlink(), 'symlink_path')
            parent = Path(directory).relative_to(self.root)
            if parent.name == '__pycache__':
                require(parent.as_posix() in ('__pycache__', 'tests/__pycache__') and not dirs and
                        all(x.endswith('.pyc') for x in files), 'unexpected_cache_content')
                continue
            for name in files:
                p = (Path(directory) / name).relative_to(self.root).as_posix()
                if p != 'publication.json':
                    actual.add(p)
        require(actual == set(rows), 'inventory_file_set')
        self.files = {}
        for name, pin in rows.items():
            relative(name)
            data = read_file(self.root / name)
            check_bytes(data, pin)
            self.files[name] = data
            if name.endswith('.json'):
                strict_json(data)
            elif name.endswith('.jsonl'):
                json_lines(data)
        require(self.files.get('common.py') == _common_source.__loaded_source__, 'loaded_common_source')
        if '__loaded_source__' in globals():
            require(self.files.get('verify.py') == __loaded_source__, 'loaded_verifier_source')
        self.selection = self.json('selection.json')
        require(self.selection['format'] == 'bsc10-development-selection-v1', 'selection_format')
        history_data = self.files['history/artifact-review/manifest.json']
        require(sha(history_data) == HISTORY_SHA == self.selection['historical_manifest_sha256'], 'history_manifest_pin')
        history = strict_json(history_data)
        original = indexed([r for rows in history['batches'].values() for r in rows])
        self.rows = indexed(self.selection['files'])
        require(len(original) == 799 and set(original) == set(self.rows), 'selection_identity_set')
        for path, row in self.rows.items():
            relative(path)
            require(row['bytes'] == original[path]['bytes'] and row['sha256'] == original[path]['sha256'], 'selection_identity_pin')
            kind = row['disposition']
            require(bool(row['reason']), 'disposition_reason')
            if kind == 'retained_exact':
                name = row['retained_path']
                relative(name)
                require(name.startswith('evidence/') and name in self.files, 'dangling_retained_reference')
                check_bytes(self.files[name], row, 'retained_identity_pin')
                if 'alias_of_original' in row:
                    other = self.rows.get(row['alias_of_original'], {})
                    require(other.get('disposition') == 'retained_exact' and 'alias_of_original' not in other and
                            other.get('retained_path') == name and other['path'] != path, 'alias_reference')
            elif kind == 'external_pinned_dependency':
                require(row['dependency'] in self.json('provenance.json')['dependencies'], 'unknown_dependency')
                if 'member' in row:
                    relative(row['member'])
            elif kind == 'derived_recipe':
                require(path.endswith('.class') and row['recipe'] in self.selection['recipes'], 'unknown_recipe')
            else:
                require(kind == 'intentionally_excluded_non_evidence' and
                        path in ('discriminant/linear-evidence.md', 'family-run/snapshots/family-label-plan.md'), 'unknown_disposition')
        counts = collections.Counter(r['disposition'] for r in self.rows.values())
        require(counts == {'retained_exact': 585, 'external_pinned_dependency': 203,
                           'derived_recipe': 9, 'intentionally_excluded_non_evidence': 2}, 'disposition_counts')
        retained = {r['retained_path'] for r in self.rows.values() if r['disposition'] == 'retained_exact'}
        require(len(retained) == 402 and sum(len(self.files[x]) for x in retained) == 4372919, 'selection_payload')

    def json(self, name):
        return strict_json(self.files[name])

    def data(self, original):
        row = self.rows[original]
        require(row['disposition'] == 'retained_exact', 'omitted_payload')
        return self.files[row['retained_path']]

    def historical_json(self, original):
        return strict_json(self.data(original))


def family_recipe(seed, recipe):
    require(recipe['format'] == 'bsc10-seed-byte-splices-v1' and recipe['no_xml_reserialization'] is True, 'recipe_format')
    require(sha(seed) == recipe['seed_sha256'], 'recipe_seed')
    edits = recipe['splices_against_original_seed']
    last = 0
    for e in edits:
        start, stop = e['seed_start_byte_0based'], e['seed_end_byte_0based']
        require(type(start) is int and type(stop) is int and last <= start <= stop <= len(seed), 'splice_span')
        last = stop
        old = base64.b64decode(e['old_base64'], validate=True)
        anchor = base64.b64decode(e['unique_seed_anchor_base64'], validate=True)
        a = e['anchor_seed_start_byte_0based']
        require(type(a) is int and 0 <= a <= start and stop <= a + len(anchor) <= len(seed), 'anchor_span')
        require(anchor and seed.count(anchor) == e['anchor_match_count'] == 1 and seed[a:a+len(anchor)] == anchor, 'anchor_identity')
        require(seed[start:stop] == old and sha(old) == e['old_sha256'], 'splice_old_bytes')
        require(e['seed_line_1based'] == seed[:start].count(b'\n') + 1 and
                e['seed_byte_column_1based'] == start - seed.rfind(b'\n', 0, start), 'splice_coordinates')
    data = seed
    for e in reversed(edits):
        data = data[:e['seed_start_byte_0based']] + base64.b64decode(e['new_base64'], validate=True) + data[e['seed_end_byte_0based']:]
    require(recipe['final_encoding'] in ('utf-8', 'utf-16-le') and type(recipe['bom']) is bool, 'recipe_encoding')
    text = data.decode('utf-8', errors='strict')
    require(any(text.startswith('<?xml version=' + q + recipe['xml_version'] + q) for q in ('"', "'")), 'xml_declaration_version')
    data = text.encode(recipe['final_encoding'], errors='strict')
    if recipe['bom']:
        data = {'utf-8': b'\xef\xbb\xbf', 'utf-16-le': b'\xff\xfe'}[recipe['final_encoding']] + data
    return data


def recipe_digest(recipe):
    import json
    return sha((json.dumps(recipe, ensure_ascii=True, sort_keys=True, indent=2) + '\n').encode())


def check_lineage(bundle):
    seed = bundle.data('discriminant/snapshots/development-seed.xml')
    check_bytes(seed, bundle.json('source-record.json')['seed'])
    previous = None
    previous_candidates = {}
    ledgers = []
    for version, script in [(1, 'design_batch.v1.py'), (2, 'design_batch.v2.py'), (3, 'design_batch.py')]:
        base = f'family-design/v{version}/'
        raw = bundle.data(base + 'proposed-ledger.json')
        ledger = strict_json(raw)
        ledgers.append(ledger)
        require(ledger['design_script_sha256'] == sha(bundle.data('family-design/' + script)), 'design_script_pin')
        require(ledger['source_provenance_sha256'] == sha(bundle.data(base + 'source-provenance.json')), 'source_provenance_pin')
        if version > 1:
            require(ledger['previous_proposal'] == {'path': f'../v{version-1}/proposed-ledger.json', 'sha256': previous}, 'predecessor_proposal')
        candidates = indexed(ledger['candidates'], 'id')
        revisions = json_lines(bundle.data(base + 'revisions.jsonl'))
        require(len(candidates) == len(revisions) == 59 and list(candidates) == [r['id'] for r in revisions], 'draft_order')
        for row, rev in zip(candidates.values(), revisions):
            require(row['assignment'] == 'DEVELOPMENT' and row['whole_family'] == row['parameter_family'] == row['id'][:3] and
                    row['whole_family'] in ledger['families'], 'family_assignment')
            require(row['oracle_evaluated'] is False and row['observation'] is None and
                    row['expected'] == dict.fromkeys(('xml', 'schema', 'business', 'scenario', 'acceptance', 'processing'), 'unknown'), 'prelabel_expectations')
            require(row['recipe_sha256'] == recipe_digest(row['recipe']) == rev['recipe_sha256'], 'recipe_digest')
            require(row['revision'] == rev['revision'] and rev['input_sha256'] == row['input']['sha256'] and rev['oracle_exposure'] is False, 'revision_binding')
            parent = rev['parent_revision']
            if version == 1:
                require(parent is None, 'initial_parent')
            elif parent is not None:
                prior = previous_candidates.get(parent['id'])
                require(prior is not None and parent['proposal'] == f'v{version-1}' and
                        parent['revision'] == prior['revision'] and parent['id'] == row['id'] and
                        prior['whole_family'] == row['whole_family'], 'revision_parent')
                require(row['revision'] in (prior['revision'], prior['revision'] + 1), 'revision_sequence')
            else:
                old = rev['supersedes_draft_id']
                require(old in previous_candidates and previous_candidates[old]['whole_family'] == row['whole_family'], 'superseded_parent')
                require(any(x['id'] == old and x['replacement'] == row['id'] for x in ledger['superseded_draft_candidates']), 'supersession_record')
            data = family_recipe(seed, row['recipe'])
            check_bytes(data, row['input'], 'recipe_input_digest')
            relative(row['input']['path'])
            require(data == bundle.data(base + row['input']['path']), 'recipe_input_bytes')
            transform = row['original_location_encoding_transformation']
            require(all(transform[k] == row['recipe'][k] for k in ('final_encoding', 'bom', 'xml_version')), 'encoding_metadata')
        for r in bundle.historical_json(base + 'file-manifest.json')['files']:
            relative(r['path'])
            check_bytes(bundle.data(base + r['path']), r, 'draft_file_manifest')
        previous, previous_candidates = sha(raw), candidates
    freeze = bundle.historical_json('family-design/freeze-v1.json')
    review = bundle.historical_json('family-design/review-bundle.json')
    for row in review['files']:
        relative(row['path'])
        check_bytes(bundle.data('family-design/' + row['path']), row, 'review_bundle_member')
    require(freeze['assignment'] == 'DEVELOPMENT' and freeze['count'] == 59 and freeze['full_run_records'] == 60 and
            freeze['proposal_sha256'] == previous, 'freeze_binding')
    require(freeze['case_order'] == [{'family': r['whole_family'], 'id': r['id'], 'sha256': r['input']['sha256']}
                                     for r in ledgers[-1]['candidates']], 'freeze_order')
    require(freeze['family_assignments'] == ledgers[-1]['families'] and
            freeze['review_bundle_sha256'] == sha(bundle.data('family-design/review-bundle.json')) and
            freeze['source_provenance_sha256'] == ledgers[-1]['source_provenance_sha256'] and
            freeze['design_script_sha256'] == ledgers[-1]['design_script_sha256'], 'freeze_commitments')
    requests = bundle.historical_json('family-run/requests.json')
    check_requests(requests, freeze, ledgers[-1])
    expected_tsv = ''.join(f"{r['id']}\t{r['family']}\t{r['input']}\t{r['sha256']}\n" for r in requests).encode()
    require(bundle.data('family-run/requests.tsv') == expected_tsv, 'requests_tsv')
    for r in requests:
        check_bytes(bundle.data('family-run/' + str(relative(r['input']))), r, 'request_input')
    declaration = bundle.historical_json('family-run/execution-declaration.json')
    require(declaration['requested_rows'] == requests and declaration['requested_count'] == len(requests) and
            declaration['requests_json_sha256'] == sha(bundle.data('family-run/requests.json')) and
            declaration['requests_tsv_sha256'] == sha(expected_tsv) and
            declaration['freeze_sha256'] == sha(bundle.data('family-design/freeze-v1.json')) and
            declaration['ledger_sha256'] == previous, 'request_declaration')
    ledger = bundle.historical_json('discriminant/ledger-before-label.json')
    require(ledger['assignment'] == 'development-only' and [r['case'] for r in ledger['recipes']] ==
            ['baseline', 'seller', 'buyer', 'combined'], 'discriminant_selection')
    import json
    for r in ledger['recipes']:
        body = {k: v for k, v in r.items() if k != 'recipe_sha256'}
        require(r['recipe_sha256'] == sha(json.dumps(body, sort_keys=True, separators=(',', ':')).encode()), 'discriminant_recipe')
        insert = r['insertion_utf8'].encode('utf-8')
        require(sha(insert) == r['insertion_sha256'] and len(insert) == r['inserted_bytes'] and
                r['expected_oracle_result'] is None and r['removed_bytes'] == 0 and r['parent_sha256'] == sha(seed), 'discriminant_insertion')
        anchor = b'      <ram:BuyerOrderReferencedDocument>'
        require(seed.count(anchor) == 1 and seed.index(anchor) == r['original_byte_offset_zero_based'], 'discriminant_offset')
        offset = r['original_byte_offset_zero_based']
        data = seed[:offset] + insert + seed[offset:]
        require(sha(data) == r['input_sha256'] and len(data) == r['input_bytes'] and
                data == bundle.data('discriminant/' + str(relative(r['input']))), 'discriminant_input')
    require(bundle.data('discriminant/ledger-before-label.sha256').strip().decode() == sha(bundle.data('discriminant/ledger-before-label.json')), 'discriminant_ledger_digest')
    inputs = {r['sha256'] for r in bundle.rows.values() if '/inputs/' in r['path'] or r['path'] == 'discriminant/snapshots/development-seed.xml'}
    require(len(inputs) == 67, 'historical_input_count')
    return requests, ledger['recipes']


def check_requests(requests, freeze, ledger):
    require(len(requests) == len(freeze['case_order']) + 1 == 60, 'request_membership')
    indexed(requests, 'id')
    baseline = requests[0]
    require(baseline['id'] == 'baseline' and baseline['family'] == 'PUBLIC-DEVELOPMENT-CONTROL' and
            baseline['sha256'] == ledger['seed']['sha256'] and baseline['bytes'] == ledger['seed']['bytes'], 'baseline_request')
    for i, (r, frozen, candidate) in enumerate(zip(requests[1:], freeze['case_order'], ledger['candidates']), 1):
        require(r['id'] == frozen['id'] == candidate['id'] and r['family'] == frozen['family'] == candidate['whole_family'] and
                r['sha256'] == frozen['sha256'] == candidate['input']['sha256'] and r['bytes'] == candidate['input']['bytes'] and
                r['recipe_sha256'] == candidate['recipe_sha256'] and r['revision'] == candidate['revision'] and
                r['original_ledger_row_0based'] == i - 1, 'request_freeze_order_family')
    for i, row in enumerate(requests):
        require(type(row['ordinal']) is int and row['ordinal'] == i and
                row['expected'] == dict.fromkeys(('xml', 'schema', 'business', 'scenario', 'acceptance', 'processing'), 'unknown'), 'request_ordinal_expectations')


def completed_stage_observations(record):
    diagnostics = record['findings'] + record['structural_findings']
    observed = {f['stage'] for f in diagnostics if f['severity'] in ('error', 'warning')}
    require(all(record['stages'][stage] == 'invalid' for stage in observed), 'base_severity_stage')
    require(all(verdict != 'invalid' or stage in observed for stage, verdict in record['stages'].items()),
            'invalid_stage_without_observation')


def classify(record, process_ok=True):
    """Bound supported completed target observations; never invent engine paths."""
    expected = {'format', 'id', 'sha256', 'report_html_elements', 'engine', 'target', 'processing', 'processing_errors',
                'scenario', 'target_matched', 'stages', 'schema', 'business', 'findings', 'structural_findings',
                'reference_acceptance', 'acceptance'}
    require(set(record) == expected and type(record['format']) is int and record['format'] == 1, 'record_shape_or_scope')
    require(isinstance(record['id'], str) and bool(record['id']) and isinstance(record['sha256'], str) and
            len(record['sha256']) == 64 and all(x in '0123456789abcdef' for x in record['sha256']), 'record_identity_type')
    require(record['report_html_elements'] is None or
            (type(record['report_html_elements']) is int and record['report_html_elements'] >= 0), 'report_html_type')
    require(record['scenario'] is None or isinstance(record['scenario'], str), 'scenario_type')
    require(record['target'] == TARGET and record['engine'] == 'KoSIT 1.6.3 / Saxon HE 12.9', 'record_target_engine')
    require(record['processing'] in ('completed', 'failed') and isinstance(record['processing_errors'], list), 'processing_state')
    require(all(isinstance(x, str) for x in record['processing_errors']), 'processing_errors_type')
    require((record['processing'] == 'failed') == bool(record['processing_errors']), 'processing_error_consistency')
    require(record['acceptance'] in ('accepted', 'rejected', 'indeterminate') and
            record['reference_acceptance'] in ('acceptable', 'reject', 'undefined'), 'recommendation_value')
    stages = record['stages']
    require(isinstance(stages, dict) and set(stages) <= {'val-xml', 'val-xsd', 'val-sch.1', 'val-sch.2'} and
            all(v in ('valid', 'invalid') for v in stages.values()), 'stage_status')
    require(record['schema'] == stages.get('val-xsd', 'not_evaluated') and
            record['business'] == ('evaluated' if {'val-sch.1', 'val-sch.2'} <= stages.keys() else 'not_evaluated'), 'stage_projection')
    if 'val-xsd' in stages:
        require(stages.get('val-xml') == 'valid', 'xsd_prerequisite')
    if 'val-sch.1' in stages or 'val-sch.2' in stages:
        require(stages.get('val-xsd') == 'valid', 'business_prerequisite')
    if 'val-sch.2' in stages:
        require('val-sch.1' in stages, 'xr_prerequisite')
    for bucket in ('findings', 'structural_findings'):
        require(isinstance(record[bucket], list), 'diagnostic_bucket')
        for finding in record[bucket]:
            require(set(finding) == {'id', 'severity', 'effective_severity', 'location', 'line', 'column', 'message', 'stage'}, 'diagnostic_shape')
            require(all(finding[k] is None or isinstance(finding[k], str) for k in ('id', 'location', 'line', 'column')) and
                    isinstance(finding['message'], str), 'diagnostic_type')
            require(finding['severity'] in (None, 'error', 'warning', 'information') and
                    finding['effective_severity'] in (None, 'error', 'warning', 'information'), 'diagnostic_severity')
            require(finding['stage'] in stages and (finding['stage'].startswith('val-sch') == (bucket == 'findings')), 'diagnostic_stage')
    require(type(record['target_matched']) is bool and record['target_matched'] == (record['scenario'] == 'EN16931 XRechnung (CII)'), 'target_match')
    if record['processing'] == 'failed':
        require(record['acceptance'] == 'indeterminate', 'failed_acceptance')
        return 'processing_failed'
    require(record['acceptance'] != 'indeterminate', 'completed_acceptance')
    require(record['reference_acceptance'] != 'undefined', 'undefined_recommendation')
    if not process_ok:
        return 'process_incomplete'
    require('val-xml' in stages, 'missing_xml_stage')
    if stages['val-xml'] == 'invalid':
        require(record['acceptance'] == 'rejected', 'xml_terminal_acceptance')
        return 'unsupported_xml_terminal'
    require(record['target_matched'], 'unmatched_target_unclassified')
    require('val-xsd' in stages, 'missing_xsd_stage')
    if stages['val-xsd'] == 'invalid':
        require(record['acceptance'] == 'rejected' and record['reference_acceptance'] == 'reject', 'xsd_terminal_recommendation')
        completed_stage_observations(record)
        return 'completed_schema_rejection'
    require({'val-sch.1', 'val-sch.2'} <= stages.keys(), 'missing_business_stage')
    accepted = record['reference_acceptance'] == 'acceptable'
    require(record['acceptance'] == ('accepted' if accepted else 'rejected'), 'recommendation_projection')
    if accepted:
        require(not any(f['effective_severity'] == 'error' for b in ('findings', 'structural_findings') for f in record[b]), 'accepted_effective_error')
    completed_stage_observations(record)
    return 'completed_business_observation'


def process_binding(command, metadata, stdout, stderr, argv, cwd, timeout=None, exit_code=0):
    actual = command['argv'] if isinstance(command, dict) else command
    require(actual == metadata['argv'] == argv and metadata['cwd'] == cwd, 'process_argv_cwd')
    if isinstance(command, dict):
        require(command['cwd'] == cwd and command['timeout_seconds'] == timeout, 'process_command_metadata')
    require(metadata.get('status', 'exited') == 'exited' and type(metadata['exit_code']) is int and metadata['exit_code'] == exit_code, 'process_status')
    require(metadata['stdout_sha256'] == sha(stdout) and metadata['stderr_sha256'] == sha(stderr), 'process_stream_hash')


def family_receipts(request, line, input_receipt, output_receipt, closure_hash, driver_hash, freeze_hash, ledger_hash):
    row = f"{request['id']}\t{request['family']}\t{request['input']}\t{request['sha256']}"
    require(type(input_receipt['ordinal']) is int and type(input_receipt['input_bytes']) is int and input_receipt == {'ordinal': request['ordinal'], 'original_request_row': row, 'id': request['id'],
        'group': request['family'], 'input': request['input'], 'input_bytes': request['bytes'], 'input_sha256': request['sha256'],
        'closure_sha256': closure_hash, 'driver_sha256': driver_hash, 'freeze_sha256': freeze_hash, 'ledger_sha256': ledger_hash}, 'input_receipt_binding')
    require(type(output_receipt['ordinal']) is int and output_receipt == {'ordinal': request['ordinal'], 'id': request['id'], 'input_sha256': request['sha256'],
                              'bridge_record_sha256': sha(line), 'evaluate_returned': True} and
            type(output_receipt['evaluate_returned']) is bool, 'output_receipt_binding')
    record = json_lines(line)
    require(len(record) == 1 and record[0]['id'] == request['id'] and record[0]['sha256'] == request['sha256'], 'raw_request_binding')
    require(classify(record[0]) in ('completed_schema_rejection', 'completed_business_observation'), 'incomplete_record')
    return record[0]


def occurrences(requests, records):
    return [{'ordinal': r['ordinal'], 'id': r['id'], 'bucket': bucket, 'occurrence_index': i, 'finding': f}
            for r, record in zip(requests, records) for bucket in ('findings', 'structural_findings')
            for i, f in enumerate(record[bucket])]


def check_history(bundle, requests, recipes):
    j = bundle.historical_json
    d = bundle.data
    for closure_path in ('family-run/runtime-closure.tsv', 'discriminant/runtime-closure.tsv', 'discriminant/attempt-02/runtime-closure.tsv'):
        base = 'family-run/' if closure_path.startswith('family-run/') else 'discriminant/'
        seen = set()
        for line in d(closure_path).decode('utf-8').splitlines():
            parts = line.split('\t')
            require(len(parts) == 2, 'historical_closure_shape')
            digest, name = parts
            relative(name)
            require(name not in seen and base + name in bundle.rows and bundle.rows[base + name]['sha256'] == digest, 'historical_closure_identity')
            seen.add(name)
    declaration = j('family-run/execution-declaration.json')
    root = declaration['cwd']
    family = root + '/.scratch/bsc10/expansion/family-run'
    disc = root + '/.scratch/bsc10/expansion/discriminant'
    jdk = declaration['environment_changes']['JAVA_HOME']
    java, javac = jdk + '/bin/java', jdk + '/bin/javac'
    processes = {}
    # Exact commands are derived from retained declarations and actual driver APIs.
    binding = j('family-run/compiled-execution-binding.json')
    closure_hash = sha(d('family-run/runtime-closure.tsv'))
    freeze_hash, ledger_hash = sha(d('family-design/freeze-v1.json')), sha(d('family-design/v3/proposed-ledger.json'))
    args = [java] + JVM_FLAGS + ['-cp', family + '/classes:' + family + '/snapshots/validator.jar', 'FamilyDriver', family,
            closure_hash, sha(d('family-run/requests.tsv')), freeze_hash, ledger_hash, jdk]
    require(binding['java_command'] == args and binding['runtime_closure_sha256'] == closure_hash and
            binding['driver_sha256'] == sha(d('family-run/FamilyDriver.java')) and
            binding['requests_sha256'] == args[-4] and binding['freeze_sha256'] == freeze_hash and binding['ledger_sha256'] == ledger_hash, 'compiled_binding')
    plans = [('family-run/', {'java-version': [java, '-version'], 'javac-version': [javac, '-version'],
        'compile': [javac, '-encoding', 'UTF-8', '-cp', family + '/snapshots/validator.jar', '-d', family + '/classes',
                    family + '/snapshots/Oracle.java', family + '/FamilyDriver.java'], 'evaluate60': args})]
    for prefix, driver, classes, closure in [('discriminant/', 'DiscriminantDriver', 'classes', 'runtime-closure.tsv'),
            ('discriminant/attempt-02/', 'DiscriminantDriverV2', 'attempt-02/classes', 'attempt-02/runtime-closure.tsv')]:
        plans.append((prefix, {'java-version': [java, '-version'], 'javac-version': [javac, '-version'],
            'javac': [javac, '-encoding', 'UTF-8', '-cp', disc + '/snapshots/validator.jar', '-d', disc + '/' + classes,
                      disc + '/snapshots/Oracle.java', disc + '/' + driver + '.java'],
            **{r['case']: [java] + JVM_FLAGS + ['-cp', disc + '/' + classes + ':' + disc + '/snapshots/validator.jar', driver,
                disc, closure, sha(d('discriminant/' + closure)), 'bsc10-development-' + r['case'], r['input'], r['input_sha256'],
                ('attempt-02/' if 'attempt-02' in prefix else '') + 'processes/' + r['case'] + '.receipt.json', jdk] for r in recipes}}))
    for prefix, commands in plans:
        meta_run = j(prefix + 'run-metadata.json')
        for name, argv in commands.items():
            path = prefix + 'processes/' + name
            meta = j(path + '.metadata.json')
            code = 1 if prefix == 'discriminant/' and name in ('baseline', 'seller', 'buyer', 'combined') else 0
            timeout = {'java-version': 30, 'javac-version': 30, 'compile': 60, 'evaluate60': 600}.get(name)
            process_binding(j(path + '.command.json'), meta, d(path + '.stdout'), d(path + '.stderr'), argv, root, timeout, code)
            require(meta in meta_run['processes'], 'run_process_binding')
            processes[path] = meta
    require(len(processes) == 18, 'process_count')
    for label, fields in [('before', {'freeze_sha256': freeze_hash, 'ledger_sha256': ledger_hash}),
                          ('after', {'records_returned': len(requests)})]:
        receipt = j('family-run/raw/closure-' + label + '.json')
        require(receipt['verified'] is True and receipt['closure_sha256'] == closure_hash and
                receipt['requests_sha256'] == sha(d('family-run/requests.tsv')) and
                all(receipt[k] == v for k, v in fields.items()), 'closure_receipt_binding')
    before, after = j('family-run/raw/closure-before.json'), j('family-run/raw/closure-after.json')
    require(before['loaded_classes'] == after['loaded_classes'] and before['jvm_arguments'] == JVM_FLAGS and
            before['java_command'] == java and before['java_command_realpath'] == declaration['jdk']['java']['realpath'] and
            before['java_home_realpath'] == str(Path(declaration['jdk']['java']['realpath']).parent.parent) and
            before['java_version'] == '27.0.1' and before['java_runtime_version'] == '27.0.1+11-FR', 'loaded_runtime_binding')
    for loaded, name in zip(before['loaded_classes'][:2], ('FamilyDriver', 'Oracle')):
        require(loaded['class'] == name and loaded['code_source_realpath'] == family + '/classes' and
                loaded['resource'] == 'file:' + family + '/classes/' + name + '.class' and
                loaded['resource_sha256'] == bundle.rows['family-run/classes/' + name + '.class']['sha256'], 'loaded_class_binding')
    require(len(before['loaded_classes']) == 5, 'loaded_resource_count')
    for loaded in before['loaded_classes'][2:]:
        require(loaded['code_source_realpath'] == family + '/snapshots/validator.jar' and
                loaded['resource'] == 'jar:file:' + family + '/snapshots/validator.jar!/' + loaded['class'].replace('.', '/') + '.class', 'loaded_jar_location')
        require(len(loaded['resource_sha256']) == 64, 'loaded_resource_hash')
    config = j('family-run/raw/configuration-loaded.json')
    require(config == {'configuration': family + '/config/scenarios.xml', 'resolving_mode': 'STRICT_LOCAL',
                      'configuration_loads': 1, 'evaluations_requested': len(requests), 'warmup': False, 'timing_measurements': False}, 'configuration_receipt')
    observations = j('family-run/observations.json')
    require(len(observations['observations']) == len(requests), 'derived_count')
    records, raw = [], []
    for r, observation in zip(requests, observations['observations']):
        prefix = f"family-run/raw/{r['ordinal']:02d}-{r['id']}"
        line = d(prefix + '.bridge.json')
        inp, out = j(prefix + '.input-receipt.json'), j(prefix + '.output-receipt.json')
        record = family_receipts(r, line, inp, out, closure_hash, sha(d('family-run/FamilyDriver.java')), freeze_hash, ledger_hash)
        require(observation == {'request': r, 'observation_status': 'observed', 'record': record, 'errors': [],
            'input_receipt': inp, 'bridge_record_sha256': sha(line), 'output_receipt': out}, 'derived_observation')
        records.append(record)
        raw.append(line)
    require(d('family-run/processes/evaluate60.stdout') == b''.join(raw), 'raw_stream_order_count')
    require(j('family-run/finding-occurrences.json') == occurrences(requests, records), 'derived_occurrences')
    summary = j('family-run/summary.json')
    require(observations['summary'] == summary and summary == {
        'status': 'complete', 'requested': len(requests), 'observed': len(records), 'unobserved': 0,
        'execution_process': processes['family-run/processes/evaluate60'], 'all_requested_observed': True,
        'all_processing_completed': all(r['processing'] == 'completed' for r in records),
        'consumed_closure_pre_post_verified': True,
        # Reconcile historical summaries with raw evidence; never require an expected verdict total.
        'acceptance_counts': dict(collections.Counter(r['acceptance'] for r in records)),
        'processing_counts': dict(collections.Counter(r['processing'] for r in records)),
        'contract_errors': [], 'execution_failures': [], 'stdout_bridge_lines': len(records), 'stdout_other_lines': 0,
        'finding_occurrences': len(occurrences(requests, records)), 'freeze_sha256': freeze_hash, 'ledger_sha256': ledger_hash,
        'timing_measurements': False, 'original_prelabel_expectations_unchanged': True, 'no_expected_verdict_assertions': True}, 'summary_binding')
    corrected = j('discriminant/attempt-02/observations.json')
    original = j('discriminant/observations.json')
    require(len(corrected) == len(original) == len(recipes), 'discriminant_observation_count')
    for recipe, observation, failed in zip(recipes, corrected, original):
        name = recipe['case']
        prefix = 'discriminant/attempt-02/processes/' + name
        line = d(prefix + '.stdout')
        parsed = json_lines(line)
        require(len(parsed) == 1, 'discriminant_record_count')
        record, receipt = parsed[0], j(prefix + '.receipt.json')
        require(record['id'] == 'bsc10-development-' + name and record['sha256'] == recipe['input_sha256'], 'discriminant_record_binding')
        require(classify(record) in ('completed_schema_rejection', 'completed_business_observation'), 'discriminant_processing')
        require(receipt == {'input_sha256': recipe['input_sha256'], 'closure_sha256': sha(d('discriminant/attempt-02/runtime-closure.tsv')),
            'closure_pre_post_verified': True, 'loaded_jar': disc + '/snapshots/validator.jar',
            'loaded_oracle_classes': disc + '/attempt-02/classes', 'java_home': before['java_home'],
            'java_version': before['java_version'], 'java_runtime_version': before['java_runtime_version']}, 'discriminant_receipt_binding')
        require(observation == {'case': name, 'status': 'observed', 'record': record, 'receipt': receipt}, 'discriminant_derived')
        require(failed == {'case': name, 'status': 'process_failed', 'exit_code': 1} and
                d('discriminant/processes/' + name + '.stdout') == b'' and
                b'JDK home mismatch' in d('discriminant/processes/' + name + '.stderr'), 'failed_guard_history')
    return {'historical_processes': len(processes), 'completed_request_positions': len(records) + len(corrected),
            'evaluated_distinct_input_bytes': len({r['sha256'] for r in records} | {o['record']['sha256'] for o in corrected})}


def archive_members(data, expected, exact=False):
    """Validate names before reading any member from a held verified archive."""
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        names = set()
        files = {}
        for info in archive.infolist():
            name = info.filename.rstrip('/') if info.is_dir() else info.filename
            relative(name)
            require(name not in names, 'duplicate_archive_member')
            names.add(name)
            mode = info.external_attr >> 16
            require(mode & 0o170000 != 0o120000, 'archive_symlink')
            if not info.is_dir():
                files[name] = info
        require(set(expected) <= files.keys() and (not exact or set(expected) == set(files)), 'archive_inventory')
        result = {}
        for name, pin in expected.items():
            result[name] = archive.read(files[name])
            check_bytes(result[name], pin, 'archive_member_pin')
        return result


def optional_dependencies(bundle, dependencies):
    if dependencies is None:
        return {'status': 'unavailable', 'reason': 'No explicit dependency directory supplied.'}
    root = no_symlinks(dependencies)
    provenance = bundle.json('provenance.json')
    result = {}
    for name in ('validator.jar', 'config.zip', 'cen.zip', 'xr.zip'):
        path = root / name
        if not os.path.lexists(path):
            result[name] = 'unavailable'
            continue
        data = read_file(path)
        check_bytes(data, provenance['dependencies'][name], 'dependency_pin')
        members = {r['member']: r for r in provenance['external_members'] if r['dependency'] == name}
        members.update({r['member']: r for r in provenance['source_member_pins'].values() if r['archive'] == name})
        held_members = archive_members(data, members, exact=name == 'config.zip')
        if name == 'validator.jar':
            loaded = bundle.historical_json('family-run/raw/closure-before.json')['loaded_classes'][2:]
            archive_members(data, {r['class'].replace('.', '/') + '.class': {'sha256': r['resource_sha256']} for r in loaded})
        if name == 'xr.zip':
            member = bundle.json('xr-citations.json')['member']['member']
            source = held_members[member]
            lines = source.splitlines(keepends=True)
            for r in bundle.json('xr-citations.json')['citations']:
                require(source[r['byte_start_0based']:r['byte_end_exclusive']] ==
                        b''.join(lines[r['line_start_1based']-1:r['line_end_1based']]) ==
                        base64.b64decode(r['excerpt_base64'], validate=True), 'xr_held_member_citation')
        result[name] = 'verified_selected_members'
    return {'status': 'verified_supplied_only', 'dependencies': result,
            'historical_artifact_lock': 'unavailable; opaque Git commitment only'}


def verify(bundle, dependencies=None):
    requests, recipes = check_lineage(bundle)
    facts = check_history(bundle, requests, recipes)
    require(facts['evaluated_distinct_input_bytes'] == 63, 'evaluated_input_count')
    source = bundle.json('source-record.json')
    provenance = bundle.json('provenance.json')
    require(source == provenance['sinabis'] and source['selector']['expected_unique_rows'] == 1 and
            source['selector']['idx'] == 0 and source['selector']['encoding_rule'] ==
            "source['xml'].encode('utf-8'); no XML parse/reserialization/newline normalization", 'source_cross_reference')
    citations = bundle.json('xr-citations.json')
    original_cites = bundle.historical_json('family-design/v3/source-provenance.json')['citations']
    for r in citations['citations']:
        excerpt = base64.b64decode(r['excerpt_base64'], validate=True)
        require(len(excerpt) == r['bytes'] == r['byte_end_exclusive'] - r['byte_start_0based'] and
                sha(excerpt) == r['observed_sha256'] == r['utf8_excerpt_sha256'] == original_cites[r['id']]['utf8_excerpt_sha256'] and
                r['line_start_1based'] == original_cites[r['id']]['line_start_1based'] and
                r['line_end_1based'] == original_cites[r['id']]['line_end_1based'], 'xr_citation_commitment')
    for r in bundle.json('notice-facts.json')['notices']:
        if 'notice_bytes_base64' in r:
            data = base64.b64decode(r['notice_bytes_base64'], validate=True)
            require(sha(data) == r['notice_sha256'] and data == bundle.files['notices/UNCEFACT-' + r['material'] + '.txt'], 'notice_bytes')
    for name, expected in [('CEN-LICENSE.txt', '281eed0c9d46e36be532978deb16e6351025c6ff32d97fe24e8cb190e4e0d923'),
                           ('XRechnung-LICENSE', '57e93bb611b8aeb93ff0f23b271a23291217b0016f6e66297dfd5e9467538a5f')]:
        require(sha(bundle.files['notices/' + name]) == expected, 'upstream_license_pin')
    return {'status': 'verified', 'retained_integrity': 'verified', 'history_input_lineage': 'verified_all_three_drafts',
            'historical_process_binding': 'verified', 'applicable_stage_sufficiency': 'verified_for_retained_completed_records',
            'external_dependency_integrity': optional_dependencies(bundle, dependencies), 'derived_class_reconstruction': 'not_rebuilt',
            'publication_sha256': bundle.inventory_sha256, 'historical_input_streams': 67, **facts, **LIMITS}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--dependencies', type=Path)
    args = parser.parse_args()
    report = None
    try:
        protected = [args.manifest.absolute().parent, Path(__file__).absolute().parent]
        if args.dependencies:
            protected.append(args.dependencies)
        report = Report(args.out, protected)
        result = verify(Bundle(args.manifest), args.dependencies)
        report.finish(result)
        print('Verified retained DEVELOPMENT evidence; external/derived/native limitations remain explicit.')
        return 0
    except (Invalid, OSError, KeyError, TypeError, ValueError, zipfile.BadZipFile) as exc:
        code = str(exc) if isinstance(exc, Invalid) else 'invalid_evidence_structure_or_io'
        if report:
            report.finish({'status': 'invalid', 'error': code, **LIMITS})
        print(code, file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
