"""Synthetic source-only regression tests; no cache or network required."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import warnings
import zipfile

import inventory as inv


def document(xml, member="root.sch", archive="source.zip"):
    data = xml.encode("utf-8") if isinstance(xml, str) else xml
    doc = inv.parse_xml(data, archive, member, "a" * 64, "b" * 64)
    doc["key"] = archive + "!" + member
    inv.describe(doc)
    return doc


def schematron(body):
    return '<s:schema xmlns:s="' + inv.S + '">' + body + '</s:schema>'


def stylesheet(body):
    return '<x:stylesheet xmlns:x="' + inv.X + '" xmlns:v="' + inv.V + '" version="2.0">' + body + '</x:stylesheet>'


def diagnostic(ident="A", test="truth", guard="actual", kind="failed-assert"):
    return ('<x:if test="' + guard + '"><v:' + kind + ' id="' + ident + '">'
            '<x:attribute name="test">' + test + '</x:attribute>'
            '<v:text>message <x:value-of select="name()"/></v:text></v:' + kind + '></x:if>')


def paired(source_body, executable_body):
    source = document(schematron(source_body))
    target = document(stylesheet(executable_body), "compiled.xsl")
    return source, target, inv.compare(source, target)


class SnapshotTests(unittest.TestCase):
    def archive(self, entries):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as archive:
            for name, data in entries:
                archive.writestr(name, data)
        snapshot = stream.getvalue()
        return snapshot, {"sha256": inv.sha(snapshot), "members": {n: inv.sha(b) for n, b in entries}}

    def test_hash_and_consumption_use_same_snapshot(self):
        snapshot, pin = self.archive([("root.sch", b"<a/>"), ("not-selected.bin", b"opaque")])
        self.assertEqual(inv.read_archive(snapshot, pin, ("root.sch",)), {"root.sch": b"<a/>"})
        with self.assertRaisesRegex(inv.InventoryError, "archive digest"):
            inv.read_archive(snapshot + b"changed", pin, ("root.sch",))

    def test_selected_payload_digest(self):
        snapshot, pin = self.archive([("root.sch", b"<a/>")])
        pin["members"]["root.sch"] = "0" * 64
        with self.assertRaisesRegex(inv.InventoryError, "selected member digest"):
            inv.read_archive(snapshot, pin, ("root.sch",))

    def test_exact_member_set_and_required_selection(self):
        snapshot, pin = self.archive([("root.sch", b"<a/>")])
        with self.assertRaisesRegex(inv.InventoryError, "required source member"):
            inv.read_archive(snapshot, pin, ("missing.sch",))
        pin["members"]["unaccounted.bin"] = "0" * 64
        with self.assertRaisesRegex(inv.InventoryError, "member-set"):
            inv.read_archive(snapshot, pin, ("root.sch",))

    def test_duplicate_members_refused_without_disclosing_names(self):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            snapshot, pin = self.archive([("private-name", b"one"), ("private-name", b"two")])
        with self.assertRaises(inv.InventoryError) as error:
            inv.read_archive(snapshot, pin, ())
        self.assertNotIn("private-name", str(error.exception))

    def test_allowlist_is_exact_and_optional_provenance_separate(self):
        self.assertEqual({a: len(v) for a, v in inv.ALLOWLIST.items()},
                         {"cen.zip": 7, "xr.zip": 2, "config.zip": 7})
        self.assertNotIn(inv.RELEASE_XSL, inv.ALLOWLIST["cen.zip"])
        self.assertEqual(len(set(inv.ALLOWLIST["cen.zip"])), 7)

    def test_lock_anchor_checked_before_archive_reads(self):
        with tempfile.TemporaryDirectory() as temp:
            lock = Path(temp) / "lock.json"
            lock.write_bytes(b"{}")
            with self.assertRaisesRegex(inv.InventoryError, "lock anchor"):
                inv.build(Path(temp), lock_path=lock)

    def test_unsupported_required_root_fails_after_verified_selection(self):
        snapshot, pin = self.archive([("root.sch", b"<unexpected/>")])
        pin["file"] = "source.zip"
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            (base / "downloads").mkdir()
            (base / "downloads/source.zip").write_bytes(snapshot)
            lock = base / "lock.json"
            lock.write_bytes(inv.json_bytes({"artifacts": [pin]}))
            with patch.object(inv, "ALLOWLIST", {"source.zip": ("root.sch",)}), patch.object(inv, "LOCK_SHA256", inv.sha(lock.read_bytes())):
                with self.assertRaisesRegex(inv.InventoryError, "required XML root shape"):
                    inv.build(base, lock_path=lock)


class XmlTests(unittest.TestCase):
    def test_reject_dtd_entities_and_network_system_identifiers(self):
        for xml in ('<!DOCTYPE a><a/>', '<!DOCTYPE a SYSTEM "https://invalid.example/a"><a/>',
                    '<!DOCTYPE a [<!ENTITY x "expanded">]><a>&x;</a>',
                    '<!DOCTYPE a [<!ENTITY % x SYSTEM "file:///private">%x;]><a/>'):
            with self.subTest(xml=xml), self.assertRaises(inv.InventoryError):
                document(xml)

    def test_strict_xml_version_encoding_and_malformed_diagnostics(self):
        for data in (b'<?xml version="1.1"?><a/>', '<?xml version="1.0"?><a/>'.encode("utf-16"),
                     b'<?xml version="1.0" encoding="ASCII"?><a/>', b'<a>\xff</a>', b'<secret-name>'):
            with self.subTest(data=data), self.assertRaises(inv.InventoryError) as error:
                document(data)
            self.assertNotIn("secret-name", str(error.exception))

    def test_declared_latin1_is_preserved_without_reencoding(self):
        data = b'<?xml version="1.0" encoding="ISO-8859-1"?><a>\xe4</a>'
        doc = document(data)
        self.assertEqual(doc["sha256"], inv.sha(data))
        self.assertEqual(doc["xml_declaration"]["encoding"], "ISO-8859-1")
        self.assertEqual(doc["nodes"][0]["content"], [{"text": "ä"}])

    def test_identity_is_digest_bound_stable_and_occurrence_specific(self):
        data = schematron('<s:rule context="same"><s:assert id="A" test="x"/></s:rule>' * 2)
        a, b = document(data), document(data)
        self.assertEqual(a, b)
        rules = inv.indexes(a, inv.S, {"rule"})
        self.assertNotEqual(a["nodes"][rules[0]]["site_id"], a["nodes"][rules[1]]["site_id"])
        self.assertEqual(a["nodes"][rules[1]]["path"][-1], [inv.S, "rule", 2])
        changed = document(data + "\n")
        self.assertNotEqual(a["nodes"][0]["site_id"], changed["nodes"][0]["site_id"])
        moved = document(data, member="other.sch")
        self.assertNotEqual(a["nodes"][0]["site_id"], moved["nodes"][0]["site_id"])

    def test_namespace_identity_scope_prefix_and_lexical_fields(self):
        doc = document('<a:root xmlns:a="urn:s" xmlns:b="urn:s"><b:n a:q="  x  " test=" name() = &apos;b:n&apos; "/><n xmlns="urn:other"/><a:n xmlns:a="urn:inner"/></a:root>')
        self.assertEqual(doc["nodes"][1]["name"], ["urn:s", "n"])
        self.assertEqual(doc["nodes"][1]["lexical_name"], "b:n")
        self.assertEqual(doc["nodes"][1]["lexical_attributes"]["{urn:s}q"], "a:q")
        self.assertEqual(doc["nodes"][1]["attributes"]["{urn:s}q"], "  x  ")
        self.assertEqual(doc["nodes"][1]["attributes"]["test"], " name() = 'b:n' ")
        self.assertEqual(doc["nodes"][3]["namespaces"]["a"], "urn:inner")
        self.assertEqual(doc["nodes"][1]["namespaces"]["a"], "urn:s")

    def test_constructor_order_text_tail_comments_pi_and_locations(self):
        doc = document(schematron('<s:assert id="A" test="  x  ">before<s:name/>between<!--c--><?p t?><s:value-of select="name()"/>after</s:assert>'))
        node = doc["nodes"][1]
        self.assertEqual(node["original_id"], "A")
        self.assertEqual(node["content"], [{"text": "before"}, {"element": 2}, {"text": "between"},
                                           {"comment": "c"}, {"pi": ["p", "t"]}, {"element": 3}, {"text": "after"}])
        self.assertEqual(node["byte_span"], "unavailable")
        self.assertEqual(node["line"], 1)

    def test_duplicate_parameters_retain_order_values_and_distinct_ids(self):
        doc = document(schematron('<s:pattern is-a="abstract"><s:param name="A" value=" first "/><s:param name="A" value="second"/></s:pattern>'))
        params = doc["duplicate_parameters"]
        self.assertEqual(len(params), 1)
        nodes = [doc["nodes"][i] for i in params[0]["occurrences"]]
        self.assertEqual([n["attributes"]["value"] for n in nodes], [" first ", "second"])
        self.assertEqual(len({n["site_id"] for n in nodes}), 2)


class AssociationTests(unittest.TestCase):
    def test_extension_uncertainty_is_scoped_to_direct_attribute_producers(self):
        template = '<x:template match="r"><v:failed-assert id="A">{}</v:failed-assert></x:template>'
        cases = [
            ('', '<e:instruction/>', "literal"),
            ('extension-element-prefixes="e"', '<e:instruction/>', "ambiguous"),
            ('extension-element-prefixes="e"', '<e:instruction><x:fallback><x:attribute name="id">B</x:attribute></x:fallback></e:instruction>', "ambiguous"),
            ('extension-element-prefixes="e"', '<other:literal/>', "literal"),
            ('extension-element-prefixes="e"', '', "literal"),
            ('', '<e:instruction x:extension-element-prefixes="e"/>', "ambiguous"),
            ('', '<literal xmlns="urn:e" x:extension-element-prefixes="#default"/>', "ambiguous"),
            ('', '<e:instruction x:extension-element-prefixes="unbound"/>', "ambiguous"),
            # Prefixes resolve where declared, not after a descendant rebinds them.
            ('extension-element-prefixes="e"', '<e:literal xmlns:e="urn:ordinary"/>', "literal"),
            ('extension-element-prefixes="e"', '<alias:instruction xmlns:alias="urn:e" xmlns:e="urn:ordinary"/>', "ambiguous"),
        ]
        for declaration, child, expected in cases:
            with self.subTest(declaration=declaration, child=child):
                xml = (f'<x:stylesheet xmlns:x="{inv.X}" xmlns:v="{inv.V}" xmlns:e="urn:e" '
                       f'xmlns:other="urn:ordinary" version="2.0" {declaration}>'
                       + template.format(child) + '</x:stylesheet>')
                doc = document(xml, "extension.xsl")
                self.assertEqual(doc["executable"]["emitters"][0]["fields"]["id"]["status"], expected)
        unrelated = '<x:template match="other" extension-element-prefixes="e"><e:instruction xmlns:e="urn:e"/></x:template>'
        doc = document(stylesheet(unrelated + template.format('<foreign xmlns="urn:e"/>')), "unrelated.xsl")
        self.assertEqual(doc["executable"]["emitters"][0]["fields"]["id"]["status"], "literal")

    def test_actual_guard_is_independent_of_metadata_and_template_fields(self):
        source, target, join = paired('<s:rule context=" /r "><s:assert id="A" test="truth" flag="fatal"/></s:rule>',
                                     '<x:template match="/r" mode="M" priority="17">' + diagnostic(guard="not(other)") + '</x:template>')
        association = join["associations"][0]
        self.assertEqual(association["fields"]["test"]["comparison"], "exact")
        self.assertEqual(association["fields"]["context"]["comparison"], "boundary_whitespace_only")
        self.assertEqual(association["fields"]["context"]["source"], " /r ")
        emitter = target["executable"]["emitters"][0]
        self.assertEqual(emitter["control"][-1]["attributes"]["test"], "not(other)")
        self.assertEqual(emitter["template_attributes"]["priority"], "17")
        self.assertEqual(emitter["template_attributes"]["mode"], "M")
        self.assertEqual(target["nodes"][emitter["constructor_node"]]["name"], [inv.V, "failed-assert"])

    def test_otherwise_and_later_when_preserve_preceding_branch_conditions(self):
        doc = document(stylesheet('<x:template match="r"><x:choose><x:when test="first"/><x:when test="second">' + diagnostic() + '</x:when><x:otherwise>' + diagnostic() + '</x:otherwise></x:choose></x:template>'), "a.xsl")
        controls = [e["control"] for e in doc["executable"]["emitters"]]
        self.assertEqual([b["attributes"]["test"] for b in controls[0][-2]["preceding_when_branches"]], ["first"])
        self.assertEqual([b["attributes"]["test"] for b in controls[1][-2]["preceding_when_branches"]], ["first", "second"])

    def test_missing_duplicate_source_and_duplicate_target_ids_are_not_guessed(self):
        for source, target, expected in (
            ('<s:assert test="x"/>', diagnostic(), "missing_id"),
            ('<s:assert id="absent" test="x"/>', diagnostic(), "unmapped"),
            ('<s:assert id="A"/>' * 2, diagnostic(), "ambiguous"),
            ('<s:assert id="A"/>', diagnostic() * 2, "ambiguous"),
        ):
            with self.subTest(expected=expected):
                _, _, join = paired('<s:rule context="r">' + source + '</s:rule>', '<x:template match="r">' + target + '</x:template>')
                self.assertTrue(all(a["status"] == expected for a in join["associations"]))
                self.assertTrue(all("fields" not in a for a in join["associations"]))

    def test_dynamic_constructors_and_ambiguous_attributes_stay_unknown(self):
        cases = [
            '<v:failed-assert id="{dynamic}"><x:attribute name="test" select="computed"/></v:failed-assert>',
            '<v:failed-assert><x:attribute name="id"><x:value-of select="computed"/></x:attribute></v:failed-assert>',
            '<v:failed-assert id="A"><x:attribute name="id">B</x:attribute></v:failed-assert>',
            '<v:failed-assert id="A"><x:if test="yes"><x:attribute name="id">B</x:attribute></x:if></v:failed-assert>',
            '<v:failed-assert id="A"><x:attribute name="{dynamic}">B</x:attribute></v:failed-assert>',
            '<v:failed-assert id="A"><x:attribute name="p:id" namespace="">B</x:attribute></v:failed-assert>',
            '<v:failed-assert id="A" x:use-attribute-sets="attributes"/>',
        ]
        for case in cases:
            with self.subTest(case=case):
                source, target, join = paired('<s:rule context="r"><s:assert id="A" test="x"/></s:rule>', '<x:template match="r">' + case + '</x:template>')
                self.assertEqual(join["associations"][0]["status"], "unmapped")
                self.assertEqual(len(join["unknown_id_emitters"]), 1)
        doc = document(stylesheet('<v:failed-assert><x:attribute name="test" select="expr"/></v:failed-assert>'), "a.xsl")
        self.assertEqual(doc["executable"]["emitters"][0]["fields"]["test"]["status"], "dynamic_unknown")

    def test_static_text_and_xsl_text_only_have_bounded_literal_interpretation(self):
        doc = document(stylesheet('<v:failed-assert><x:attribute name="id"><x:text> A </x:text></x:attribute></v:failed-assert>'), "a.xsl")
        self.assertEqual(doc["executable"]["emitters"][0]["fields"]["id"]["value"], " A ")

    def test_distinct_rules_same_context_and_report_kind(self):
        source, _, join = paired('<s:rule context="r"><s:assert id="A" test="truth"/></s:rule><s:rule context="r"><s:report id="B" test="truth"/></s:rule>',
                                 '<x:template match="r">' + diagnostic() + diagnostic(ident="B", kind="successful-report") + '</x:template>')
        self.assertEqual(len(inv.indexes(source, inv.S, {"rule"})), 2)
        self.assertEqual([a["emitter_kind_agrees"] for a in join["associations"]], [True, True])


class RelationTests(unittest.TestCase):
    def test_order_duplicate_include_missing_target_cycle_and_abstract_candidates(self):
        root = document(schematron('<s:include href="parts/a.sch"/><s:include href="parts/a.sch"/><s:include href="missing.sch"/><s:phase id="p"><s:active pattern="abstract"/></s:phase><s:pattern is-a="abstract"><s:param name="v" value="x"/></s:pattern>'))
        part = document(schematron('<s:include href="../root.sch"/><s:pattern id="abstract" abstract="true"/>'), "parts/a.sch")
        graph = inv.relations({d["key"]: d for d in (root, part)})
        edges = graph["edges"]
        self.assertEqual([e["raw"] for e in edges[:3]], ["parts/a.sch", "parts/a.sch", "missing.sch"])
        self.assertEqual(edges[0]["target"], edges[1]["target"])
        self.assertEqual(edges[2]["status"], "outside_selected_scope")
        self.assertEqual(len(graph["cycle_back_edges"]), 1)
        self.assertEqual(edges[3]["status"], "declared_phase")
        self.assertEqual(edges[4]["status"], "unresolved_static_candidates")
        self.assertEqual(edges[6]["status"], "unexpanded_parameter_occurrence")

    def test_xml_base_and_external_or_escaping_references_never_resolved(self):
        for href in ("https://invalid.example/a", "../../a.sch", "/absolute.sch", "a.sch#part", "%61.sch", "a.sch?query", "a\\b.sch", " a.sch", "http://[invalid"):
            self.assertIsNone(inv.resolve_member("root.sch", href))
        self.assertEqual(inv.resolve_member("root.sch", ""), "root.sch")
        root = document(schematron('<s:include xml:base="other/" href="part.sch"/>'))
        part = document(schematron(""), "part.sch")
        edge = inv.relations({d["key"]: d for d in (root, part)})["edges"][0]
        self.assertEqual(edge["status"], "xml_base_unresolved")
        self.assertIsNone(edge["target"])

    def test_empty_uri_components_remain_unresolved_but_empty_href_is_self(self):
        root = document(schematron(''.join('<s:include href="' + href + '"/>'
                                            for href in ("part.sch?", "part.sch#", "?", "#", ""))))
        part = document(schematron(""), "part.sch")
        edges = inv.relations({d["key"]: d for d in (root, part)})["edges"]
        self.assertEqual([e["status"] for e in edges], ["outside_selected_scope"] * 4 + ["selected"])
        self.assertTrue(all(e["target"] is None for e in edges[:4]))
        self.assertEqual(edges[-1]["target"], root["key"])

    def test_xsd_import_namespace_mismatch_and_locationless_import_visible(self):
        root = document('<schema xmlns="' + inv.D + '"><import namespace="urn:expected" schemaLocation="part.xsd"/><import namespace="urn:external"/></schema>', "root.xsd")
        part = document('<schema xmlns="' + inv.D + '" targetNamespace="urn:actual"/>', "part.xsd")
        graph = inv.relations({d["key"]: d for d in (root, part)})
        self.assertFalse(graph["edges"][0]["namespace_comparison"]["equal"])
        self.assertEqual(graph["edges"][1]["status"], "missing_location")


class OutputTests(unittest.TestCase):
    def test_checked_summary_binds_current_inspector_bytes_without_cache(self):
        summary = json.loads(Path(__file__).with_name("evidence-summary.json").read_bytes())
        self.assertEqual(summary["inspector_sha256"], inv.sha(Path(inv.__file__).read_bytes()))

    def report(self):
        docs = [document(schematron('<s:rule context="r"><s:assert id="A" test="truth"/></s:rule>'), inv.CEN_PRE, "cen.zip"),
                document(schematron('<s:rule context="r"><s:assert id="B" test="truth"/></s:rule>'), inv.XR_ROOT, "xr.zip"),
                document(schematron(""), "schematron/common.sch", "xr.zip"),
                document(schematron('<s:rule><s:assert/><s:assert/></s:rule>'), "schematron/abstract/example.sch", "cen.zip"),
                document(stylesheet('<x:template match="r">' + diagnostic() * 3 + '</x:template>'), inv.CEN_XSL, "config.zip"),
                document(stylesheet('<x:template match="r">' + diagnostic(ident="B") + '</x:template>'), inv.XR_XSL, "config.zip")]
        for doc in docs:
            doc["representation"] = "synthetic"
        return {"format": inv.FORMAT, "artifact_lock_sha256": "a" * 64,
                "inspector_sha256": "b" * 64, "policy_sha256": "c" * 64,
                "claims": dict(inv.CLAIMS), "archives": {},
                "documents": {d["key"]: d for d in docs}, "relations": inv.relations({d["key"]: d for d in docs}),
                "associations": [inv.compare(docs[0], docs[4]), inv.compare(docs[1], docs[5])]}

    def test_summary_counts_each_representation_independently_and_binds_report(self):
        report = self.report()
        result = inv.summary(report)
        self.assertEqual(result["denominators"]["original_declarations"]["assert"], 3)
        self.assertEqual(result["denominators"]["selected_source_sites"]["assert"], 2)
        self.assertEqual(result["denominators"]["configured_emitters"]["failed-assert"], 4)
        self.assertEqual(result["report_sha256"], inv.sha(inv.json_bytes(report)))
        self.assertEqual(result["comparisons"][0]["statuses"], {"ambiguous": 1})
        self.assertEqual(result["claims"], inv.CLAIMS)

    def test_emitter_kind_disagreement_is_in_compact_evidence(self):
        report = self.report()
        association = report["associations"][1]["associations"][0]
        self.assertTrue(all(f["comparison"] == "exact" for f in association["fields"].values()))
        association["emitter_kind_agrees"] = False
        result = inv.summary(report)
        self.assertFalse(result["comparisons"][1]["non_whitespace_differences"][0]["emitter_kind_agrees"])

    def test_verify_compares_exact_summary_before_creating_full_output(self):
        report = self.report()
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            expected, output = base / "summary.json", base / "report.json"
            expected.write_bytes(b"{}")
            stdout, stderr = SimpleNamespace(buffer=io.BytesIO()), io.StringIO()
            args = ["verify", "--cache", str(base / "cache"), "--expected-summary", str(expected), "--output", str(output)]
            with patch.object(inv, "build", return_value=report), patch.object(inv.sys, "stdout", stdout), contextlib.redirect_stderr(stderr):
                self.assertEqual(inv.main(args), 1)
                self.assertFalse(output.exists())
                self.assertEqual(stdout.buffer.getvalue(), b"")
                expected.write_bytes(inv.json_bytes(inv.summary(report)))
                self.assertEqual(inv.main(args), 0)
            self.assertEqual(output.read_bytes(), inv.json_bytes(report))
            self.assertEqual(stdout.buffer.getvalue(), expected.read_bytes())

    def test_fresh_output_and_alias_protection(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            repo, cache = base / "repo", base / "cache"
            repo.mkdir()
            cache.mkdir()
            (repo / ".scratch").mkdir()
            for path in (repo / "tools" / "inventory.json", repo / "frozen.json", cache / "new.json"):
                with self.assertRaisesRegex(inv.InventoryError, "protected output"):
                    inv.write_fresh(path, b"data", cache, repo)
            alias = base / "alias"
            alias.symlink_to(cache, target_is_directory=True)
            with self.assertRaises(inv.InventoryError):
                inv.write_fresh(alias / "new.json", b"data", cache, repo)
            output = repo / ".scratch" / "report.json"
            inv.write_fresh(output, b"first", cache, repo)
            with self.assertRaises(inv.InventoryError):
                inv.write_fresh(output, b"second", cache, repo)
            self.assertEqual(output.read_bytes(), b"first")
            link = base / "file-alias"
            link.symlink_to(output)
            with self.assertRaises(inv.InventoryError):
                inv.write_fresh(link, b"second", cache, repo)
            dangling = base / "dangling"
            dangling.symlink_to(base / "absent")
            with self.assertRaisesRegex(inv.InventoryError, "symlink"):
                inv.write_fresh(dangling, b"data", cache, repo)
            self.assertFalse((base / "absent").exists())

    def test_errors_do_not_disclose_host_paths(self):
        with patch.object(inv, "build", side_effect=OSError("sensitive host path")):
            error = io.StringIO()
            with contextlib.redirect_stderr(error):
                result = inv.main(["build", "--cache", "/irrelevant"])
            self.assertEqual(result, 1)
            self.assertNotIn("sensitive", error.getvalue())

    def test_looping_output_ancestor_is_a_bounded_cli_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            loop = base / "loop"
            loop.symlink_to(loop, target_is_directory=True)
            stdout, stderr = SimpleNamespace(buffer=io.BytesIO()), io.StringIO()
            with patch.object(inv, "build", return_value=self.report()), patch.object(inv.sys, "stdout", stdout), contextlib.redirect_stderr(stderr):
                result = inv.main(["build", "--cache", str(base / "cache"), "--output", str(loop / "report.json")])
            self.assertEqual(result, 1)
            self.assertEqual(stdout.buffer.getvalue(), b"")
            self.assertEqual(stderr.getvalue(), "source-inventory: output path resolution failed\n")
            for cache, root in ((loop, base), (base, loop)):
                with self.assertRaisesRegex(inv.InventoryError, "path resolution failed"):
                    inv.write_fresh(base / "report.json", b"data", cache, root)
            self.assertFalse((base / "report.json").exists())

    def test_claims_remain_unavailable_and_json_is_canonical(self):
        self.assertEqual(inv.CLAIMS["runtime_coverage"], "unavailable")
        self.assertEqual(inv.CLAIMS["native_tested_coverage"], "unavailable")
        self.assertTrue(all(v is False for k, v in inv.CLAIMS.items() if k not in ("runtime_coverage", "native_tested_coverage", "semantic_equivalence")))
        self.assertEqual(inv.canonical({"b": 2, "a": "ä"}), b'{"a":"\xc3\xa4","b":2}')
        self.assertEqual(json.loads(inv.json_bytes(inv.CLAIMS)), inv.CLAIMS)


if __name__ == "__main__":
    unittest.main()
