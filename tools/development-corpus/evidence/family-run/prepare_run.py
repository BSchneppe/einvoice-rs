import base64, collections, io, json, os, runpy, zipfile
from common import *

def main():
    os.umask(0o077)
    py=interpreter()
    require(not (OUT/'preparation.json').exists() and not (OUT/'snapshots').exists(),'fresh_preparation_required')
    freeze_bytes=(DESIGN/'freeze-v1.json').read_bytes()
    ledger_bytes=(DESIGN/'v3/proposed-ledger.json').read_bytes()
    require(sha(freeze_bytes)==FREEZE_SHA and sha(ledger_bytes)==LEDGER_SHA,'freeze_and_ledger_pins')
    freeze=json.loads(freeze_bytes); ledger=json.loads(ledger_bytes)
    bundle_bytes=(DESIGN/'review-bundle.json').read_bytes(); bundle=json.loads(bundle_bytes)
    require(sha(bundle_bytes)==freeze['review_bundle_sha256'] and len(bundle['files'])==199,'review_bundle_pin')
    checked=[]
    for e in bundle['files']:
        p=DESIGN/e['path']; require(p.resolve().is_relative_to(DESIGN),'bundle_path')
        b=p.read_bytes(); require(len(b)==e['bytes'] and sha(b)==e['sha256'],'bundle_member_hash')
        checked.append(e)
    write(OUT/'frozen-before.json',frozen(),True)
    script=DESIGN/'design_batch.py'
    require(sha(script.read_bytes())==freeze['design_script_sha256']==ledger['design_script_sha256'],'design_script_pin')
    # Verified existing read-only checker: exact recipes, seven source allowlist members,
    # XML1.0 synthetic cardinalities; does not call any evaluator.
    design=runpy.run_path(str(script))
    replay_check=design['verify'](DESIGN/'v3')
    seed=(OUT.parent/'dev-seed-smoke-01/development-seed.xml').read_bytes()
    require(sha(seed)==SEED_SHA and len(seed)==11214,'baseline_pin')
    rows=ledger['candidates']; require(len(rows)==59 and freeze['count']==59,'candidate_count')
    require([{'family':r['whole_family'],'id':r['id'],'sha256':r['input']['sha256']} for r in rows]==freeze['case_order'],'frozen_order')
    counts=dict(collections.Counter(r['whole_family'] for r in rows))
    require(counts=={'F01':5,'F02':14,'F03':11,'F04':6,'F05':5,'F06':18},'family_count')
    require([r['whole_family'] for r in rows]==sorted(r['whole_family'] for r in rows),'whole_family_order')
    snapshots=OUT/'snapshots'; snapshots.mkdir()
    for name,b in [('freeze-v1.json',freeze_bytes),('proposed-ledger.json',ledger_bytes),('review-bundle.json',bundle_bytes),
                   ('design_batch.py',script.read_bytes()),('source-provenance.json',(DESIGN/'v3/source-provenance.json').read_bytes()),
                   ('revisions.jsonl',(DESIGN/'v3/revisions.jsonl').read_bytes()),('family-label-plan.md',(OUT.parent/'family-label-plan.md').read_bytes()),
                   ('design.md',(DESIGN/'design.md').read_bytes())]: put(snapshots/name,b,True)
    requests=[{'ordinal':0,'id':'baseline','family':'PUBLIC-DEVELOPMENT-CONTROL','role':'identity_control',
               'input':'inputs/00-baseline.xml','sha256':SEED_SHA,'bytes':len(seed),'expected':{k:'unknown' for k in rows[0]['expected']},
               'source_path':str(OUT.parent/'dev-seed-smoke-01/development-seed.xml'),'recipe':None}]
    put(OUT/requests[0]['input'],seed,True)
    for index,row in enumerate(rows,1):
        require(row['assignment']=='DEVELOPMENT' and all(v=='unknown' for v in row['expected'].values()) and row['observation'] is None and row['oracle_evaluated'] is False,'unlabelled_development')
        b=(DESIGN/'v3'/row['input']['path']).read_bytes()
        require(design['replay'](seed,row['recipe'])==b and sha(b)==row['input']['sha256'] and len(b)==row['input']['bytes'],'exact_input_replay')
        # Extra explicit original-offset nonoverlap and anchor/coordinate checks.
        previous=0
        for splice in row['recipe']['splices_against_original_seed']:
            start=splice['seed_start_byte_0based']; stop=splice['seed_end_byte_0based']
            require(previous<=start<=stop,'splice_order_nonoverlap'); previous=stop
            require(seed[:start].count(b'\n')+1==splice['seed_line_1based'],'original_line')
            require(start-(seed.rfind(b'\n',0,start)+1)+1==splice['seed_byte_column_1based'],'original_byte_column')
        path=f'inputs/{index:02d}-{row["id"]}.xml'; put(OUT/path,b,True)
        requests.append({'ordinal':index,'id':row['id'],'family':row['whole_family'],'role':row['role'],'input':path,
                         'sha256':sha(b),'bytes':len(b),'expected':row['expected'],'source_path':str(DESIGN/'v3'/row['input']['path']),
                         'recipe_sha256':row['recipe_sha256'],'revision':row['revision'],'original_ledger_row_0based':index-1})
    write(OUT/'requests.json',requests,True)
    put(OUT/'requests.tsv',''.join(f'{r["id"]}\t{r["family"]}\t{r["input"]}\t{r["sha256"]}\n' for r in requests).encode(),True)
    lock_bytes=(ROOT/'tools/oracle/artifacts.lock.json').read_bytes()
    put(snapshots/'artifacts.lock.json',lock_bytes,True)
    pins={p['file']:p for p in json.loads(lock_bytes)['artifacts']}
    artifacts={}
    # Only the original consumed runtime archive is extracted. No source/test/example dumps.
    for name in ('validator.jar','config.zip'):
        b=(CACHE/'downloads'/name).read_bytes(); pin=pins[name]
        require(sha(b)==pin['sha256'],'runtime_archive_pin')
        put(snapshots/name,b,True)
        artifacts[name]={k:pin[k] for k in ('sha256','version','url')}
    config_members=[]
    with zipfile.ZipFile(snapshots/'config.zip') as z:
        require({i.filename for i in z.infolist() if not i.is_dir()}==set(pins['config.zip']['members']),'full_original_config_member_set')
        for name,expected in pins['config.zip']['members'].items():
            b=z.read(name); require(sha(b)==expected,'original_config_member_digest')
            dest=OUT/'config'/name; require(dest.resolve().is_relative_to(OUT/'config'),'config_path')
            put(dest,b,True); config_members.append({'path':'config/'+name,'sha256':sha(b),'bytes':len(b)})
    bridge=(ROOT/'tools/oracle/java/Oracle.java').read_bytes(); require(sha(bridge)==BRIDGE_SHA,'unchanged_bridge_pin')
    put(snapshots/'Oracle.java',bridge,True)
    jdk={name:digest_file(JDK/'bin'/name) for name in ('java','javac')}
    jdk['release']=digest_file(JDK/'release'); put(snapshots/'jdk-release',(JDK/'release').read_bytes(),True)
    verification={'freeze_sha256':FREEZE_SHA,'proposal_sha256':LEDGER_SHA,'bundle_sha256':sha(bundle_bytes),
                  'bundle_members_verified':checked,'bundle_member_count':199,'recipe_synthetic_verification':replay_check,
                  'full_case_order_verified':True,'family_counts':counts,'requests':60,'all_expected_unknown':True,
                  'baseline_sha256':SEED_SHA,'inputs_readonly':True,'mutation_lineage':'unchanged copied ledger and revisions; byte replay from exact original public seed'}
    write(OUT/'preflight-verification.json',verification,True)
    prep={'python':py,'jdk':jdk,'artifacts':artifacts,'config_members':config_members,'bridge_sha256':BRIDGE_SHA,
          'cache_read_only':True,'cache_path':str(CACHE),'no_downloads':True,'source_provenance_sha256':sha((snapshots/'source-provenance.json').read_bytes()),
          'attribution':{'creator':'Sinabis GmbH','year':2026,'title':'Sinabis Green Energy – 10k GoBD-compliant German E-Invoices',
                         'revision':'8d9ab205ab74f360fac344a50fcd54c35d0cd9ba','artifact_sha256':'f45674ce80b477a859c684c68e423f8bb52ad2c1647c97a9e29050f6519dee13',
                         'original_member':'zugferd/b2b_service_maintenance/0000/0000000_SRV-2025-47731.xml','license':'CC-BY-4.0',
                         'license_url':'https://creativecommons.org/licenses/by/4.0/','modifications':'baseline unchanged; all59 exact recipes and parent byte anchors in snapshots/proposed-ledger.json; per-case revisions retained'},
          'scope':'discovery evidence for later independent artifact review; no conformance/attestation/native/resource/timing proof'}
    write(OUT/'preparation.json',prep,True)
    print(json.dumps({'preparation':'passed','requests':60,'bundle_members':199,'family_counts':counts}))

if __name__=='__main__': main()
