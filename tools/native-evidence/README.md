# Native evidence requirements

This proposal defines what BSC-10 needs to evaluate authentic native observations: which facts to retain, how to represent missing evidence, and when processing is complete. BSC-8 owns the runtime, source walker, capture and serializer; BSC-10 owns these requirements and the later independent consumer. Serialization and versioning need BSC-8 agreement before any wire format is frozen. No producer or consumer is implemented here.

The [API map](API-MAP.md) distinguishes inspected APIs from future evidence requirements and pins each cited file by revision and SHA-256. The merged XSD snapshot is `adb46923c33d175ba08b3c48d9292823643afadd`; the cited files are unchanged from `40349ae9018350cf917256d39805a7cf9dcff75f`. The source arena was inspected at immutable local commit `79c93ff332b0f5967dbb7ef3431d391264efc92e`, unpublished at inspection. This identifies source bytes, not a loaded or executing implementation. Moving scalar-runtime APIs are excluded.

## Preserve what the backend observed

A producer must retain one arrival-ordered stream of admitted error and warning callbacks, including duplicates and every raw field. Retain error constraint, message, nullable location, subject, structured instance path and local-name element path; retain warning code, message, nullable location, subject and structured path. Preserve namespace/local-name parts and sibling positions, rather than only rendered paths. Raw display text may accompany these facts but cannot replace them. The inspected warning API has no rich evidence accessor. [X1–X3](API-MAP.md#xsd-observations)

For each error, preserve optional `DiagnosticEvidence` and all available native enum variants and payloads, including nested value/facet errors and every component/type field. The [payload inventory](API-MAP.md#typed-payload-inventory) states what is available. Keep outer governing type, innermost observed type, specific component and facet origin distinct. Anonymous names and unavailable origins stay unavailable. The observed innermost dispatch is not necessarily a primitive type or the declaration supplying an inherited facet.

Missing evidence needs an explicit reason at the applicable observation: not exported by this API, not captured at this branch, not applicable, or lost because observation stopped, as actually known. An API `None` without further information means unavailable; it does not justify a more specific explanation. Do not guess positions, causal parents, primary/collateral relationships, expected particles/cardinalities, enumeration sets or other constraint parameters from prose, codes or nearby schema declarations. Do not suppress extra diagnostics or deduplicate equal tuples. Equal counts, codes or prose do not establish structural semantic equivalence. [X1, X2; F1](API-MAP.md#evidence-matrix)

`tools/schema-backend/examples/raw_records.rs` is insufficient as a rich producer: it prints Debug-oriented tuples, omits `evidence()`, and prints all errors before warnings. The collecting sink also separates the two arrays, losing their interleaving. The example supplies neither original-source joins nor loaded-byte/output bindings nor a partial-stop artifact. Its text cannot recover the missing observations. [X3, X5](API-MAP.md#xsd-observations)

## Completion and validity are separate observations

Retain the actual `end_validation()` result, optional `operational_failure()`, any adapter stop cause, the retained event extent and whether observation is complete. Completion can itself emit deferred IDREF findings, with subjects saved when deferred; the finalization batch can stop part-way. Traversal ending or the root element closing is insufficient for successful finalization. [X4](API-MAP.md#xsd-observations)

The first operational failure persists even if a stop signal clears. Later pushes and callbacks are inert, and prior admitted findings remain. Preserve that failure out of band, rather than inserting it as an invoice diagnostic. The backend's boolean stop hook and generic `validation-aborted` code do not identify an adapter's exhausted count/byte dimension, cancellation or output failure; preserve the actual adapter cause when observed and mark the remainder unknown. A completion error without an operational failure must retain its actual error and context, not be assigned a guessed operational category.

An `Ok(())` completion is not a validity verdict. Retain actually observed validity and assessment extent separately; `NotKnown` is not valid. A node validity observation before finalization does not erase deferred findings. A final XSD stage verdict needs successful finalization and the actual validity evidence, including deferred findings. A failed stage can preserve earlier observed invalidity, but failure, cancellation, overflow, observer refusal, timeout, crash, unsupported capability or truncated output never becomes completed rejection or successful parity. [X4, X6; F1](API-MAP.md#evidence-matrix)

## Original XML witnesses

When a real source walker supplies a diagnostic-to-handle join, bind it to exact original XML bytes (digest and length), encoding, BOM length, source/parser identity and the validated root. These joins are currently unavailable. Preserve schema-source and rule-source coordinates separately from original invoice coordinates. A schema URI requires a binding to the actually loaded schema document; its recorded span may cover only a start tag. Neither that span nor a compiler `Location` is an invoice range. [S1–S3; X2; I1](API-MAP.md#evidence-matrix)

| Witness | Required meaning |
| --- | --- |
| Original UTF-8 range | Zero-based, half-open offsets into original bytes, including the BOM offset. A node after a UTF-8 BOM starts after those bytes. |
| Original UTF-16/Latin1/ASCII range | Zero-based, half-open Unicode scalar offsets into the original decoded text before normalization, excluding BOM. CRLF counts twice, a supplementary character once, and references retain lexical spelling. Never substitute offsets into transcoded UTF-8. |
| Source line/column | Original one-based position, with scalar column excluding BOM; CRLF is one diagnostic newline. Preserve missing coordinates without guessing. |
| Node/attribute identity | Opaque, parsed-document-qualified typed identity. It is not a portable wire ID, cross-parse ID or source-order event ID. A future serialized reference needs an explicit document-scoped binding agreed with BSC-8. |
| Path | Expanded-name occurrence witness; same-name sibling position matters. XSD paths start at the pushed validation root, which may be a fragment. Verify the root/full-document binding before joining to the source arena. Paths cannot deduplicate diagnostic occurrences. |
| Range role | Distinguish node covering range, exact attribute token, unquoted attribute value and explicit owner fallback. A text covering range may span several lexical pieces; retain available ordered pieces when used as evidence. |

An attribute has its own identity and an owner. Missing-required-attribute evidence names an absent attribute on an existing owner; it cannot supply an attribute token. An owner fallback must remain labeled as such, even if a diagnostic path names an attribute. `UnexpectedChild` retains a child expanded name while the raw subject is the parent; no triggering-child occurrence or handle is supplied by that cause. Namespace declaration tokens and effective namespace-axis nodes also have distinct roles. [S2, S3; X1, X2](API-MAP.md#evidence-matrix)

Source construction publishes an arena only after successful semantic EOF/alignment and final checks. A failed parse supplies no partial arena to join. Resource, cancellation, unsupported encoding/version/DTD, alignment and writer errors retain their actual meaning; none alone proves invoice invalidity. [S4](API-MAP.md#source-observations)

## Execution and resource evidence still needed

The following are requirements for real execution instrumentation, currently unavailable from the inspected boundary. They do not prescribe an observer framework or runtime architecture. IR source accounting, compilation, deserialization and absence of findings cannot satisfy them. [I1; F1](API-MAP.md#evidence-matrix)

| Boundary | Evidence needed from actual execution |
| --- | --- |
| Load/admission | Request/document binding; actual backend/build/features and capabilities; executable rule package and source closure; IR identity; exact selected target/source-package; resolved candidate and fixed decision-manifest content identities when used. State what bytes each digest covers. Expected pins alone do not prove loading. |
| Stage | Entered/completed/failed/skipped decision, observed verdict where applicable, reason and terminal document/run state. XML parsing, target selection, XSD, CEN, XR and acceptance remain distinct. For the exact target, CEN invalidity does not itself skip XR. Source parsing alone is not a completed invoice XML stage. |
| Context | Active schema/pattern/phase, rule and context-expression identity, actual selected source document/node and scoped evaluation occurrence. Distinguish match, nonmatch, documented recovered match fault and fatal failure. First matching rule per visited node per pattern leaves later patterns independent. |
| Demanded site | Loaded-package-scoped site identity, selected-context reference, repeated evaluation ordinal and actual outcome. Assertion true with no finding is executed; assertion false may emit a finding; evaluation error is distinct. Retain predicate/message failure boundaries and diagnostic references. Selecting a rule does not prove every site was demanded. |
| Extent | Ordered observation extent, repeated occurrences, final count/completeness and stop reason. Inactive, suppressed or not-demanded decisions count only when observed. An absent event alone leaves execution unknown. |

Logical allowances, process containment and worker lifecycle need separate evidence:

- **Logical limits:** configured dimension, unit, scope and charging rule; component revision; observed refusal and only actually exported counters. Source storage/string/namespace charges are cumulative and can overlap. The private construction meter and `QueryBudget` supply no public usage snapshot. Do not invent current/peak use by subtracting defaults or private counters. [S4](API-MAP.md#source-observations)
- **RSS and deadline:** actual enforcement settings/results, measurement source, unit/window/peak when measured, and termination/exit/signal/reaping evidence where applicable. Logical payload bytes are not RSS or allocator capacity. Source cancellation and XSD sink stop are cooperative and cannot establish a hard deadline or interruption inside native work. [S4; X3, X4](API-MAP.md#evidence-matrix)
- **Worker lifecycle:** actual worker identity/generation and admission, load, completion, discard/retirement or kill/reap/reuse decisions; concurrency and cancellation ownership where claimed. Immutable source sharing and a fresh validation runtime do not prove memory isolation or safe reuse of persistent schema state. No installed controller lifecycle evidence is established here. [X5; S4](API-MAP.md#evidence-matrix)

The existing [conformance contract](../conformance/CONTRACT.md) stays frozen: v1 coverage and resources remain unavailable and `release_gate_eligible` remains false. BSC-7 retains compiler correspondence ownership; these requirements do not repeat or certify that audit. Authentic execution also needs a controller boundary beyond candidate self-report. Custody selection remains open: the clarified boundary excludes tuning-agent credentials/tools and candidate code; a trusted human maintainer acting as custodian/operator is possible, and an external operator is optional. No policy or mandatory organizational separation is selected here.

## Prerequisite for a later independent consumer

BSC-8 agreement on capture, missingness and versioned serialization comes first. Then obtain actual XSD-only producer artifacts from owner-authored public XML/XSD examples: at least one successful finalization with separately observed validity and one partial-stop run. Retain exact input/loaded-schema identities, backend/build/features, producer identity, original output bytes/digest, callback order, full available typed payloads and terminal observations. The partial run must show the first failure, retained prefix and incomplete extent; the successful run must include finalization observations. These are backend-harness artifacts, with invoice/source/evaluator joins unavailable until integrated.

Only after that dependency is met should BSC-10 build the bounded independent consumer to verify byte bindings, agreed version/variant handling, order/duplicates, missingness, references and terminal consistency. Synthetic malformed artifacts can exercise negative checks; a synthetic positive cannot establish a genuine producer. Original-source witness checking waits for genuine walker joins. Process containment and rule execution require their own authentic evidence.

## Proposed adverse acceptance cases

These are reviewable future cases, not executed tests or new workloads in this slice. Positive paths require genuine producer emissions; mutations assess the later consumer's rejection of inconsistent evidence.

| Case | Required acceptance boundary |
| --- | --- |
| Error/warning/error, duplicate identical errors | Retain arrival order and multiplicity; detect split-array concatenation, sorting, omission or deduplication. |
| Typed facet or content failure | Preserve all available payloads and explicit unknowns; reject invented enumeration sets, content cardinalities, primitive/facet origins or causal-parent links. Extra diagnostics remain present. |
| UTF-8 BOM/multibyte; UTF-16 BOM/supplementary character/CRLF; Latin1/ASCII | Check original digest, encoding, coordinate unit and BOM convention; reject transcoded-byte, normalized-scalar or byte/scalar substitutions. Requires real source joins. |
| Namespace aliases and repeated/interleaved siblings | Resolve the actual expanded-name occurrence; reject a wrong repeated node even when code/message/count match. Requires real source joins. |
| Attribute value, missing attribute, explicit owner fallback; split text pieces | Keep attribute token/value/owner roles distinct; reject owner-as-exact-attribute and covering-text-as-single-token claims. Requires real source joins. |
| Schema/rule span presented as invoice span | Reject the coordinate-domain substitution; retain actual schema origin with its loaded-document binding. |
| Stop after admitted events, clear signal, push again | Preserve first failure and retained prefix; no resumed callbacks or completed rejection. Distinguish actual adapter cause from unknown details. |
| Stop during deferred IDREF finalization | Retain deferred subjects and the admitted prefix; traversal end and earlier observed invalidity cannot rescue incomplete processing. |
| Same site at two contexts; true/no finding; false/finding; predicate/message error | Preserve each actual evaluation and outcome; no source-list or empty-finding coverage inference. Requires real evaluator instrumentation. |
| Changed loaded identity, stale terminal, truncation, output overflow, unsupported, timeout/crash | Reject completeness/acceptance claims while preserving partial observations; self-reported hashes/timestamps cannot prove authentic execution. |
| Logical refusal with absent usage; memory kill or failed worker reuse | Keep unknown counters/dimensions explicit; no RSS/deadline/isolation conclusion from logical limits. Positive containment claims require controller evidence. |
