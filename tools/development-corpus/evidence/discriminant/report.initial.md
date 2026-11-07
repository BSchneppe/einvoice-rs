# BSC-10 fresh development discriminant observations

Bounded development-only investigation, 2026-11-06. Original-source association remains unresolved. No native conformance, coverage, resource, attestation or release claim. All new files belong exclusively to this scratch directory; no tracked source, sibling work, frozen pins/results/splits/workload, runtime, facade, compiler, builder or benchmark was edited. No download, timing measurement, benchmark loop, commit or PR was performed.

The pinned original complete KoSIT configuration and unchanged `Oracle.evaluate` completed once for each of four committed input byte sequences. All four actually passed `val-xsd`; this is execution evidence, not an inference from static XSD reading. Baseline is the existing public ancestral control, not additional independent input evidence. Three mutations share its single document shape.

## Observed outcomes

| Development case | XML / XSD | CEN / XR stages | Findings in observed order (BASE / effective) | KoSIT recommendation | Bridge acceptance |
| --- | --- | --- | --- | --- | --- |
| baseline | valid / valid | valid / valid | none | acceptable | accepted |
| seller | valid / valid | invalid / valid | BR-18 (error / error); BR-19 (error / error); BR-20 (error / error); BR-56 (error / error) | reject | rejected |
| buyer | valid / valid | invalid / valid | CII-SR-282 (warning / warning) | acceptable | accepted |
| combined | valid / valid | invalid / valid | BR-18 (error / error); BR-19 (error / error); BR-20 (error / error); BR-56 (error / error); CII-SR-282 (warning / warning) | reject | rejected |

All four processed successfully, matched exactly `EN16931 XRechnung (CII)`, evaluated both business stages, had zero structural findings, and exited Java with code 0. The buyer case was accepted despite an invalid CEN stage and a warning. Stage status and acceptance are deliberately recorded separately.

`severity` in these full-bridge records is the KoSIT report’s normalized **BASE** `rep:message/@level`, not the source/SVRL raw flag. `effective_severity` is the unchanged bridge’s override lookup applied to that BASE level. For these observed findings BASE and effective agree. Static `CII-SR-282/@flag="warning"` is recorded separately in the source node evidence. No raw-flag observation is fabricated from the bridge output.

Seller-only had no CII-SR-282 diagnostic but retained four collateral errors; this absence does not establish passing or evaluated-site coverage. Buyer-only and combined each emitted CII-SR-282 once. All collateral errors, messages, locations, null line/column fields and original order survive in the raw JSON records and full appendix below. No attempt was selected or discarded based on acceptance.

## Retained development failure and explicit revision

The first driver was compiled successfully, then all four Java launches exited 1 at `DiscriminantDriver.java:31` with `IllegalStateException: JDK home mismatch`. The guard ran before input reading or configuration loading. Classification: driver provenance guard/infrastructure failure; XML, XSD and business stages were not evaluated, and no invoice labels existed for these attempts. All four original stdout files are empty; their stderr, argv, exit metadata, compiled classes, source and closure are retained under `processes/`, `classes/`, `observations.json` and `runtime-closure.tsv`.

Diagnosis command (observed exit 0):

```sh
/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home/bin/java -XshowSettings:properties -version
```

Java reported `java.home=/Users/bene/.local/share/mise/installs/java/corretto-27.0.1.0.11`; the requested `/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home` resolves to that same installation. The explicit revision compares `toRealPath()` values. It preserves invoked binaries, their hashes, inputs, recipes and original source/configuration bytes. New class/output paths preserve attempt 1. The wrapper also retains timeout partial streams and now fails if any case process fails. The first wrapper itself exited 0 even though its four children failed; those child exit codes remain explicit and are not treated as successful evaluation.

`revision-before-label.json` SHA-256 `d827a710d6d9a2c4afb81a620e45156afd4b0eaf298e72b5894463e01e85c668` was written before the corrected four evaluations. No recipe, invoice placement, child shape or input revision was needed; no schema-invalid attempt occurred. This is the complete sequence, including the infrastructure failures. The corrected compile and all four evaluator subprocesses exited 0. Version-process outputs and every argv/exit/stdout/stderr digest are retained separately in each attempt’s metadata.

## Pinned XSD placement and actual validation

`config/resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd` has SHA-256 `1c42244298a863bcc0b5196983ff0a5d084fcabe58aea77ef82b242e7cd0ddfd`. At lines 311–341 `HeaderTradeAgreementType` is a sequence: BuyerTradeParty (316), optional intermediate parties (317–320), BuyerTaxRepresentativeTradeParty (321), SellerTaxRepresentativeTradeParty (322), then later referenced documents (325 onward). Both representatives have minOccurs=0 and default maxOccurs=1, type `ram:TradePartyType`.

`TradePartyType` (1029–1043) makes every declared child optional. ID is first (1031; repeatable), Name follows GlobalID (1033; singleton); PostalTradeAddress and SpecifiedTaxRegistration are also optional at the XSD layer. No child is required for these two representative declarations. `CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd:79–91` makes IDType simple content over `xsd:token` with optional attributes; TextType at 145–152 extends `xsd:string` with optional attributes. The seller ID-only and buyer Name-only structures were therefore predeclared without adding business-required seller data. Business-required fields were not mistaken for XSD-required fields.

The seed contains BuyerReference, SellerTradeParty, BuyerTradeParty, BuyerOrderReferencedDocument and ContractReferencedDocument in the agreement. The single insertion is immediately before BuyerOrderReferencedDocument, after BuyerTradeParty. Combined inserts buyer then seller. `schema-inspection.txt` retains the inspected original lines and expanded child names. The four original D16B XSD files are consumed unchanged through the original full `scenarios.xml` target configuration, `STRICT_LOCAL` resolution and KoSIT 1.6.3. Each actual `val-xsd=valid` appears in the full bridge record. No separate JAXP checker, substituted schema, simplified stage or configuration wrapper was used.

## Prelabel ledger, original bytes and authored changes

`ledger-before-label.json` SHA-256 `8496eb001e6bfea360d40565cbebec1d95c955dfb5f732888af22c9af1c4f099` commits the seed, source/node inspection, four recipes, exact input hashes and driver scripts before full oracle labels. Expected results are explicitly unknown. Neutral literals `DEV-SELLER-REP-41` and `Development Buyer Representative 73` were researcher-selected before labels, without builder/runtime examples. Ledger and revision ordering are local reproducibility bookkeeping, not cryptographic remote attestation or a protected evaluator boundary.

Source attribution: Sinabis GmbH (2026), Sinabis Green Energy – 10k GoBD-compliant German E-Invoices; revision `8d9ab205ab74f360fac344a50fcd54c35d0cd9ba`; original Parquet SHA-256 `f45674ce80b477a859c684c68e423f8bb52ad2c1647c97a9e29050f6519dee13`; original source path `zugferd/b2b_service_maintenance/0000/0000000_SRV-2025-47731.xml`; CC-BY-4.0. This investigation used only the already-retained preselected XML, parent SHA-256 `881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c`, 11,214 bytes; it did not reselect or scan corpus entries. Modifications are the following exact UTF-8 byte insertions. Existing bytes, whitespace, prefixes, namespace declarations and ordering are otherwise unchanged; no XML reserialization occurred.

### baseline

Input `inputs/baseline.xml`; 11214 bytes; SHA-256 `881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c`. Recipe SHA-256 `27e2b7aec4d4e27d5ec61ab0ca919d3287cab2761fa6b27c6120945d2ba959b3`. Original zero-based byte offset 8575; removed bytes 0; inserted bytes 0.

Identity control, no inserted bytes.

### seller

Input `inputs/seller.xml`; 11350 bytes; SHA-256 `5b497fed620cc394630caafdfa2f57bdf69f02577b9bd2d9d07824d5dd99c3b9`. Recipe SHA-256 `828af4bcc09a162ddaf86157c00f8af6c37fb5ffd79ea4f83e089dadbf675c84`. Original zero-based byte offset 8575; removed bytes 0; inserted bytes 136.

```xml
      <ram:SellerTaxRepresentativeTradeParty>
        <ram:ID>DEV-SELLER-REP-41</ram:ID>
      </ram:SellerTaxRepresentativeTradeParty>
```
The inserted bytes include a final LF after the closing tag, as recorded exactly in the JSON ledger.

### buyer

Input `inputs/buyer.xml`; 11370 bytes; SHA-256 `e5741e2fc511ea60e8500457d46b65bba8a96581ed175233b8e84e6cbf1891a7`. Recipe SHA-256 `4ea3ecb1efc3b7692bee488f530eaf64da12d2e9a125f8d1a91970bd94479edc`. Original zero-based byte offset 8575; removed bytes 0; inserted bytes 156.

```xml
      <ram:BuyerTaxRepresentativeTradeParty>
        <ram:Name>Development Buyer Representative 73</ram:Name>
      </ram:BuyerTaxRepresentativeTradeParty>
```
The inserted bytes include a final LF after the closing tag, as recorded exactly in the JSON ledger.

### combined

Input `inputs/combined.xml`; 11506 bytes; SHA-256 `3c5b59ed20b1cdf49c9d5030c1c3cb1a7b4ac942779dfe3dd48b1b8d88838431`. Recipe SHA-256 `04464c3d4a9680cecca95b4c93ac56cb7377d9a95490a270a8a88c4b95d7988d`. Original zero-based byte offset 8575; removed bytes 0; inserted bytes 292.

```xml
      <ram:BuyerTaxRepresentativeTradeParty>
        <ram:Name>Development Buyer Representative 73</ram:Name>
      </ram:BuyerTaxRepresentativeTradeParty>
      <ram:SellerTaxRepresentativeTradeParty>
        <ram:ID>DEV-SELLER-REP-41</ram:ID>
      </ram:SellerTaxRepresentativeTradeParty>
```
The inserted bytes include a final LF after the closing tag, as recorded exactly in the JSON ledger.

## Exact original source association retained

Only these three exact CEN source members were read from the verified `cen.zip` snapshot: `schematron/preprocessed/EN16931-CII-validation-preprocessed.sch`, `schematron/CII/EN16931-CII-syntax.sch`, `schematron/abstract/EN16931-CII-syntax.sch`. Source node records also inspect the already configured `resources/cii/16b/xsl/EN16931-CII-validation.xsl`. No example/test/corpus source members were selected. Complete config members are verified and extracted for executing the unchanged full configuration, not for selecting invoice examples.

With `sch=http://purl.oclc.org/dsdl/schematron`, preprocessed node `/sch:schema[1]/sch:pattern[2]/sch:rule[14]/sch:assert[61]` (line 659) has ID CII-SR-282 and test `not(ram:SellerTaxRepresentativeTradeParty/ram:ID)`; the rule context is `/rsm:CrossIndustryInvoice/rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement`.

Original binding nodes `/sch:pattern[1]/sch:param[289]` and `[290]` (427 and 429) preserve the same name CII-SR-282 twice, first value `not(ram:BuyerTaxRepresentativeTradeParty)`, then `not(ram:SellerTaxRepresentativeTradeParty/ram:ID)`. Abstract `/sch:pattern[1]/sch:rule[14]/sch:assert[61]` (353) has `test="$CII-SR-282"` and its original `$ApplicableHeaderTradeAgreement ` context retains trailing whitespace.

Configured `xsl:transform[1]/xsl:template[79]/xsl:choose[1]/xsl:otherwise[1]/schxslt:rule[1]/xsl:if[61]/svrl:failed-assert[1]` (7442–7448, xsl:if at 7441) emits CII-SR-282 with test `not(ram:BuyerTaxRepresentativeTradeParty)` under guard `not(not(ram:BuyerTaxRepresentativeTradeParty))`; template match retains its original trailing space, priority 40, mode d17e41. `source-nodes.json` retains exact expanded namespace locators, ordered duplicate occurrences, attributes, contexts and source member hashes. The original complete source bytes are retained alongside these decoded node views.

The fresh full-oracle observations provide bounded development behavior associated with the configured executable bytes. The preprocessed assertion and configured emitter still have different tests. No expected verdict was inferred from that difference, no alternate source test was executed, and neither test was chosen as the governing meaning. Duplicate substitution remains a possible explanation, not an established cause. Root/compiler/runtime still need a reviewed source-association policy; these observations do not authorize changing source pins or implementing an invoice specialcase.

## Consumed-byte closure and process provenance

`prepare.py` reads each selected artifact once into memory, verifies the original pin, validates ZIP member sets without printing them, then copies the verified archive/JAR bytes. Config is extracted from that same verified in-memory archive and every original member digest checked. CEN source extraction uses the explicit three-member allowlist. Original Oracle.java is copied after checking its exact frozen hash. javac compiles that copied source, and Java uses the copied JAR and full copied config; neither process consumes a subsequently reopened cache JAR/config/bridge.

Before/after launch the Python driver checks the snapshot archives/JAR, copied bridge, every extracted config byte against the verified original archive, pinned Python executable, JDK java/javac binaries and JDK release file. The in-process Java driver verifies a hashed closure manifest before configuration loading and after evaluation, binds actual loaded DefaultCheck JAR and Oracle class locations, and hashes the exact byte array passed into unchanged Oracle.evaluate. The manifest includes the full original configuration and actual compiled class bytes. Original compilation inputs are also bound. Each completed receipt binds the input and runtime closure hashes. Read-only file modes reduce accidental mutation; this is local byte provenance, not a security boundary or attestation.

Corrected runtime closure SHA-256 `d399d7b5470563d3c1ef43849bd4f39aad79602062eb193e741c4868a576d5e3`; 103 verified entries. Its member inventory is retained locally, not dumped here. `provenance.json`, `run-metadata.json`, `attempt-02/run-metadata.json`, each `*.command.json`, `*.metadata.json`, `*.receipt.json`, and raw streams bind commands, working directory, executable paths, process outcomes and byte digests. Inherited JAVA_TOOL_OPTIONS, JDK_JAVA_OPTIONS, _JAVA_OPTIONS, CLASSPATH and JDK_JAVAC_OPTIONS are removed; JAVA_HOME is set to the exact requested JDK. Shell was `/bin/bash`, login=false.

Selected artifact pins:

```json
{
  "validator.jar": {
    "sha256": "799e64befca97d4080e03608c80b85dd5a5ecc5f4ae4f35d1116ec2855b9a7c9",
    "version": "1.6.3",
    "url": "https://github.com/itplr-kosit/validator/releases/download/v1.6.3/validator-1.6.3-standalone.jar"
  },
  "config.zip": {
    "sha256": "2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8",
    "version": "2026-08-31",
    "url": "https://github.com/itplr-kosit/validator-configuration-xrechnung/releases/download/v2026-08-31/xrechnung-3.0.2-validator-configuration-2026-08-31.zip"
  },
  "cen.zip": {
    "sha256": "1cd53cb8a84d38aedc82c0caede217da983a7934dd663f793a092fd66443c561",
    "version": "1.3.16",
    "url": "https://github.com/ConnectingEurope/eInvoicing-EN16931/releases/download/validation-1.3.16/en16931-cii-1.3.16.zip"
  }
}
```

Unchanged Oracle.java SHA-256 `37565bfaba63f452018596d422a65a7d969f06245bbe33a134e95f92fe2cad3e`. Full configuration selected CEN XSLT SHA-256 `0911e927f13f9ae2cdc7f973643f27bf0ad3c5da02483c04a5b8a47eb4822199`; selected XR XSLT SHA-256 `ec54f662803a87f527ec606d1320bc404f1657a848af6a76f37f32905e4aac40`. No separate release CEN stylesheet was substituted.

Exact executable metadata (java/javac and release hashes rechecked after evaluations):

```json
{
  "python": {
    "invoked": "/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache/test-venv/bin/python",
    "resolved": "/Library/Frameworks/Python.framework/Versions/3.12/bin/python3.12",
    "version": "3.12.6",
    "sha256": "69e6adfac00978215c4e5f3eaf63d262a5fb646b6dca1897b29f78872c4c5a26"
  },
  "jdk": {
    "java": {
      "invoked": "/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home/bin/java",
      "resolved": "/Users/bene/.local/share/mise/installs/java/corretto-27.0.1.0.11/bin/java",
      "sha256": "346f29eee10a91d3d5012c27e08f2fe13d7977e10599084fba835f5bdb018800"
    },
    "javac": {
      "invoked": "/Library/Java/JavaVirtualMachines/corretto-27.0.1.0.11/Contents/Home/bin/javac",
      "resolved": "/Users/bene/.local/share/mise/installs/java/corretto-27.0.1.0.11/bin/javac",
      "sha256": "ddc0e3c472717b1f2c579bf08c1cd7a8eed5c96224bb3e179232a18dc030164a"
    },
    "release_sha256": "bb9c7cae305c4a62cf9e2ac132984689869e78cff3f942f47a57bde3d82ce91a"
  }
}
```

Version output:

```text
openjdk version "27.0.1" 2026-10-20
OpenJDK Runtime Environment Corretto-27.0.1.0.11 (build 27.0.1+11-FR)
OpenJDK 64-Bit Server VM Corretto-27.0.1.0.11 (build 27.0.1+11-FR, mixed mode, sharing)
javac 27.0.1
```

## Reproduction

The scripts refuse existing attempt/output paths. Preserve this observation directory; copy only these six authoring/driver files and the report generator into a new adjacent bounded scratch directory to repeat the exact failed-then-corrected sequence. Do not copy generated outputs. The preselected parent seed remains the same exact SHA-256, and original source pins/cache are read-only. Before rerunning, verify copied script digests against the ledgers in this report’s directory. Substitute the new bounded directory for DIR in the commands below. No invocation calls Oracle.main, warmup loops, metrics, benchmark loops or the cache-writing oracle Python CLI.

```sh
PY=/Users/bene/.t3/worktrees/rust-einvoice/benediktschneppe-bsc-5--45552c76f3d0fedb/tools/oracle/.cache/test-venv/bin/python
DIR=.scratch/bsc10/expansion/discriminant
"$PY" -B "$DIR/prepare.py"
"$PY" -B "$DIR/declare.py"
"$PY" -B "$DIR/run.py"
# Retained first-attempt JDK guard failures are expected process observations.
"$PY" -B "$DIR/revise.py"
"$PY" -B "$DIR/run_v2.py"
"$PY" -B "$DIR/report_results.py"
```

Required copied files are `prepare.py`, `declare.py`, `run.py`, `DiscriminantDriver.java`, `revise.py`, and `report_results.py`; revise.py reproducibly creates run_v2.py and DiscriminantDriverV2.java. Full exact javac/java argv arrays are in retained per-process command JSON. The compact commands above are exactly the executed authoring/evaluation/report entry points, not a download or cache-preparation recipe.

## Frozen audit and limits

```json
{
  "frozen_before": {
    "files": 39,
    "status": "opaque_bytes_verified",
    "lock_sha256": "07bba584efbda8a8ac7433bd332fea5222c5f6f92e698bd97c8586bf5d090524"
  },
  "frozen_after": {
    "files": 39,
    "status": "opaque_bytes_verified",
    "lock_sha256": "07bba584efbda8a8ac7433bd332fea5222c5f6f92e698bd97c8586bf5d090524"
  },
  "artifact_lock_sha256": "5dec502919d8ee137cd2c1453be8c65c1176e8bc707e358f646cef3fb1f55175",
  "ledger_sha256": "8496eb001e6bfea360d40565cbebec1d95c955dfb5f732888af22c9af1c4f099",
  "revision_sha256": "d827a710d6d9a2c4afb81a620e45156afd4b0eaf298e72b5894463e01e85c668",
  "runtime_closure_sha256": "d399d7b5470563d3c1ef43849bd4f39aad79602062eb193e741c4868a576d5e3",
  "runtime_closure_entries_verified": 103,
  "all_four_exact_insertion_lineages_verified": true,
  "complete_attempts_retained": 8,
  "oracle_evaluate_completed": 4,
  "native_execution": false,
  "runtime_coverage": "unavailable",
  "conformance_claim": false,
  "resource_claim": false,
  "attestation_claim": false,
  "release_claim": false,
  "report_generator_sha256": "3fbef1fd09e3766168cdfad6828fbcd80411ff3c94ede7e667c4ca17acf80943"
}
```

The 39 frozen files were checked as opaque byte streams against the unchanged evidence lock before and after both attempts, including the frozen results, splits and workload; no result labels were deserialized. Artifact lock and original bridge are unchanged. No held-out inputs, labels or family identities were accessed or emitted. No implementation inventory changes were made. Root AGENTS.md was absent in the bound worktree; the project-root `/Users/bene/RustroverProjects/rust-einvoice/AGENTS.md` and assigned expansion plan were read. The sibling source-inventory work was not edited.

Limits: one known public development ancestor and three deterministic mutations; no independent corpus diversity or protected evaluator separation. Full KoSIT bridge output is retained, not a raw SVRL capture. BASE severity is normalized. Diagnostic absence does not prove assertion evaluation or coverage. JDK executable/release hashes are provided, but the complete OS/JVM/Python transitive runtime is not attested. No alternate source compiler/test was run. No native implementation comparison or decision about which conflicting source representation wins was made.

## Full corrected bridge records

### baseline

Raw unchanged JSON stdout: `attempt-02/processes/baseline.stdout`.

```json
{
  "format": 1,
  "id": "bsc10-development-baseline",
  "sha256": "881f7bc3ff0513a6627a981301af2c0ec9bcdce2c9d1628d7858e581f4d8e91c",
  "report_html_elements": 1,
  "engine": "KoSIT 1.6.3 / Saxon HE 12.9",
  "target": "xrechnung-3.0.2-cii-2026-08-31",
  "processing": "completed",
  "processing_errors": [],
  "scenario": "EN16931 XRechnung (CII)",
  "target_matched": true,
  "stages": {
    "val-xsd": "valid",
    "val-sch.1": "valid",
    "val-sch.2": "valid",
    "val-xml": "valid"
  },
  "schema": "valid",
  "business": "evaluated",
  "findings": [],
  "structural_findings": [],
  "reference_acceptance": "acceptable",
  "acceptance": "accepted"
}
```

### seller

Raw unchanged JSON stdout: `attempt-02/processes/seller.stdout`.

```json
{
  "format": 1,
  "id": "bsc10-development-seller",
  "sha256": "5b497fed620cc394630caafdfa2f57bdf69f02577b9bd2d9d07824d5dd99c3b9",
  "report_html_elements": 1,
  "engine": "KoSIT 1.6.3 / Saxon HE 12.9",
  "target": "xrechnung-3.0.2-cii-2026-08-31",
  "processing": "completed",
  "processing_errors": [],
  "scenario": "EN16931 XRechnung (CII)",
  "target_matched": true,
  "stages": {
    "val-xsd": "valid",
    "val-sch.1": "invalid",
    "val-sch.2": "valid",
    "val-xml": "valid"
  },
  "schema": "valid",
  "business": "evaluated",
  "findings": [
    {
      "id": "BR-18",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-18]-The Seller tax representative name (BT-62) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
      "stage": "val-sch.1"
    },
    {
      "id": "BR-19",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-19]-The Seller tax representative postal address (BG-12) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
      "stage": "val-sch.1"
    },
    {
      "id": "BR-20",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-20]-The Seller tax representative postal address (BG-12) shall contain a Tax representative country code (BT-69), if the Seller (BG-4) has a Seller tax representative party (BG-11).",
      "stage": "val-sch.1"
    },
    {
      "id": "BR-56",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-56]-Each Seller tax representative party (BG-11) shall have a Seller tax representative VAT identifier (BT-63).",
      "stage": "val-sch.1"
    }
  ],
  "structural_findings": [],
  "reference_acceptance": "reject",
  "acceptance": "rejected"
}
```

### buyer

Raw unchanged JSON stdout: `attempt-02/processes/buyer.stdout`.

```json
{
  "format": 1,
  "id": "bsc10-development-buyer",
  "sha256": "e5741e2fc511ea60e8500457d46b65bba8a96581ed175233b8e84e6cbf1891a7",
  "report_html_elements": 1,
  "engine": "KoSIT 1.6.3 / Saxon HE 12.9",
  "target": "xrechnung-3.0.2-cii-2026-08-31",
  "processing": "completed",
  "processing_errors": [],
  "scenario": "EN16931 XRechnung (CII)",
  "target_matched": true,
  "stages": {
    "val-xsd": "valid",
    "val-sch.1": "invalid",
    "val-sch.2": "valid",
    "val-xml": "valid"
  },
  "schema": "valid",
  "business": "evaluated",
  "findings": [
    {
      "id": "CII-SR-282",
      "severity": "warning",
      "effective_severity": "warning",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]",
      "line": null,
      "column": null,
      "message": "[CII-SR-282] - BuyerTaxRepresentativeTradeParty should not be present",
      "stage": "val-sch.1"
    }
  ],
  "structural_findings": [],
  "reference_acceptance": "acceptable",
  "acceptance": "accepted"
}
```

### combined

Raw unchanged JSON stdout: `attempt-02/processes/combined.stdout`.

```json
{
  "format": 1,
  "id": "bsc10-development-combined",
  "sha256": "3c5b59ed20b1cdf49c9d5030c1c3cb1a7b4ac942779dfe3dd48b1b8d88838431",
  "report_html_elements": 1,
  "engine": "KoSIT 1.6.3 / Saxon HE 12.9",
  "target": "xrechnung-3.0.2-cii-2026-08-31",
  "processing": "completed",
  "processing_errors": [],
  "scenario": "EN16931 XRechnung (CII)",
  "target_matched": true,
  "stages": {
    "val-xsd": "valid",
    "val-sch.1": "invalid",
    "val-sch.2": "valid",
    "val-xml": "valid"
  },
  "schema": "valid",
  "business": "evaluated",
  "findings": [
    {
      "id": "BR-18",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-18]-The Seller tax representative name (BT-62) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
      "stage": "val-sch.1"
    },
    {
      "id": "BR-19",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-19]-The Seller tax representative postal address (BG-12) shall be provided in the Invoice, if the Seller (BG-4) has a Seller tax representative party (BG-11).",
      "stage": "val-sch.1"
    },
    {
      "id": "BR-20",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-20]-The Seller tax representative postal address (BG-12) shall contain a Tax representative country code (BT-69), if the Seller (BG-4) has a Seller tax representative party (BG-11).",
      "stage": "val-sch.1"
    },
    {
      "id": "BR-56",
      "severity": "error",
      "effective_severity": "error",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}SellerTaxRepresentativeTradeParty[1]",
      "line": null,
      "column": null,
      "message": "[BR-56]-Each Seller tax representative party (BG-11) shall have a Seller tax representative VAT identifier (BT-63).",
      "stage": "val-sch.1"
    },
    {
      "id": "CII-SR-282",
      "severity": "warning",
      "effective_severity": "warning",
      "location": "/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}CrossIndustryInvoice[1]/Q{urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100}SupplyChainTradeTransaction[1]/Q{urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100}ApplicableHeaderTradeAgreement[1]",
      "line": null,
      "column": null,
      "message": "[CII-SR-282] - BuyerTaxRepresentativeTradeParty should not be present",
      "stage": "val-sch.1"
    }
  ],
  "structural_findings": [],
  "reference_acceptance": "reject",
  "acceptance": "rejected"
}
```

