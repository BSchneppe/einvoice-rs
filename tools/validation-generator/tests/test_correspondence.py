#!/usr/bin/env python3
"""Pinned-source regressions: compare facets directly as well as authenticating bytes.

In-memory mutations deliberately bypass outer hashes to exercise the independent
comparison. File mutations separately prove that reviewed candidate/member pins
cannot be replaced with self-reported digests. No invoice or XSLT is executed.
"""

import argparse
import copy
import io
import subprocess
import sys
import shutil
import tempfile
import unittest
import zipfile
from pathlib import Path

import correspondence as audit
import candidate as candidate_checks
from inventory import all_expressions
from test_xpath_structure import ir_number


def rules(value):
    return [r for s in value["effective_package"]["schemas"] for p in s["patterns"] for r in p["rules"]]


def sites(value):
    return [a for r in rules(value) for a in r["sites"]]


class CorrespondenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.value, cls.docs, cls.payload, cls.ident = audit.load_inputs(cls.archives, cls.directory)
        cls.rows, cls.coverage, cls.locations = audit.enumerate_obligations(
            cls.value, cls.docs, cls.payload, cls.ident
        )
        cls.summary = cls.summarize(cls.rows)

    @classmethod
    def summarize(cls, rows):
        return audit.summarize(cls.value, rows, cls.coverage, cls.locations, cls.payload, cls.ident)

    def changed(self, edit, facet, before_status="mapped-exact", after_status="different"):
        value = copy.deepcopy(self.value)
        edit(value)
        rows, _, _ = audit.enumerate_obligations(value, self.docs, self.payload, self.ident)
        self.assert_affected(self.rows, rows, facet, before_status, after_status)
        with self.assertRaisesRegex(ValueError, "correspondence accounting changed"):
            audit.check_accounting(self.summarize(rows), self.summary)

    def assert_affected(self, baseline, rows, facet, before_status="mapped-exact", after_status="different"):
        before = {r["obligation_id"]: r for r in baseline if r["facet"] == facet}
        affected = [(before[r["obligation_id"]], r) for r in rows
                    if r["obligation_id"] in before and r["evidence"] != before[r["obligation_id"]]["evidence"]]
        self.assertTrue(affected, "comparison ignored mutation: " + facet)
        for original, changed in affected:
            with self.subTest(obligation=original["obligation_id"]):
                self.assertEqual(original["status"], before_status)
                self.assertEqual(changed["status"], after_status)
                self.assertEqual(original["source"], changed["source"])
                self.assertEqual(original["evidence"]["source"], changed["evidence"]["source"])
                self.assertNotEqual(original["evidence"]["candidate"], changed["evidence"]["candidate"])
                if before_status == "mapped-exact":
                    self.assertEqual(original["evidence"]["source"], original["evidence"]["candidate"])
                self.assertNotEqual(changed["evidence"]["source"], changed["evidence"]["candidate"])
        return affected

    def test_reviewed_positive_control(self):
        expected = audit.read_json((audit.HERE / "CORRESPONDENCE-ACCOUNTING.json").read_bytes())
        audit.check_accounting(self.summary, expected)
        self.assertEqual(self.summary["result"], "incomplete")
        self.assertEqual(self.summary["denominator"], 12571)
        self.assertEqual(
            self.summary["counts"],
            {"mapped-exact": 12492, "different": 24, "ambiguous": 3, "unsupported": 27, "unexamined": 25},
        )
        self.assertEqual(len(self.summary["dispatch_entry_profiles"]), 2)
        for profile in self.summary["dispatch_entry_profiles"]:
            self.assertEqual(profile["status"], "mapped-exact")
            self.assertTrue(profile["entry_bytes_equal"])
            self.assertEqual(len(profile["entry"]["raw_sha256"]), 64)
            self.assertIn("no strict XSLT 2.0 portability", profile["entry_note"])

    def test_moved_assertion_to_another_rule(self):
        def edit(value):
            source, dest = [r for r in rules(value) if r["sites"]][:2]
            source["sites"][0], dest["sites"][0] = dest["sites"][0], source["sites"][0]

        self.changed(edit, "ordered rule/site membership")

    def test_missing_extra_duplicate_obligation(self):
        # These mutations model enumeration regressions with the same candidate
        # bytes. Counts alone cannot certify the source denominator.
        for kind in ["missing", "extra", "duplicate"]:
            with self.subTest(kind=kind):
                rows = list(self.rows)
                row = copy.deepcopy(rows[0])
                if kind == "missing":
                    rows.pop(0)
                elif kind == "extra":
                    row["obligation_id"] = "invented-occurrence-facet"
                    rows.append(row)
                else:
                    rows.append(row)
                with self.assertRaises(ValueError):
                    audit.check_accounting(self.summarize(rows), self.summary)
        # Same counts, identities and statuses, but a changed exact occurrence:
        rows = copy.deepcopy(self.rows)
        row = next(r for r in rows if r["status"] == "mapped-exact")
        row["source"]["raw_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "accounting changed"):
            audit.check_accounting(self.summarize(rows), self.summary)

    def test_missing_assertion(self):
        value = copy.deepcopy(self.value)
        rules(value)[0]["sites"].pop(0)
        with self.assertRaises(KeyError):
            audit.enumerate_obligations(value, self.docs, self.payload, self.ident)

    def test_extra_and_duplicate_assertion(self):
        for extra in [False, True]:
            with self.subTest(extra=extra):

                def edit(value, extra=extra):
                    rule = rules(value)[0]
                    site = copy.deepcopy(rule["sites"][0])
                    if extra:
                        site["identity"] = "invented"
                        site["source"]["xml_byte_start"] += 1
                    rule["sites"].append(site)

                value = copy.deepcopy(self.value)
                edit(value)
                with self.assertRaisesRegex(ValueError, r"zip\(\) argument 2 is longer"):
                    audit.enumerate_obligations(value, self.docs, self.payload, self.ident)

    def test_reordered_contexts(self):
        def edit(value):
            first, second = rules(value)[:2]
            first["context"], second["context"] = second["context"], first["context"]

        self.changed(edit, "configured template match")

    def test_reordered_phase_activation(self):
        def edit(value):
            phase = next(
                p
                for s in value["effective_package"]["schemas"]
                for p in s["phases"]
                if len(p["active_patterns"]) > 1
            )
            phase["active_patterns"].reverse()

        self.changed(edit, "phase ordered active patterns")

    def test_swapped_predicate_operands(self):
        def edit(value):
            expression = next(a["test"] for a in sites(value) if a["id"] == "CII-SR-030")
            left, right = expression["text"].split(" <= ")
            expression["text"] = right + " <= " + left

        self.changed(edit, "expression lexical:test")

    def test_numeric_lexeme_is_exact(self):
        def edit(value):
            expression = next(e for e in all_expressions(value["effective_package"]) if "0.02" in e["text"])
            expression["text"] = expression["text"].replace("0.02", ".02")

        self.changed(edit, "expression lexical:value")

    def test_padded_codelist_string_is_exact(self):
        def edit(value):
            expression = value["effective_package"]["schemas"][0]["patterns"][2]["rules"][0]["sites"][0][
                "test"
            ]
            literal = next(
                t
                for kind, t in audit.lex.tokens(expression["text"])
                if kind == "string" and t.startswith(" ") and t.endswith(" ") and t.strip()
            )
            before = [t for kind, t in audit.lex.tokens(expression["text"]) if kind == "string"]
            # Replace the complete quoted literal, never a separator or single-space literal.
            quoted = next(q + literal.replace(q, q + q) + q for q in ["'", '"']
                          if q + literal.replace(q, q + q) + q in expression["text"])
            expression["text"] = expression["text"].replace(quoted, quoted[0] + literal[1:-1] + quoted[-1], 1)
            after = [t for kind, t in audit.lex.tokens(expression["text"]) if kind == "string"]
            self.assertIn(literal, before)
            self.assertNotIn(literal, after)
            self.assertIn(literal[1:-1], after)
            self.assertNotEqual(before, after)

        self.changed(edit, "original codelist predicate")

    def test_expression_whitespace_is_exact(self):
        def edit(value):
            expression = next(a["test"] for a in sites(value) if a["id"] == "CII-SR-030")
            self.assertIn(" <= ", expression["text"])
            expression["text"] = expression["text"].replace(" <= ", "<=")

        self.changed(edit, "expression lexical:test")

    def test_wrong_variable_binding_declaration_id(self):
        def edit(value):
            binding = next(s["globals"][0] for s in value["effective_package"]["schemas"] if s["globals"])
            binding["id"] += "/wrong"

        self.changed(edit, "binding declaration/initializer")

    def test_binding_facets_reject_candidate_mutants(self):
        def objects(x):
            if isinstance(x, dict):
                yield x
                for child in x.values():
                    yield from objects(child)
            elif isinstance(x, list):
                for child in x:
                    yield from objects(child)

        admitted = [r for r in self.rows if r["status"] == "mapped-exact" and r["evidence"].get("references")]
        declarations = [x for x in objects(self.value["effective_package"])
                        if "declared_owner" in x and "id" in x]
        value, cases, used = copy.deepcopy(self.value), [], set()
        for label in ["wrong declaration ID", "same-name other scope", "range declaration ID"]:
            for row in admitted:
                pointer = row["candidate_locations"][0]
                if pointer in used:
                    continue
                nodes = list(objects(audit.resolve_pointer(value, pointer)))
                if label == "range declaration ID":
                    target = next((x for x in nodes if x.get("kind") == "bind"), None)
                    if target is None:
                        continue
                    old = target["bindings"][0]["id"]
                    target["bindings"][0]["id"] += "/wrong"
                    # Coordinated mutation remains self-consistent but must fail.
                    for ref in nodes:
                        if ref.get("kind") == "variable" and ref["binding"] == old:
                            ref["binding"] += "/wrong"
                else:
                    ref = next(x for x in nodes if x.get("kind") == "variable")
                    others = [d for d in declarations if d["id"] != ref["binding"] and d["name"] == ref["name"]]
                    if label == "same-name other scope" and not others:
                        continue
                    ref["binding"] = others[0]["id"] if label == "same-name other scope" else "wrong-declaration-id"
                used.add(pointer)
                cases.append((label, row["obligation_id"]))
                break
            else:
                self.fail("missing real-source mutant witness: " + label)
        rows, _, _ = audit.enumerate_obligations(value, self.docs, self.payload, self.ident)
        after = {r["obligation_id"]: r for r in rows}
        for label, identity in cases:
            with self.subTest(mutant=label, obligation=identity):
                self.assertEqual(after[identity]["status"], "different")
                self.assertNotEqual(after[identity]["evidence"]["source_tree"], after[identity]["evidence"]["candidate_tree"])
        self.assertEqual([r for r in rows if "expression lexical:" in r["facet"]],
                         [r for r in self.rows if "expression lexical:" in r["facet"]])
        with self.assertRaisesRegex(ValueError, "accounting changed"):
            audit.check_accounting(self.summarize(rows), self.summary)

    def test_structure_facets_reject_candidate_mutants(self):
        def objects(x):
            if isinstance(x, dict):
                yield x
                for child in x.values():
                    yield from objects(child)
            elif isinstance(x, list):
                for child in x:
                    yield from objects(child)

        def chain(x):
            return (x.get("kind") == "binary" and x.get("operator") in {"and", "or", "/", "*"}
                    and x["left"].get("kind") == "binary" and x["left"]["operator"] == x["operator"])

        def reassociate(x):
            left = x["left"]
            x["left"], x["right"] = left["left"], {
                "kind": "binary", "operator": x["operator"], "left": left["right"], "right": x["right"]}

        def move_group(x):
            body = x["operand"]
            x.clear()
            x.update(body)
            x["left"] = {"kind": "group", "operand": x["left"]}

        def move_predicate(x):
            predicate = x["right"]
            base = {"kind": "binary", "operator": "/", "left": x["left"], "right": predicate["base"]}
            x.clear()
            x.update(kind="filter", base=base, predicate=predicate["predicate"])

        mutations = [
            ("reassociation", chain, reassociate),
            ("moved Group", lambda x: x.get("kind") == "group" and x["operand"].get("kind") == "binary",
             move_group),
            ("moved predicate", lambda x: x.get("kind") == "binary" and x["operator"] == "/"
             and x["right"].get("kind") == "filter", move_predicate),
            ("swapped axis", lambda x: x.get("kind") == "step" and x["axis"] == "child",
             lambda x: x.update(axis="descendant")),
            ("wrong arity", lambda x: x.get("kind") == "call" and bool(x["arguments"]),
             lambda x: x["arguments"].pop()),
        ]
        value, docs, cases, used = copy.deepcopy(self.value), copy.deepcopy(self.docs), [], set()
        for label, predicate, mutate in mutations:
            row = next(r for r in self.rows if "expression AST:" in r["facet"] and r["status"] == "mapped-exact"
                       and r["candidate_locations"][0] not in used
                       and any(predicate(x) for x in objects(r["evidence"]["candidate"])))
            pointer = row["candidate_locations"][0]
            used.add(pointer)
            cases.append((label, row["obligation_id"], pointer, predicate, mutate))

        # The pinned decimal also exercises binding identity. Seed variable-free
        # fixtures at distinct real facets to exercise numeric admission end to end.
        for label, mutate in [("numeric lexeme", lambda x: x.update(lexeme=".02")),
                              ("numeric kind", lambda x: x.update(lexical_kind="double"))]:
            row = next(r for r in self.rows if r["facet"] == "expression AST:value"
                       and r["status"] == "mapped-exact" and r["candidate_locations"][0] not in used)
            pointer = row["candidate_locations"][0]
            used.add(pointer)
            expression = audit.resolve_pointer(value, pointer.rsplit("/", 1)[0])
            expression.update(text="0.02", tree=ir_number("0.02", "decimal"))
            source = row["source"]
            docs[source["archive"] + "!" + source["member"]]["nodes"][source["node_index"]]["attributes"]["value"] = "0.02"
            cases.append((label, row["obligation_id"], pointer, lambda x: x.get("kind") == "number", mutate))

        baseline, _, _ = audit.enumerate_obligations(value, docs, self.payload, self.ident)
        for _, _, pointer, predicate, mutate in cases:
            mutate(next(x for x in objects(audit.resolve_pointer(value, pointer)) if predicate(x)))
        changed, _, _ = audit.enumerate_obligations(value, docs, self.payload, self.ident)
        before = {r["obligation_id"]: r for r in baseline}
        after = {r["obligation_id"]: r for r in changed}
        for label, obligation, _, _, _ in cases:
            with self.subTest(mutation=label, obligation=obligation):
                original, mutant = before[obligation], after[obligation]
                self.assertEqual(original["status"], "mapped-exact")
                self.assertEqual(mutant["status"], "different")
                self.assertEqual(original["source"], mutant["source"])
                self.assertEqual(original["evidence"]["source_tree"], original["evidence"]["candidate_tree"])
                self.assertNotEqual(mutant["evidence"]["source_tree"], mutant["evidence"]["candidate_tree"])
        # Structural mutations do not alter the separately compared lexical facets.
        self.assertEqual([r for r in baseline if "expression lexical:" in r["facet"]],
                         [r for r in changed if "expression lexical:" in r["facet"]])

    def test_binding_facets_and_pattern_visibility_remain_separate(self):
        binding_rows = [r for r in self.rows if "references" in r["evidence"]]
        admitted = [r for r in binding_rows if r["evidence"]["resolved"]]
        pending = [r for r in binding_rows if "effective_common_global" in r["evidence"]]
        self.assertEqual(len(binding_rows), 81)
        self.assertEqual(len(admitted), 64)
        self.assertEqual(sum(len(r["evidence"]["references"]) for r in binding_rows), 191)
        self.assertEqual({r["obligation_id"] for r in pending}, {
            "b329f14a8cb36ac93be9", "eccccc0efee5a982a454", "4036fc85c968d975885d",
            "b98070b51f0f74c6e430", "dd0b100db042bed2fbed", "28bf9a8f1bb20eb518f6",
            "0d37ce538f348307174f", "3131bb804852f7674cbe", "8893d4623ff18b30cee0",
            "f339b7d99262eddbea6f", "cda00b50e2279dec3dc3", "5b052493cceaf9eb75ed",
            "341313d310e334a46da1", "5d967cd0080a6ad08a9a", "bf3353c69e4a4d7ca129",
            "cd9a050eb0b8c88b5de6", "f2f5d148c3ca6930100c",
        })
        for row in pending:
            self.assertEqual(row["status"], "mapped-exact")
            self.assertFalse(row["evidence"]["resolved"])
            self.assertTrue(row["evidence"]["effective_common_global"]["matching"])
            for ref in row["evidence"]["references"]:
                if ref["strict_pattern_local"] == "unresolvable":
                    proof = ref["effective"]["source_declaration"]["effective_scope_proof"]
                    self.assertEqual(proof["rule"], "COMMON_GLOBAL")
                    for occurrence in [proof["comment"], proof["configured_variable"]]:
                        key = occurrence["archive"] + "!" + occurrence["member"]
                        lo, hi = occurrence["raw_span"]
                        self.assertEqual(audit.sha(self.payload[key][lo:hi]), occurrence["raw_sha256"])
        for row in admitted:
            self.assertTrue(row["evidence"]["matching"])
            self.assertTrue(all(ref["matching"] and ref["source_declaration"] and ref["candidate_binding_id"]
                                for ref in row["evidence"]["references"]))
        decimal = next(r for r in admitted if "0.02" in r["evidence"]["source"])
        self.assertEqual(decimal["obligation_id"], "181157d697cc95badd84")

    def test_common_global_missing_proof_and_wrong_candidate(self):
        import xpath_bindings as xb
        from xpath_structure import Parser
        promoted = [r for r in self.rows if "effective_common_global" in r["evidence"]]
        self.assertEqual(len(promoted), 17)
        for mutation in ["remove-comment", "alter-comment", "remove-witness", "wrong-owner", "wrong-candidate"]:
            docs = copy.deepcopy(self.docs)
            if mutation == "remove-comment":
                docs[xb.COMMON]["comments"] = []
            elif mutation == "alter-comment":
                docs[xb.COMMON]["comments"][0]["text"] += "."
            elif mutation == "wrong-owner":
                docs[xb.COMMON]["nodes"][0]["attributes"]["id"] = "another-pattern"
            elif mutation == "remove-witness":
                for n in docs[xb.CONFIGURED]["nodes"]:
                    if n["parent"] == 0 and n["name"] == [audit.X, "variable"]:
                        n["name"] = [audit.X, "param"]
            scopes = xb.SourceScopes(docs)
            for row in promoted:
                source = row["source"]
                key = source["archive"] + "!" + source["member"]
                attr = row["facet"].split(":", 1)[1]
                text = row["evidence"]["source"]
                candidate = copy.deepcopy(row["evidence"]["candidate"])
                if mutation == "wrong-candidate":
                    def change(v):
                        if isinstance(v, dict):
                            if v.get("kind") == "variable":
                                name = v["name"]["local"]
                                other = next((d["id"] for d in scopes.declarations.values()
                                              if d["name"]["local"] == name and d["id"] != v["binding"]), None)
                                if other:
                                    v["binding"] = other
                                    return True
                            return any(change(c) for c in v.values())
                        return isinstance(v, list) and any(change(c) for c in v)
                    # A real same-name declaration in another scope, when available.
                    if not change(candidate):
                        continue
                callback = lambda tree, effective=False: scopes.resolve(
                    tree, key, source["node_index"], attr, text, effective)
                status, reason, _ = audit.classify_expression(text, candidate, lambda: {}, {}, callback)
                self.assertEqual(status, "different" if mutation == "wrong-candidate" else "unexamined",
                                 (mutation, row["obligation_id"]))
                if mutation != "wrong-candidate":
                    self.assertEqual(reason, "unresolved source binding" if mutation == "wrong-owner"
                                     else "missing independent proof for common-global binding")

    def test_reordered_name_value_of_and_text_message_parts(self):
        def edit(value):
            site = next(a for a in sites(value) if a["id"] == "BR-DE-18")
            self.assertEqual(
                [p["kind"] for p in site["message"]], ["text", "name", "text", "value_of", "text"]
            )
            site["message"][1], site["message"][3] = site["message"][3], site["message"][1]
            site["message"][0], site["message"][4] = site["message"][4], site["message"][0]

        self.changed(edit, "configured ordered message constructor")

    def test_each_of_nine_severity_overrides(self):
        values = next(
            c["values"]
            for c in self.value["effective_package"]["policy"]["clauses"]
            if c["kind"] == "overrides"
        )
        self.assertEqual(len(values), 9)
        for name in values:
            with self.subTest(assertion=name):

                def edit(value, name=name):
                    clause = next(
                        c for c in value["effective_package"]["policy"]["clauses"] if c["kind"] == "overrides"
                    )
                    clause["values"][name] = "error" if values[name] != "error" else "warning"

                self.changed(edit, "scenario severity override")

    def test_reversed_candidate_stages(self):
        actual = next(r for r in self.rows if r["facet"] == "scenario ordered stages")
        self.assertEqual(actual["evidence"]["source"], ["cii-xsd", "cen", "xrechnung", "report"])
        self.assertEqual(actual["evidence"]["candidate"], self.value["effective_package"]["policy"]["clauses"][0]["stages"])
        self.assertEqual(actual["status"], "different")  # candidate also lists five engine stages
        value = copy.deepcopy(self.value)
        stages = value["effective_package"]["policy"]["clauses"][0]["stages"]
        stages.reverse()
        rows, _, _ = audit.enumerate_obligations(value, self.docs, self.payload, self.ident)
        reversed_row = next(r for r in rows if r["facet"] == actual["facet"])
        self.assertEqual(reversed_row["status"], "different")
        self.assertEqual(reversed_row["evidence"]["candidate"], stages)
        # Isolate order from the existing engine-stage difference with an exact control.
        stages[:] = ["cii-xsd", "cen", "xrechnung", "report"]
        baseline, _, _ = audit.enumerate_obligations(value, self.docs, self.payload, self.ident)
        baseline = copy.deepcopy(baseline)
        stages.reverse()
        rows, _, _ = audit.enumerate_obligations(value, self.docs, self.payload, self.ident)
        self.assert_affected(baseline, rows, actual["facet"])

    def test_missing_stage_resource(self):
        # Use a stage resource that is not also a report-owner lookup below.
        def edit(value):
            del value["effective_package"]["resources"][
                "config.zip/resources/cii/16b/xsd/CrossIndustryInvoice_100pD16B.xsd"]

        self.changed(edit, "scenario resource presence")

    def test_candidate_helper_body(self):
        def edit(value):
            helper = next(h for s in value["effective_package"]["schemas"] for h in s["helpers"])
            helper["body"].append({"kind": "text", "text": "changed helper literal"})

        self.changed(edit, "configured helper declaration/constructor")

    def test_candidate_helper_without_body_is_unexamined(self):
        def edit(value):
            helper = next(h for s in value["effective_package"]["schemas"] for h in s["helpers"])
            del helper["body"]

        self.changed(edit, "configured helper declaration/constructor", after_status="unexamined")

    def test_candidate_content_initializer(self):
        def edit(value):
            def walk(item):
                if isinstance(item, dict):
                    yield item
                    for child in item.values():
                        yield from walk(child)
                elif isinstance(item, list):
                    for child in item:
                        yield from walk(child)

            binding = next(b for b in walk(value["effective_package"])
                           if "effective_scope" in b and b["value"]["kind"] == "content")
            literal = next(i for i in binding["value"]["body"] if i["kind"] == "text")
            self.assertTrue(literal["text"].strip())
            literal["text"] += "changed literal"

        self.changed(edit, "binding declaration/initializer")

    def source_changed(self, key, transform):
        """Bypass outer pins only, parsing actual mutated XML with raw evidence.

        Existing XML paths stay fixed. Refresh candidate source ranges by those
        paths so insertions exercise correspondence, rather than location guards.
        """
        payload = dict(self.payload)
        payload[key] = transform(payload[key])
        self.assertNotEqual(payload[key], self.payload[key])
        archive, member = key.split("!", 1)
        docs = dict(self.docs)
        docs[key] = audit.parse_xml(payload[key], archive, member,
                                   self.ident["archives"][archive]["sha256"], self.ident["artifact_lock_sha256"])
        if "executable" in self.docs[key]:
            docs[key]["executable"] = audit.executable_view(docs[key])
        value = copy.deepcopy(self.value)
        by_path = {audit.enc(n["path"]): n for n in docs[key]["nodes"]}
        ranges = {}
        for original in self.docs[key]["nodes"]:
            changed = by_path[audit.enc(original["path"])]
            ranges[original["raw_span"][0], None] = changed["raw_span"]
            for attr, raw in original["raw_attributes"].items():
                ranges[raw["span"][0], attr] = changed["raw_attributes"][attr]["span"]

        def refresh(item):
            if isinstance(item, dict):
                if item.get("resource") == key.replace("!", "/", 1) and "xml_byte_start" in item:
                    lo, hi = ranges[item["xml_byte_start"], item["attribute"]]
                    item["xml_byte_start"], item["xml_byte_end"] = lo, hi
                    prefix = payload[key][:lo].decode("utf-8")
                    item["line"] = prefix.count("\n") + 1
                    item["column"] = len(prefix.rsplit("\n", 1)[-1]) + 1
                for child in item.values():
                    refresh(child)
            elif isinstance(item, list):
                for child in item:
                    refresh(child)

        refresh(value)
        rows, coverage, locations = audit.enumerate_obligations(value, docs, payload, self.ident)
        return rows, audit.summarize(value, rows, coverage, locations, payload, self.ident), payload

    def test_source_include_href(self):
        key = "xr.zip!schematron/cii/XRechnung-CII-validation.sch"
        rows, _, _ = self.source_changed(key, lambda b: b.replace(b"../common.sch", b"../absent.sch"))
        before = next(r for r in self.rows if r["facet"] == "include edge")
        after = next(r for r in rows if r["obligation_id"] == before["obligation_id"])
        self.assertEqual(before["status"], "mapped-exact")
        self.assertEqual(after["status"], "different")
        self.assertEqual(before["evidence"]["source"], before["evidence"]["candidate"])
        self.assertEqual(after["evidence"]["source"]["to"], "xr.zip/schematron/absent.sch")
        self.assertEqual(after["evidence"]["candidate"]["to"], "xr.zip/schematron/common.sch")
        self.assertEqual(after["evidence"]["href"], "../absent.sch")

    def test_unknown_primary_elements_and_attributes(self):
        key = "xr.zip!schematron/common.sch"
        suffix = (b'<s:future xmlns:s="http://purl.oclc.org/dsdl/schematron">raw evidence</s:future>'
                  b'<x:future xmlns:x="http://www.w3.org/1999/XSL/Transform"/>'
                  b'<x:assert xmlns:x="http://www.w3.org/1999/XSL/Transform" test="true()"/>'
                  b'<s:title xmlns:s="http://purl.oclc.org/dsdl/schematron" future="raw &amp; evidence"/>'
                  b'<x:choose xmlns:x="http://www.w3.org/1999/XSL/Transform" future="raw attribute"/>')
        def inject(data):
            start = data.rfind(b"</")
            return data[:start] + suffix + data[start:]

        rows, _, payload = self.source_changed(key, inject)
        elements = [r for r in rows if r["facet"] == "unsupported primary element"]
        attributes = [r for r in rows if r["facet"] == "unsupported primary attribute:future"]
        self.assertEqual(len(elements), 3)
        self.assertEqual(len(attributes), 2)
        self.assertEqual({r["evidence"]["source"]["name"][0] for r in elements}, {audit.S, audit.X})
        self.assertEqual({r["evidence"]["source"]["value"] for r in attributes},
                         {"raw & evidence", "raw attribute"})
        for row in elements + attributes:
            self.assertEqual(row["status"], "unsupported")
            self.assertIsNone(row["evidence"]["candidate"])
            lo, hi = row["source"]["raw_span"]
            self.assertEqual(row["source"]["raw_sha256"], audit.sha(payload[key][lo:hi]))
            self.assertTrue(b"future" in payload[key][lo:hi] or b"x:assert" in payload[key][lo:hi])

    def test_unknown_message_instruction(self):
        key = "config.zip!resources/cii/16b/xsl/EN16931-CII-validation.xsl"
        emitter = next(e for e in self.docs[key]["executable"]["emitters"] if e["kind"] == "failed-assert")
        text_index = next(i for i in self.docs[key]["nodes"][emitter["node"]]["children"]
                          if self.docs[key]["nodes"][i]["name"] == [audit.V, "text"])
        lo, hi = self.docs[key]["nodes"][text_index]["raw_span"]
        def inject(data):
            end = data.rfind(b"</", lo, hi)
            return data[:end] + b'<xsl:sequence select="\'unknown message\'"/>' + data[end:]

        rows, _, _ = self.source_changed(key, inject)
        before = next(r for r in self.rows if r["source"]["member"] == key.split("!", 1)[1]
                      and r["source"]["node_index"] == text_index
                      and r["facet"] == "configured ordered message constructor")
        after = next(r for r in rows if r["obligation_id"] == before["obligation_id"])
        self.assertEqual(before["status"], "mapped-exact")
        self.assertEqual(after["status"], "unsupported")
        self.assertEqual(after["evidence"]["unknown_constructors"], [
            {"name": [audit.X, "sequence"], "attributes": {"select": "'unknown message'"}, "content": []}])
        self.assertEqual(before["evidence"]["candidate"], after["evidence"]["candidate"])

    def test_excluded_scaffolding_is_explicit_and_sorted(self):
        key = "config.zip!resources/cii/16b/xsl/EN16931-CII-validation.xsl"
        name = "new-compiler-scaffolding"
        self.assertFalse(any(r["name"] == name for r in self.summary["excluded_configured_scaffolding"]))
        def inject(data):
            end = data.rfind(b"</")
            return data[:end] + b'<xsl:variable name="new-compiler-scaffolding" select="42"/>' + data[end:]

        rows, summary, payload = self.source_changed(key, inject)
        excluded = summary["excluded_configured_scaffolding"]
        added = [r for r in excluded if r["name"] == name]
        self.assertEqual(len(added), 1)
        self.assertEqual(len(excluded), len(self.summary["excluded_configured_scaffolding"]) + 1)
        self.assertEqual(excluded, sorted(excluded, key=lambda r: (r["name"], r["source"]["archive"],
                                                                 r["source"]["member"], r["source"]["node_index"])))
        lo, hi = added[0]["source"]["raw_span"]
        self.assertIn(name.encode(), payload[key][lo:hi])
        self.assertEqual(added[0]["source"]["raw_sha256"], audit.sha(payload[key][lo:hi]))
        self.assertEqual(summary["denominator"], self.summary["denominator"])
        self.assertFalse(any(r["source"] == added[0]["source"] for r in rows))
        with self.assertRaisesRegex(ValueError, "accounting changed"):
            audit.check_accounting(summary, self.summary)

    def test_scaffolding_exclusion_ignores_candidate_scope(self):
        def dicts(v):
            if isinstance(v, dict):
                yield v
            for c in v.values() if isinstance(v, dict) else v if isinstance(v, list) else []:
                yield from dicts(c)

        value = copy.deepcopy(self.value)
        bindings = [v for v in dicts(value["effective_package"])
                    if v.get("effective_scope") == "xslt2_schema_global" and v["name"]["qualified"] == "profile"]
        self.assertEqual(len(bindings), 1)
        bindings[0]["effective_scope"] = "unknown_scope"
        rows, _, _ = audit.enumerate_obligations(value, self.docs, self.payload, self.ident)
        summary = self.summarize(rows)
        self.assertFalse(any(r["name"] == "profile" for r in summary["excluded_configured_scaffolding"]))
        self.assertTrue(any(r["facet"] == "configured binding association" and r["status"] == "unsupported"
                            and r["evidence"]["source"]["attributes"].get("name") == "profile" for r in rows))

    def test_optimized_trust_guards(self):
        code = """
import correspondence as a
checks = [lambda: a.read_json(b'{"x":1,"x":2}'),
          lambda: a.read_archive(b'wrong', {'file':'x', 'sha256':'0'*64}, {}),
          lambda: a.check_accounting({}, {'wrong':1}),
          lambda: a.summarize({}, [{'obligation_id':'x'}]*2, {}, 0, {}, {}),
          lambda: a.check_occurrences("'left'", {'kind':'string', 'value':'right'})]
for check in checks:
    try:
        check()
    except ValueError:
        continue
    raise RuntimeError('optimized guard disappeared')
"""
        code = "import sys\nsys.path.append(" + repr(str(Path(candidate_checks.__file__).parent)) + ")\n" + code
        result = subprocess.run([sys.executable, "-O", "-c", code], cwd=Path(audit.__file__).parent,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_tampered_member_bytes(self):
        # Refresh the archive digest to isolate the independent member pin.
        def archive(content):
            output = io.BytesIO()
            with zipfile.ZipFile(output, "w") as z:
                z.writestr("fixture.sch", content)
            return output.getvalue()

        original = b"<schema>  exact text  </schema>"
        tampered = archive(original.replace(b"exact", b"wrong"))
        members = {"fixture.sch": audit.sha(original)}
        pin = {"file": "fixture.zip", "sha256": audit.sha(tampered), "members": members}
        with self.assertRaisesRegex(ValueError, "member digest"):
            audit.read_archive(tampered, pin, members)
        pin["sha256"] = audit.sha(archive(original))
        with self.assertRaisesRegex(ValueError, "archive digest"):
            audit.read_archive(tampered, pin, members)

    def test_candidate_authentication_and_summary_pairing(self):
        with tempfile.TemporaryDirectory(prefix="correspondence-pair-") as temp:
            other = Path(temp) / "candidate"
            shutil.copytree(self.directory, other)
            path = other / "resolved-candidate.json"
            value = audit.read_json(path.read_bytes())
            value["source_package"] = "sha256:" + "0" * 64
            path.write_bytes(candidate_checks.canonical(value))
            manifest_path = other / "resolved-manifest.json"
            manifest = audit.read_json(manifest_path.read_bytes())
            manifest["candidate_sha256"] = audit.sha(path.read_bytes())
            manifest["effective_package_sha256"] = audit.sha(candidate_checks.canonical(value["effective_package"]))
            manifest["files"] = {k: v for k, v in candidate_checks.files(other).items()
                                 if k != "resolved-manifest.json"}
            manifest_path.write_bytes(candidate_checks.canonical(manifest))
            self.assertEqual(manifest["candidate_sha256"], audit.sha(path.read_bytes()))
            self.assertEqual(manifest["effective_package_sha256"], audit.sha(candidate_checks.canonical(
                audit.read_json(path.read_bytes())["effective_package"])))
            self.assertEqual(manifest["files"], {k: v for k, v in candidate_checks.files(other).items()
                                               if k != "resolved-manifest.json"})
            with self.assertRaisesRegex(ValueError, "CANDIDATE-ACCOUNTING"):
                audit.check(self.archives, other)
        other_summary = dict(self.summary, candidate_sha256="0" * 64)
        with self.assertRaisesRegex(ValueError, "candidate/summary pairing"):
            audit.check_accounting(self.summary, other_summary)

    def test_reversed_inputs_generate_identical_summary_bytes(self):
        actual = audit.check(self.archives, self.directory, reverse=True)
        self.assertEqual(audit.canonical(actual), audit.canonical(self.summary))

    def test_duplicate_json_keys_and_dtd_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            audit.read_json(b'{"source":1,"source":2}')
        with self.assertRaisesRegex(ValueError, "DTD/entity"):
            audit.parse_xml(
                b'<!DOCTYPE x [<!ENTITY e "x">]><x>&e;</x>', "fixture.zip", "fixture.xml", "a" * 64, "b" * 64
            )


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--archives", type=Path, required=True)
    args = parser.parse_args()
    CorrespondenceTests.directory = args.candidate
    CorrespondenceTests.archives = args.archives
    unittest.main(argv=["test_correspondence.py"])
