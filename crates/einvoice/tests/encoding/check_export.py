#!/usr/bin/env python3.12
"""Independently parse actual prepare vectors; no arithmetic or oracle bridge.

Requires Python 3.12. Checks the hand-maintained expanded-name mapping, original
A-J constants and CR/tab/Unicode preservation with ElementTree (not xml-rs).
Read-only: never rewrites exports, fixtures, metadata or oracle inputs.
"""

import csv
from pathlib import Path
import sys
import xml.etree.ElementTree as ET

NS = {
    "rsm": "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
    "ram": "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
    "udt": "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
}
ALIASES = {
    "C": "rsm:ExchangedDocumentContext",
    "D": "rsm:ExchangedDocument",
    "L": "rsm:SupplyChainTradeTransaction/IncludedSupplyChainTradeLineItem",
    "A": "rsm:SupplyChainTradeTransaction/ApplicableHeaderTradeAgreement",
    "H": "rsm:SupplyChainTradeTransaction/ApplicableHeaderTradeDelivery",
    "S": "rsm:SupplyChainTradeTransaction/ApplicableHeaderTradeSettlement",
}
SPECIAL = 'A&<tag>]]>"\'\rB\nC\r\nD\tE Grüße 東京 😀 e\u0301'
HERE = Path(__file__).resolve().parent


def expanded(name):
    prefix, local = name.split(":", 1) if ":" in name else ("ram", name)
    return "{" + NS[prefix] + "}" + local


def at(root, path):
    first, _, tail = path.partition("/")
    path = ALIASES.get(first, first) + ("/" + tail if tail else "")
    node = root
    for segment in path.split("/"):
        if "[" in segment:
            name, index = segment.split("[")
            index = int(index[:-1]) - 1
        else:
            name, index = segment, 0
        candidates = [child for child in node if child.tag == expanded(name)]
        assert len(candidates) > index, path
        node = candidates[index]
    return node


def parse(path):
    data = path.read_bytes()
    assert data.startswith(b'<?xml version="1.0" encoding="UTF-8"?>')
    assert not data.endswith(b"\n")
    assert b"<ram:ApplicableHeaderTradeDelivery/>" in data
    root = ET.fromstring(data)
    assert root.tag == expanded("rsm:CrossIndustryInvoice")
    return root


def check_all_fields(directory):
    root = parse(directory / "all-fields.xml")
    expected_leaves = []
    expected_attributes = 0
    seen = set()
    for row in (HERE / "all-fields.tsv").read_text().splitlines():
        if row.startswith("#"):
            continue
        path, value = row.split("\t")
        assert path not in seen
        seen.add(path)
        if "/@" in path:
            path, attribute = path.split("/@")
            assert at(root, path).attrib[attribute] == value, path
            expected_attributes += 1
        else:
            node = at(root, path)
            assert len(node) == 0 and (node.text or "") == value, path
            expected_leaves.append(node)
    actual_leaves = [node for node in root.iter() if len(node) == 0]
    assert actual_leaves == expected_leaves, "complete expanded-name mapping and leaf order"
    assert sum(len(node.attrib) for node in root.iter()) == expected_attributes


def check_originals(directory):
    count = 0
    for row in (HERE.parent / "fixtures/calculation/original.tsv").read_text().splitlines():
        if row.startswith("#"):
            continue
        case, currency, lines, groups, net, tax, gross, due = row.split("\t")
        root = parse(directory / (case + ".xml"))
        transaction = at(root, "rsm:SupplyChainTradeTransaction")
        actual_lines = transaction.findall("ram:IncludedSupplyChainTradeLineItem", NS)
        expected_lines = [line.split(",") for line in lines.split(";")]
        assert len(actual_lines) == len(expected_lines)
        for line, expected in zip(actual_lines, expected_lines, strict=True):
            assert at(line, "SpecifiedLineTradeSettlement/SpecifiedTradeSettlementLineMonetarySummation/LineTotalAmount").text == expected[4]
        actual_groups = at(root, "S").findall("ram:ApplicableTradeTax", NS)
        expected_groups = [group.split(",") for group in groups.split(";")]
        assert len(actual_groups) == len(expected_groups)
        for group, expected in zip(actual_groups, expected_groups, strict=True):
            assert [at(group, name).text for name in ("RateApplicablePercent", "BasisAmount", "CalculatedAmount")] == expected
        assert at(root, "S/InvoiceCurrencyCode").text == currency
        for name, value in (("LineTotalAmount", net), ("TaxBasisTotalAmount", net),
                            ("TaxTotalAmount", tax), ("GrandTotalAmount", gross),
                            ("DuePayableAmount", due)):
            node = at(root, "S/SpecifiedTradeSettlementHeaderMonetarySummation/" + name)
            assert node.text == value
            assert node.attrib == ({"currencyID": currency} if name == "TaxTotalAmount" else {})
        count += 1
    assert count == 10


def check_escaping(directory):
    path = directory / "escaping.xml"
    root = parse(path)
    data = path.read_bytes()
    assert b"\r" not in data
    assert b"&#xD;" in data and b"&#x9;" in data and b"&#xA;" in data
    assert b"&amp;" in data and b"&lt;" in data and b"&gt;" in data and b"&quot;" in data
    assert at(root, "A/SellerTradeParty/Name").text == SPECIAL
    assert at(root, "A/BuyerTradeParty/URIUniversalCommunication/URIID").attrib["schemeID"] == SPECIAL
    assert at(root, "L/SpecifiedLineTradeDelivery/BilledQuantity").attrib["unitCode"] == SPECIAL
    assert at(root, "L/SpecifiedLineTradeAgreement/NetPriceProductTradePrice/BasisQuantity").attrib["unitCode"] == SPECIAL
    for i in (1, 2):
        assert at(root, f"S/SpecifiedTradeSettlementPaymentMeans[{i}]/Information").text == SPECIAL
    assert at(root, "S/SpecifiedTradePaymentTerms/Description").text == SPECIAL


def main():
    assert sys.version_info[:2] == (3, 12), "use Python 3.12"
    assert len(sys.argv) == 2, "usage: check_export.py ABSOLUTE_EXPORT_DIRECTORY"
    directory = Path(sys.argv[1])
    assert directory.is_absolute() and directory.is_dir()
    with (directory / "cases.tsv").open() as metadata:
        rows = list(csv.DictReader((line for line in metadata if not line.startswith("#")), delimiter="\t"))
    assert len(rows) == 12 and len({row["filename"] for row in rows}) == 12
    for row in rows:
        name = row["filename"]
        assert Path(name).name == name and name.endswith(".xml")
        assert len((directory / name).read_bytes()) == int(row["bytes"])
    check_originals(directory)
    check_all_fields(directory)
    check_escaping(directory)
    print("PASS: 12 actual prepare exports; independent expanded-name mapping/order, A-J constants, CR/LF/tab/Unicode preservation")


if __name__ == "__main__":
    main()
