import hashlib, json, os, platform, sys
from pathlib import Path

OUT = Path(__file__).resolve().parent
ROOT = OUT.parents[3]
DESIGN = OUT.parent/'family-design'
CACHE = Path('/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache')
PYTHON = CACHE/'test-venv/bin/python'
JDK = Path('/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home')
FREEZE_SHA = 'e2d8275974c59a96a6dff1b000a684d75abd2dc61f3e1331e1d593c1a228c2ae'
LEDGER_SHA = '60ae574a2908a3f8b40e79bf1dde1b37d116cd7809814623a4a72408f2baf7eb'
SEED_SHA = '881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c'
BRIDGE_SHA = '37565bfaba63f452018596d422a65a7d969f06245bbe33a134e95f92fe2cad3e'

def sha(b): return hashlib.sha256(b).hexdigest()
def require(condition, message):
    if not condition: raise RuntimeError(message)
def write(path, obj, readonly=False):
    data=(json.dumps(obj,ensure_ascii=True,indent=2)+'\n').encode()
    put(path,data,readonly)
def put(path,data,readonly=False):
    path=Path(path)
    require(path.resolve().is_relative_to(OUT),'write_outside_owned_scratch')
    path.parent.mkdir(parents=True,exist_ok=True)
    with path.open('xb') as f: f.write(data)
    if readonly: path.chmod(0o400)
def digest_file(path):
    b=path.read_bytes()
    return {'path':str(path),'realpath':str(path.resolve()),'bytes':len(b),'sha256':sha(b)}
def interpreter():
    require(platform.python_version()=='3.12.6' and Path(sys.executable).absolute()==PYTHON,'python_pin')
    return {**digest_file(PYTHON),'version':platform.python_version()}
def frozen():
    path=ROOT/'tools/conformance/evidence.lock.json'
    b=path.read_bytes(); lock=json.loads(b)
    require(len(lock['files'])==39 and lock['reviewed_revision']=='87e0a0a0c7095412faf23efad7aa8f0710328d03','frozen_anchor')
    verified=[]
    for e in lock['files']:
        data=(ROOT/e['path']).read_bytes()  # Opaque bytes only; never deserialize labels.
        require(len(data)==e['bytes'] and sha(data)==e['sha256'],'frozen_file_digest')
        verified.append({'path':e['path'],'bytes':len(data),'sha256':sha(data)})
    return {'status':'39_opaque_file_digests_verified','reviewed_revision':lock['reviewed_revision'],
            'lock_sha256':sha(b),'files':verified,'label_deserialization':False}
