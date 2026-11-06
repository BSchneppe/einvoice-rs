# Maintained xsd-schema 0.3.0 patches

Apply these UTF-8 unified diffs in provenance-manifest order to the full,
verified published package. Paths are relative to the extracted package root.

1. `0001-msrv-unpublished.patch`: declare Rust 1.83 and prohibit publishing
   this local package. Name/version and the original manifest/VCS metadata stay
   upstream's; this is not a new upstream release.
2. `0002-owned-diagnostic-evidence.patch`: retain actual typed causes in
   optional boxed diagnostic storage; copy available component names/schema
   origins at native emission sites. Raw diagnostics remain unchanged.
3. `0003-cooperative-stop.patch`: backward-compatible sink stop query, guarded
   dispatch and terminal out-of-band abort, including IDREF finalization.

Regression fixtures and maintenance commands live in `tools/schema-backend`.
`verify_provenance.py` checks every consumed package byte and patch offline.
Supplying the pinned archive additionally verifies original bytes and exact
forward/reverse application; `--reproduce` reconstructs a new package directory.
The provenance JSON intentionally contains no self-digest. Its original hashes
are checked against the authenticated archive only when that archive is supplied.

All upstream headers, notices, licenses, docs, tests and assets are retained.
The bundled W3C assets remain byte-identical, including xlink's CRLF line endings.
