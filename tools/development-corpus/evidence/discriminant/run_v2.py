import json, os, platform, subprocess, sys, zipfile
from prepare import OUT, ROOT, JDK, PYTHON, sha, check, write, frozen

def main():
    os.umask(0o077)
    check(platform.python_version() == '3.12.6' and Path(sys.executable).absolute() == PYTHON,'python_pin')
    revision = json.loads((OUT/'revision-before-label.json').read_bytes())
    for n,h in revision['scripts'].items(): check(sha((OUT/n).read_bytes())==h,'revision_script_drift')
    ledger_b = (OUT/'ledger-before-label.json').read_bytes(); ledger = json.loads(ledger_b)
    check(sha(ledger_b) == (OUT/'ledger-before-label.sha256').read_text().strip(),'ledger_hash')
    for n,h in ledger['scripts'].items(): check(sha((OUT/n).read_bytes()) == h,'driver_script_drift')
    check(not (OUT/'attempt-02/processes').exists(),'fresh_process_directory')
    processes = OUT/'attempt-02/processes'; processes.mkdir()
    provenance = json.loads((OUT/'provenance.json').read_text())
    env = dict(os.environ)
    removed = []
    for n in ('JAVA_TOOL_OPTIONS','JDK_JAVA_OPTIONS','_JAVA_OPTIONS','CLASSPATH','JDK_JAVAC_OPTIONS'):
        if n in env: removed.append(n)
        env.pop(n,None)
    env['JAVA_HOME'] = str(JDK)
    outcomes = []
    def execute(name, argv, timeout):
        write(processes/(name+'.command.json'),argv)
        try:
            run = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=ROOT, timeout=timeout)
        except subprocess.TimeoutExpired as exc:
            (processes/(name+'.stdout')).write_bytes(exc.stdout or b'')
            (processes/(name+'.stderr')).write_bytes(exc.stderr or b'')
            outcome = {'process':name,'argv':argv,'exit_code':None,'status':'timeout','stdout_sha256':sha(exc.stdout or b''),'stderr_sha256':sha(exc.stderr or b''),'cwd':str(ROOT)}
            outcomes.append(outcome); write(processes/(name+'.metadata.json'),outcome)
            raise
        (processes/(name+'.stdout')).write_bytes(run.stdout)
        (processes/(name+'.stderr')).write_bytes(run.stderr)
        outcome = {'process':name,'argv':argv,'exit_code':run.returncode,'stdout_sha256':sha(run.stdout),'stderr_sha256':sha(run.stderr),'cwd':str(ROOT)}
        outcomes.append(outcome); write(processes/(name+'.metadata.json'),outcome)
        return run
    def verify_snapshot():
        for n,p in provenance['artifacts'].items(): check(sha((OUT/'snapshots'/n).read_bytes())==p['sha256'],'artifact_copy_drift')
        check(sha((OUT/'snapshots/Oracle.java').read_bytes()) == provenance['bridge_sha256'],'bridge_copy_drift')
        # Expected members and hashes come from the verified original archive snapshot,
        # not subsequently reopened cache paths.
        with zipfile.ZipFile(OUT/'snapshots/config.zip') as z:
            for info in z.infolist():
                if not info.is_dir(): check((OUT/'config'/info.filename).read_bytes()==z.read(info),'config_copy_drift')
        for n in ('java','javac'):
            check(sha((JDK/'bin'/n).read_bytes())==provenance['jdk'][n]['sha256'],'jdk_binary_drift')
        check(sha((JDK/'release').read_bytes())==provenance['jdk']['release_sha256'],'jdk_release_drift')
        check(sha(PYTHON.read_bytes())==provenance['python']['sha256'],'python_binary_drift')
    try:
        verify_snapshot()
        check(frozen() == json.loads((OUT/'frozen-before.json').read_text()),'frozen_before_drift')
        version = execute('java-version',[str(JDK/'bin/java'),'-version'],30)
        check(version.returncode==0 and b'Corretto-27.0.1.0.11' in version.stderr,'java_version')
        version = execute('javac-version',[str(JDK/'bin/javac'),'-version'],30)
        check(version.returncode==0 and b'javac 27.0.1' in version.stdout,'javac_version')
        classes = OUT/'attempt-02/classes'; classes.mkdir()
        jar = OUT/'snapshots/validator.jar'
        compile_run = execute('javac',[str(JDK/'bin/javac'),'-encoding','UTF-8','-cp',str(jar),'-d',str(classes),str(OUT/'snapshots/Oracle.java'),str(OUT/'DiscriminantDriverV2.java')],60)
        check(compile_run.returncode==0,'compile_failed')
        # Runtime closure includes every original config member, jar and actual compiled classes.
        closure_paths = [OUT/'snapshots/validator.jar', OUT/'snapshots/Oracle.java', OUT/'DiscriminantDriverV2.java']
        closure_paths += sorted((OUT/'config').rglob('*'))
        closure_paths += sorted(classes.rglob('*.class'))
        closure_paths = [p for p in closure_paths if p.is_file()]
        for p in classes.rglob('*.class'): p.chmod(0o400)
        closure = ''.join(sha(p.read_bytes())+'\t'+str(p.relative_to(OUT))+'\n' for p in closure_paths)
        (OUT/'attempt-02/runtime-closure.tsv').write_text(closure); (OUT/'attempt-02/runtime-closure.tsv').chmod(0o400)
        closure_hash = sha((OUT/'attempt-02/runtime-closure.tsv').read_bytes())
        observations = []
        for recipe in ledger['recipes']:
            name = recipe['case']; inp = OUT/recipe['input']
            check(sha(inp.read_bytes())==recipe['input_sha256'],'input_drift')
            command = [str(JDK/'bin/java'),'-Xms256m','-Xmx1024m','-XX:ActiveProcessorCount=1','-XX:+UseSerialGC','-Dorg.slf4j.simpleLogger.defaultLogLevel=error',
              '-cp',os.pathsep.join([str(classes),str(jar)]),'DiscriminantDriverV2',str(OUT),'attempt-02/runtime-closure.tsv',closure_hash,
              'bsc10-development-'+name,recipe['input'],recipe['input_sha256'],'attempt-02/processes/'+name+'.receipt.json',str(JDK)]
            run = execute(name,command,180)
            if run.returncode != 0:
                observations.append({'case':name,'status':'process_failed','exit_code':run.returncode})
                continue
            records = [json.loads(line) for line in run.stdout.splitlines() if line.startswith(b'{')]
            check(len(records)==1,'record_cardinality')
            record = records[0]
            check(record['sha256']==recipe['input_sha256'] and record['id']=='bsc10-development-'+name,'record_binding')
            receipt = json.loads((processes/(name+'.receipt.json')).read_text())
            check(receipt['closure_pre_post_verified'] and receipt['input_sha256']==record['sha256'],'receipt_binding')
            observations.append({'case':name,'status':'observed','record':record,'receipt':receipt})
            write(OUT/'attempt-02/observations.json',observations)
            print(json.dumps({'case':name,**{k:record[k] for k in ('processing','schema','stages','acceptance','findings','structural_findings')}}),flush=True)
        verify_snapshot()
        write(OUT/'attempt-02/observations.json',observations)
        check(all(o['status']=='observed' for o in observations),'one_or_more_processes_failed')
    finally:
        write(OUT/'attempt-02/frozen-after.json',frozen())
        write(OUT/'attempt-02/run-metadata.json',{'ledger_sha256':sha(ledger_b),'script_sha256':sha((OUT/'run_v2.py').read_bytes()),'python':provenance['python'],'removed_environment_override_names':removed,'java_home':str(JDK),'processes':outcomes,'timing_measurements':False,'benchmark_loops':False})

from pathlib import Path
if __name__ == '__main__': main()
