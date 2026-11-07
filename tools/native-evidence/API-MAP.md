# Native evidence API map

This map supports the [proposed requirements](README.md). API availability means source can expose a fact; it does not mean a run produced or serialized it.

`M` means immutable merged snapshot `adb46923c33d175ba08b3c48d9292823643afadd`. Every M file below is byte-identical at earlier merged snapshot `40349ae9018350cf917256d39805a7cf9dcff75f`. `L` means immutable local BSC-8 snapshot `79c93ff332b0f5967dbb7ef3431d391264efc92e`, whose parent is M; it was unpublished at inspection. L is not a claim of merged, hosted, installed or executing source. Moving scalar-runtime APIs are outside this map.

Locators below are repository paths and one-based line ranges within those snapshots. The [hash appendix](#hash-appendix) binds the exact file bytes. Inspect a file with `git show <full-revision>:<repository-path>` in a repository containing that revision; no machine-specific checkout path is an interface requirement. L paths are written as code because they are absent at M.

## Evidence matrix

| Evidence | Inspected API/source | Gap and ownership |
| --- | --- | --- |
| Raw ordered diagnostic observations | M X1/X3: error and warning fields, synchronous callbacks | Collecting sink splits arrays. BSC-8 capture/serializer must retain arrival order, duplicates and all fields; BSC-10 later checks preservation. |
| Typed cause/type/component | M X2: optional owned evidence and native payloads | Branch coverage is incomplete; facet origin unavailable. No causal-parent graph or general constraint-parameter inventory. |
| Terminal state and observed validity | M X4/X6: finalization result, first operational failure, node assessment | No rich serialized partial-stop artifact. Actual adapter stop cause and final stage interpretation need BSC-8 integration. |
| Original bytes and scoped handles | L S1/S2: bytes, encoding, BOM, typed document-qualified identities | No inspected source-to-XSD walker/diagnostic handle join or portable serialized registry. BSC-8 owns integration. |
| Original versus schema/rule coordinates | L S1/S3; M X2/I1 | Source ranges exist separately; a verified join and actual loaded document bindings are missing. |
| Stage/context/site outcomes | M I1 contains inspection identities; F1 states stage obligations | Actual loaded identities, driver/context/site instrumentation and repeated evaluations are unavailable at this boundary. Compilation and absence of findings supply no execution evidence. |
| Logical resources | L S4: limits, typed refusals, cooperative cancellation | No public construction/query usage snapshot or end-to-end resource observations. BSC-8 owns actual runtime observations. |
| RSS/hard deadlines/workers | M X3/X5 and L S4 explicitly limit cooperative/logical claims | Actual enforcement, measurement, kill/reap/retire/reuse and isolation evidence need controller integration; none is established here. |
| Existing consumer limits | M F1 | Frozen v1 accepts unavailable coverage/resources and cannot confer release eligibility. BSC-10 owns future evidence/consumer requirements. |

## XSD observations

- **X1:** M `vendor/xsd-schema/src/validation/errors.rs:78–171,206–265,461–487,590–616`. `ValidationSubject::{Element,Attribute}` contains expanded names. `PathStep::Element` contains name and one-based same-expanded-name sibling position; `Attribute` contains name. `InstancePath::steps()` exposes ordered structured steps; its root is the pushed validation root, including fragments. `ValidationError` exposes `constraint`, `message`, nullable `location`, `subject()`, `instance_path()` and `element_path()`. The latter drops namespaces/positions. Unexpected-child raw subjects identify the parent; deferred diagnostics retain the recorded subject.
- **X2:** M `vendor/xsd-schema/src/validation/errors.rs:508–543,1028–1072,1172–1223`. `ValidationError::evidence()` returns optional owned `DiagnosticEvidence`: `cause`, `governing_type`, `actual_type`, `component`, `facet_origin`. `SchemaComponent` retains `kind`, optional declared `name`, applicable `global`, `document_uri`, `schema_span`; kinds are `SimpleType`, `ComplexType`, `Attribute`, `Element`. Anonymous types have no name; local names are not global identities. Facet origin is currently unavailable; type origin does not identify an inherited effective facet. Schema spans are recorded schema ranges, often start tags. M `vendor/xsd-schema/src/parser/location.rs:19–87` defines byte `SourceSpan`, schema `SourceRef`, and `SourceLocation { base_uri, line, column }` (one-based line/column); diagnostic locations and component schema spans remain separate evidence.
- **X3:** M `vendor/xsd-schema/src/validation/validator.rs:25–145`. `ValidationSink::{on_error,on_warning,stop_requested}` supplies two callback kinds and a cooperative boolean hook. `ValidationWarning` exposes `code`, `message`, nullable `location`, `subject()`, `instance_path()`; no public rich evidence accessor. `CollectingValidationSink` holds separate error/warning vectors; `ErrorOnlySink` discards warnings. The hook cannot interrupt an in-progress regex/type/native content-model operation.
- **X4:** M `vendor/xsd-schema/src/validation/runtime.rs:5575–5657,6054–6107`. `ValidationRuntime::end_validation()` returns `Result<(), ValidationError>`, checks control/state/stack and emits deferred IDREF diagnostics. A stop can interrupt that batch. `abort_run()` records the first out-of-band operational failure; `operational_failure()` exposes it. Dispatch checks control before and after callbacks; first failure persists and later pushes/callbacks are inert. Earlier sink observations survive. Completion errors also include invalid state/unclosed elements, so `Err` and optional operational failure must both be retained.
- **X5:** M `tools/schema-backend/examples/raw_records.rs:6–50` prints layouts and raw tuples, errors before warnings, without `evidence()` or a terminal stop artifact. M `tools/schema-backend/README.md:3–14,16–27,37–42,46–85` identifies the unpublished default-feature backend harness, source-digest domain, evidence/control limits and open worker/persistent-name-table policy. It does not implement the invoice facade, source walker or worker pool. `vendor/xsd-schema.provenance.json:2–34` records `xsd-schema 0.3.0`, local revision `bsc8-maintenance-1`, archive digest and ordered patches. This map checks selected file bytes, not archive replay, complete package identity, feature resolution or executing binary identity.
- **X6:** M `vendor/xsd-schema/src/validation/info.rs:11–33,338–350` exposes `SchemaValidity::{NotKnown,Valid,Invalid}`, `ValidationAttempted::{None,Partial,Full}` and `SchemaInfo.validity/validation_attempted`. M `vendor/xsd-schema/src/validation/runtime.rs:4595–4613` exposes `validate_end_element_novalue()` returning node validity. These node observations and `end_validation()` completion are distinct; neither alone proves complete invoice acceptance.

### Typed payload inventory

These are source variants and fields, not proposed wire tags. Preserve the variant and its entire available payload; an unhandled variant must not silently disappear or become a prose-only cause. The future agreed version boundary must handle incompatibility explicitly.

| X2 `NativeCause` variants | Available payload |
| --- | --- |
| `Value`, `Facet` | Complete nested type-validation error / facet error below |
| `UnexpectedChild` | `found` expanded name; no triggering-child occurrence |
| `IncompleteContent` | No expected-particle/cardinality payload |
| `MissingRequiredAttribute` | Required expanded `name`; no absent token |
| `ProhibitedAttribute`, `AttributeNotAllowed` | No exact use-component/wildcard payload |
| `AttributeFixedValue` | `expected`, `actual` strings |
| `UndeclaredElement`, `NilNotAllowed`, `NilWithFixedValue`, `NilWithContent` | No additional payload |
| `XsiTypeUnresolved` | Original lexical `value` |
| `XsiTypeDerivation` | Original lexical `value`, `blocked` boolean |

M `vendor/xsd-schema/src/types/validators.rs:30–50` defines the nested type `ValidationError` (distinct from the instance diagnostic):

| Variant | Entire payload |
| --- | --- |
| `InvalidLexical` | `value`, `type_name`, `message` |
| `FacetViolation` | Complete `FacetError` |
| `TypeError` | Typed `expected`, `actual` type codes |
| `RangeError` | `value`, `type_name` |

M `vendor/xsd-schema/src/error.rs:14–89` defines `FacetError`:

| Variants | Entire payload |
| --- | --- |
| `LengthViolation`, `ExplicitTimezoneViolation`, `DerivationRestriction`, `ConflictingFacets` | `message` |
| `MinLengthViolation` | `actual`, `min` |
| `MaxLengthViolation`, `TotalDigitsViolation`, `FractionDigitsViolation` | `actual`, `max` (retain native numeric type/range) |
| `PatternViolation` | `value`, `pattern` |
| `EnumerationViolation` | `value`; no allowed-set payload |
| `MinInclusiveViolation`, `MinExclusiveViolation` | `value`, `min` |
| `MaxInclusiveViolation`, `MaxExclusiveViolation` | `value`, `max` |
| `InvalidPattern` | `pattern`, `message` |
| `FixedFacetViolation` | `facet_name`, `base_value`, `derived_value` |
| `NotApplicable` | `facet`, `type_name` |

## Source observations

- **S1:** L `crates/einvoice-source/src/lib.rs:1–24` exports the source types. `crates/einvoice-source/src/document.rs:7–62,215–249,280–295` defines `Encoding`, `EvidenceSpan::{Utf8Byte,UnicodeScalar}`, `OriginalUtf8Span`, `OriginalScalarSpan`, source `SourceLocation`, and `SourceDocument::{parse,parse_with_limits,parse_cancellable,original_bytes,source_view,encoding,bom_bytes}`. UTF-8 ranges index original bytes including BOM offset; other encodings use original decoded scalar coordinates excluding BOM and before normalization. `source_view()` excludes BOM. `crates/einvoice-source/README.md:119–130` specifies CRLF, supplementary-character and lexical-reference counting.
- **S2:** L `crates/einvoice-source/src/document.rs:297–330,396–418,474–529,661–674`. Opaque `NodeIdentity` combines parsed-document identity and typed local identity. `Node::{identity,occurrence,covering_span,location,pieces,compare_order}` and `Attribute::{owner,token,value_span,owner_fallback_span,identity,order,canonical_path}` distinguish node, attribute and owner evidence. Order comparison is same-document only; identities are not importable local ordinals or a portable wire API. Text covering ranges are not single tokens. `DocumentOrder` has no arbitrary cross-document ordering.
- **S3:** L `crates/einvoice-source/src/document.rs:676–745` exposes `LedgerOwner::{Node,Attribute,Declaration}` and `LedgerEntry::{kind,source,gap_before,span,value_span,location,owner}`. L `crates/einvoice-source/src/render.rs:67–134` renders budgeted document, element, attribute, text, comment and PI paths, with cached occurrence (PI by target). L `crates/einvoice-source/README.md:89–130` distinguishes rendering evidence from identity and lexical pieces from nodes; namespace declaration tokens and namespace-axis nodes remain distinct.
- **S4:** L `crates/einvoice-source/src/parse.rs:20–40,189–220` publishes the arena only after semantic EOF/alignment, occurrence caching and final checks. L `crates/einvoice-source/src/error.rs:4–39` defines `Resource { dimension, limit }` (`limit: None` for opaque parser refusal) and `SourceError` categories: invalid limits, resource, cancelled, syntax with nullable position, unsupported DTD/encoding/version, alignment, output. L `crates/einvoice-source/src/budget.rs:6–51,116–155,235–278` defines finite logical `SourceLimits`, private `Meter`, and `QueryBudget` with no public usage snapshot. L `crates/einvoice-source/README.md:140–178` states cumulative overlapping payload charges, allocator limits and checkpoint cancellation, without RSS/hard-deadline guarantees.

## Inspection identities and frozen consumer

- **I1:** M `tools/validation-generator/crates/validation-ir/src/lib.rs:19–34,189–224,243–268,350–384` supplies rule-source XML `Location`, ordered site/rule/pattern data, schema selection/dispatch and exact target/source-package selection. `tools/validation-generator/crates/validation-ir/src/resolved.rs:1–2,47–82` supplies non-executable candidate/lineage data with raw IR and fixed decision-manifest digests. `tools/validation-generator/crates/validation-ir/README.md:3–25,73–102` states first-match-per-node-per-pattern dispatch, loaded-package-scoped site identities, inspection-only status, pending compiler correspondence and future demand/error-boundary obligations. None is a runtime loading, matching or evaluation observation API. BSC-7's correspondence audit remains separate.
- **F1:** M `tools/conformance/CONTRACT.md:66–86,129–181,205–241,258–268,296–302` distinguishes completion/validity/acceptance, loaded identities, six-stage observations, original-coordinate obligations and unresolved structural semantics; self-reported digests do not establish fresh controlled execution. `tools/conformance/run.schema.json:532–539,708–724` requires unavailable coverage/resources and `release_gate_eligible: false`. This proposal leaves that boundary unchanged.

## Hash appendix

SHA-256 hashes below cover each complete file's exact Git blob bytes, with no normalization or concatenation. They pin this inspection only. M file links navigate the repository; use the revision above for immutable meaning.

| M repository path | SHA-256 |
| --- | --- |
| [vendor/xsd-schema/src/validation/errors.rs](../../vendor/xsd-schema/src/validation/errors.rs) | `feb956962afd64fd95e7e12ae219f1eedecfefb2510f7fb106637bbff74d2647` |
| [vendor/xsd-schema/src/validation/validator.rs](../../vendor/xsd-schema/src/validation/validator.rs) | `ee20fba28de8cd6915eeac10a0bbd5077d3da9d48e577cd860e0e857965f5353` |
| [vendor/xsd-schema/src/validation/runtime.rs](../../vendor/xsd-schema/src/validation/runtime.rs) | `858954017515118f5a57de074270660bf5aa8afa2c8ad2f97bc9e8e7cb4ce072` |
| [vendor/xsd-schema/src/validation/info.rs](../../vendor/xsd-schema/src/validation/info.rs) | `0371b1e9584d540d81e14c9f0b95f652b66855545d2ead366768dfb6d508580d` |
| [vendor/xsd-schema/src/parser/location.rs](../../vendor/xsd-schema/src/parser/location.rs) | `0a031d00ed401c5cacd45204917b18cf6f3afe1c0e4dcc3466b0a499eb69e2d9` |
| [vendor/xsd-schema/src/types/validators.rs](../../vendor/xsd-schema/src/types/validators.rs) | `6777986dc17c53b6d597a434454878fcc0f6edae2c858cd38aedba3b501ecd85` |
| [vendor/xsd-schema/src/error.rs](../../vendor/xsd-schema/src/error.rs) | `3b5c4dc6ac46f0954be9b31a674d020020d7c9b27a04c22447f7161908cf883f` |
| [tools/schema-backend/examples/raw_records.rs](../schema-backend/examples/raw_records.rs) | `61a26b07e412886b0251916cf46f90d9d36589fc3c81ab733cc13decb74760fc` |
| [tools/schema-backend/README.md](../schema-backend/README.md) | `9a9956f1757f7ab186e286ffaf399438fd83e91c3ab6db9ed3a82542656f5269` |
| [vendor/xsd-schema.provenance.json](../../vendor/xsd-schema.provenance.json) | `558ec7ceb130ce667f38fe6d2d48bbba460952646af3d61de8222d1b02f310ee` |
| [tools/validation-generator/crates/validation-ir/src/lib.rs](../validation-generator/crates/validation-ir/src/lib.rs) | `6d6ac99afa5fd30d606ef88f3d07f9237b399e939905723dc2381b3ca10b5800` |
| [tools/validation-generator/crates/validation-ir/src/resolved.rs](../validation-generator/crates/validation-ir/src/resolved.rs) | `70d5125ab01e8ced691822f5e72f27c002d05fa55e85cfb4eea4c1c19fd1c54e` |
| [tools/validation-generator/crates/validation-ir/README.md](../validation-generator/crates/validation-ir/README.md) | `9070afdb8d247f3dd3b0cdbd1a1cd6f728bcebe5bd0c438f350f50bc0ba0a677` |
| [tools/conformance/CONTRACT.md](../conformance/CONTRACT.md) | `3775d2ee750c65a733d49316b6b900e45fc35de7fb328075734bf79daca9d323` |
| [tools/conformance/run.schema.json](../conformance/run.schema.json) | `7937f9e905fecaf7de2f0252e48af3590a0ed6de0121d1d7beb6a36ff0a83128` |

| L repository path (local unpublished snapshot) | SHA-256 |
| --- | --- |
| `crates/einvoice-source/README.md` | `042f1461de913eae965e2bc19f4197addd92fbb5217afcbf54780aeaeee86db6` |
| `crates/einvoice-source/src/lib.rs` | `b1e5de7bbe3913642448f221f7ccedaf4e415f3ec63a4fb568d8a2fb3cab34f5` |
| `crates/einvoice-source/src/document.rs` | `4d365b450eb715e62ec45028ad97dcbca17ebd996356538305d3ed670cb7dc76` |
| `crates/einvoice-source/src/budget.rs` | `60130ee2031773bbe388c50eb38464678d008c9d0802fcd7f3360e12fafa972b` |
| `crates/einvoice-source/src/error.rs` | `d4a50ca9ef3791b666beda1d22467f42935e42cc98169a347332db3d13e6deea` |
| `crates/einvoice-source/src/render.rs` | `f729fea7d72d19558cfd3d5647a77c3d28b974a607f03f9ad3de113fb462ba43` |
| `crates/einvoice-source/src/parse.rs` | `f3719a27c3f0f1f9d4b95708cc2f63aa921792c984c4222f1447907106ce130c` |
