#!/usr/bin/env python3
"""Pinned development oracle. All network access is explicit in `fetch`."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parent
CACHE = ROOT / '.cache'
REVISION = '8d9ab205ab74f360fac344a50fcd54c35d0cd9ba'
NS = {'s': 'http://www.xoev.de/de/validator/framework/1/scenarios'}
THEMES = {
    'b2b_service_maintenance': 'development',
    'b2b_hardware_delivery': 'development',
    'municipal_epc_milestone': 'held_out',
    'utility_energy_feedin': 'development',
    'reverse_charge_subcontractor': 'held_out',
    'credit_note_correction': 'held_out',
}
FAMILIES = {
    'extra-attribute': 'development', 'unknown-element': 'held_out',
    'duplicate-context': 'held_out', 'invalid-decimal': 'development',
    'missing-line-total': 'development', 'equivalent-prefix': 'development',
    'wrong-namespace': 'held_out', 'malformed-xml': 'held_out',
    'large-text': 'held_out', 'deep-unknown': 'held_out',
    'severity-override': 'development', 'severity-escalation': 'held_out',
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic_text(path, text):
    path=Path(path)
    with tempfile.NamedTemporaryFile(mode='w',encoding='utf-8',dir=path.parent,delete=False) as temporary:
        temporary.write(text)
        name=temporary.name
    try:
        os.replace(name,path)
    finally:
        Path(name).unlink(missing_ok=True)


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    atomic_text(path, json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def jsonl(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line]


def write_jsonl(path, rows):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    atomic_text(path, ''.join(json.dumps(row, ensure_ascii=False, separators=(',', ':'))+'\n' for row in rows))


def pins():
    return read_json(ROOT / 'artifacts.lock.json')['artifacts']


def verified_artifact(pin):
    path = CACHE / 'downloads' / pin['file']
    if not path.exists() or digest(path.read_bytes()) != pin['sha256']:
        raise ValueError(f"Missing or corrupt artifact {path}; run fetch (never trust a partial download)")
    return path


def fetch():
    for pin in pins():
        path = CACHE / 'downloads' / pin['file']
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists():
            verified_artifact(pin)
            continue
        temporary = path.with_suffix(path.suffix+'.partial')
        try:
            with urllib.request.urlopen(pin['url'], timeout=120) as source, temporary.open('wb') as target:
                shutil.copyfileobj(source, target)
            if digest(temporary.read_bytes()) != pin['sha256']:
                raise ValueError(f"Checksum mismatch: {pin['file']}")
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)
    print('All pinned downloads verified')


def prepare():
    """Rebuild derived artifacts only from verified archives, never modify the originals."""
    for pin in pins():
        path = verified_artifact(pin)
        if 'members' not in pin:
            continue
        destination = CACHE / pin['file'].removesuffix('.zip')
        with zipfile.ZipFile(path) as archive:
            actual = {n for n in archive.namelist() if not n.endswith('/')}
            if actual != set(pin['members']):
                raise ValueError('Archive member inventory changed')
            for name, sha in pin['members'].items():
                target = destination / name
                if not target.resolve().is_relative_to(destination.resolve()):
                    raise ValueError('Unsafe archive path')
                data = archive.read(name)
                if digest(data) != sha:
                    raise ValueError(f'Archive member checksum mismatch: {name}')
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
    config = CACHE / 'config'
    # Supported report customization: override only the named HTML template. The
    # assessment, severity overrides, all validation stages and match remain upstream.
    tree = ET.parse(config / 'scenarios.xml')
    report_locations = tree.findall('.//s:createReport/s:resource/s:location', NS)
    report_locations += tree.findall('.//s:noScenarioReport/s:resource/s:location', NS)
    for location in report_locations:
        original = location.text
        wrapper = 'no-html-' + Path(original).name
        (config / wrapper).write_text(
            '<xsl:stylesheet version="2.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform" '
            'xmlns:html="http://www.w3.org/1999/xhtml">'
            f'<xsl:import href="{original}"/>'
            '<xsl:template name="html:html"/></xsl:stylesheet>\n')
        location.text = wrapper
    tree.write(config / 'scenarios-no-html.xml', encoding='utf-8', xml_declaration=True)
    classes = CACHE / 'classes'
    classes.mkdir(exist_ok=True)
    subprocess.run(['javac', '-cp', str(CACHE/'downloads/validator.jar'), '-d', str(classes),
                    *[str(p) for p in sorted((ROOT/'java').glob('*.java'))]], check=True)


def mutate(data, family):
    """Byte-oriented, deterministic edits; labels are never inferred here."""
    def replace(pattern, replacement):
        result, count = re.subn(pattern, replacement, data, count=1, flags=re.DOTALL)
        if count != 1:
            raise ValueError(f'Mutation anchor missing: {family}')
        return result
    if family == 'extra-attribute':
        return replace(b'<rsm:CrossIndustryInvoice', b'<rsm:CrossIndustryInvoice unexpected="1"')
    if family == 'unknown-element':
        return replace(b'</rsm:CrossIndustryInvoice>', b'<ram:Unknown>1</ram:Unknown></rsm:CrossIndustryInvoice>')
    if family == 'duplicate-context':
        return replace(b'(<rsm:ExchangedDocumentContext>.*?</rsm:ExchangedDocumentContext>)', rb'\1\1')
    if family == 'invalid-decimal':
        return replace(b'(<ram:ChargeAmount>)[^<]+', rb'\g<1>not-a-number')
    if family == 'missing-line-total':
        return replace(b'<ram:LineTotalAmount>[^<]*</ram:LineTotalAmount>', b'')
    if family == 'equivalent-prefix':
        return data.replace(b'rsm:', b'invoice:').replace(b'xmlns:rsm=', b'xmlns:invoice=')
    if family == 'wrong-namespace':
        return data.replace(b'urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100', b'urn:wrong')
    if family == 'malformed-xml':
        return data[:-50]
    if family == 'large-text':
        return replace(b'(<ram:Name>)[^<]+', rb'\g<1>'+b'A'*262144)
    if family == 'deep-unknown':
        return replace(b'</rsm:CrossIndustryInvoice>', b'<ram:Unknown>'*128+b'</ram:Unknown>'*128+b'</rsm:CrossIndustryInvoice>')
    if family == 'severity-override':
        return replace(b'unitCode="[^"]+"', b'unitCode="INVALID"')
    if family == 'severity-escalation':
        return replace(b'(<ram:SpecifiedTradePaymentTerms>.*?</ram:SpecifiedTradePaymentTerms>)', rb'\1\1')
    raise ValueError(f'Unknown mutation family {family}')


def corpus():
    import pyarrow.parquet as pq  # Import-only dependency; not needed for validation.
    prepare()
    pin = next(p for p in pins() if p['file']=='sinabis.parquet')
    table = pq.read_table(verified_artifact(pin), columns=['idx','xml_file','xml','sha256_xml','theme'])
    rows, representatives = [], {}
    for item in table.to_pylist():
        data = item['xml'].encode('utf-8')
        if digest(data) != item['sha256_xml']:
            raise ValueError(f"Dataset XML hash mismatch: {item['idx']}")
        theme = item['theme']
        destination = CACHE / 'corpus/sinabis' / f"{item['idx']:05d}.xml"
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        row = dict(id=f"sinabis-{item['idx']:05d}",path=str(destination.relative_to(ROOT)),
                   sha256=digest(data),bytes=len(data),source='sinabis.parquet',revision=REVISION,
                   source_path=item['xml_file'],scope='complete',scenario=theme,
                   family='original',split=THEMES[theme],group=theme)
        rows.append(row)
        if theme not in representatives:
            representatives[theme] = row
    if len(rows) != 10000 or set(representatives) != set(THEMES):
        raise ValueError('Dataset shape changed')
    selected = list(representatives.values())
    # Preserve the twelve original discovery examples for probe reproduction.
    selected += [r for r in rows[:12] if r not in selected]
    for path in sorted((CACHE/'cen/examples').glob('*.xml')):
        data = path.read_bytes()
        row = dict(id='official-'+path.stem,path=str(path.relative_to(ROOT)),sha256=digest(data),bytes=len(data),
                   source='cen.zip',source_path='examples/'+path.name,scope='complete',
                   scenario='official-cen',family=path.stem,split='held_out',group='official-complete')
        selected.append(row)
    source = CACHE/'cen-source/eInvoicing-EN16931-validation-1.3.16'
    for path in sorted((source/'test/cii').glob('*.xml')):
        # Keep the ORIGINAL testSet container, its assertions and all fragments.
        data = path.read_bytes()
        selected.append(dict(id='scoped-'+path.stem,path=str(path.relative_to(ROOT)),sha256=digest(data),bytes=len(data),
            source='cen-source.zip',source_path=str(path.relative_to(source)),scope='rule_test_set',
            rule_ids=sorted({e.text for e in ET.fromstring(data).iter() if e.tag.endswith('}scope')}),
            scenario='official-rule-scoped',family=path.stem,split='held_out',group='rule-scoped'))
    base = rows[0]
    for family, split in FAMILIES.items():
        data = mutate((ROOT/base['path']).read_bytes(), family)
        path = CACHE/'corpus/mutations'/f'{family}.xml'
        path.parent.mkdir(parents=True,exist_ok=True)
        path.write_bytes(data)
        selected.append(dict(id='mutation-'+family,path=str(path.relative_to(ROOT)),sha256=digest(data),bytes=len(data),
            source='local-mutation-v1',parent=base['id'],parent_sha256=base['sha256'],scope='complete',
            scenario=base['scenario'],family=family,split=split,group='mutation-'+family))
    write_jsonl(ROOT/'corpus/sinabis.jsonl',rows)
    write_jsonl(ROOT/'corpus/initial.jsonl',selected)
    write_json(ROOT/'corpus/split.json',dict(format=1,themes=THEMES,mutation_families=FAMILIES,
        policy='Whole themes and mutation families, never random adjacent rows. Official examples and rule families held out. Shared mutation seed is public; no held-out label-guided optimization.',
        dataset_rows=len(rows),initial_cases=len(selected)))
    # Equal workload-group weighting, fixed before measuring. Rule test containers
    # are not invoices and therefore never enter full-validation throughput.
    groups=sorted({r['group'] for r in selected if r['scope']=='complete'})
    write_json(ROOT/'workload.json',dict(format=1,manifest='corpus/initial.jsonl',
        groups={g:1/len(groups) for g in groups},warmup_passes=20,runs=5,repeats=10,
        aggregation='1 / sum(group_weight * mean_latency_seconds); report every group and run'))
    print(f'Imported {len(rows)} standalone XML files; initial corpus: {len(selected)} cases')


def run_java(rows, html=False, warmup=0, runs=0, repeats=1, timeout=1800):
    for row in rows:
        path = ROOT/row['path']
        if digest(path.read_bytes()) != row['sha256']:
            raise ValueError(f"Corpus hash mismatch: {row['id']}")
    CACHE.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=CACHE) as directory:
        listing = Path(directory)/'inputs.tsv'
        listing.write_text(''.join(f"{r['id']}\t{r['group']}\t{ROOT/r['path']}\t{r['sha256']}\n" for r in rows))
        command=['java','-Xms256m','-Xmx1024m','-XX:ActiveProcessorCount=1','-XX:+UseSerialGC',
                 '-Dorg.slf4j.simpleLogger.defaultLogLevel=error','-cp',
                 os.pathsep.join([str(CACHE/'classes'),str(CACHE/'downloads/validator.jar')]),'Oracle',
                 str(CACHE/'config'/('scenarios.xml' if html else 'scenarios-no-html.xml')),str(listing),
                 str(warmup),str(runs),str(repeats)]
        start=time.perf_counter_ns()
        result=subprocess.run(command,capture_output=True,text=True,timeout=timeout)
        if result.returncode:
            raise RuntimeError(f'Oracle failed ({result.returncode}): {result.stderr[-8000:]}')
        output=[json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
        outcomes=[o for o in output if 'id' in o]
        if [o['id'] for o in outcomes] != [r['id'] for r in rows]:
            raise RuntimeError('Oracle output missing, duplicated or reordered documents')
        html_counts={o['id']:o.pop('report_html_elements') for o in outcomes}
        if not html and any(count != 0 for count in html_counts.values()):
            raise RuntimeError('Primary reference report rendered HTML or could not verify its absence')
        for outcome,row in zip(outcomes, rows):
            outcome.update(scope=row['scope'],split=row['split'])
        meta=[o for o in output if o.get('type')=='metadata']
        if len(meta)!=1:
            raise RuntimeError('Missing oracle completion metadata')
        meta[0].update(process_wall_ns=time.perf_counter_ns()-start,html=html,
                       report_html_element_counts=html_counts,
                       artifact_lock_sha256=digest((ROOT/'artifacts.lock.json').read_bytes()),
                       bridge_sha256=digest((ROOT/'java/Oracle.java').read_bytes()))
        return outcomes,[o for o in output if o.get('type')=='benchmark_run'],meta[0]


def select(manifest, ids=None):
    rows=[r for r in jsonl(manifest) if r['scope']=='complete']
    if ids:
        rows=[r for r in rows if r['id'] in ids]
        if {r['id'] for r in rows} != set(ids):
            raise ValueError('Unknown or rule-scoped ID selected for full validation')
    if not rows:
        raise ValueError('Empty workload')
    return rows


def label(args):
    prepare()
    rows=select(args.manifest,args.ids)
    try:
        outcomes,_,metadata=run_java(rows,html=args.html,timeout=args.timeout)
    except (RuntimeError,OSError,subprocess.SubprocessError) as error:
        # A killed/timed-out engine has no completed document evaluation. Never
        # preserve stale successful output at the requested destination.
        outcomes=[dict(format=1,id=r['id'],sha256=r['sha256'],scope=r['scope'],split=r['split'],
            processing='failed',processing_errors=[str(error)],schema='not_evaluated',business='not_evaluated',
            findings=[],structural_findings=[],acceptance='indeterminate',reference_acceptance='undefined') for r in rows]
        write_jsonl(args.output,outcomes)
        write_json(str(args.output)+'.metadata.json',dict(type='run_failure',processing='failed',error=str(error)))
        return 2
    write_jsonl(args.output,outcomes)
    write_json(str(args.output)+'.metadata.json',metadata)
    print(f'Wrote {len(outcomes)} reference outcomes to {args.output}')
    return 2 if any(o['processing']=='failed' for o in outcomes) else 0


def benchmark(args):
    prepare()
    config=read_json(ROOT/'workload.json')
    rows=select(ROOT/config['manifest'])
    # Full HTML is an independent validation pass; require parity before timing.
    plain,_,_=run_java(rows)
    full,_,full_meta=run_java(rows,html=True)
    if plain != full:
        raise ValueError('HTML suppression changed reference semantics')
    warm=args.warmup if args.warmup is not None else config['warmup_passes']
    runs=args.runs if args.runs is not None else config['runs']
    repeats=args.repeats if args.repeats is not None else config['repeats']
    if min(warm,runs,repeats)<1:
        raise ValueError('Warmup, runs and repeats must be positive')
    _,measurements,metadata=run_java(rows,warmup=warm,runs=runs,repeats=repeats,timeout=args.timeout)
    _,html_measurements,html_metadata=run_java(rows,html=True,warmup=warm,runs=runs,repeats=repeats,timeout=args.timeout)
    def summarize(measures):
        result=[]
        for run in measures:
            groups={}
            for group,samples in run['latencies_ns'].items():
                samples=sorted(samples)
                groups[group]=dict(count=len(samples),mean_ns=statistics.mean(samples),
                    p50_ns=statistics.median(samples),p95_ns=samples[min(len(samples)-1,int(len(samples)*.95))])
            if set(groups)!=set(config['groups']):
                raise ValueError('Benchmark workload groups incomplete')
            mean=sum(config['groups'][g]*v['mean_ns'] for g,v in groups.items())
            result.append(dict(run=run['run'],groups=groups,weighted_documents_per_second=1e9/mean))
        return result
    write_json(args.output,dict(format=1,kind='reference_only_no_native_speedup_claim',
        host=platform.platform(),machine_label=args.machine,cpu_allocation=args.cpu_allocation,
        affinity_enforced_by_harness=False,workers=1,workload_sha256=digest((ROOT/'workload.json').read_bytes()),
        manifest_sha256=digest((ROOT/config['manifest']).read_bytes()),weights=config['groups'],
        primary=summarize(measurements),full_html=summarize(html_measurements),
        observed_outcomes={o['id']:dict(processing=o['processing'],schema=o['schema'],business=o['business'],acceptance=o['acceptance']) for o in plain},
        metadata=metadata,full_html_metadata=html_metadata,parity_full_metadata=full_meta,
        raw_measurements=measurements,raw_html_measurements=html_measurements,
        release_gate_eligible=False,
        limits='Reference infrastructure only. No native implementation/diagnostic parity or 5x gate proven. CPU affinity must be imposed externally; ActiveProcessorCount is not an OS CPU quota.'))
    print(f'Wrote repeated reference benchmark to {args.output}; no speedup claim')


def scoped(args):
    prepare()
    rows=[r for r in jsonl(ROOT/'corpus/initial.jsonl') if r['scope']=='rule_test_set']
    for row in rows:
        if digest((ROOT/row['path']).read_bytes()) != row['sha256']:
            raise ValueError('Rule fixture hash mismatch')
    cp=os.pathsep.join([str(CACHE/'classes'),str(CACHE/'downloads/validator.jar')])
    result=subprocess.run(['java','-cp',cp,'ScopedOracle',
        str(CACHE/'config/resources/cii/16b/xsl/EN16931-CII-validation.xsl'),
        *[str(ROOT/r['path']) for r in rows]],check=True,capture_output=True,text=True,timeout=120)
    outcomes=[json.loads(line) for line in result.stdout.splitlines()]
    if not outcomes:
        raise ValueError('No scoped outcomes')
    write_jsonl(args.output,outcomes)
    print(f'Wrote {len(outcomes)} scoped outcomes; no global validity labels')
    return 0 if all(o['processing']=='completed' and o['expectations_met'] for o in outcomes) else 2


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    for name in ['fetch','prepare','corpus']:
        sub.add_parser(name)
    p=sub.add_parser('scoped')
    p.add_argument('--output',type=Path,required=True)
    p=sub.add_parser('label')
    p.add_argument('--manifest',type=Path,default=ROOT/'corpus/initial.jsonl')
    p.add_argument('--ids',nargs='+')
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--html',action='store_true')
    p.add_argument('--timeout',type=int,default=1800)
    b=sub.add_parser('benchmark')
    b.add_argument('--output',type=Path,required=True)
    b.add_argument('--machine',required=True)
    b.add_argument('--cpu-allocation',required=True,help='Describe externally imposed CPU allocation (not merely JVM count)')
    for option in ['warmup','runs','repeats']:
        b.add_argument('--'+option,type=int)
    b.add_argument('--timeout',type=int,default=1800)
    args=parser.parse_args()
    # A failed new run must not leave the old destination looking successful.
    if hasattr(args,'output'):
        args.output.unlink(missing_ok=True)
        Path(str(args.output)+'.metadata.json').unlink(missing_ok=True)
    try:
        if args.command=='label':return label(args)
        if args.command=='scoped':return scoped(args)
        if args.command=='benchmark':benchmark(args)
        else:globals()[args.command]()
        return 0
    except (ValueError,RuntimeError,OSError,subprocess.SubprocessError) as error:
        print(json.dumps(dict(type='infrastructure_failure',processing='failed',acceptance='indeterminate',error=str(error))),file=sys.stderr)
        return 2


if __name__=='__main__':
    sys.exit(main())
