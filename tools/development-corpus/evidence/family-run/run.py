import json, os, subprocess, traceback, zipfile
from common import *

def main():
    os.umask(0o077)
    py=interpreter(); prep=json.loads((OUT/'preparation.json').read_bytes())
    require(not (OUT/'processes').exists(),'fresh_single_attempt_required')
    (OUT/'processes').mkdir(); (OUT/'raw').mkdir(); (OUT/'classes').mkdir()
    requests=json.loads((OUT/'requests.json').read_bytes())
    require(len(requests)==60,'requested_count')
    env=dict(os.environ); removed=[]
    for name in ('JAVA_TOOL_OPTIONS','JDK_JAVA_OPTIONS','_JAVA_OPTIONS','CLASSPATH','JDK_JAVAC_OPTIONS'):
        if name in env: removed.append(name)
        env.pop(name,None)
    env['JAVA_HOME']=str(JDK)
    classes=OUT/'classes'; jar=OUT/'snapshots/validator.jar'
    compile_command=[str(JDK/'bin/javac'),'-encoding','UTF-8','-cp',str(jar),'-d',str(classes),str(OUT/'snapshots/Oracle.java'),str(OUT/'FamilyDriver.java')]
    java_prefix=[str(JDK/'bin/java'),'-Xms256m','-Xmx1024m','-XX:ActiveProcessorCount=1','-XX:+UseSerialGC',
                 '-Dorg.slf4j.simpleLogger.defaultLogLevel=error','-cp',os.pathsep.join([str(classes),str(jar)]),'FamilyDriver',str(OUT)]
    declare={'format':'bsc10-family-execution-declaration-v1','attempt':1,'no_automatic_reruns':True,
             'authorization':'https://linear.app/bschneppe/issue/BSC-10/prove-corpus-coverage-and-adversarial-validation-behavior#comment-adb2df42',
             'freeze_sha256':FREEZE_SHA,'ledger_sha256':LEDGER_SHA,'requests_tsv_sha256':sha((OUT/'requests.tsv').read_bytes()),
             'requests_json_sha256':sha((OUT/'requests.json').read_bytes()),'requested_count':60,'order':'baseline first, then exact59 frozen ledger order',
             'all_expected_unknown':True,'python':py,'jdk':prep['jdk'],'cwd':str(ROOT),'shell':'/bin/bash','login':False,
             'scripts':{p.name:sha(p.read_bytes()) for p in (OUT/'common.py',OUT/'prepare_run.py',OUT/'run.py',OUT/'summarize.py',OUT/'FamilyDriver.java')},
             'java_version_command':[str(JDK/'bin/java'),'-version'],'javac_version_command':[str(JDK/'bin/javac'),'-version'],
             'compile_command':compile_command,'evaluation_command_prefix':java_prefix,
             'evaluation_command_suffix_contract':['runtime-closure.tsv SHA256 (after compile, bound before launch)',
                  'requests.tsv SHA256',FREEZE_SHA,LEDGER_SHA,str(JDK)],
             'process_timeouts_seconds':{'java-version':30,'javac-version':30,'compile':60,'evaluate60':600},
             'removed_environment_override_names':removed,'environment_changes':{'JAVA_HOME':str(JDK)},
             'passes':1,'configuration_loads':1,'warmup':False,'timing_measurements':False,'benchmarks':False,
             'expected_assertions':'binding/count/contracts/stage consistency only; no verdict expectations',
             'failure_policy':'retain partial raw records; all missing requests explicit; nonzero/timeout incomplete even if all60 observed',
             'inputs':'read-only originals copied without XML serialization','requested_rows':requests}
    write(OUT/'execution-declaration.json',declare,True)
    put(OUT/'declared-commands.txt',('\n'.join([
        'Pinned Python launch: '+str(PYTHON)+' -B '+str(OUT/'run.py'),
        'Commands below are structured argv in execution-declaration.json; final evaluation argv bound in processes/evaluate60.command.json before launch.',
        json.dumps(declare['java_version_command']),json.dumps(declare['javac_version_command']),json.dumps(compile_command),
        json.dumps(java_prefix)+' + declared suffix',
        'No oracle.py main/prepare/label, no downloads, no warmup, no benchmark, no native adapter, no rerun.'
    ])+'\n').encode(),True)
    processes=[]; failures=[]
    def execute(name,argv,timeout):
        write(OUT/'processes'/f'{name}.command.json',{'argv':argv,'cwd':str(ROOT),'timeout_seconds':timeout},True)
        status={'name':name,'argv':argv,'cwd':str(ROOT),'exit_code':None,'status':'launch_error'}
        stdout=b''; stderr=b''
        try:
            result=subprocess.run(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,cwd=ROOT,timeout=timeout)
            stdout=result.stdout; stderr=result.stderr
            status.update(exit_code=result.returncode,status='exited')
        except subprocess.TimeoutExpired as exc:
            stdout=exc.stdout or b''; stderr=exc.stderr or b''; status['status']='timeout'
        except Exception as exc:
            status['launch_error']=repr(exc)
        put(OUT/'processes'/f'{name}.stdout',stdout,True); put(OUT/'processes'/f'{name}.stderr',stderr,True)
        status.update(stdout_sha256=sha(stdout),stderr_sha256=sha(stderr))
        write(OUT/'processes'/f'{name}.metadata.json',status,True); processes.append(status)
        require(status['status']=='exited' and status['exit_code']==0,f'{name}_process_unsuccessful')
        return stdout,stderr
    def verify():
        require(interpreter()==prep['python'],'python_binary_drift')
        for name in ('java','javac'):
            require(digest_file(JDK/'bin'/name)==prep['jdk'][name],'jdk_binary_drift')
        require(digest_file(JDK/'release')==prep['jdk']['release'],'jdk_release_drift')
        for name,pin in prep['artifacts'].items(): require(sha((OUT/'snapshots'/name).read_bytes())==pin['sha256'],'archive_snapshot_drift')
        require(sha((OUT/'snapshots/Oracle.java').read_bytes())==BRIDGE_SHA,'bridge_drift')
        with zipfile.ZipFile(OUT/'snapshots/config.zip') as z:
            for e in prep['config_members']:
                require((OUT/e['path']).read_bytes()==z.read(e['path'][len('config/'):]),'consumed_config_snapshot_drift')
        for r in requests:
            require(sha((OUT/r['input']).read_bytes())==r['sha256'] and (OUT/r['input']).stat().st_mode & 0o222 == 0,'readonly_input_drift')
        for name,h in declare['scripts'].items(): require(sha((OUT/name).read_bytes())==h,'declared_script_drift')
        require(sha((DESIGN/'freeze-v1.json').read_bytes())==FREEZE_SHA and sha((DESIGN/'v3/proposed-ledger.json').read_bytes())==LEDGER_SHA,'original_freeze_ledger_drift')
        freeze=json.loads((OUT/'snapshots/freeze-v1.json').read_bytes())
        bundle_bytes=(DESIGN/'review-bundle.json').read_bytes()
        require(sha(bundle_bytes)==freeze['review_bundle_sha256'],'original_bundle_drift')
        for e in json.loads(bundle_bytes)['files']:
            b=(DESIGN/e['path']).read_bytes()
            require(sha(b)==e['sha256'] and len(b)==e['bytes'],'original199_bundle_members_drift')
    try:
        verify(); require(frozen()==json.loads((OUT/'frozen-before.json').read_bytes()),'before_frozen_drift')
        stdout,stderr=execute('java-version',declare['java_version_command'],30)
        require(b'Corretto-27.0.1.0.11' in stderr,'java_version_pin')
        stdout,stderr=execute('javac-version',declare['javac_version_command'],30)
        require(b'javac 27.0.1' in stdout,'javac_version_pin')
        execute('compile',compile_command,60)
        class_files=sorted(classes.rglob('*.class')); require(bool(class_files),'compiled_classes_missing')
        for p in class_files: p.chmod(0o400)
        paths=[p for p in sorted((OUT/'config').rglob('*')) if p.is_file()]+class_files
        paths += sorted((OUT/'inputs').glob('*.xml'))
        paths += sorted((OUT/'snapshots').iterdir())
        paths += [OUT/name for name in declare['scripts']]
        paths += [OUT/'requests.tsv',OUT/'requests.json',OUT/'execution-declaration.json',OUT/'preparation.json',OUT/'preflight-verification.json',OUT/'declared-commands.txt']
        require(len(paths)==len(set(paths)),'closure_duplicates')
        closure=''.join(sha(p.read_bytes())+'\t'+str(p.relative_to(OUT))+'\n' for p in paths).encode()
        put(OUT/'runtime-closure.tsv',closure,True)
        closure_sha=sha(closure)
        command=java_prefix+[closure_sha,declare['requests_tsv_sha256'],FREEZE_SHA,LEDGER_SHA,str(JDK)]
        write(OUT/'compiled-execution-binding.json',{'runtime_closure_sha256':closure_sha,
              'compiled_classes':[digest_file(p) for p in class_files],
              'driver_sha256':declare['scripts']['FamilyDriver.java'],'java_command':command,
              'requests_sha256':declare['requests_tsv_sha256'],'freeze_sha256':FREEZE_SHA,'ledger_sha256':LEDGER_SHA},True)
        execute('evaluate60',command,600)
    except Exception as exc:
        failures.append({'phase':'execution','error':repr(exc),'traceback':traceback.format_exc()})
    finally:
        try: verify(); write(OUT/'closure-python-after.json',{'verified':True,'runtime_snapshot_matches_original_config_archive':True},True)
        except Exception as exc: failures.append({'phase':'post_closure','error':repr(exc),'traceback':traceback.format_exc()})
        try:
            after=frozen(); write(OUT/'frozen-after.json',after,True)
            require(after==json.loads((OUT/'frozen-before.json').read_bytes()),'frozen_before_after_mismatch')
        except Exception as exc: failures.append({'phase':'frozen_after','error':repr(exc),'traceback':traceback.format_exc()})
        write(OUT/'run-metadata.json',{'attempt':1,'processes':processes,'failures':failures,
              'execution_declaration_sha256':sha((OUT/'execution-declaration.json').read_bytes()),
              'python':py,'removed_environment_override_names':removed,'timing_measurements':False,'reruns':0},True)
    # Always account for every requested row, including unobserved requests on failure.
    import summarize
    complete=summarize.main()
    print(json.dumps({'complete':complete,'requested':60,'report':str(OUT/'report.md')}),flush=True)
    return 0 if complete else 1

if __name__=='__main__': raise SystemExit(main())
