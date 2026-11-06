# Third-party files in `assets/`

`xml.xsd` and `xlink.xsd` are W3C documents, shipped **unmodified** (byte-identical to the downloads below; the
repository's `.gitattributes` keeps git from converting their line ends, and `xlink.xsd` has CRLF line ends as
published). Both are compiled into the library:

- `xml.xsd` as `xsd_schema::XML_XSD`, served under the location `embedded://xml.xsd`, to which the schema catalog maps
  the XML namespace (`http://www.w3.org/XML/1998/namespace`);
- `xlink.xsd` as `xsd_schema::XLINK_XSD`, served under the location `embedded://xlink.xsd`, to which the schema catalog
  maps the XLink namespace (`http://www.w3.org/1999/xlink`).

| File | Source URL | Retrieved | Bytes | SHA-256 | Licence |
|---|---|---|---:|---|---|
| `xml.xsd` | `http://www.w3.org/2001/xml.xsd` (the edition dated 2009-01, also published at `http://www.w3.org/2009/01/xml.xsd`) | 2026-10-19 | 8,836 | `61960fb3131e38022caad5360e2f33a3382578ab3c80cd58bd74320ede61b20c` | W3C Software and Document License (2023), full text in `W3C-SOFTWARE-DOC-LICENSE-2023.txt`; the file carries no notice of its own |
| `xlink.xsd` | `http://www.w3.org/XML/2008/06/xlink.xsd` (the XLink 1.1 schema document) | 2026-10-29 | 9,386 | `c83df86c7fdc16eb9c862b83dfb53fc1b1a4bcafd6e1d1217199e0188b82f24a` | W3C Software and Document License (2023), full text in `W3C-SOFTWARE-DOC-LICENSE-2023.txt`; the file carries no notice of its own |
| `W3C-SOFTWARE-DOC-LICENSE-2023.txt` | extracted from `https://www.w3.org/copyright/software-license-2023/` | 2026-10-19 | 1,962 | `1b1b6a2c9e89d7084cb654dd5e62b154e34cd668addd1af3407c7cbb53ce33f4` | the licence text itself |

- **Copyright:** `xml.xsd` and `xlink.xsd` — Copyright © World Wide Web Consortium.
  <https://www.w3.org/copyright/software-license-2023/> (the W3C software and document short notice, which the
  licence asks for when a work carries no notice of its own). Title to copyright remains with the copyright holders.
- **Licence text:** `W3C-SOFTWARE-DOC-LICENSE-2023.txt` holds the licence's title, status line, terms and
  disclaimers verbatim (lines re-wrapped as plain text). The rest of the crate is licensed as stated in `Cargo.toml`
  (`MIT OR Apache-2.0`); that licence does not apply to `xml.xsd` or `xlink.xsd`.
- **Changes:** none.
