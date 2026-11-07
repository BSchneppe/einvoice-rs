#!/usr/bin/env python3
"""Dispatch fixtures and row-specific in-memory mutations; never execute XSLT."""

import argparse
import copy
import unittest
from pathlib import Path

import correspondence as audit
import dispatch


def parse(key, data):
    return audit.parse_xml(data, *key.split("!", 1), "mutation", "mutation")


def fixture():
    """Small authored instance of the admitted grammar, independent of archives."""
    rule = '''<xsl:template match="ram:{match}" mode="m" priority="{priority}">
      <xsl:param name="schxslt:patterns-matched" as="xs:string*"/>
      <xsl:choose><xsl:when test="$schxslt:patterns-matched[. = 'p']">
        <schxslt:rule pattern="p"><xsl:comment>suppressed</xsl:comment>
          <svrl:suppressed-rule><xsl:attribute name="context">ram:{match}</xsl:attribute></svrl:suppressed-rule>
        </schxslt:rule><xsl:next-match><xsl:with-param name="schxslt:patterns-matched" as="xs:string*" select="$schxslt:patterns-matched"/></xsl:next-match>
      </xsl:when><xsl:otherwise><schxslt:rule pattern="p">
        <svrl:fired-rule><xsl:attribute name="context">ram:{match}</xsl:attribute></svrl:fired-rule>
      </schxslt:rule><xsl:next-match><xsl:with-param name="schxslt:patterns-matched" as="xs:string*" select="($schxslt:patterns-matched, 'p')"/></xsl:next-match>
      </xsl:otherwise></xsl:choose></xsl:template>'''
    return ('''<xsl:transform xmlns:xsl="http://www.w3.org/1999/XSL/Transform"
      xmlns:schxslt="https://doi.org/10.5281/zenodo.1495494"
      xmlns:svrl="http://purl.oclc.org/dsdl/svrl" xmlns:xs="http://www.w3.org/2001/XMLSchema"
      xmlns:ram="urn:fixture" version="2.0">
      <xsl:template name="schxslt.validate"><xsl:apply-templates select="document($schxslt.validate.initial-document-uri)"/></xsl:template>
      <xsl:template match="root()"><xsl:param name="schxslt.validate.recursive-call" as="xs:boolean" select="false()"/>
        <xsl:choose><xsl:when test="not($schxslt.validate.recursive-call) and (normalize-space($schxslt.validate.initial-document-uri) != '')">
          <xsl:apply-templates select="document($schxslt.validate.initial-document-uri)"><xsl:with-param name="schxslt.validate.recursive-call" as="xs:boolean" select="true()"/></xsl:apply-templates>
        </xsl:when><xsl:otherwise><xsl:variable name="metadata"/>
          <xsl:variable name="report" as="element(schxslt:report)"><schxslt:report><xsl:call-template name="m"/></schxslt:report></xsl:variable>
          <xsl:variable name="schxslt:report"/><svrl:schematron-output/>
        </xsl:otherwise></xsl:choose></xsl:template>
      <xsl:template name="m"><schxslt:document><schxslt:pattern id="p">
        <xsl:if test="exists(base-uri(root()))"><xsl:attribute name="documents" select="base-uri(root())"/></xsl:if>
        <xsl:for-each select="root()"><svrl:active-pattern id="pattern" name="pattern"><xsl:attribute name="documents" select="base-uri(.)"/></svrl:active-pattern></xsl:for-each>
      </schxslt:pattern><xsl:apply-templates select="root()" mode="m"/></schxslt:document></xsl:template>
      ''' + rule.format(match="A", priority=1) + rule.format(match="B", priority=0) + '''
      <xsl:template match="text() | @*" mode="#all" priority="-10"/>
      <xsl:template match="/" mode="#all" priority="-10"><xsl:apply-templates select="node()" mode="#current"/></xsl:template>
      <xsl:template match="*" mode="#all" priority="-10"><xsl:apply-templates select="@*" mode="#current"/><xsl:apply-templates select="node()" mode="#current"/></xsl:template>
    </xsl:transform>''').encode()


class RecognizerTests(unittest.TestCase):
    def test_authored_shape_ranks_priorities(self):
        key = dispatch.CONFIGURED[0]
        data = fixture()
        table = dispatch.derive(key, parse(key, data))
        self.assertEqual([r["match"] for r in table["rules"]], ["ram:A", "ram:B"])
        self.assertEqual(table["mode"], "m")
        self.assertFalse(table["entry_bytes_equal"])
        swapped = data.replace(b'priority="1"', b'priority="-1"')
        table = dispatch.derive(key, parse(key, swapped))
        self.assertEqual([r["match"] for r in table["rules"]], ["ram:B", "ram:A"])

    def test_unknown_shapes_fail_closed(self):
        for before, after, reason in (
            (b'<xsl:transform ', b'<xsl:transform xpath-default-namespace="urn:other" ', "expected transform"),
            (b'<xsl:template name="m">', b'<xsl:import href="other"/><xsl:template name="m">', "top-level"),
            (b'<xsl:template name="m">', b'<xsl:evaluate xpath="x"/><xsl:template name="m">', "top-level"),
            (b'priority="1"', b'priority="0"', "unique rule priorities"),
            (b'priority="1"', b'priority="-10"', "above defaults"),
            (b'<xsl:next-match>', b'<xsl:next-match use-when="false()">', "static context"),
            (b'<xsl:next-match>', b'<xsl:next-match xmlns:schxslt="urn:other">', "namespace rebinding"),
            (b' select="$schxslt:patterns-matched"', b' select="()"', "with-param"),
            (b'as="xs:string*"/>', b'as="xs:string*" tunnel="yes"/>', "expected param"),
            (b'as="xs:string*"/>', b'as="xs:string*" select="\'seed\'"/>', "expected param"),
            (b'<xsl:choose><xsl:when test="$schxslt:patterns-matched',
             b'<xsl:variable xmlns:s="https://doi.org/10.5281/zenodo.1495494" name="s:patterns-matched" select="()"/><xsl:choose><xsl:when test="$schxslt:patterns-matched',
             "suppression shadow"),
        ):
            with self.subTest(reason=reason):
                key = dispatch.CONFIGURED[0]
                with self.assertRaisesRegex(dispatch.Unsupported, reason):
                    dispatch.derive(key, parse(key, fixture().replace(before, after, 1)))

    def test_match_outside_admitted_pattern_subset_is_unsupported(self):
        key = dispatch.CONFIGURED[0]
        data = fixture().replace(b'ram:A', b'self::ram:A')
        with self.assertRaisesRegex(dispatch.Unsupported, "rule match outside bounded XSLT 2.0 pattern subset"):
            dispatch.derive(key, parse(key, data))

    def test_only_expression_local_binding_ids_are_normalized(self):
        qname = ("", "x")
        def tree(local, host):
            return ("bind", "some", ((qname, local, ("variable", qname, host)),), ("variable", qname, local))
        self.assertEqual(dispatch.canonical_tree(tree("a", "host:1")), dispatch.canonical_tree(tree("b", "host:1")))
        self.assertNotEqual(dispatch.canonical_tree(tree("a", "host:1")), dispatch.canonical_tree(tree("b", "host:2")))


class DispatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.value, cls.docs, cls.payload, cls.ident = audit.load_inputs(cls.archives, cls.directory)
        rows, _, _ = audit.enumerate_obligations(cls.value, cls.docs, cls.payload, cls.ident)
        cls.rows = [r for r in rows if r["facet"] in (dispatch.RULE_FACET, dispatch.CONTRACT_FACET)]
        cls.tables = {k: dispatch.derive(k, cls.docs[k]) for k in dispatch.CONFIGURED}

    def run_changed(self, key=None, data=None, value=None):
        """Keep the audit's actual selected rows; refresh mutated XML occurrences.

        Mutations bypass outer pins deliberately. Diagnostic IDs retain the
        original association; Expat supplies new indexes, spans, hashes and IDs.
        No test copies the recognizer's extraction or comparison logic.
        """
        docs, rows = dict(self.docs), copy.deepcopy(self.rows)
        if key:
            docs[key] = parse(key, data)
            executable = audit.executable_view(docs[key])
            current_templates = {e["fields"]["id"]["value"]: e["template"] for e in executable["emitters"]
                                 if e["kind"] in ("failed-assert", "successful-report")}
            for row in rows:
                src = row["source"]
                if src["archive"] + "!" + src["member"] != key:
                    continue
                marker = next(e["fields"]["id"]["value"] for e in self.docs[key]["executable"]["emitters"]
                              if e["template"] == src["node_index"] and e["kind"] in ("failed-assert", "successful-report"))
                i = current_templates[marker]
                n = docs[key]["nodes"][i]
                src.update(node_index=i, raw_span=n["raw_span"], raw_sha256=n["raw_sha256"])
                row["obligation_id"] = audit.sha(audit.enc([key, i, row["category"], row["facet"]]))[:20]
        dispatch.update_obligations(rows, value if value is not None else self.value, docs)
        # Position pairs identify the exact original obligations even after byte
        # insertion shifts XML node indexes. Baseline IDs are asserted separately.
        return {original["obligation_id"]: changed for original, changed in zip(self.rows, rows, strict=True)}

    def rule_id(self, key, index=0):
        ti = self.tables[key]["rules"][index]["template"]["node_index"]
        return next(r["obligation_id"] for r in self.rows if r["facet"] == dispatch.RULE_FACET and
                    r["source"]["member"] == key.split("!", 1)[1] and r["source"]["node_index"] == ti)

    def contract_id(self, si):
        return next(r["obligation_id"] for r in self.rows if r["facet"] == dispatch.CONTRACT_FACET and
                    r["candidate_locations"] == [f"/effective_package/schemas/{si}/dispatch"])

    def test_all_158_existing_rows_are_exact(self):
        self.assertEqual(len(self.rows), 158)
        self.assertEqual({r["status"] for r in self.rows}, {"mapped-exact"})
        self.assertEqual([self.contract_id(si) for si in range(2)], ["5fb2765f4e1d6d8d7388", "cf2b899b8b38f62798ae"])
        self.assertEqual(self.rule_id(dispatch.CONFIGURED[0]), "7fec5fb861edf27e8398")
        self.assertEqual(self.rule_id(dispatch.CONFIGURED[1]), "c916f9c053c1ae8cc3b3")
        for si, key in enumerate(dispatch.CONFIGURED):
            table = self.tables[key]
            self.assertEqual(len(table["rules"]), [113, 43][si])
            self.assertEqual(table["template_count"], [119, 49][si])
            proof = next(r for r in self.rows if r["obligation_id"] == self.contract_id(si))["evidence"]["dispatch"]
            self.assertEqual(proof["entry"]["raw_sha256"], dispatch.ENTRY_SHA256[key])
            self.assertIn("no strict XSLT 2.0 portability", proof["entry_note"])

    def test_configured_mutations_change_specific_rows(self):
        for si, key in enumerate(dispatch.CONFIGURED):
            table, data = self.tables[key], self.payload[key]
            first, second = table["rules"][:2]
            def replace(evidence, transform):
                a, b = evidence["byte_span"]
                return data[:a] + transform(data[a:b]) + data[b:]
            swapped = replace(first["template"], lambda b: b.replace(
                f'priority="{first["priority"]}"'.encode(), f'priority="{second["priority"]}"'.encode(), 1))
            a, b = second["template"]["byte_span"]
            swapped = swapped[:a] + swapped[a:b].replace(f'priority="{second["priority"]}"'.encode(),
                f'priority="{first["priority"]}"'.encode(), 1) + swapped[b:]
            other = next(p["token"] for p in table["patterns"] if p["token"] != first["token"])
            fallback = next(d for d in table["defaults"] if d["attributes"]["match"] == "*")
            cases = {
                "swap priorities": (swapped, "different", "different"),
                "other mode": (replace(first["template"], lambda b: b.replace(f'mode="{table["mode"]}"'.encode(), b'mode="other"', 1)), "unsupported", "unsupported"),
                "no guard": (replace(first["guard"], lambda b: b.replace(first["guard"]["attributes"]["test"].encode(), b'false()', 1)), "unsupported", "unsupported"),
                "no fired next": (replace(first["fired"]["next_match"], lambda _: b''), "unsupported", "unsupported"),
                "no suppressed next": (replace(first["suppressed"]["next_match"], lambda _: b''), "unsupported", "unsupported"),
                "wrong append": (replace(first["fired"]["with_param"], lambda b: b.replace(first["token"].encode(), other.encode())), "unsupported", "unsupported"),
                "no append": (replace(first["fired"]["with_param"], lambda b: b.replace(f"(${dispatch.P}, '{first['token']}')".encode(), ('$' + dispatch.P).encode())), "unsupported", "unsupported"),
                "changed match and markers": (replace(first["template"], lambda b: b.replace(first["match"].encode(), b'ram:Changed')), "different", "different"),
                "extra template": (data.replace(b'</xsl:transform>', f'<xsl:template match="ram:Extra" mode="{table["mode"]}" priority="999"/></xsl:transform>'.encode()), "unsupported", "unsupported"),
                "missing default": (replace(fallback, lambda _: b''), "unsupported", "unsupported"),
                "forward tunnel state": (replace(fallback, lambda b: b.replace(b'/>', b'><xsl:with-param name="schxslt:patterns-matched" tunnel="yes" select="\'leak\'"/></xsl:apply-templates>', 1)), "unsupported", "unsupported"),
                "forward ordinary state": (replace(fallback, lambda b: b.replace(b'/>', b'><xsl:with-param name="schxslt:patterns-matched" select="\'leak\'"/></xsl:apply-templates>', 1)), "unsupported", "unsupported"),
                "changed entry bytes": (replace(table["entry"], lambda b: b.replace(b'<xsl:template ', b'<xsl:template  ', 1)), "mapped-exact", "unexamined"),
                "changed root pattern": (replace(table["entry"], lambda b: b.replace(b'match="root()"', b'match="/"', 1)), "unsupported", "unexamined"),
                "conditional launch": (replace(table["driver_call"], lambda b: b'<xsl:if test="false()">' + b + b'</xsl:if>'), "unsupported", "unexamined"),
                "rebind guard prefix": (replace(first["guard"], lambda b: b.replace(b'<xsl:when ', b'<xsl:when xmlns:schxslt="urn:other" ', 1)), "unsupported", "unsupported"),
                "tunnel parameter": (replace(first["parameter"], lambda b: b.replace(b'/>', b' tunnel="yes"/>')), "unsupported", "unsupported"),
                "preseed parameter": (replace(first["parameter"], lambda b: b.replace(b'/>', b' select="\'seed\'"/>')), "unsupported", "unsupported"),
                "hidden dispatch": (data.replace(b'</xsl:transform>', b'<xsl:function name="schxslt:hidden"><xsl:next-match/></xsl:function></xsl:transform>'), "unsupported", "unsupported"),
            }
            for name, (changed, status, contract_status) in cases.items():
                with self.subTest(document=si, mutation=name):
                    self.assertNotEqual(data, changed, "mutation did not apply")
                    rows = self.run_changed(key, changed)
                    self.assertEqual(rows[self.rule_id(key)]["status"], status)
                    self.assertEqual(rows[self.contract_id(si)]["status"], contract_status)
                    self.assertEqual(rows[self.contract_id(1-si)]["status"], "mapped-exact")
                    if status == "unsupported":
                        self.assertTrue(all(r["status"] == "unsupported" for r in rows.values() if
                            r["facet"] == dispatch.RULE_FACET and r["source"]["member"] == key.split("!", 1)[1]))
                        self.assertIn("unsupported", rows[self.rule_id(key)]["reason"])
                    if name == "swap priorities":
                        self.assertEqual(rows[self.rule_id(key, 1)]["status"], "different")

    def test_candidate_order_activation_and_binding_mutations(self):
        for si, key in enumerate(dispatch.CONFIGURED):
            for name in ("rule order", "pattern order", "phase order", "phase membership", "selection", "inactive", "host binding"):
                with self.subTest(document=si, mutation=name):
                    value = copy.deepcopy(self.value)
                    schema = value["effective_package"]["schemas"][si]
                    pattern = next(p for p in schema["patterns"] if len(p["rules"]) > 1)
                    if name == "rule order":
                        pattern["rules"][0], pattern["rules"][1] = pattern["rules"][1], pattern["rules"][0]
                    elif name == "pattern order":
                        schema["patterns"].reverse()
                    elif name == "phase order":
                        if len(schema["phases"]) > 1:
                            schema["phases"].reverse()
                        else:
                            schema["phases"][0]["active_patterns"].reverse()
                    elif name == "phase membership":
                        schema["phases"][0]["active_patterns"].append("missing")
                    elif name == "selection":
                        schema["selection"] = "other"
                    elif name == "inactive":
                        schema["patterns"][0]["active"] = False
                    else:
                        if si == 0:
                            continue  # CEN matches have no host binding references.
                        def corrupt(item):
                            if isinstance(item, dict):
                                if item.get("kind") == "variable":
                                    item["binding"] = "wrong-host"
                                for child in item.values():
                                    corrupt(child)
                            elif isinstance(item, list):
                                for child in item:
                                    corrupt(child)
                        for p in schema["patterns"]:
                            for r in p["rules"]:
                                corrupt(r["context"]["tree"])
                    rows = self.run_changed(value=value)
                    self.assertEqual(rows[self.contract_id(si)]["status"], "different")
                    if name != "host binding":
                        self.assertEqual(rows[self.rule_id(key)]["status"], "different")

    def test_candidate_proof_mutations_change_specific_rows(self):
        for si, key in enumerate(dispatch.CONFIGURED):
            for name in ("dispatch enum", "default phase", "source span", "context text"):
                with self.subTest(document=si, mutation=name):
                    value = copy.deepcopy(self.value)
                    schema = value["effective_package"]["schemas"][si]
                    rule = next(p for p in schema["patterns"] if p["rules"])["rules"][0]
                    if name == "dispatch enum":
                        schema["dispatch"] = "AnyRule"
                    elif name == "default phase":
                        schema["declared_default_phase"] = "other"
                    elif name == "source span":
                        rule["source"]["xml_byte_start"] += 1
                    else:
                        rule["context"]["text"] = "//ram:Changed"
                    self.assertNotEqual(value, self.value, "mutation did not apply")
                    rows = self.run_changed(value=value)
                    affected = {self.contract_id(si), self.rule_id(key)}
                    if name in ("dispatch enum", "default phase"):
                        affected.update(r["obligation_id"] for r in self.rows
                                        if r["facet"] == dispatch.RULE_FACET and
                                        r["source"]["member"] == key.split("!", 1)[1])
                    for oid, row in rows.items():
                        self.assertEqual(row["status"], "different" if oid in affected else "mapped-exact", oid)

    def test_configured_suppressed_marker_context_must_equal_match(self):
        for si, key in enumerate(dispatch.CONFIGURED):
            with self.subTest(document=si):
                first = self.tables[key]["rules"][0]
                doc = self.docs[key]
                wrapper = first["suppressed"]["wrapper"]["node_index"]
                marker = next(i for i in doc["nodes"][wrapper]["children"]
                              if doc["nodes"][i]["name"] == [dispatch.V, "suppressed-rule"])
                context = next(i for i in doc["nodes"][marker]["children"]
                               if doc["nodes"][i]["name"] == [dispatch.X, "attribute"] and
                               doc["nodes"][i]["attributes"].get("name") == "context")
                a, b = doc["nodes"][context]["raw_span"]
                data = self.payload[key]
                changed = data[:a] + data[a:b].replace(first["match"].encode(), b'//ram:Wrong') + data[b:]
                self.assertNotEqual(data, changed, "mutation did not apply")
                with self.assertRaisesRegex(dispatch.Unsupported, "marker context must equal match"):
                    dispatch.derive(key, parse(key, changed))
                rows = self.run_changed(key, changed)
                self.assertEqual(rows[self.rule_id(key)]["status"], "unsupported")
                self.assertEqual(rows[self.contract_id(si)]["status"], "unsupported")
                self.assertEqual(rows[self.contract_id(1-si)]["status"], "mapped-exact")

    def test_coordinated_compiled_and_candidate_match_still_differs_from_source(self):
        key = dispatch.CONFIGURED[0]
        first = self.tables[key]["rules"][0]
        value = copy.deepcopy(self.value)
        context = value["effective_package"]["schemas"][0]["patterns"][0]["rules"][0]["context"]
        context["text"] = "//ram:Changed"
        context["tree"]["right"]["test"]["name"].update(qualified="ram:Changed", local="Changed")
        a, b = first["template"]["byte_span"]
        data = self.payload[key]
        data = data[:a] + data[a:b].replace(first["match"].encode(), b'//ram:Changed') + data[b:]
        rows = self.run_changed(key, data, value)
        self.assertEqual(rows[self.rule_id(key)]["status"], "different")
        self.assertEqual(rows[self.contract_id(0)]["status"], "different")

    def test_join_rejects_missing_duplicate_and_changed_source_occurrences(self):
        for mutation in ("missing", "duplicate", "source"):
            rows = copy.deepcopy(self.rows)
            if mutation == "missing":
                rows.pop()
            elif mutation == "duplicate":
                rows[-1] = rows[0]
            else:
                rows[0]["source"]["raw_sha256"] = "0" * 64
            with self.subTest(mutation=mutation), self.assertRaises(dispatch.Unsupported):
                dispatch.update_obligations(rows, self.value, self.docs)

    def test_integration_updates_actual_priority_rows(self):
        key = dispatch.CONFIGURED[0]
        payload, docs = dict(self.payload), dict(self.docs)
        data = payload[key]
        # Increasing the second rule's priority changes source order without
        # touching any candidate location or template association.
        payload[key] = data.replace(b'priority="111"', b'priority="113"', 1)
        docs[key] = parse(key, payload[key])
        docs[key]["executable"] = audit.executable_view(docs[key])
        rows, _, _ = audit.enumerate_obligations(self.value, docs, payload, self.ident)
        by_id = {r["obligation_id"]: r for r in rows}
        self.assertEqual(by_id[self.rule_id(key)]["status"], "different")
        self.assertEqual(by_id[self.rule_id(key, 1)]["status"], "different")
        self.assertEqual(by_id[self.contract_id(0)]["status"], "different")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--archives", type=Path, required=True)
    args = parser.parse_args()
    DispatchTests.directory, DispatchTests.archives = args.candidate, args.archives
    unittest.main(argv=["test_dispatch.py"])
