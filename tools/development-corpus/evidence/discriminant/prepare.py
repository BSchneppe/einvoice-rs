import hashlib, io, json, os, platform, sys, zipfile
from pathlib import Path
import xml.etree.ElementTree as ET

OUT = Path(__file__).resolve().parent
ROOT = OUT.parents[3]
CACHE = Path('/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache')
PYTHON = CACHE / 'test-venv/bin/python'
JDK = Path('/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home')
SEED_HASH = '881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c'
def sha(b): return hashlib.sha256(b).hexdigest()
def check(ok, code):
    if not ok: raise RuntimeError(code)
def write(p, obj): p.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + '\n')
def frozen():
    b = (ROOT/'tools/conformance/evidence.lock.json').read_bytes()
    lock = json.loads(b)
    check(len(lock['files']) == 39 and lock['reviewed_revision'] == '87e0a0a0c7095412faf23efad7aa8f0710328d03', 'frozen_anchor')
    for e in lock['files']:
        data = (ROOT/e['path']).read_bytes()
        check(len(data) == e['bytes'] and sha(data) == e['sha256'], 'frozen_integrity')
    return {'files':39, 'status':'opaque_bytes_verified', 'lock_sha256':sha(b)}

def main():
    os.umask(0o077)
    check(platform.python_version() == '3.12.6' and Path(sys.executable).absolute() == PYTHON, 'python_pin')
    check(not (OUT/'snapshots').exists(), 'fresh_output_required')
    write(OUT/'frozen-before.json', frozen())
    lock_b = (ROOT/'tools/oracle/artifacts.lock.json').read_bytes()
    pins = {p['file']:p for p in json.loads(lock_b)['artifacts']}
    snapshots = OUT/'snapshots'; snapshots.mkdir()
    provenance = {'artifact_lock_sha256':sha(lock_b), 'artifacts':{}, 'selected_source_members':{}, 'python':{'invoked':str(PYTHON),'resolved':str(PYTHON.resolve()),'version':platform.python_version(),'sha256':sha(PYTHON.read_bytes())}, 'jdk':{}}
    source_allowlist = {
      'cen.zip': ['schematron/preprocessed/EN16931-CII-validation-preprocessed.sch', 'schematron/CII/EN16931-CII-syntax.sch', 'schematron/abstract/EN16931-CII-syntax.sch'],
    }
    for name in ('validator.jar','config.zip','cen.zip'):
        b = (CACHE/'downloads'/name).read_bytes(); p = pins[name]
        check(sha(b) == p['sha256'], 'artifact_hash')
        dest = snapshots/name; dest.write_bytes(b); dest.chmod(0o400)
        provenance['artifacts'][name] = {k:p[k] for k in ('sha256','version','url')}
        if name.endswith('.zip'):
            with zipfile.ZipFile(io.BytesIO(b)) as z:
                check({n for n in z.namelist() if not n.endswith('/')} == set(p['members']), 'archive_member_set')
                names = list(p['members']) if name == 'config.zip' else source_allowlist[name]
                for member in names:
                    data = z.read(member); check(sha(data) == p['members'][member], 'member_hash')
                    target = (OUT/'config'/member) if name == 'config.zip' else (OUT/'source'/member)
                    check(target.resolve().is_relative_to(OUT), 'member_path')
                    target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(data); target.chmod(0o400)
                    if name != 'config.zip': provenance['selected_source_members'][member] = sha(data)
    bridge = (ROOT/'tools/oracle/java/Oracle.java').read_bytes()
    check(sha(bridge) == '37565bfaba63f452018596d422a65a7d969f06245bbe33a134e95f92fe2cad3e','bridge_pin')
    (snapshots/'Oracle.java').write_bytes(bridge); (snapshots/'Oracle.java').chmod(0o400)
    provenance['bridge_sha256'] = sha(bridge)
    for n in ('java','javac'):
        path = JDK/'bin'/n
        provenance['jdk'][n] = {'invoked':str(path),'resolved':str(path.resolve()),'sha256':sha(path.read_bytes())}
    provenance['jdk']['release_sha256'] = sha((JDK/'release').read_bytes())
    seed = (OUT.parent/'dev-seed-smoke-01/development-seed.xml').read_bytes()
    check(sha(seed) == SEED_HASH, 'seed_hash')
    (snapshots/'development-seed.xml').write_bytes(seed); (snapshots/'development-seed.xml').chmod(0o400)
    provenance['seed'] = {'parent_path':str(OUT.parent/'dev-seed-smoke-01/development-seed.xml'), 'sha256':sha(seed),'bytes':len(seed),'source':'Sinabis GmbH (2026), Sinabis Green Energy – 10k GoBD-compliant German E-Invoices','revision':'8d9ab205ab74f360fac344a50fcd54c35d0cd9ba','artifact_sha256':'f45674ce80b477a859c684c68e423f8bb52ad2c1647c97a9e29050f6519dee13','source_path':'zugferd/b2b_service_maintenance/0000/0000000_SRV-2025-47731.xml','license':'CC-BY-4.0','modifications':'Only explicit inserted byte sequences recorded in ledger; baseline unchanged.'}
    schema = OUT/'config/resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd'
    lines = schema.read_text().splitlines()
    sections = [(311,341),(909,915),(925,946),(1029,1045)]
    inspection = ['Pinned XSD SHA-256 '+sha(schema.read_bytes())]
    for lo,hi in sections:
        inspection.extend(f'{i}: {lines[i-1]}' for i in range(lo,hi+1))
    ram = '{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}'
    agreements = list(ET.fromstring(seed).iter(ram+'ApplicableHeaderTradeAgreement'))
    check(len(agreements)==1, 'agreement_cardinality')
    inspection.append('Development seed agreement child expanded names: '+json.dumps([e.tag for e in agreements[0]]))
    (OUT/'schema-inspection.txt').write_text('\n'.join(inspection)+'\n')
    write(OUT/'provenance.json',provenance)
    print((OUT/'schema-inspection.txt').read_text())

if __name__ == '__main__': main()
