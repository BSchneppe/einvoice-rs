import collections, json
from common import *

def main():
    requests=json.loads((OUT/'requests.json').read_bytes())
    meta=json.loads((OUT/'run-metadata.json').read_bytes())
    prep=json.loads((OUT/'preparation.json').read_bytes())
    observations=[]; contract_errors=[]; occurrences=[]
    stdout_path=OUT/'processes/evaluate60.stdout'
    stdout=stdout_path.read_bytes() if stdout_path.exists() else b''
    stdout_lines=stdout.splitlines(keepends=True)
    raw_records=[]
    for r in requests:
        prefix=OUT/f'raw/{r["ordinal"]:02d}-{r["id"]}'
        raw=Path(str(prefix)+'.bridge.json'); inp=Path(str(prefix)+'.input-receipt.json'); out=Path(str(prefix)+'.output-receipt.json')
        observation={'request':r,'observation_status':'unobserved','record':None,'errors':[]}
        if inp.exists(): observation['input_receipt']=json.loads(inp.read_bytes())
        if raw.exists():
            b=raw.read_bytes()
            try:
                record=json.loads(b); observation.update(observation_status='observed',record=record,bridge_record_sha256=sha(b))
                raw_records.append(b)
                require(record['id']==r['id'] and record['sha256']==r['sha256'],'raw_record_input_binding')
                receipt=observation.get('input_receipt'); require(receipt is not None and receipt['input_sha256']==r['sha256'] and receipt['ordinal']==r['ordinal'],'input_receipt_binding')
                require(receipt['original_request_row'].split('\t')==[r['id'],r['family'],r['input'],r['sha256']],'original_request_row_binding')
                if out.exists():
                    output_receipt=json.loads(out.read_bytes()); observation['output_receipt']=output_receipt
                    require(output_receipt['bridge_record_sha256']==sha(b) and output_receipt['input_sha256']==r['sha256'],'output_receipt_binding')
                else: observation['errors'].append('output_receipt_missing')
                stages=record['stages']; require(record['schema']==stages.get('val-xsd','not_evaluated'),'schema_stage_consistency')
                require(record['business']==('evaluated' if 'val-sch.1' in stages and 'val-sch.2' in stages else 'not_evaluated'),'business_stage_consistency')
                require(record['target_matched']==(record['scenario']=='EN16931 XRechnung (CII)'),'scenario_binding')
                if record['processing']!='completed': require(record['acceptance']=='indeterminate','processing_failure_acceptance_contract')
                if record['acceptance']=='accepted':
                    require(record['processing']=='completed' and record['target_matched'] and all(stages.get(k)=='valid' for k in ['val-xml','val-xsd']) and 'val-sch.1' in stages and 'val-sch.2' in stages,'acceptance_stage_contract')
                for bucket in ('findings','structural_findings'):
                    require(isinstance(record[bucket],list),'finding_array_contract')
                    for index,finding in enumerate(record[bucket]):
                        occurrences.append({'ordinal':r['ordinal'],'id':r['id'],'bucket':bucket,'occurrence_index':index,'finding':finding})
            except Exception as exc:
                observation['errors'].append(repr(exc)); observation['observation_status']='observed_contract_error'
        else:
            observation['missing_reason']='evaluation_did_not_produce_a_bridge_record; see process status and raw stderr; verdict unknown'
        if observation['errors']: contract_errors.append({'id':r['id'],'errors':observation['errors']})
        observations.append(observation)
    # Preserve exact stdout. Compare JSON bridge lines to original raw files in order;
    # non-JSON stdout remains retained and is explicitly counted.
    json_lines=[line for line in stdout_lines if line.startswith(b'{')]
    if json_lines!=raw_records: contract_errors.append({'stdout_binding':'raw_bridge_lines_differ_or_missing_or_extra'})
    process=next((p for p in meta['processes'] if p['name']=='evaluate60'),None)
    all_observed=all(o['observation_status']=='observed' for o in observations)
    closure_before=OUT/'raw/closure-before.json'; closure_after=OUT/'raw/closure-after.json'
    closure_ok=False
    if closure_before.exists() and closure_after.exists():
        before=json.loads(closure_before.read_bytes()); after=json.loads(closure_after.read_bytes())
        binding=json.loads((OUT/'compiled-execution-binding.json').read_bytes())
        closure_ok=(before['verified'] and after['verified'] and before['closure_sha256']==after['closure_sha256']==binding['runtime_closure_sha256'] and after['records_returned']==60)
    processing_complete=all(o['record'] is not None and o['record']['processing']=='completed' for o in observations)
    complete=bool(process and process['status']=='exited' and process['exit_code']==0 and not meta['failures'] and not contract_errors and all_observed and closure_ok and processing_complete)
    summary={'status':'complete' if complete else 'failed_or_incomplete','requested':60,
             'observed':sum(o['record'] is not None for o in observations),'unobserved':sum(o['record'] is None for o in observations),
             'execution_process':process,'all_requested_observed':all_observed,'all_processing_completed':processing_complete,'consumed_closure_pre_post_verified':closure_ok,
             'acceptance_counts':dict(collections.Counter(o['record']['acceptance'] if o['record'] else 'unknown' for o in observations)),
             'processing_counts':dict(collections.Counter(o['record']['processing'] if o['record'] else 'unknown' for o in observations)),
             'contract_errors':contract_errors,'execution_failures':meta['failures'],'stdout_bridge_lines':len(json_lines),
             'stdout_other_lines':len(stdout_lines)-len(json_lines),'finding_occurrences':len(occurrences),
             'freeze_sha256':FREEZE_SHA,'ledger_sha256':LEDGER_SHA,'timing_measurements':False,
             'original_prelabel_expectations_unchanged':True,'no_expected_verdict_assertions':True}
    write(OUT/'observations.json',{'summary':summary,'observations':observations},True)
    write(OUT/'finding-occurrences.json',occurrences,True)
    write(OUT/'summary.json',summary,True)
    def cell(value):
        if value is None: return 'null'
        return str(value).replace('|','\\|').replace('\n','<br>').replace('\r','\\r')
    def counts(record,bucket):
        if record is None: return 'unknown'
        fs=record[bucket]
        # Each occurrence stays present, including duplicates and null identifiers.
        return ', '.join(f'{cell(f["id"])} [{cell(f["severity"])}→{cell(f["effective_severity"])}]' for f in fs) or 'none'
    lines=['# BSC10 frozen DEVELOPMENT evaluation observations','',
      f'Batch status: **{summary["status"]}**. Requested 60; observed {summary["observed"]}; unobserved {summary["unobserved"]}. '
      'The baseline is request 0; requests 1–59 preserve the frozen ledger order. The earlier four CII-SR-282 discriminants are a separate batch and are excluded.','',
      'All prelabel expectations remain unknown in the unchanged copied ledger. These observations are discovery evidence for later independent artifact review/publication; they do not establish full conformance, evaluator attestation, native behavior, source-site coverage, resources, or performance.','',
      f'Acceptance counts: `{json.dumps(summary["acceptance_counts"],sort_keys=True)}`. Processing counts: `{json.dumps(summary["processing_counts"],sort_keys=True)}`. '
      f'Consumed closure verified before/after: `{closure_ok}`. Process status: `{process["status"] if process else "not_launched"}`; exit: `{process["exit_code"] if process else None}`. '
      f'Contract errors: {len(contract_errors)}; execution failures: {len(meta["failures"])}.','',
      '## Actual observation matrix','',
      'Missing stages are `not_evaluated`, not validity results. `null` is the original nullable value. Codes are listed occurrence by occurrence in original per-bucket order; the full exact messages, BASE severity, effective severity, paths, string coordinates, and duplicate occurrences remain in raw bridge JSON and observations.json.','',
      '| # | Request | Processing | XML | XSD | SCH.1 | SCH.2 | Scenario / target match | Reference recommendation | Acceptance | Business findings [BASE→effective] | Structural findings [BASE→effective] |',
      '|---:|---|---|---|---|---|---|---|---|---|---|---|']
    for o in observations:
        r=o['request']; rec=o['record']
        if rec:
            st=rec['stages']; values=[r['ordinal'],r['id'],rec['processing']]+[st.get(k,'not_evaluated') for k in ['val-xml','val-xsd','val-sch.1','val-sch.2']]+[f'{cell(rec["scenario"])} / {rec["target_matched"]}',rec['reference_acceptance'],rec['acceptance'],counts(rec,'findings'),counts(rec,'structural_findings')]
        else: values=[r['ordinal'],r['id'],'unobserved']+['unknown']*9
        lines.append('| '+' | '.join(cell(v) for v in values)+' |')
    lines += ['', '## Provenance and execution closure','',
      f'- Freeze SHA-256: `{FREEZE_SHA}`; proposal SHA-256: `{LEDGER_SHA}`.',
      '- The freeze binds the verified review-bundle hash and all199 members. Full59 byte replay, original anchors/coordinates, ordered sequential cardinalities, and whole-family DEVELOPMENT allocation passed before execution; see preflight-verification.json.',
      f'- Exact Python: `{PYTHON}`; version `{prep["python"]["version"]}`; resolved executable `{prep["python"]["realpath"]}`.',
      f'- Exact Corretto requested home: `{JDK}`. Java/javac realpaths, binary digests, release digest and raw version process outputs are retained. Loaded class resource hashes/code sources and the running Java executable realpath are in raw/closure-before.json.',
      '- Original pinned validator JAR and original full97-member configuration were copied from verified read-only cache archive snapshots. Every config member was checked against the copied archive actually consumed, before and after. STRICT_LOCAL resolution was used; no alternative XSLT was substituted.',
      '- Oracle.java was copied unchanged and hash checked. The custom driver calls only Oracle.evaluate, once per request in one Java process after one configuration load. Oracle.main (timing/benchmark entry point) was never invoked. No warmup, benchmark, timing measurement, fake adapter, package install, download, or shared-cache write occurred.',
      '- Runtime closure binds compiled classes, JAR/config, original byte input copies, requested manifest, freeze/ledger snapshots, driver and scripts. Java checks its actual loaded paths, real JDK home/executable, requested byte hashes, and closure before and after. Read-only input copies were passed as original byte arrays without parsing/reserialization by the harness.',
      '- All39 frozen evidence files were hashed as opaque bytes before and after; labels were not deserialized. See frozen-before.json and frozen-after.json. Source pins, original labels/splits/workload and source-inventory review files were untouched.',
      '- execution-declaration.json and declared-commands.txt were written before the first Java/javac subprocess; the compiled binding and exact evaluation command were written before evaluation. All process argv, raw stdout/stderr, exit codes and timeout/error statuses are retained under processes/. There was one attempt and no rerun.',
      '- Baseline SHA-256: `'+SEED_SHA+'`. Sinabis GmbH (2026), *Sinabis Green Energy – 10k GoBD-compliant German E-Invoices*, CC-BY-4.0 (<https://creativecommons.org/licenses/by/4.0/>). Parent revision `8d9ab205ab74f360fac344a50fcd54c35d0cd9ba`, source artifact SHA-256 `f45674ce80b477a859c684c68e423f8bb52ad2c1647c97a9e29050f6519dee13`, member `zugferd/b2b_service_maintenance/0000/0000000_SRV-2025-47731.xml`. Baseline unchanged; exact mutation lineage, recipes, original locations and predecessor revisions are retained separately in snapshots/proposed-ledger.json and snapshots/revisions.jsonl.','',
      '## Outcome contrasts and semantic gaps','',
      'No verdict was predicted or asserted. Accepted cases with negative design intent and rejected cases with positive design intent remain evidence; no recipe, allocation, shape, value, revision, input, or request order was changed after observation. Absence of a finding provides no source-hit or coverage inference.','']
    ledger=json.loads((OUT/'snapshots/proposed-ledger.json').read_bytes()); byid={r['id']:r for r in ledger['candidates']}
    for o in observations:
        rec=o['record']; row=byid.get(o['request']['id'])
        if rec and row and ((rec['acceptance']=='accepted' and row['intent']['negative']) or (rec['acceptance']=='rejected' and row['intent']['positive'])):
            lines.append(f'- `{row["id"]}`: {rec["acceptance"]}; positive intents={json.dumps(row["intent"]["positive"],ensure_ascii=True)}; negative intents={json.dumps(row["intent"]["negative"],ensure_ascii=True)}. This is an intent/outcome contrast, not a failed expected-result check.')
    lines += ['',
      '- Parser/XML, XSD, Schematron stages, scenario matching, processing status and reference recommendation are distinct fields. Processing exceptions or missing output are never invoice-rejection labels. Observed bridge records retain all collateral diagnostics, even when unrelated to the mutation intent.',
      '- All six technique families share one public ancestor/source shape. The57 mutations, two encoding controls and separate identity baseline provide no independent-source or held-out claim. No held-out inputs, labels or family names were inspected.',
      '- Accepted XML1.1/source-form/encoding variants can establish only this configured evaluator’s submitted-byte result. Diagnostic equivalence or absent findings does not reveal the intermediate normalized tree, parser internals, XPath node selection, or source-site execution.',
      '- The copied bridge retains ordered messages within its business and structural arrays and stage validity in insertion order. It does not emit the full KoSIT report DOM; this batch makes no stronger cross-bucket event-order or full-report claim. All bridge records are preserved exactly, including nullable coordinates and strings.',
      '- No independent artifact review/publication was performed here. Frozen-byte integrity and local receipts bind this run; they are not a cryptographic evaluator attestation.',
      '', '## Files','',
      '- `observations.json`: all60 original requested rows, actual records or explicit unobserved/error statuses, per-input receipts and summary.',
      '- `raw/*.bridge.json`: unchanged individual raw bridge records; `processes/evaluate60.stdout` is the original combined process stream. `finding-occurrences.json` preserves every per-bucket occurrence without deduplication.',
      '- `execution-declaration.json`, `compiled-execution-binding.json`, `runtime-closure.tsv`, `preflight-verification.json`, `preparation.json`: execution and provenance commitments.',
      '- `processes/*`: every attempted subprocess command, stdout, stderr and metadata. `run-metadata.json` retains infrastructure failures. No silent overwrite/rerun.',
      '- `hash-manifest.json`: hashes of every retained file except itself; verify against files before review.','']
    if meta['failures'] or contract_errors:
        lines += ['## Failures retained','', '```json',json.dumps({'failures':meta['failures'],'contract_errors':contract_errors},indent=2),'```','']
    put(OUT/'report.md','\n'.join(lines).encode(),True)
    files=sorted(p for p in OUT.rglob('*') if p.is_file())
    write(OUT/'hash-manifest.json',{'self_exclusion':'hash-manifest.json only','files':[{'path':str(p.relative_to(OUT)),'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in files]},True)
    return complete
