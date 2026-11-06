import json, os
from prepare import OUT, sha, check, write

def replace_once(text, old, new):
    check(text.count(old)==1,'revision_anchor')
    return text.replace(old,new)

def main():
    os.umask(0o077)
    check(not (OUT/'attempt-02').exists(),'fresh_revision_required')
    (OUT/'attempt-02').mkdir()
    driver = (OUT/'DiscriminantDriver.java').read_text()
    driver = replace_once(driver,'public final class DiscriminantDriver {','public final class DiscriminantDriverV2 {')
    driver = replace_once(driver,'root.resolve("classes")','root.resolve("attempt-02/classes")')
    driver = replace_once(driver,'!System.getProperty("java.home").equals(args[7])','!Path.of(System.getProperty("java.home")).toRealPath().equals(Path.of(args[7]).toRealPath())')
    (OUT/'DiscriminantDriverV2.java').write_text(driver)
    runner = (OUT/'run.py').read_text()
    for old,new in [
      ("OUT/'processes'", "OUT/'attempt-02/processes'"),
      ("OUT/'classes'", "OUT/'attempt-02/classes'"),
      ("OUT/'observations.json'", "OUT/'attempt-02/observations.json'"),
      ("OUT/'frozen-after.json'", "OUT/'attempt-02/frozen-after.json'"),
      ("OUT/'run-metadata.json'", "OUT/'attempt-02/run-metadata.json'"),
      ("OUT/'runtime-closure.tsv'", "OUT/'attempt-02/runtime-closure.tsv'"),
      ("OUT/'DiscriminantDriver.java'", "OUT/'DiscriminantDriverV2.java'"),
      ("'DiscriminantDriver',str(OUT),'runtime-closure.tsv'", "'DiscriminantDriverV2',str(OUT),'attempt-02/runtime-closure.tsv'"),
      ("'processes/'+name+'.receipt.json'", "'attempt-02/processes/'+name+'.receipt.json'"),
      ("sha((OUT/'run.py').read_bytes())", "sha((OUT/'run_v2.py').read_bytes())"),
    ]:
        check(old in runner,'runner_revision_anchor'); runner = runner.replace(old,new)
    runner = replace_once(runner,"    ledger_b = (OUT/'ledger-before-label.json').read_bytes(); ledger = json.loads(ledger_b)","    revision = json.loads((OUT/'revision-before-label.json').read_bytes())\n    for n,h in revision['scripts'].items(): check(sha((OUT/n).read_bytes())==h,'revision_script_drift')\n    ledger_b = (OUT/'ledger-before-label.json').read_bytes(); ledger = json.loads(ledger_b)")
    # Preserve partial stdout/stderr and metadata if a bounded subprocess times out.
    runner = replace_once(runner,"        run = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=ROOT, timeout=timeout)","        try:\n            run = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=ROOT, timeout=timeout)\n        except subprocess.TimeoutExpired as exc:\n            (processes/(name+'.stdout')).write_bytes(exc.stdout or b'')\n            (processes/(name+'.stderr')).write_bytes(exc.stderr or b'')\n            outcome = {'process':name,'argv':argv,'exit_code':None,'status':'timeout','stdout_sha256':sha(exc.stdout or b''),'stderr_sha256':sha(exc.stderr or b''),'cwd':str(ROOT)}\n            outcomes.append(outcome); write(processes/(name+'.metadata.json'),outcome)\n            raise")
    runner = replace_once(runner,"        write(OUT/'attempt-02/observations.json',observations)\n    finally:","        write(OUT/'attempt-02/observations.json',observations)\n        check(all(o['status']=='observed' for o in observations),'one_or_more_processes_failed')\n    finally:")
    (OUT/'run_v2.py').write_text(runner)
    previous = json.loads((OUT/'observations.json').read_text())
    check(len(previous)==4 and all(o['status']=='process_failed' and o['exit_code']==1 for o in previous),'retained_failure_set')
    revision = {
      'sequence':1,'original_ledger_sha256':sha((OUT/'ledger-before-label.json').read_bytes()),
      'previous_observations_sha256':sha((OUT/'observations.json').read_bytes()),
      'previous_attempts':'All four Java processes exit 1 at literal JDK home guard before input read/configuration loading; no invoice stages or labels.',
      'diagnosis':'Requested JDK path is a symlink; Java java.home reports resolved mise installation. Same executable hashes and Corretto version retained.',
      'changes':['Compare java.home paths through toRealPath, preserving exact invoked JDK executable and binary hashes.','Use new driver class/output directory; retain original sources, compiled classes, logs and metadata.','Retain partial subprocess streams on timeout; fail wrapper if any case process fails.'],
      'recipes_or_inputs_changed':False,'new_full_oracle_labels_at_revision':'unknown',
      'scripts':{n:sha((OUT/n).read_bytes()) for n in ('revise.py','run_v2.py','DiscriminantDriverV2.java')},
    }
    write(OUT/'revision-before-label.json',revision)
    (OUT/'revision-before-label.json').chmod(0o400)
    print(json.dumps({'revision_sha256':sha((OUT/'revision-before-label.json').read_bytes()),'inputs_changed':False}))

if __name__ == '__main__': main()
