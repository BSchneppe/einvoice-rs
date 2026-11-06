# Current proposed candidate index

All 59 candidates are DEVELOPMENT and all verdicts are unknown. Exact seed-byte anchors, replacements, paths and hashes are in `v3/proposed-ledger.json`. JSON escapes below describe literal characters unless the value itself contains XML character-reference text.

| Candidate | Exact parameters | Source references |
|---|---|---|
| F01-prefix-b | `{"prefix": "b"}` | namespace, names, empty |
| F01-default-ram | `{"prefix": ""}` | namespace, names, empty |
| F01-empty-delivery-ram | `{"delivery_children": 0, "prefix": "ram"}` | namespace, names, empty, delivery-empty-position |
| F01-empty-delivery-b | `{"delivery_children": 0, "prefix": "b"}` | namespace, names, empty, delivery-empty-position |
| F01-local-shadow | `{"prefix": "ram", "uri": "urn:bsc10:neutral:other"}` | namespace, names |
| F02-buyer-missing | `{"presence": "missing"}` | buyer-reference, buyer-cardinality, empty |
| F02-buyer-empty | `{"presence": "empty"}` | buyer-reference, buyer-cardinality, empty |
| F02-buyer-whitespace | `{"presence": "whitespace"}` | buyer-reference, buyer-cardinality, empty |
| F02-buyer-duplicate | `{"presence": "duplicate"}` | buyer-reference, buyer-cardinality, empty |
| F02-careof | `{"value": "Neutral Contact"}` | careof-forbidden, careof-position |
| F02-issue-date-duplicate | `{"count": 2}` | date-cardinality |
| F02-zero-lines | `{"line_count": 0}` | lines-xsd, lines-required |
| F02-terms-duplicate | `{"terms_count": 2}` | terms-count, terms-position |
| F02-tax-zero | `{"line": 2, "tax_count": 0}` | line-tax-count, line-tax-position |
| F02-tax-two | `{"line": 2, "tax_count": 2}` | line-tax-count, line-tax-position |
| F02-name-missing-2 | `{"line_contexts": [2]}` | product-required, product-xsd |
| F02-name-missing-3 | `{"line_contexts": [3]}` | product-required, product-xsd |
| F02-name-missing-2-3 | `{"line_contexts": [2, 3]}` | product-required, product-xsd |
| F02-name-missing-2-3-shifted | `{"inserted_comment_lf_count": 3, "line_contexts": [2, 3]}` | product-required, product-xsd, xml10-comments |
| F03-plus | `{"line": 1, "line_total_lexical": "+1927.35"}` | amount-xsd, line-precision, line-sum-round, line-arithmetic |
| F03-leading-zero | `{"line": 1, "line_total_lexical": "01927.35"}` | amount-xsd, line-precision, line-sum-round, line-arithmetic |
| F03-trailing-zero | `{"line": 1, "line_total_lexical": "1927.350"}` | amount-xsd, line-precision, line-sum-round, line-arithmetic |
| F03-exponent | `{"line": 1, "line_total_lexical": "1.92735E3"}` | amount-xsd, line-precision, line-sum-round, line-arithmetic |
| F03-magnitude | `{"line": 1, "line_total_lexical": "123456789012345678901234567890.12"}` | amount-xsd, line-precision, line-sum-round, line-arithmetic |
| F03-negative-price | `{"price": "-0.01"}` | amount-xsd, price-sign, line-arithmetic |
| F03-round-below | `{"header_line_total": "5159.52", "line1_total": "1927.344", "other_lines_sum": "3232.18", "prices_unchanged": true, "tax_total_unchanged": "980.31", "unrounded_sum": "5159.524"}` | line-sum-round, line-precision, taxbasis-round, grandtotal-round, line-arithmetic, slack |
| F03-round-tie | `{"header_line_total": "5159.53", "line1_total": "1927.345", "other_lines_sum": "3232.18", "prices_unchanged": true, "tax_total_unchanged": "980.31", "unrounded_sum": "5159.525"}` | line-sum-round, line-precision, taxbasis-round, grandtotal-round, line-arithmetic, slack |
| F03-round-above | `{"header_line_total": "5159.53", "line1_total": "1927.346", "other_lines_sum": "3232.18", "prices_unchanged": true, "tax_total_unchanged": "980.31", "unrounded_sum": "5159.526"}` | line-sum-round, line-precision, taxbasis-round, grandtotal-round, line-arithmetic, slack |
| F03-round-tie-lower-header | `{"header_line_total": "5159.52", "line1_total": "1927.345", "other_lines_sum": "3232.18", "prices_unchanged": true, "tax_total_unchanged": "980.31", "unrounded_sum": "5159.525"}` | line-sum-round, line-precision, taxbasis-round, grandtotal-round, line-arithmetic, slack |
| F03-precision-lines-2-3 | `{"lines": [2, 3], "values": ["728.400", "2503.780"]}` | amount-xsd, line-precision, line-sum-round |
| F04-leap-day | `{"format": "102", "issue_date": "20240229"}` | date-shape, date-required, date-string |
| F04-nonleap-day | `{"format": "102", "issue_date": "20230229"}` | date-shape, date-required, date-string |
| F04-month-13 | `{"format": "102", "issue_date": "20251301"}` | date-shape, date-required, date-string |
| F04-day-zero | `{"format": "102", "issue_date": "20251200"}` | date-shape, date-required, date-string |
| F04-hyphenated | `{"format": "102", "issue_date": "2025-12-06"}` | date-shape, date-required, date-string |
| F04-format-203 | `{"format": "203", "issue_date": "20251206"}` | date-shape, date-required, date-string |
| F05-currency-case | `{"currency": "eur"}` | currency-code |
| F05-currency-space | `{"currency": " EUR "}` | currency-code |
| F05-currency-outside | `{"currency": "ZZZ"}` | currency-code |
| F05-unit-case | `{"line": 2, "unit": "hur"}` | unit-code |
| F05-unit-lines-2-3 | `{"line_contexts": [2, 3], "unit": "ZZZ"}` | unit-code |
| F06-xml10-crlf-literal | `{"content_escaped": "A\r\nB", "kind": "crlf", "source_form": "literal", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml10-crlf-reference | `{"content_escaped": "A&#xD;&#xA;B", "kind": "crlf", "source_form": "reference", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml10-cr-literal | `{"content_escaped": "A\rB", "kind": "cr", "source_form": "literal", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml10-cr-reference | `{"content_escaped": "A&#xD;B", "kind": "cr", "source_form": "reference", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml10-nel-literal | `{"content_escaped": "A\u0085B", "kind": "nel", "source_form": "literal", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml10-nel-reference | `{"content_escaped": "A&#x85;B", "kind": "nel", "source_form": "reference", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml11-nel-literal | `{"content_escaped": "A\u0085B", "kind": "nel", "source_form": "literal", "version": "1.1"}` | note-content, xml10-eol, xml10-ref, xml11-eol, xml11-version |
| F06-xml11-nel-reference | `{"content_escaped": "A&#x85;B", "kind": "nel", "source_form": "reference", "version": "1.1"}` | note-content, xml10-eol, xml10-ref, xml11-eol, xml11-version |
| F06-xml10-ls-literal | `{"content_escaped": "A\u2028B", "kind": "ls", "source_form": "literal", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml10-ls-reference | `{"content_escaped": "A&#x2028;B", "kind": "ls", "source_form": "reference", "version": "1.0"}` | note-content, xml10-eol, xml10-ref |
| F06-xml11-ls-literal | `{"content_escaped": "A\u2028B", "kind": "ls", "source_form": "literal", "version": "1.1"}` | note-content, xml10-eol, xml10-ref, xml11-eol, xml11-version |
| F06-xml11-ls-reference | `{"content_escaped": "A&#x2028;B", "kind": "ls", "source_form": "reference", "version": "1.1"}` | note-content, xml10-eol, xml10-ref, xml11-eol, xml11-version |
| F06-attr-tab-literal | `{"attribute": "languageID", "separator_kind": "tab", "source_form": "literal", "value_escaped": "de\tDE", "version": "1.0"}` | text-attributes, note-content, xml10-attr, xml10-eol, xml10-ref |
| F06-attr-tab-reference | `{"attribute": "languageID", "separator_kind": "tab", "source_form": "reference", "value_escaped": "de&#x9;DE", "version": "1.0"}` | text-attributes, note-content, xml10-attr, xml10-eol, xml10-ref |
| F06-xml11-attr-nel-literal | `{"attribute": "languageID", "separator_kind": "nel", "source_form": "literal", "value_escaped": "de\u0085DE", "version": "1.1"}` | text-attributes, note-content, xml10-attr, xml10-eol, xml10-ref, xml11-attr, xml11-eol, xml11-version |
| F06-xml11-attr-nel-reference | `{"attribute": "languageID", "separator_kind": "nel", "source_form": "reference", "value_escaped": "de&#x85;DE", "version": "1.1"}` | text-attributes, note-content, xml10-attr, xml10-eol, xml10-ref, xml11-attr, xml11-eol, xml11-version |
| F06-utf8-bom | `{"bom_hex": "efbbbf", "encoding": "UTF-8"}` | xml10-encoding |
| F06-utf16-bom | `{"bom_hex": "fffe", "byte_order": "little", "encoding": "UTF-16"}` | xml10-encoding |
