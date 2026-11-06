# Facade test fixture

`profile-only.xml` was authored for this crate's syntax, profile-preflight and
immutable-byte tests under the repository's MIT license. It is not copied from
the legacy core fixtures or an upstream invoice corpus.

The XML deliberately lacks required invoice data and contains non-schema test
attributes/content. It is not an official example, builder output, schema-valid
invoice or independently KoSIT-accepted input. Its matching profile exercises the
current unavailable-engine report; comments, a processing instruction, entities,
CDATA and attribute spelling provide distinct source bytes to preserve.

The fixture lives inside this crate so its integration tests can also run from
the packaged archive without the sibling `einvoice-core` checkout.
