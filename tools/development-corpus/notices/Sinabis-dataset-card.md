---
license: cc-by-4.0
language:
  - de
  - en
task_categories:
  - text-generation
  - text-classification
  - token-classification
  - feature-extraction
language_creators:
  - machine-generated
multilinguality:
  - multilingual
source_datasets:
  - original
tags:
  - e-invoicing
  - e-rechnung
  - zugferd
  - factur-x
  - xrechnung
  - en-16931
  - gobd
  - cii
  - un-cefact
  - pdf-a3
  - synthetic
  - synthetic-data
  - invoices
  - digital-forensics
  - accounting
  - enterprise
  - b2b
  - b2g
  - leitweg-id
  - reverse-charge
  - information-extraction
  - table-understanding
size_categories:
  - 10K<n<100K
configs:
  - config_name: default
    data_files:
      - split: train
        path: "data/train-*.parquet"
---

# Sinabis Green Energy – 10k GoBD-compliant German E-Invoices (XRechnung 3.0 / ZUGFeRD)

A specialized dataset of **10,000 electronic invoices (E-Rechnungen)** fully compliant with:
1. **XRechnung 3.0** (German CIUS of **EN 16931**, specification identifier `urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0`), embedded as **ZUGFeRD 2.x profile XRECHNUNG**.
2. **Hybrid PDF/A-3b**: Human-readable PDF (embedded TrueType fonts, sRGB OutputIntent) with the machine-readable UN/CEFACT CII XML embedded as `xrechnung.xml` (`/AFRelationship /Alternative`, Factur-X XMP extension schema). The same XML is also shipped as a standalone `.xml` next to every PDF.
   Samples were checked with the **KoSIT validator** (XRechnung 3.0.2 configuration) and **Mustang/veraPDF** (PDF/A-3b + ZUGFeRD).
3. **GoBD-Konformität (Grundsätze zur ordnungsmäßigen Führung und Aufbewahrung von Büchern, Aufzeichnungen und Unterlagen in elektronischer Form)**:
   - **Cent-accurate congruence**: Every amount visible in the PDF (net total, tax bases, VAT rates, VAT totals, gross total, cash discounts/skonto) matches the embedded XML down to the exact cent.
   - **§ 14 UStG mandatory fields**: Unique invoice numbers, sequential dates, complete legal supplier/buyer profiles, German VAT IDs (`DE...`), tax numbers, commercial register entries (Amtsgericht HRB/HRA), managing directors, UN/CEFACT unit codes (`C62`, `HUR`, `MWH`, `MTR`), and SEPA bank details (IBAN/BIC).
   - **Revisionssicherheit**: Unique cryptographic SHA-256 hashes generated for every single PDF and embedded XML document.

This 10k edition is a subset of the [100k edition](https://huggingface.co/datasets/sinabis-group/sinabis-100k-synthetic-zugferd-xrechnung): per theme the first invoices (by `idx`), so every
theme keeps exactly its share of the larger editions. File names, folder paths, manifest records and SHA-256 hashes are identical.

> All figures in this dataset card were **computed directly from the files in this edition**
> (as of 24 Oct 2026).

## At a glance

| Metric | Value |
|---|---|
| Total E-Invoices | **10,000** hybrid PDF/A-3b documents (557 MB uncompressed incl. standalone XML, 396 MB in 1 ZIP archive) |
| Standard & Profile | **XRechnung 3.0 (EN 16931 CIUS) in ZUGFeRD profile XRECHNUNG** |
| Embedded XML | `xrechnung.xml` (UN/CEFACT CII D16B syntax) inside each PDF + standalone `.xml` |
| Legal / Tax Scope | **§ 14 UStG, § 13b UStG (Reverse Charge), GoBD, VOB/B, EEG 2023** |
| Total Invoiced Volume (Gross) | **676,316,339.51 EUR** (576,765,855.06 EUR net + 99,550,484.45 EUR VAT) |
| Municipal & B2G with Leitweg-ID | **1,500** invoices to public buyers with Leitweg-ID (valid Mod 97-10 check digits) |
| § 13b UStG Reverse Charge | **1,000** subcontractor invoices (Category `AE`, 0% VAT, mandatory legal notice) |
| Cash Discount / Skonto | **2,500** B2B deliveries with § 14 UStG Skonto conditions (2% within 14 days) |
| Issuing Organisations | **26** corporate entities (Sinabis group companies & certified EU suppliers) |
| Client / Buyer Organisations | **64** corporate clients, municipal utilities, city authorities & grid operators |
| Cryptographic Integrity | SHA-256 hash calculated for both PDF and embedded XML in all manifests |

## Document Themes & Tax Types

The dataset covers the complete spectrum of corporate B2B and public B2G transactions in the renewable energy sector:

| Theme | Count | Share | Description & Tax Characteristics |
|---|---:|---:|---|
| `b2b_service_maintenance` | 2,500 | 25.0 % | Engineering, inspection and maintenance services (19 % VAT, hourly rates HUR, DGUV V3, flat rates) |
| `b2b_hardware_delivery` | 2,500 | 25.0 % | Delivery of hardware, solar modules and inverters (19 % VAT, cash discount, EAN/SKU, C62/MTR) |
| `municipal_epc_milestone` | 1,500 | 15.0 % | Municipal construction-milestone invoices under VOB/B with German Leitweg-ID (B2G routing) |
| `utility_energy_feedin` | 1,500 | 15.0 % | Renewable electricity feed-in and grid billing (MWh/kWh, RLM metering, market location, EEG) |
| `reverse_charge_subcontractor` | 1,000 | 10.0 % | Subcontractor construction services with § 13b UStG reverse charge (category AE, 0 % VAT) |
| `credit_note_correction` | 1,000 | 10.0 % | Credit notes and invoice corrections (type code 381, reference to the original invoice) |

## Architecture of the Hybrid PDF/A-3b Document

Each document in this dataset satisfies the strict dual-nature requirement of modern European e-invoicing:

```
┌────────────────────────────────────────────────────────┐
│  Sinabis E-Invoice (Hybrid PDF/A-3b)                   │
│                                                        │
│  ┌──────────────────────────────────────────────────┐  │
│  │ Visually Rendered PDF Document (Human Readable)  │  │
│  │ - High-res vector company branding & logo        │  │
│  │ - Complete DIN 5008 address field & metadata     │  │
│  │ - Itemized table with quantities & tax rates     │  │
│  │ - Tax breakdown (§ 14 Abs. 4 Nr. 8 UStG)        │  │
│  │ - Bank details (SEPA IBAN/BIC) & Skonto notice   │  │
│  └──────────────────────────────────────────────────┘  │
│                          │                             │
│       Embedded File Attachment: /AFRelationship        │
│       /Alternative & PDF/A-3 XMP Metadata Schema       │
│                          │                             │
│  ┌──────────────────────────────────────────────────┐  │
│  │ xrechnung.xml (UN/CEFACT CII D16B Machine Data)  │  │
│  │ - XRechnung 3.0 CIUS (EN 16931 compliant)        │  │
│  │ - Cent-for-cent identical numerical summation    │  │
│  │ - Standardized unit codes (C62, HUR, MWH, MTR)   │  │
│  │ - Tax categories: S (Standard 19%), AE (§ 13b)   │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────┘
```

## Quick start

The Parquet file under `data/` holds one row per invoice: all manifest fields plus the extracted
text, so the dataset can be browsed in the Dataset Viewer above and loaded directly:

```python
from datasets import load_dataset

ds = load_dataset("sinabis-group/sinabis-10k-synthetic-zugferd-xrechnung", split="train")
inv = ds[0]
print(inv["invoice_number"], inv["issue_date"], inv["grand_total"], inv["currency"])
print(inv["text"])   # text layer of the PDF
print(inv["xml"])    # full XRechnung XML (ground truth)
```

Each row is one invoice: all manifest fields (amounts, tax, Leitweg-ID, Skonto, SHA-256 hashes, see "Manifest & Verification Hash Schema" below), `text` with the text layer of the PDF, `pages`, and `xml` with the complete embedded XRechnung 3.0 / UN/CEFACT CII XML. Together, `text` and `xml` make a ready-made extraction benchmark: the PDF text is the input, the XML is the exact ground truth. The original PDF/A-3b files are in the ZIP archive.

## Downloads

| File | Size | Contents |
|---|---:|---|
| `data/train-*.parquet` (1 file) | 6 MB | Manifest fields + PDF `text` + full XRechnung `xml`, one row per invoice (Dataset Viewer, `load_dataset`) |
| `Sinabis_10k_XRechnung3.0_ZUGFeRD.zip` | 396 MB | `10,000` e-invoices (557 MB uncompressed) |

`Sinabis_10k_XRechnung3.0_ZUGFeRD_manifest.zip` (2.0 MB) holds the JSON Lines audit records for all 10,000
invoices (see schema below).

### Unpacking

Both archives share one directory structure. Unpacking them into the same
directory merges everything:

```bash
hf download sinabis-group/sinabis-10k-synthetic-zugferd-xrechnung \
  --repo-type dataset --local-dir sinabis-10k-zugferd
cd sinabis-10k-zugferd
for z in Sinabis_10k_XRechnung3.0_ZUGFeRD*.zip; do unzip -q "$z"; done
```

## Folder structure

In the repository:

```
.
├── README.md
├── data/
│   └── train-*.parquet                 (rows for the Dataset Viewer and load_dataset)
├── Sinabis_10k_XRechnung3.0_ZUGFeRD.zip             → zugferd/
└── Sinabis_10k_XRechnung3.0_ZUGFeRD_manifest.zip    → manifest/
```

After unpacking both archives:

```
.
├── zugferd/
│   ├── b2b_hardware_delivery/
│   │   ├── 0000/
│   │   │   ├── 0000000_<invoice-number>.pdf
│   │   │   ├── 0000000_<invoice-number>.xml
│   │   │   └── …
│   │   └── …
│   └── … (one folder per theme)
└── manifest/
    ├── zb00000.jsonl
    └── …
```

## Manifest & Verification Hash Schema

Each block includes detailed JSON Lines audit records (`manifest/zb*.jsonl`):

```json
{
  "idx": 4501,
  "file": "zugferd/b2b_hardware_delivery/0004/0004501_RE-2024-00102.pdf",
  "xml_file": "zugferd/b2b_hardware_delivery/0004/0004501_RE-2024-00102.xml",
  "format": "pdf_a3b_zugferd_xrechnung",
  "profile": "XRECHNUNG 3.0",
  "theme": "b2b_hardware_delivery",
  "type_code": "380",
  "invoice_number": "RE-2024-00102",
  "issue_date": "2024-05-15",
  "delivery_date": "2024-05-08",
  "payment_due_date": "2024-06-14",
  "currency": "EUR",
  "seller_name": "Rheinaue Kabeltechnik GmbH",
  "seller_vat_id": "DE535809207",
  "buyer_name": "Sinabis Green Energy Deutschland GmbH",
  "buyer_reference": "KD-46686",
  "line_total": 42350.00,
  "tax_basis": 42350.00,
  "tax_total": 8046.50,
  "grand_total": 50396.50,
  "due_payable": 50396.50,
  "lines_count": 4,
  "sha256_pdf": "a3b4c5...",
  "sha256_xml": "e7f8a9...",
  "gobd_congruent": true
}
```

## How to extract and parse with Python

```python
import io
from pypdf import PdfReader
from lxml import etree

# 1. Read PDF and extract the embedded XRechnung XML
reader = PdfReader("0004501_RE-2024-00102.pdf")
xml_bytes = reader.attachments["xrechnung.xml"][0]

# 2. Parse UN/CEFACT CII XML
root = etree.fromstring(xml_bytes)
ns = {"rsm": "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
      "ram": "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100"}

invoice_id = root.find(".//rsm:ExchangedDocument/ram:ID", ns).text
grand_total = root.find(".//ram:SpecifiedTradeSettlementHeaderMonetarySummation/ram:GrandTotalAmount", ns).text

print(f"Invoice: {invoice_id}, Total: {grand_total} EUR")
```

## License

This dataset is released under the **Creative Commons Attribution 4.0 International (CC-BY-4.0)** license.
You are free to share and adapt the material for any purpose, including commercial applications,
provided appropriate credit is given:

> Sinabis GmbH (2026). Sinabis Green Energy – 10k GoBD-compliant German E-Invoices (XRechnung 3.0 / ZUGFeRD).
> https://huggingface.co/datasets/sinabis-group/sinabis-10k-synthetic-zugferd-xrechnung

## Citation

```bibtex
@misc{sinabis_green_einvoice_10k_dataset_2026,
  author = {{Sinabis GmbH}},
  title  = {Sinabis Green Energy -- 10k GoBD-compliant German E-Invoices (XRechnung 3.0 / ZUGFeRD)},
  year   = {2026},
  url    = {https://huggingface.co/datasets/sinabis-group/sinabis-10k-synthetic-zugferd-xrechnung}
}
```
