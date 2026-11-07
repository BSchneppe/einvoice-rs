# Static source inventory

This standalone Python 3.12 tool accounts for a bounded, pinned XML source
closure. It reads no invoices, corpus manifests, labels, generated IR, native
runtime results or benchmark data. It does not evaluate XPath, expand abstract
Schematron, compile XSLT/XSD, execute a validator, download sources or write the
cache. CI runs synthetic static tests and checks the summary's inspector digest;
it does not acquire or audit the pinned sources.

The checked [evidence-summary.json](evidence-summary.json) is a reviewable
regression artifact derived from the frozen artifact lock, not replacement pins
or evidence of conformance. Original declarations, selected source sites and
configured XSLT emitters have separate denominators. Identical counts and raw-ID
lexical matches do not establish equivalent behavior. Execution coverage,
native-tested coverage and semantic equivalence remain `unavailable`; the
conformance, resource/native attestation and release-evidence flags remain false.

## Commands

Use an existing Python 3.12 interpreter and explicitly supply the existing
read-only oracle cache directory containing `downloads/`. No dependencies need
installation. From the repository root:

```sh
python3.12 -B -m unittest discover -s tools/source-inventory -p test_inventory.py -v
python3.12 -B tools/source-inventory/inventory.py build --cache /path/to/existing/cache
python3.12 -B tools/source-inventory/inventory.py verify --cache /path/to/existing/cache
BSC10_SOURCE_CACHE=/path/to/existing/cache python3.12 -B -m unittest discover -s tools/source-inventory -p test_real_source.py -v
```

`build` prints the bounded summary as JSON. `verify` independently rebuilds the
report and requires byte equality with the checked summary before writing any
output. An explicit `--expected-summary` can select a separate review artifact.
This is a regression comparison, never an authority to refresh source pins.
Both commands exit 1 on missing/corrupt source, unsupported required XML root,
summary difference, or output failure. Dependency errors are bounded and omit
host paths, XML snippets and unselected member names.

`--output /private/tmp/fresh-inventory.json` additionally writes the deterministic
full report. The parent directory must exist, and the file must be fresh.
Existing files, hardlink aliases and terminal symlink aliases (including dangling
ones) are refused. Resolved destinations inside the cache or the repository
outside `.scratch/` are refused. Directory symlink aliases are resolved before
protection checks; opening uses exclusive creation and no-follow. These checks
protect ordinary file use, not hostile concurrent replacement of ancestor
directories. The tool creates no directories or temporary cache files. A failed
write may leave a partial fresh output file; discard it and choose a new path.
Shell redirection is outside these file-output protections.

`--release-provenance` adds only the separately pinned CEN release XSLT and a
third lexical comparison. It never replaces the configured executable or changes
its denominators. Its summary has different report/policy digests; use a separate
expected summary when verifying that optional view. The real-source audit also
checks this optional view without retaining its large report in Git.

## Exact source boundary

`inventory.py:ALLOWLIST` is the complete selection policy. There are sixteen
required members, and one optional provenance member:

* `cen.zip`: `schematron/EN16931-CII-validation.sch`,
  `schematron/preprocessed/EN16931-CII-validation-preprocessed.sch`,
  `schematron/abstract/EN16931-CII-model.sch`,
  `schematron/abstract/EN16931-CII-syntax.sch`,
  `schematron/CII/EN16931-CII-model.sch`,
  `schematron/CII/EN16931-CII-syntax.sch`, and
  `schematron/codelist/EN16931-CII-codes.sch`.
* `xr.zip`: `schematron/cii/XRechnung-CII-validation.sch` and
  `schematron/common.sch`.
* `config.zip`: `scenarios.xml`,
  `resources/cii/16b/xsl/EN16931-CII-validation.xsl`,
  `resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl`, and the four
  invoice schemas under `resources/cii/16b/xsd/`:
  `CrossIndustryInvoice_100pD16B.xsd`,
  `CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd`,
  `CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd`,
  `CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd`.
* Optional `cen.zip`: `xslt/EN16931-CII-validation.xslt`.

The exact baseline artifact-lock byte SHA-256 is anchored in the inspector.
Each archive is read once into memory, verified against that lock, and consumed
from the same immutable byte snapshot. Its complete regular-member name set is
compared with the lock, including duplicate-name refusal, without logging the
set. Only explicitly selected payloads are opened and independently member-hashed.
Directory records are not members for that set comparison. Whole-archive hashes
bind unselected bytes, but the inspector does not independently read/hash each
unselected payload. Extracted cache files are never consumed or trusted.

The closure intentionally excludes other invoice syntaxes, package test/example
payloads, report stylesheets, validator JAR resources and configuration schemas.
The selected `scenarios.xml` is retained as configuration source, including its
other declared scenarios; it does not cause traversal of those resource paths.
Configured CII executable selection is fixed by the explicit allowlist and pinned
configuration, not inferred from archive globs or report-time execution.

XML uses stdlib Expat with namespace processing. Every DTD, entity declaration,
external entity and skipped entity is refused. Expat performs no network fetch;
the inspector supplies no resolver. XML 1.0 with UTF-8 (including a UTF-8 BOM) or
declared ISO-8859-1 is supported. Other versions/encodings and NUL bytes are
refused. Two required CEN source files actually declare ISO-8859-1; their declared
decoding is preserved. This corrects the initial planning assumption that all
selected files declare UTF-8. Parsing applies XML-defined newline and attribute
normalization; “exact decoded” refers to the resulting XML values, not raw lexical
quote/entity spelling. Raw member hashes retain that spelling and original bytes.

## Format: `bsc10-source-inventory-v1`

JSON is UTF-8, with sorted object keys, two-space indentation, literal Unicode,
finite values only, and a trailing LF. Lists preserve source/selection order.
No timestamps, host paths or interpreter-dependent representations are emitted.
The summary's `report_sha256` hashes precisely these full-report bytes, the same
bytes written with `--output`. Internal identities and `policy_sha256` instead
hash compact canonical JSON (sorted keys, literal Unicode, no final LF).

The report has these fields:

| Field | Meaning |
| --- | --- |
| `format` | Exact version string above. |
| `artifact_lock_sha256` | Consumed baseline lock-byte digest. |
| `inspector_sha256` | Entire inspector file digest, binding implementation policy. |
| `policy_sha256` | Canonical digest of format, allowlist, optional-release selection/member and claims. |
| `claims` | Explicit unavailable execution fields and false evidence/attestation flags. |
| `archives` | Selected archive digests, member-set verification and selected-member counts; no inventory of excluded names. |
| `documents` | Map keyed by `archive!member`, with digest, byte length, XML declaration, representation, flat node tree, counts, duplicate parameters and optional executable view. |
| `relations` | Ordered static relation edges and DFS cycle back-edges; never expanded instances. |
| `associations` | Separate source-to-configured comparisons, plus optional release comparison. |

Every document's `nodes` list is a preorder traversal of elements within its
root. Indices are zero-based and document-local; `ordinal` is one-based preorder.
`parent` is the parent index or null, and `children` lists element indices in
order. `content` is an ordered mixed-content sequence: `{"text": decoded_text}`,
`{"element": index}`, `{"comment": text}`, or `{"pi": [target, text]}`.
Element tails appear as text in their parent's sequence. Adjacent parser text
chunks are merged. CDATA/entity lexical spellings and prolog/epilog comments/PIs
are not reconstructed. Message constructors use these same complete node trees;
`sch:name`, `sch:value-of`, `xsl:value-of` and other constructors are retained
without executing them or flattening their structure into a guessed message.

Each node contains expanded `name: [namespace_uri, local_name]`, `lexical_name`,
expanded-key `attributes` (Clark notation for namespaced attributes),
`lexical_attributes` (original qualified names), full in-scope `namespaces`,
separate `original_id` (null when absent), `line` (one-based parser line), and
`byte_span: "unavailable"`. Attribute/context/test/flag/role/parameter values are
never trimmed. Schematron `ns`, `let`, phase/default-phase/query-binding,
XSLT variable/helper/static-context attributes and XSD declarations are retained
as ordinary nodes/attributes. XML namespace scopes do not substitute for resolving
Schematron XPath namespace declarations or evaluating XPath static context.
`xml_declaration` records version, encoding (null when unspecified), and Expat's
standalone value (-1 unspecified, 0 no, 1 yes); an absent declaration defaults to
version 1.0 and unspecified encoding/standalone.

`path` is a root-to-element list of `[namespace_uri, local_name,
same_expanded_name_sibling_ordinal]`, with one-based ordinals and root ordinal 1.
`site_id` is SHA-256 of the canonical object:

```json
{
  "domain": "bsc10-source-site-v1",
  "artifact_lock_sha256": "...",
  "archive_sha256": "...",
  "member": "...",
  "member_sha256": "...",
  "element_path": [["namespace", "local", 1]],
  "kind": ["namespace", "local"]
}
```

All elements receive such IDs, so rules with missing IDs and repeated contexts
remain distinct. A changed source digest changes identities. Original,
preprocessed and configured records cannot silently share identity. These are
declaration IDs, never effective expanded-instance IDs or runtime hit IDs.

`duplicate_parameters` groups only same-parent `sch:param` name occurrences and
retains each ordered node index. Values remain on the nodes; no first/last-wins
substitution occurs. Parameters under distinct pattern parents are independent.

Relation edges contain `source` document key, `node` occurrence, `kind`, and
`status`. Include/import/schema edges additionally retain exact `raw` references
and a `target` key only when the relative reference resolves inside the selected
same-archive closure. Empty references denote the current member. External,
escaping, fragment/query/percent/backslash/whitespace-bearing references and
unselected/missing targets have `outside_selected_scope`; absent locations have
`missing_location`. Any inherited `xml:base` makes resolution
`xml_base_unresolved`. No additional document is opened on behalf of an edge.
XSD imports with selected targets retain declared/target namespaces and their
exact equality check. This is not schema validation or an assertion that the
selected files are the complete runtime resolver closure.

Phase declarations have `declared_phase` status. Parameter edges have
`unexpanded_parameter_occurrence`, raw attributes and parent index. `active`,
`extends` and `is-a` links retain raw references and ordered archive-local
same-kind raw-ID candidates with `unresolved_static_candidates`; candidates are
not resolved abstract lineage or phase selection. Repeated include sites remain
repeated edges. `cycle_back_edges` records DFS back-edge indices/source/target
for the resource graph; the full edges preserve the graph, without recursion
through effective includes or enumeration of every simple cycle.

## Configured view and limited association

An executable document's `executable` object contains all XSLT `templates` indices
and ordered literal SVRL `emitters`: failed assertions, successful reports,
fired rules, suppressed rules and active patterns. These are static declaration
sites, including alternative branches; none is an observed event.

Each emitter retains its index/kind, nearest ancestor template and its complete
attributes (`match`, `mode`, `priority`, etc.), full ancestor chain, and ordered
XSLT `control` ancestors with exact attributes. `when`/`otherwise` additionally
retain preceding sibling `when` conditions. `constructor_node` points at the
complete emission/message tree. This exposes actual guards separately from
diagnostic metadata. It does not calculate reachability, template dispatch,
branch truth, helper effects, dynamic resources, or emitted diagnostic counts.
Fired/suppressed/active-pattern emitters are never added to source rule counts.

Emitter `fields` describes `id`, `test`, `flag`, `role` and `context` metadata as
`{status, value, constructors}`. Status is `literal`, `absent`,
`dynamic_unknown`, or `ambiguous`. Only a direct literal attribute without AVT
braces, or a single direct unnamespaced `xsl:attribute` containing direct text
and/or `xsl:text`, has extractable literal metadata. This is decoded constructor
text, not execution of XSLT whitespace/attribute rules. `select`, other
constructors, indirect attribute-producing instructions, attribute sets, dynamic
names/namespaces and conflicting constructions stay unknown/ambiguous. Qualified
constructed attribute names are conservatively unknown. The complete raw tree
always remains available; this extractor is not an XSLT interpreter.

A direct child designated as an extension instruction makes the emitter's fields
ambiguous. Namespace designations accumulate from ancestor-or-self declarations,
resolving prefixes at each declaration; `#default` uses that declaration's default
namespace. An unbound designation is unsupported and also makes fields ambiguous.
Unused declarations, unrelated templates, and ordinary foreign literal children
do not by themselves make metadata unknown. This bounded scope check follows
[XSLT 2.0 extension instruction declarations](https://www.w3.org/TR/2007/REC-xslt20-20070123/#extension-instruction);
it executes neither extensions nor fallback instructions.

Associations join assertion/report `@id` to extractable raw diagnostic IDs only.
Statuses are `missing_id`, `ambiguous`, `unmapped`, or `unique_raw_id`.
Duplicates on either side prevent selection. Every source site and unassociated
emitter remains visible, including `unknown_id_emitters`. A unique association
compares decoded `test`, `flag`, `role`, and source rule context against template
`match`. Field results are `exact`, `boundary_whitespace_only`, `different`, or
`unknown`. Boundary comparison removes only XML whitespace at the two ends for
classification; both original strings remain intact. There is also an explicit
assert/report emitter-kind comparison. Message structure and guards are exposed
for inspection but are not proved equivalent by these field comparisons.

The compact `bsc10-source-inventory-v1-summary` binds the complete report,
inspector, policy, lock, archive and member digests. It retains separate counts,
per-member declaration counts/encodings, every duplicate parameter occurrence,
comparison status/field counts, all non-boundary-only differences and their
actual control ancestors, unassociated emitters, relation status counts and XSD
namespace comparisons. All boundary-only differences remain in the full report.

In the checked source, CII-SR-282's preprocessed test is
`not(ram:SellerTaxRepresentativeTradeParty/ram:ID)`. Configured metadata is
`not(ram:BuyerTaxRepresentativeTradeParty)` and its actual guard is
`not(not(ram:BuyerTaxRepresentativeTradeParty))`. Both original same-name
parameters are preserved in order, buyer then seller-ID. The optional release
view agrees with the seller-ID test for this site, while retaining its other
lexical differences and an unknown AVT field. These facts require an independent
mapping/authority decision before claiming equivalence. The inspector makes no
such decision and contains no rule-ID-specific extraction behavior.

The [root source-resolution direction](https://linear.app/bschneppe/issue/BSC-7/generate-version-pinned-validation-packages-through-an-explicit-ir#comment-f83ec40f)
separately retains configured KoSIT authority and selects the existing buyer-side
original declarative parameter for a reviewed compiler-time resolution. It requires
distinct raw and resolved candidate artifacts, exact source provenance and
fail-closed drift/ambiguity checks. The inspector does not apply that decision:
both original tests and parameter occurrences remain unchanged observations.
This one decision does not establish complete predicate/context/helper/message/
policy correspondence, runtime capability or native conformance.

## Correlation conventions and limits

No source compiler is created and no generated IR is consumed. To correlate a
future independently supplied artifact, bind its exact byte digest, source
package identity, source member digest and its own site identity. A frontend's
`resource@raw-byte-start` identity needs independent checking against those exact
source bytes; this inventory's line numbers cannot establish that byte start.
Store correlation candidates/missing/ambiguous mappings explicitly. Effective
instances additionally need entry root, selected phase, ordered include chain
and original instantiation/parameter occurrences. None is inferred here from a
matching rule ID, context string, generated template name or equal counts.

The real-source audit checks the exact source closure, separate denominators,
all configured field differences, duplicate parameters, actual guard, namespace
imports, optional provenance and repeated full-report byte determinism. Synthetic
tests exercise corrupt snapshots/member sets, changed identities, namespace and
constructor preservation, duplicate/missing/dynamic associations, source cycles,
unsafe XML and output aliases. Neither suite is a native conformance, execution
coverage, resource, performance, attestation or release test.
