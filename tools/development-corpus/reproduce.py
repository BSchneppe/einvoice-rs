#!/usr/bin/env python3
"""Replay only the two already-declared DEVELOPMENT batches with supplied pins."""
import argparse
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import stat
import types

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

_verifier_source = _load_local_source('verify')

from common import (Invalid, check_bytes, fresh_path, json_bytes, json_lines, no_symlinks,
                    read_file, relative, require, save_new, sha, strict_json, write_new)
from verify import (Bundle, JVM_FLAGS, LIMITS, archive_members, classify, family_receipts,
                    indexed, verify)

FORMAT = 'bsc10-development-replay-v1'
ENV_OVERRIDES = ('JAVA_TOOL_OPTIONS', 'JDK_JAVA_OPTIONS', '_JAVA_OPTIONS', 'CLASSPATH', 'JDK_JAVAC_OPTIONS')


def execute(directory, name, argv, cwd, timeout, affected, env):
    """Real subprocesses only. Persist intent first and outcome even on launch failure."""
    relative(name)
    require('/' not in name, 'process_name')
    prefix = directory / name
    command = {'argv': argv, 'cwd': str(cwd), 'timeout_seconds': timeout, 'requests': affected}
    save_new(prefix.with_suffix('.command.json'), command)
    status, exit_code, started, error = 'launch_error', None, False, None
    child = None
    def stop_child():
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        return child.wait()
    with prefix.with_suffix('.stdout').open('xb') as stdout, prefix.with_suffix('.stderr').open('xb') as stderr:
        try:
            child = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                     stdout=stdout, stderr=stderr, start_new_session=True)
            started = True
            exit_code = child.wait(timeout=timeout)
            status = 'exited'
        except subprocess.TimeoutExpired as exc:
            status = 'timeout'
            error = {'kind': type(exc).__name__, 'detail': str(exc)}
            exit_code = stop_child()
        except KeyboardInterrupt as exc:
            status = 'interrupted'
            error = {'kind': type(exc).__name__, 'detail': str(exc)}
            if child is not None:
                exit_code = stop_child()
        except OSError as exc:
            error = {'kind': type(exc).__name__, 'detail': str(exc)}
            if child is not None:
                status = 'interrupted'
                exit_code = stop_child()
        finally:
            stdout.flush()
            stderr.flush()
            os.fsync(stdout.fileno())
            os.fsync(stderr.fileno())
    outcome = {**command, 'name': name, 'directory': str(directory),
               'attempted': True, 'started': started, 'status': status, 'exit_code': exit_code,
               'exception': error, 'stdout_sha256': sha(read_file(prefix.with_suffix('.stdout'))),
               'stderr_sha256': sha(read_file(prefix.with_suffix('.stderr')))}
    save_new(prefix.with_suffix('.metadata.json'), outcome)
    return outcome


def process_ok(outcome):
    return outcome['status'] == 'exited' and outcome['exit_code'] == 0 and outcome['started'] is True


def pin_runtime(bundle, python, jdk_home):
    pins = bundle.json('provenance.json')['runtime_pins']
    require(jdk_home.resolve(strict=True) == Path(pins['jdk']['release']['realpath']).parent, 'jdk_home_realpath_pin')
    # Only these explicitly recorded runtime aliases are permitted. No discovery.
    paths = {'python': Path(python), 'java': jdk_home / 'bin/java',
             'javac': jdk_home / 'bin/javac', 'release': jdk_home / 'release'}
    result = {}
    for name, path in paths.items():
        pin = pins['python'] if name == 'python' else pins['jdk'][name]
        resolved = path.resolve(strict=True)
        require(str(resolved) == pin['realpath'], 'runtime_realpath_pin')
        data = read_file(resolved)
        check_bytes(data, pin, 'runtime_bytes_pin')
        result[name] = {'path': str(path.absolute()), 'realpath': str(resolved), 'bytes': len(data), 'sha256': sha(data)}
    require(Path(sys.executable).resolve() == Path(python).resolve() and platform.python_version() == '3.12.6', 'running_python_pin')
    result['python']['version'] = platform.python_version()
    result['jdk_release'] = pins['jdk']['release']
    return result


def put(root, path, data):
    relative(path)
    dest = root / path
    dest.parent.mkdir(parents=True, exist_ok=True)
    write_new(dest, data)
    dest.chmod(0o400)


def closure(root, name, operational):
    """Commit exactly owned consumed files, distinct from historical closures."""
    files = []
    for p in sorted(root.rglob('*')):
        if p.is_file() and (p.parts[-2] != 'processes') and 'raw' not in p.relative_to(root).parts:
            rel = p.relative_to(root).as_posix()
            data = read_file(p)
            files.append({'path': rel, 'bytes': len(data), 'sha256': sha(data)})
    require(name not in {x['path'] for x in files}, 'closure_already_exists')
    raw = ''.join(r['sha256'] + '\t' + r['path'] + '\n' for r in files).encode()
    put(root, name, raw)
    save_new(root / 'operational-closure.json', {'format': FORMAT, 'domain': 'actual_consumed_files_v1',
             'historical_closure_reused': False, 'tsv': name, 'tsv_sha256': sha(raw),
             'runtime': operational, 'files': files, **LIMITS})
    return sha(raw), files


def check_closure(root, entries):
    for row in entries:
        check_bytes(read_file(root / row['path']), row, 'consumed_file_drift')


def accounting(request, outcome, input_seen, record_seen, returned_seen, completed, error=None):
    return {'id': request['id'], 'ordinal': request['ordinal'], 'sha256': request['sha256'], 'batch': request.get('batch'), 'declared': True,
            'process_attempted': outcome is not None, 'process_started': bool(outcome and outcome['started']),
            'process_evidence': {k: outcome.get(k) for k in ('name', 'directory', 'status', 'exit_code', 'stdout_sha256', 'stderr_sha256')} if outcome else None,
            'input_receipt_observed': input_seen, 'record_bytes_observed': record_seen,
            'returned_receipt_observed': returned_seen, 'status': 'verified_completed' if completed else 'incomplete',
            'evidence': 'request_specific_evidence_observed' if input_seen or record_seen or returned_seen else
                        'no_request_specific_execution_evidence', 'error': error,
            'absence_does_not_prove_request_not_reached': True}


def inspect_family(root, requests, outcome, closure_hash, bundle):
    results, comparisons = [], []
    stream = read_file(root / 'processes/evaluate60.stdout') if outcome else b''
    stream_parts = []
    for r in requests:
        prefix = root / f"raw/{r['ordinal']:02d}-{r['id']}"
        inp = Path(str(prefix) + '.input-receipt.json')
        rec = Path(str(prefix) + '.bridge.json')
        ret = Path(str(prefix) + '.output-receipt.json')
        seen = (inp.exists(), rec.exists(), ret.exists())
        complete, error = False, None
        if all(seen):
            try:
                raw = read_file(rec)
                family_receipts(r, raw, strict_json(read_file(inp)), strict_json(read_file(ret)), closure_hash,
                    sha(bundle.data('family-run/FamilyDriver.java')), sha(bundle.data('family-design/freeze-v1.json')),
                    sha(bundle.data('family-design/v3/proposed-ledger.json')))
                stream_parts.append(raw)
                old = bundle.data(f"family-run/raw/{r['ordinal']:02d}-{r['id']}.bridge.json")
                comparisons.append({'id': r['id'], 'ordinal': r['ordinal'], 'identical': raw == old,
                                    'historical_sha256': sha(old), 'replay_sha256': sha(raw)})
                complete = bool(outcome and process_ok(outcome))
            except (Invalid, KeyError, TypeError, ValueError, OSError) as exc:
                error = str(exc) if isinstance(exc, Invalid) else 'invalid_request_evidence'
        results.append(accounting(r, outcome, *seen, complete, error))
    if outcome and process_ok(outcome):
        try:
            family_completion(root, requests, stream, stream_parts, closure_hash, bundle)
        except (Invalid, KeyError, TypeError, ValueError, OSError) as exc:
            error = str(exc) if isinstance(exc, Invalid) else 'invalid_batch_completion_evidence'
            for row in results:
                row['status'] = 'incomplete'
                row['error'] = row['error'] or error
    return results, comparisons


def family_completion(root, requests, stream, stream_parts, closure_hash, bundle):
    require(len(stream_parts) == len(requests) and stream == b''.join(stream_parts), 'replay_stream_order')
    before = strict_json(read_file(root / 'raw/closure-before.json'))
    after = strict_json(read_file(root / 'raw/closure-after.json'))
    runtime = strict_json(read_file(root / 'runtime-identities.json'))
    require(before['verified'] is True and after['verified'] is True and before['closure_sha256'] == after['closure_sha256'] == closure_hash and
            after['records_returned'] == len(requests) and before['loaded_classes'] == after['loaded_classes'] and
            before['requests_sha256'] == after['requests_sha256'] == sha(bundle.data('family-run/requests.tsv')) and
            before['freeze_sha256'] == sha(bundle.data('family-design/freeze-v1.json')) and
            before['ledger_sha256'] == sha(bundle.data('family-design/v3/proposed-ledger.json')) and
            before['java_command_realpath'] == runtime['java']['realpath'] and
            before['java_command'] == runtime['java']['path'] and
            before['java_home_realpath'] == str(Path(runtime['java']['realpath']).parent.parent) and
            before['java_version'] == '27.0.1' and before['java_runtime_version'] == '27.0.1+11-FR' and
            before['jvm_arguments'] == JVM_FLAGS, 'replay_closure_receipts')
    expected = []
    for entry in bundle.historical_json('family-run/raw/closure-before.json')['loaded_classes']:
        name = entry['class']
        source = str(root / ('classes' if name in ('FamilyDriver', 'Oracle') else 'snapshots/validator.jar'))
        resource = ('file:' + source + '/' + name + '.class') if name in ('FamilyDriver', 'Oracle') else (
            'jar:file:' + source + '!/' + name.replace('.', '/') + '.class')
        expected.append({**entry, 'code_source_realpath': source, 'resource': resource})
    require(before['loaded_classes'] == expected, 'replay_loaded_resource_binding')
    config = strict_json(read_file(root / 'raw/configuration-loaded.json'))
    require(config == {'configuration': str(root / 'config/scenarios.xml'), 'resolving_mode': 'STRICT_LOCAL',
                       'configuration_loads': 1, 'evaluations_requested': len(requests), 'warmup': False, 'timing_measurements': False}, 'replay_configuration')


def inspect_discriminant(root, request, recipe, outcome, closure_hash, bundle):
    name = recipe['case']
    prefix = root / 'processes' / name
    receipt = prefix.with_suffix('.receipt.json')
    raw = read_file(prefix.with_suffix('.stdout')) if outcome else b''
    complete, error, comparisons = False, None, []
    if raw and receipt.exists():
        try:
            records = json_lines(raw)
            require(len(records) == 1, 'replay_discriminant_count')
            r = records[0]
            require(r['id'] == request['id'] and r['sha256'] == request['sha256'], 'replay_discriminant_identity')
            require(classify(r) in ('completed_schema_rejection', 'completed_business_observation'), 'replay_discriminant_stage')
            returned = strict_json(read_file(receipt))
            require(returned['closure_pre_post_verified'] is True and returned['closure_sha256'] == closure_hash and
                    returned['input_sha256'] == request['sha256'] and returned['loaded_jar'] == str(root / 'snapshots/validator.jar') and
                    returned['loaded_oracle_classes'] == str(root / 'attempt-02/classes') and returned['java_version'] == '27.0.1' and
                    returned['java_runtime_version'] == '27.0.1+11-FR', 'replay_discriminant_receipt')
            old = bundle.data('discriminant/attempt-02/processes/' + name + '.stdout')
            comparisons.append({'id': request['id'], 'ordinal': request['ordinal'], 'identical': raw == old,
                                'historical_sha256': sha(old), 'replay_sha256': sha(raw)})
            complete = bool(outcome and process_ok(outcome))
        except (Invalid, KeyError, TypeError, ValueError, OSError) as exc:
            error = str(exc) if isinstance(exc, Invalid) else 'invalid_request_evidence'
    # This driver's sole receipt is emitted after evaluate and closure checking;
    # it has no separate per-request pre-evaluate input receipt.
    return accounting(request, outcome, False, bool(raw), receipt.exists(), complete, error), comparisons


def run(args, out):
    bundle = Bundle(args.bundle / 'publication.json')
    offline = verify(bundle)
    save_new(out / 'bundle-verification.json', offline)
    require(Path(__file__).resolve().parent == bundle.root.resolve(), 'runner_bundle_identity')
    from verify import check_lineage
    family_requests, recipes = check_lineage(bundle)
    chosen = []
    if args.batch in ('family60', 'both'):
        chosen.extend(dict(r, batch='family60') for r in family_requests)
    if args.batch in ('discriminant4', 'both'):
        chosen.extend({'id': 'bsc10-development-' + r['case'], 'ordinal': i, 'sha256': r['input_sha256'],
                       'bytes': r['input_bytes'], 'input': r['input'], 'batch': 'discriminant4'} for i, r in enumerate(recipes))
    save_new(out / 'requests.json', {'format': FORMAT, 'batch': args.batch, 'requests': chosen, **LIMITS})
    save_new(out / 'declaration.json', {'format': FORMAT, 'batch': args.batch, 'publication_sha256': bundle.inventory_sha256,
             'runner_sha256': sha(bundle.files['reproduce.py']), 'contract_sha256': sha(bundle.files['CONTRACT.md']),
             'requests_sha256': sha(read_file(out / 'requests.json')), 'attempt': 1, 'automatic_retries': False,
             'classification': 'Reproduction only; no new labels', 'warmup': False, 'benchmarks': False,
             'timing_measurements': False, **LIMITS})
    # Account for every declared request even when pins/setup fail before launch.
    state = {r['batch'] + ':' + str(r['ordinal']): accounting(r, None, False, False, False, False) for r in chosen}
    save_new(out / 'request-accounting.initial.json', list(state.values()))
    comparisons, outcomes = [], []
    error = None
    try:
        runtime = pin_runtime(bundle, args.python, args.jdk_home)
        save_new(out / 'runtime-identities.json', runtime)
        dep = bundle.json('provenance.json')['dependencies']
        jar = read_file(args.validator)
        check_bytes(jar, dep['validator.jar'], 'validator_pin')
        config_archive = read_file(args.config)
        check_bytes(config_archive, dep['config.zip'], 'config_pin')
        config_pins = {r['path'].removeprefix('config/'): r for r in bundle.historical_json('family-run/preparation.json')['config_members']}
        require(len(config_pins) == 97, 'full_configuration_count')
        config = archive_members(config_archive, config_pins, exact=True)
        save_new(out / 'configuration-inventory.json', {'archive_sha256': sha(config_archive), 'files': list(config_pins.values())})
        env = dict(os.environ)
        removed = [name for name in ENV_OVERRIDES if name in env]
        for name in ENV_OVERRIDES:
            env.pop(name, None)
        env['JAVA_HOME'] = str(args.jdk_home)
        save_new(out / 'environment.json', {'removed_override_names': removed, 'JAVA_HOME': str(args.jdk_home)})
        processes = out / 'processes'
        processes.mkdir()
        def command(name, argv, cwd, timeout, affected):
            result = execute(processes, name, argv, cwd, timeout, affected, env)
            outcomes.append(result)
            require(process_ok(result), 'process_' + result['status'] + ('_nonzero' if result['exit_code'] else ''))
            return result
        java, javac = str(args.jdk_home / 'bin/java'), str(args.jdk_home / 'bin/javac')
        command('java-version', [java, '-version'], out, 30, [])
        require(b'Corretto-27.0.1.0.11' in read_file(processes / 'java-version.stderr'), 'java_version_pin')
        command('javac-version', [javac, '-version'], out, 30, [])
        require(b'javac 27.0.1' in read_file(processes / 'javac-version.stdout'), 'javac_version_pin')
        for batch in ('family60', 'discriminant4'):
            requested = [r for r in chosen if r['batch'] == batch]
            if not requested:
                continue
            root = out / batch
            root.mkdir()
            (root / 'processes').mkdir()
            (root / 'raw').mkdir()
            put(root, 'snapshots/validator.jar', jar)
            put(root, 'snapshots/Oracle.java', bundle.data('family-run/snapshots/Oracle.java'))
            put(root, 'runtime-identities.json', json_bytes(runtime))
            put(root, 'request-declaration.json', json_bytes({'format': FORMAT, 'requests': requested, 'publication_sha256': bundle.inventory_sha256}))
            for name, data in config.items():
                put(root, 'config/' + name, data)
            is_family = batch == 'family60'
            driver = 'FamilyDriver' if is_family else 'DiscriminantDriverV2'
            classes = root / ('classes' if is_family else 'attempt-02/classes')
            classes.mkdir(parents=True)
            put(root, driver + '.java', bundle.data(('family-run/' if is_family else 'discriminant/') + driver + '.java'))
            for r in requested:
                original = 'family-run/' + r['input'] if is_family else 'discriminant/' + r['input']
                put(root, r['input'], bundle.data(original))
            if is_family:
                for name, original in [('requests.tsv', 'family-run/requests.tsv'), ('requests.json', 'family-run/requests.json'), ('snapshots/freeze-v1.json', 'family-design/freeze-v1.json'),
                                       ('snapshots/proposed-ledger.json', 'family-design/v3/proposed-ledger.json')]:
                    put(root, name, bundle.data(original))
            else:
                put(root, 'ledger-before-label.json', bundle.data('discriminant/ledger-before-label.json'))
                put(root, 'snapshots/freeze-v1.json', bundle.data('family-design/freeze-v1.json'))
            command('compile-' + batch, [javac, '-encoding', 'UTF-8', '-cp', str(root / 'snapshots/validator.jar'), '-d', str(classes),
                    str(root / 'snapshots/Oracle.java'), str(root / (driver + '.java'))], root, 60, [])
            class_rows = [r for r in bundle.rows.values() if r['disposition'] == 'derived_recipe' and
                          r['recipe'] == ('compile-family' if is_family else 'compile-discriminant-corrected')]
            require({p.name for p in classes.iterdir()} == {Path(r['path']).name for r in class_rows}, 'compiled_class_set')
            for r in class_rows:
                path = classes / Path(r['path']).name
                check_bytes(read_file(path), r, 'compiled_class_pin')
                path.chmod(0o400)
            closure_name = 'runtime-closure.tsv' if is_family else 'attempt-02/runtime-closure.tsv'
            closure_hash, consumed = closure(root, closure_name, runtime)
            save_new(root / 'execution-declaration.json', {'format': FORMAT, 'closure_sha256': closure_hash,
                     'requests': requested, 'jvm_arguments': JVM_FLAGS, 'full_config_members': len(config),
                     'configuration': 'STRICT_LOCAL', **LIMITS})
            cp = str(classes) + os.pathsep + str(root / 'snapshots/validator.jar')
            batch_outcomes = {}
            try:
                if is_family:
                    argv = [java] + JVM_FLAGS + ['-cp', cp, driver, str(root), closure_hash,
                            sha(bundle.data('family-run/requests.tsv')), sha(bundle.data('family-design/freeze-v1.json')),
                            sha(bundle.data('family-design/v3/proposed-ledger.json')), str(args.jdk_home)]
                    result = execute(root / 'processes', 'evaluate60', argv, root, 600,
                                     [{'id': r['id'], 'ordinal': r['ordinal']} for r in requested], env)
                    outcomes.append(result)
                    batch_outcomes['evaluate60'] = result
                else:
                    for r, recipe in zip(requested, recipes):
                        name = recipe['case']
                        argv = [java] + JVM_FLAGS + ['-cp', cp, driver, str(root), closure_name, closure_hash, r['id'],
                                r['input'], r['sha256'], 'processes/' + name + '.receipt.json', str(args.jdk_home)]
                        result = execute(root / 'processes', name, argv, root, 180,
                                         [{'id': r['id'], 'ordinal': r['ordinal']}], env)
                        outcomes.append(result)
                        batch_outcomes[name] = result
                        if not process_ok(result):
                            break
            finally:
                if is_family:
                    rows, diffs = inspect_family(root, family_requests, batch_outcomes.get('evaluate60'), closure_hash, bundle)
                    for r, entry in zip(requested, rows):
                        entry['batch'] = batch
                        state[batch + ':' + str(r['ordinal'])] = entry
                    comparisons.extend(diffs)
                else:
                    for r, recipe in zip(requested, recipes):
                        entry, diffs = inspect_discriminant(root, r, recipe, batch_outcomes.get(recipe['case']), closure_hash, bundle)
                        state[batch + ':' + str(r['ordinal'])] = entry
                        comparisons.extend(diffs)
            try:
                check_closure(root, consumed)
                require(sha(read_file(root / closure_name)) == closure_hash, 'operational_closure_drift')
            except (Invalid, OSError) as exc:
                for r in requested:
                    entry = state[batch + ':' + str(r['ordinal'])]
                    entry['status'] = 'incomplete'
                    entry['error'] = str(exc) if isinstance(exc, Invalid) else 'consumed_file_unavailable'
                raise
            require(all(process_ok(x) for x in batch_outcomes.values()) and
                    all(state[batch + ':' + str(r['ordinal'])]['status'] == 'verified_completed' for r in requested), 'batch_incomplete')
    except KeyboardInterrupt:
        error = 'interrupted'
    except (Invalid, OSError, ValueError, KeyError, TypeError) as exc:
        error = str(exc) if isinstance(exc, Invalid) else type(exc).__name__
    finally:
        save_new(out / 'request-accounting.json', list(state.values()))
        save_new(out / 'comparison.json', {'kind': 'full_raw_byte_comparison_after_execution', 'records': comparisons,
                 'differences': sum(not x['identical'] for x in comparisons), 'new_labels': False})
        # Index every actual output, including partial attempts and incomplete marker.
        files = []
        for p in sorted(out.rglob('*')):
            if p.is_file():
                data = read_file(p)
                files.append({'path': p.relative_to(out).as_posix(), 'bytes': len(data), 'sha256': sha(data)})
        save_new(out / 'output-inventory.json', {'format': FORMAT, 'files': files,
                 'excluded': ['output-inventory.json', 'result.json (final status written next)']})
        completed = error is None and all(x['status'] == 'verified_completed' for x in state.values())
        save_new(out / 'result.json', {'format': FORMAT, 'status': 'completed' if completed else 'incomplete', 'error': error,
                 'requests': len(chosen), 'verified_completed': sum(x['status'] == 'verified_completed' for x in state.values()),
                 'output_inventory_sha256': sha(read_file(out / 'output-inventory.json')),
                 'all_comparisons_identical': len(comparisons) == len(chosen) and all(x['identical'] for x in comparisons),
                 'kind': 'local_development_reproduction', **LIMITS})
    return 0 if completed else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--batch', choices=('family60', 'discriminant4', 'both'), required=True)
    parser.add_argument('--python', type=Path, required=True)
    parser.add_argument('--jdk-home', type=Path, required=True)
    parser.add_argument('--validator', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    def interrupted(_signum, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    try:
        args.bundle = no_symlinks(args.bundle)
        args.validator, args.config = no_symlinks(args.validator), no_symlinks(args.config)
        args.jdk_home = args.jdk_home.absolute()
        protected = [args.bundle, Path(__file__).absolute().parent, args.python, args.jdk_home,
                     args.validator.parent, args.config.parent]
        out = fresh_path(args.out, protected, directory=True)
        save_new(out / 'incomplete.json', {'format': FORMAT, 'status': 'incomplete',
                 'meaning': 'Completion requires result.json AND successful process exit. This marker is retained.', **LIMITS})
        return run(args, out)
    except (Invalid, OSError, ValueError, KeyError, TypeError) as exc:
        print(str(exc) if isinstance(exc, Invalid) else type(exc).__name__, file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
