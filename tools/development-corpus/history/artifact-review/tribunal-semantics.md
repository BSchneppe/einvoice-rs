# First tribunal — artifact semantics

Scope: immutable development artifact manifest `manifest.json`, SHA-256
34924b22c0d9be1f6fe3d78fe6a2ee496793e466c27bddea7e20cd7946b68a26,
799 members at reviewed HEAD e89b2fd2f3bd3701197f18f76fd2c50c0422a99b.
This is a provisional semantics tranche; independent provenance/replay review is
still outstanding. No original artifact or tracked source is changed.

S1 major — Holds up. Reading family-run/summarize.py confirms accepted records
alone require sufficient stages; the rejected branch and final completeness
predicate can accept absent execution. The critic's synthetic missing-stage and
malformed-XML probes report complete. Discriminant reporting also asserts
successful processing unconditionally. This blocks reuse as a publication
verifier, not the actual observations: all original records have valid XML;
four family records have completed invalid XSD, the other 56 have both business
stages. Preserve legitimate schema rejection and effective-warning acceptance.
Fix via a separately versioned verifier with explicit stage progression and
process outcomes; retain raw historical bridge values, scripts and reports.

S2 minor — Holds up. Standalone summarizer reads the request list without binding
it to the freeze and hardcodes requested/receipt cardinality 60. The critic
reproduces complete with observed 59. Launch guards and immutable hashes protect
the actual historical run, so do not assert a missing observation occurred.
Publication must independently bind exact manifest membership, order, families,
input digests, streams, receipts and cardinality; never assert acceptance totals
as expected results.

S3 minor — Holds up within publication scope. Artifact sources.md gives creator,
license, revision, logical XML path and Parquet digest but no upstream locator or
extraction selector. Parent discovery/seed_smoke.py contains the omitted
acquisition/extraction evidence: exact pinned Parquet, filter xml_file equal to
the preselected development source path, unique row, xml string encoded UTF-8,
sha256_xml and byte hash/length checks. A public provenance record can close this
without changing input bytes or reading other dataset rows. Verify and retain
upstream notices for material actually shipped. This is not a license violation
finding.

Alternatives: retain scratch history as immutable evidence; package a bounded
content-addressed development bundle plus verifier/reproducer separately. Avoid
shipping copied runtime archives/config/cache, absolute host dependencies, or
introducing a second conformance comparator. Exact historical commitments must
remain distinct from new publication manifests and new replay receipts.
Final package plan waits for independent replay review and adversarial verdicts.

All three Holds require fresh independent adversarial validation before fixes.
Native correctness, structural equivalence, actual site/context hits, resource
outcomes, protected held-out separation and release/timing eligibility remain
unproved. No new held-out families exist in this slice.
