"""Explicit opt-in audit of the exact pinned SOURCE allowlist, never corpus data."""
import os
from pathlib import Path
import unittest

import inventory as inv


@unittest.skipUnless(os.environ.get("BSC10_SOURCE_CACHE"), "explicit source cache required")
class RealSourceAudit(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cache = Path(os.environ["BSC10_SOURCE_CACHE"])
        cls.report = inv.build(cls.cache)

    def test_checked_evidence_and_full_report_byte_determinism(self):
        expected = Path(__file__).with_name("evidence-summary.json").read_bytes()
        self.assertEqual(inv.json_bytes(inv.summary(self.report)), expected)
        second = inv.build(self.cache)
        self.assertEqual(inv.json_bytes(self.report), inv.json_bytes(second))

    def test_separate_denominators_and_unavailable_execution_claims(self):
        summary = inv.summary(self.report)
        for view in ("original_declarations", "selected_source_sites"):
            self.assertEqual(summary["denominators"][view], {"rule": 156, "assert": 880, "report": 0})
        self.assertEqual(summary["denominators"]["configured_emitters"],
                         {"active-pattern": 10, "failed-assert": 880, "fired-rule": 156, "suppressed-rule": 156})
        self.assertEqual(self.report["claims"], inv.CLAIMS)
        self.assertEqual(len(self.report["documents"]), 16)

    def test_all_configured_associations_retained_including_differences(self):
        cen, xr = self.report["associations"]
        self.assertEqual(len(cen["associations"]), 806)
        self.assertEqual(len(xr["associations"]), 74)
        for join in (cen, xr):
            self.assertTrue(all(a["status"] == "unique_raw_id" for a in join["associations"]))
            self.assertEqual(join["unassociated_emitters"], [])
            self.assertEqual(join["unknown_id_emitters"], [])
        summary = inv.summary(self.report)
        self.assertEqual(summary["comparisons"][0]["fields"]["test"],
                         {"exact": 801, "boundary_whitespace_only": 4, "different": 1})
        self.assertEqual(summary["comparisons"][0]["fields"]["context"],
                         {"exact": 708, "boundary_whitespace_only": 98})
        self.assertTrue(all(a["fields"][field]["comparison"] == "exact"
                            for a in xr["associations"] for field in ("test", "context", "flag", "role")))

    def test_material_source_metadata_and_actual_guard_and_duplicate_occurrences(self):
        join = self.report["associations"][0]
        association = next(a for a in join["associations"] if a["original_id"] == "CII-SR-282")
        self.assertEqual(association["fields"]["test"], {
            "source": "not(ram:SellerTaxRepresentativeTradeParty/ram:ID)",
            "configured": "not(ram:BuyerTaxRepresentativeTradeParty)", "comparison": "different"})
        target = self.report["documents"][join["configured"]]
        emitter = next(e for e in target["executable"]["emitters"] if e["node"] in association["candidate_emitters"])
        self.assertEqual(emitter["control"][-1]["attributes"]["test"], "not(not(ram:BuyerTaxRepresentativeTradeParty))")
        binding = self.report["documents"]["cen.zip!schematron/CII/EN16931-CII-syntax.sch"]
        occurrences = next(p["occurrences"] for p in binding["duplicate_parameters"] if p["name"] == "CII-SR-282")
        self.assertEqual([binding["nodes"][i]["attributes"]["value"] for i in occurrences],
                         ["not(ram:BuyerTaxRepresentativeTradeParty)", "not(ram:SellerTaxRepresentativeTradeParty/ram:ID)"])

    def test_source_closure_and_import_namespace_targets(self):
        graph = self.report["relations"]
        self.assertEqual(graph["cycle_back_edges"], [])
        imports = [e for e in graph["edges"] if "namespace_comparison" in e]
        self.assertEqual(len(imports), 6)
        self.assertTrue(all(e["namespace_comparison"]["equal"] for e in imports))
        includes = [e for e in graph["edges"] if e["kind"] == "include"]
        self.assertEqual(len(includes), 6)
        self.assertTrue(all(e["status"] == "selected" for e in includes))
        bindings = self.report["documents"]["cen.zip!schematron/CII/EN16931-CII-syntax.sch"]
        self.assertEqual(bindings["xml_declaration"]["encoding"], "ISO-8859-1")

    def test_optional_release_is_additive_provenance_and_all_differences_retained(self):
        extended = inv.build(self.cache, release=True)
        self.assertEqual(len(extended["documents"]), 17)
        self.assertEqual(extended["associations"][:2], self.report["associations"])
        release_join = extended["associations"][2]
        assertion = next(a for a in release_join["associations"] if a["original_id"] == "CII-SR-282")
        self.assertEqual(assertion["fields"]["test"]["comparison"], "exact")
        comparisons = inv.summary(extended)["comparisons"][2]
        self.assertEqual(comparisons["fields"]["test"], {"exact": 792, "different": 13, "unknown": 1})
        self.assertEqual(len(comparisons["non_whitespace_differences"]), 14)


if __name__ == "__main__":
    unittest.main()
