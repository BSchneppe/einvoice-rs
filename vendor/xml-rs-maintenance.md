# Maintained XML parser

The source arena needs namespace-well-formed XML 1.0, including legal element
names using the implicitly bound `xml` prefix. Published xml-rs 0.8.28 rejects
that prefix in opening and closing element names. The parser changes remove
that prohibition and allow the relative default namespace values `xml` and
`xmlns`. Reserved namespace URI values remain forbidden as defaults; `xmlns`
remains forbidden as an element prefix, and reserved binding checks remain.
No input prefix, byte or source expression is rewritten.

All 35 published package members, original MIT license, README and VCS metadata
are retained. `xml-rs.patch` changes the two element prefix guards, the default
namespace value guard, and sets
`publish = false`. `xml-rs.provenance.json` records archive, original/modified
members and patch identities. The original archive SHA-256 is
`3ae8337f8a065cfc972643663ea4279e04e7256de865aa66fe25cec5fb912d3f`.

The root Cargo registry patch selects this identical implementation for source,
facade and legacy core consumers, avoiding divergent parser copies. The isolated
schema harness retains its own locked parser graph. Keep the parser outside the
workspace member list and use the registry patch: a direct path dependency causes
`cargo fmt --all` to rewrite upstream files even when excluded. The outer Git
attributes preserve package and patch bytes. Do not run rustfmt over this import.

From the repository root, run:

```bash
python3 crates/einvoice-source/tools/verify_parser.py --archive /path/to/xml-rs-0.8.28.crate
```

Without `--archive`, this verifies committed inventory and patch bytes, not
independent authentication of original members. With the archive, it verifies
its pinned checksum and every original member, then checks forward/reverse patch
replay. CI also runs upstream tests in a temporary copy to avoid adding generated
files to the package inventory. Package/patch bytes and source identity are not
an assertion of full XML capability, invoice validation, physical resource bounds
or publication readiness. Distributing a consumer must retain this source patch;
the facade source archive includes it, and publication remains disabled.
