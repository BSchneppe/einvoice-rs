import json, os
from pathlib import Path
from prepare import OUT, ROOT, JDK, PYTHON, SEED_HASH, sha, check, write, frozen

def j(value): return json.dumps(value,ensure_ascii=False,indent=2)

def main():
    os.umask(0o077)
    check(not (OUT/'report.md').exists(),'report_already_exists')
    ledger_b = (OUT/'ledger-before-label.json').read_bytes(); ledger = json.loads(ledger_b)
    rev_b = (OUT/'revision-before-label.json').read_bytes(); revision = json.loads(rev_b)
    provenance = json.loads((OUT/'provenance.json').read_text())
    original = json.loads((OUT/'observations.json').read_text())
    observed = json.loads((OUT/'attempt-02/observations.json').read_text())
    check(len(original)==4 and len(observed)==4,'complete_attempt_cardinality')
    seed = (OUT/'snapshots/development-seed.xml').read_bytes()
    check(sha(seed)==SEED_HASH,'seed_integrity')
    for r in ledger['recipes']:
        offset=r['original_byte_offset_zero_based']
        check((OUT/r['input']).read_bytes() == seed[:offset]+r['insertion_utf8'].encode()+seed[offset:],'exact_insertion_lineage')
    for record,recipe in zip(observed,ledger['recipes']):
        check(record['case']==recipe['case'] and record['status']=='observed','observation_order')
        raw = (OUT/'attempt-02/processes'/(record['case']+'.stdout')).read_bytes()
        check(json.loads(raw)==record['record'],'raw_record_binding')
        check(record['record']['sha256']==recipe['input_sha256'],'observed_input_binding')
    for path,h in ledger['scripts'].items(): check(sha((OUT/path).read_bytes())==h,'original_script_integrity')
    for path,h in revision['scripts'].items(): check(sha((OUT/path).read_bytes())==h,'revision_script_integrity')
    closure_b = (OUT/'attempt-02/runtime-closure.tsv').read_bytes()
    for line in closure_b.decode().splitlines():
        h,path = line.split('\t',1)
        check(sha((OUT/path).read_bytes())==h,'runtime_closure_final_integrity')
    before = json.loads((OUT/'frozen-before.json').read_text()); after=frozen()
    check(before==after==json.loads((OUT/'frozen-after.json').read_text())==json.loads((OUT/'attempt-02/frozen-after.json').read_text()),'frozen_before_after')
    check(sha((ROOT/'tools/oracle/artifacts.lock.json').read_bytes())==provenance['artifact_lock_sha256'],'artifact_lock_unchanged')
    check(sha((ROOT/'tools/oracle/java/Oracle.java').read_bytes())==provenance['bridge_sha256'],'original_bridge_unchanged')
    for n in ('java','javac'): check(sha((JDK/'bin'/n).read_bytes())==provenance['jdk'][n]['sha256'],'jdk_final_hash')
    check(sha(PYTHON.read_bytes())==provenance['python']['sha256'],'python_final_hash')
    final = {'frozen_before':before,'frozen_after':after,'artifact_lock_sha256':provenance['artifact_lock_sha256'],
      'ledger_sha256':sha(ledger_b),'revision_sha256':sha(rev_b),'runtime_closure_sha256':sha(closure_b),
      'runtime_closure_entries_verified':len(closure_b.decode().splitlines()),'all_four_exact_insertion_lineages_verified':True,
      'complete_attempts_retained':8,'oracle_evaluate_completed':4,'native_execution':False,'runtime_coverage':'unavailable',
      'conformance_claim':False,'resource_claim':False,'attestation_claim':False,'release_claim':False,
      'report_generator_sha256':sha(Path(__file__).read_bytes())}
    write(OUT/'final-audit.json',final)
    lines = ['# BSC-10 fresh development discriminant observations', '',
      'Bounded development-only investigation, 2026-11-06. Original-source association remains unresolved. No native conformance, coverage, resource, attestation or release claim. All new files belong exclusively to this scratch directory; no tracked source, sibling work, frozen pins/results/splits/workload, runtime, facade, compiler, builder or benchmark was edited. No download, timing measurement, benchmark loop, commit or PR was performed.', '',
      'The pinned original complete KoSIT configuration and unchanged `Oracle.evaluate` completed once for each of four committed input byte sequences. All four actually passed `val-xsd`; this is execution evidence, not an inference from static XSD reading. Baseline is the existing public ancestral control, not additional independent input evidence. Three mutations share its single document shape.', '',
      '## Observed outcomes', '',
      '| Development case | XML / XSD | CEN / XR stages | Findings in observed order (BASE / effective) | KoSIT recommendation | Bridge acceptance |',
      '| --- | --- | --- | --- | --- | --- |']
    for obs in observed:
        r=obs['record']; s=r['stages']; fs='; '.join(f"{f['id']} ({f['severity']} / {f['effective_severity']})" for f in r['findings']) or 'none'
        lines.append(f"| {obs['case']} | {s['val-xml']} / {s['val-xsd']} | {s['val-sch.1']} / {s['val-sch.2']} | {fs} | {r['reference_acceptance']} | {r['acceptance']} |")
    lines += ['', 'All four processed successfully, matched exactly `EN16931 XRechnung (CII)`, evaluated both business stages, had zero structural findings, and exited Java with code 0. The buyer case was accepted despite an invalid CEN stage and a warning. Stage status and acceptance are deliberately recorded separately.', '',
      '`severity` in these full-bridge records is the KoSIT report’s normalized **BASE** `rep:message/@level`, not the source/SVRL raw flag. `effective_severity` is the unchanged bridge’s override lookup applied to that BASE level. For these observed findings BASE and effective agree. Static `CII-SR-282/@flag="warning"` is recorded separately in the source node evidence. No raw-flag observation is fabricated from the bridge output.', '',
      'Seller-only had no CII-SR-282 diagnostic but retained four collateral errors; this absence does not establish passing or evaluated-site coverage. Buyer-only and combined each emitted CII-SR-282 once. All collateral errors, messages, locations, null line/column fields and original order survive in the raw JSON records and full appendix below. No attempt was selected or discarded based on acceptance.', '',
      '## Retained development failure and explicit revision', '',
      'The first driver was compiled successfully, then all four Java launches exited 1 at `DiscriminantDriver.java:31` with `IllegalStateException: JDK home mismatch`. The guard ran before input reading or configuration loading. Classification: driver provenance guard/infrastructure failure; XML, XSD and business stages were not evaluated, and no invoice labels existed for these attempts. All four original stdout files are empty; their stderr, argv, exit metadata, compiled classes, source and closure are retained under `processes/`, `classes/`, `observations.json` and `runtime-closure.tsv`.', '',
      'Diagnosis command (observed exit 0):', '',
      '```sh', str(JDK/'bin/java')+' -XshowSettings:properties -version', '```', '',
      'Java reported `java.home=/Users/bene/.local/share/mise/installs/java/corretto-27.0.1.0.11`; the requested `/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home` resolves to that same installation. The explicit revision compares `toRealPath()` values. It preserves invoked binaries, their hashes, inputs, recipes and original source/configuration bytes. New class/output paths preserve attempt 1. The wrapper also retains timeout partial streams and now fails if any case process fails. The first wrapper itself exited 0 even though its four children failed; those child exit codes remain explicit and are not treated as successful evaluation.', '',
      f"`revision-before-label.json` SHA-256 `{sha(rev_b)}` was written before the corrected four evaluations. No recipe, invoice placement, child shape or input revision was needed; no schema-invalid attempt occurred. This is the complete sequence, including the infrastructure failures. The corrected compile and all four evaluator subprocesses exited 0. Version-process outputs and every argv/exit/stdout/stderr digest are retained separately in each attempt’s metadata.", '',
      '## Pinned XSD placement and actual validation', '',
      '`config/resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd` has SHA-256 `1c42244298a863bcc0b5196983ff0a5d084fcabe58aea77ef82b242e7cd0ddfd`. At lines 311–341 `HeaderTradeAgreementType` is a sequence: BuyerTradeParty (316), optional intermediate parties (317–320), BuyerTaxRepresentativeTradeParty (321), SellerTaxRepresentativeTradeParty (322), then later referenced documents (325 onward). Both representatives have minOccurs=0 and default maxOccurs=1, type `ram:TradePartyType`.', '',
      '`TradePartyType` (1029–1043) makes every declared child optional. ID is first (1031; repeatable), Name follows GlobalID (1033; singleton); PostalTradeAddress and SpecifiedTaxRegistration are also optional at the XSD layer. No child is required for these two representative declarations. `CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd:79–91` makes IDType simple content over `xsd:token` with optional attributes; TextType at 145–152 extends `xsd:string` with optional attributes. The seller ID-only and buyer Name-only structures were therefore predeclared without adding business-required seller data. Business-required fields were not mistaken for XSD-required fields.', '',
      'The seed contains BuyerReference, SellerTradeParty, BuyerTradeParty, BuyerOrderReferencedDocument and ContractReferencedDocument in the agreement. The single insertion is immediately before BuyerOrderReferencedDocument, after BuyerTradeParty. Combined inserts buyer then seller. `schema-inspection.txt` retains the inspected original lines and expanded child names. The four original D16B XSD files are consumed unchanged through the original full `scenarios.xml` target configuration, `STRICT_LOCAL` resolution and KoSIT 1.6.3. Each actual `val-xsd=valid` appears in the full bridge record. No separate JAXP checker, substituted schema, simplified stage or configuration wrapper was used.', '',
      '## Prelabel ledger, original bytes and authored changes', '',
      f"`ledger-before-label.json` SHA-256 `{sha(ledger_b)}` commits the seed, source/node inspection, four recipes, exact input hashes and driver scripts before full oracle labels. Expected results are explicitly unknown. Neutral literals `DEV-SELLER-REP-41` and `Development Buyer Representative 73` were researcher-selected before labels, without builder/runtime examples. Ledger and revision ordering are local reproducibility bookkeeping, not cryptographic remote attestation or a protected evaluator boundary.", '',
      f"Source attribution: {provenance['seed']['source']}; revision `{provenance['seed']['revision']}`; original Parquet SHA-256 `{provenance['seed']['artifact_sha256']}`; original source path `{provenance['seed']['source_path']}`; CC-BY-4.0. This investigation used only the already-retained preselected XML, parent SHA-256 `{SEED_HASH}`, 11,214 bytes; it did not reselect or scan corpus entries. Modifications are the following exact UTF-8 byte insertions. Existing bytes, whitespace, prefixes, namespace declarations and ordering are otherwise unchanged; no XML reserialization occurred.", '']
    for r in ledger['recipes']:
        lines += [f"### {r['case']}", '', f"Input `{r['input']}`; {r['input_bytes']} bytes; SHA-256 `{r['input_sha256']}`. Recipe SHA-256 `{r['recipe_sha256']}`. Original zero-based byte offset {r['original_byte_offset_zero_based']}; removed bytes 0; inserted bytes {r['inserted_bytes']}.", '']
        if r['insertion_utf8']:
            lines += ['```xml',r['insertion_utf8'].rstrip('\n'),'```','The inserted bytes include a final LF after the closing tag, as recorded exactly in the JSON ledger.','']
        else: lines += ['Identity control, no inserted bytes.','']
    lines += ['## Exact original source association retained', '',
      'Only these three exact CEN source members were read from the verified `cen.zip` snapshot: `schematron/preprocessed/EN16931-CII-validation-preprocessed.sch`, `schematron/CII/EN16931-CII-syntax.sch`, `schematron/abstract/EN16931-CII-syntax.sch`. Source node records also inspect the already configured `resources/cii/16b/xsl/EN16931-CII-validation.xsl`. No example/test/corpus source members were selected. Complete config members are verified and extracted for executing the unchanged full configuration, not for selecting invoice examples.', '',
      'With `sch=http://purl.oclc.org/dsdl/schematron`, preprocessed node `/sch:schema[1]/sch:pattern[2]/sch:rule[14]/sch:assert[61]` (line 659) has ID CII-SR-282 and test `not(ram:SellerTaxRepresentativeTradeParty/ram:ID)`; the rule context is `/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement`.', '',
      'Original binding nodes `/sch:pattern[1]/sch:param[289]` and `[290]` (427 and 429) preserve the same name CII-SR-282 twice, first value `not(ram:BuyerTaxRepresentativeTradeParty)`, then `not(ram:SellerTaxRepresentativeTradeParty/ram:ID)`. Abstract `/sch:pattern[1]/sch:rule[14]/sch:assert[61]` (353) has `test="$CII-SR-282"` and its original `$ApplicableHeaderTradeAgreement ` context retains trailing whitespace.', '',
      'Configured `xsl:transform[1]/xsl:template[79]/xsl:choose[1]/xsl:otherwise[1]/schxslt:rule[1]/xsl:if[61]/svrl:failed-assert[1]` (7442–7448, xsl:if at 7441) emits CII-SR-282 with test `not(ram:BuyerTaxRepresentativeTradeParty)` under guard `not(not(ram:BuyerTaxRepresentativeTradeParty))`; template match retains its original trailing space, priority 40, mode d17e41. `source-nodes.json` retains exact expanded namespace locators, ordered duplicate occurrences, attributes, contexts and source member hashes. The original complete source bytes are retained alongside these decoded node views.', '',
      'The fresh full-oracle observations provide bounded development behavior associated with the configured executable bytes. The preprocessed assertion and configured emitter still have different tests. No expected verdict was inferred from that difference, no alternate source test was executed, and neither test was chosen as the governing meaning. Duplicate substitution remains a possible explanation, not an established cause. Root/compiler/runtime still need a reviewed source-association policy; these observations do not authorize changing source pins or implementing an invoice specialcase.', '',
      '## Consumed-byte closure and process provenance', '',
      '`prepare.py` reads each selected artifact once into memory, verifies the original pin, validates ZIP member sets without printing them, then copies the verified archive/JAR bytes. Config is extracted from that same verified in-memory archive and every original member digest checked. CEN source extraction uses the explicit three-member allowlist. Original Oracle.java is copied after checking its exact frozen hash. javac compiles that copied source, and Java uses the copied JAR and full copied config; neither process consumes a subsequently reopened cache JAR/config/bridge.', '',
      'Before/after launch the Python driver checks the snapshot archives/JAR, copied bridge, every extracted config byte against the verified original archive, pinned Python executable, JDK java/javac binaries and JDK release file. The in-process Java driver verifies a hashed closure manifest before configuration loading and after evaluation, binds actual loaded DefaultCheck JAR and Oracle class locations, and hashes the exact byte array passed into unchanged Oracle.evaluate. The manifest includes the full original configuration and actual compiled class bytes. Original compilation inputs are also bound. Each completed receipt binds the input and runtime closure hashes. Read-only file modes reduce accidental mutation; this is local byte provenance, not a security boundary or attestation.', '',
      f"Corrected runtime closure SHA-256 `{sha(closure_b)}`; {final['runtime_closure_entries_verified']} verified entries. Its member inventory is retained locally, not dumped here. `provenance.json`, `run-metadata.json`, `attempt-02/run-metadata.json`, each `*.command.json`, `*.metadata.json`, `*.receipt.json`, and raw streams bind commands, working directory, executable paths, process outcomes and byte digests. Inherited JAVA_TOOL_OPTIONS, JDK_JAVA_OPTIONS, _JAVA_OPTIONS, CLASSPATH and JDK_JAVAC_OPTIONS are removed; JAVA_HOME is set to the exact requested JDK. Shell was `/bin/bash`, login=false.", '',
      'Selected artifact pins:', '', '```json',j(provenance['artifacts']),'```', '',
      f"Unchanged Oracle.java SHA-256 `{provenance['bridge_sha256']}`. Full configuration selected CEN XSLT SHA-256 `{sha((OUT/'config/resources/cii/16b/xsl/EN16931-CII-validation.xsl').read_bytes())}`; selected XR XSLT SHA-256 `{sha((OUT/'config/resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl').read_bytes())}`. No separate release CEN stylesheet was substituted.", '',
      'Exact executable metadata (java/javac and release hashes rechecked after evaluations):', '', '```json',j({'python':provenance['python'],'jdk':provenance['jdk']}),'```','',
      'Version output:', '', '```text',(OUT/'attempt-02/processes/java-version.stderr').read_text().rstrip(),(OUT/'attempt-02/processes/javac-version.stdout').read_text().rstrip(),'```','',
      '## Reproduction', '',
      'The scripts refuse existing attempt/output paths. Preserve this observation directory; copy only the six files listed below into a new adjacent bounded scratch directory to repeat the exact failed-then-corrected sequence. Do not copy generated outputs. The preselected parent seed remains the same exact SHA-256, and original source pins/cache are read-only. Before rerunning, verify copied script digests against the ledgers and final audit in this report’s directory. Substitute the new bounded directory for DIR in the commands below. No invocation calls Oracle.main, warmup loops, metrics, benchmark loops or the cache-writing oracle Python CLI.', '',
      '```sh', 'PY='+str(PYTHON), 'DIR=.scratch/bsc10/expansion/discriminant',
      '"$PY" -B "$DIR/prepare.py"', '"$PY" -B "$DIR/declare.py"', '"$PY" -B "$DIR/run.py"',
      '# Retained first-attempt JDK guard failures are expected process observations.',
      '"$PY" -B "$DIR/revise.py"', '"$PY" -B "$DIR/run_v2.py"', '"$PY" -B "$DIR/report_results.py"', '```','',
      'Required copied files are `prepare.py`, `declare.py`, `run.py`, `DiscriminantDriver.java`, `revise.py`, and `report_results.py`; revise.py reproducibly creates run_v2.py and DiscriminantDriverV2.java. Full exact javac/java argv arrays are in retained per-process command JSON. The compact commands above are exactly the executed authoring/evaluation/report entry points, not a download or cache-preparation recipe.', '',
      '## Frozen audit and limits', '', '```json',j(final),'```','',
      'The 39 frozen files were checked as opaque byte streams against the unchanged evidence lock before and after both attempts, including the frozen results, splits and workload; no result labels were deserialized. Artifact lock and original bridge are unchanged. No held-out inputs, labels or family identities were accessed or emitted. No implementation inventory changes were made. Root AGENTS.md was absent in the bound worktree; the project-root `/Users/bene/RustroverProjects/rust-einvoice/AGENTS.md` and assigned expansion plan were read. The sibling source-inventory work was not edited.', '',
      'Limits: one known public development ancestor and three deterministic mutations; no independent corpus diversity or protected evaluator separation. Full KoSIT bridge output is retained, not a raw SVRL capture. BASE severity is normalized. Diagnostic absence does not prove assertion evaluation or coverage. JDK executable/release hashes are provided, but the complete OS/JVM/Python transitive runtime is not attested. No alternate source compiler/test was run. No native implementation comparison or decision about which conflicting source representation wins was made.', '',
      '## Full corrected bridge records', '']
    for obs in observed:
        lines += ['### '+obs['case'],'','Raw unchanged JSON stdout: `attempt-02/processes/'+obs['case']+'.stdout`.','','```json',j(obs['record']),'```','']
    (OUT/'report.md').write_text('\n'.join(lines)+'\n')
    print(json.dumps({'report':str(OUT/'report.md'),'frozen_files_verified':39,'complete_attempts_retained':8,'completed_evaluations':4,'source_association':'unresolved'}))

if __name__ == '__main__': main()
