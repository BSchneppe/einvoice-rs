# Original exact builder scenarios

`original.tsv` contains A–J from the independently authored BSC-9 scenario
matrix (2026-11-06). Inputs and expected constants were copied from that original
development evidence; no runtime-generated expected values or corpus data appear
here. Quantities/prices/bases/rates are original text. Line/group/total results
were independently computed with Python `Fraction`; a separate precision-160
`Decimal` HALF_UP check confirmed them. This fixture requires neither Python nor
an oracle at test time.

Policy: exact quantity × price, then one direct base division rounded to two
places with ties away from zero; sum rounded line nets by numeric S rate; round
group basis × rate / 100 once; sum group taxes; gross = due = net + tax.
H is strictly below .005 beyond 18 digits and detects double rounding. G/J
exercise signed lines/totals; I exercises zero. B distinguishes group tax from
per-line tax and uses equivalent rate spellings. F retains two decimals for HUF.

Source relationships: EN16931 CII BR-27, BR-S-05/08/09, BR-CO-10/14/15/16/17,
BR-DEC-09/12/14/18/19/20/23 and XRechnung R120/R121/R130. Bundle identity:
XRechnung 3.0.2 CII D16B, KoSIT 2026-08-31, CEN 1.3.16, XR Schematron 2.6.0.
KoSIT archive SHA-256: `2530cd107c414511c5d0462ec10f886910395abfca820db82e83d70bf01221a8`;
CEN archive: `1cd53cb8a84d38aedc82c0caede217da983a7934dd663f793a092fd66443c561`;
XR archive: `ca5e07afd04e72cd283d581590ffff3a15f4aac9d2f40c993345d78e67ea22b4`.

The source validators have tolerances and XPath semantics that differ from exact
builder expectation equality. These arithmetic tests do not execute those rules,
produce XML, establish invoice conformance, or claim native/oracle acceptance of
any output from this implementation.
