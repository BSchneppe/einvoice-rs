"""Authored source scopes and candidate identity mutations; no evaluation."""

import copy
import unittest

import correspondence as audit
import xpath_bindings as xb
from xpath_structure import Parser, Unsupported
from test_xpath_structure import qname

KEY = "fixture.zip!schema.sch"


def fixture(body):
    data = ('<sch:schema xmlns:sch="' + audit.S + '" xmlns:xsl="' + audit.X +
            '" xmlns:a="urn:v" xmlns:b="urn:v">' + body + '</sch:schema>').encode()
    return audit.parse_xml(data, "fixture.zip", "schema.sch", "a", "l")


def variable(name, identity):
    return {"kind": "variable", "name": qname(name), "binding": identity}


class BindingTests(unittest.TestCase):
    def test_common_global_requires_exact_owner_comment_and_witness(self):
        def run(comment=xb.COMMON_COMMENT, owner="variable-pattern", witness=True, wrong_candidate=False,
                member="common.sch", nested=False, elsewhere=False):
            key = "xr.zip!schematron/" + member
            common = audit.parse_xml(('<sch:pattern xmlns:sch="' + audit.S + '" id="' + owner +
                '">' + ('<sch:rule><!--' + comment + '--></sch:rule>' if elsewhere else '<!--' + comment + '-->') +
                '<sch:let name="x" value="1"/></sch:pattern>').encode(),
                *key.split("!", 1), "a", "l")
            schema = audit.parse_xml(('<sch:schema xmlns:sch="' + audit.S +
                '"><sch:include href="' + member + '"/><sch:pattern><sch:rule context="a">' +
                '<sch:assert test="$x"/></sch:rule></sch:pattern>' +
                '<sch:pattern><sch:let name="x" value="2"/></sch:pattern></sch:schema>').encode(),
                "xr.zip", "schematron/main.sch", "a", "l")
            configured = audit.parse_xml(('<xsl:stylesheet xmlns:xsl="' + audit.X + '">' +
                ('<xsl:template match="/">' if nested else '') +
                ('<xsl:variable name="x" select="1"/>' if witness else '') +
                ('</xsl:template>' if nested else '') +
                '</xsl:stylesheet>').encode(), *xb.CONFIGURED.split("!", 1), "a", "l")
            docs = {key: common, "xr.zip!schematron/main.sch": schema, xb.CONFIGURED: configured}
            scopes = xb.SourceScopes(docs)
            callback = lambda tree, effective=False: scopes.resolve(
                tree, "xr.zip!schematron/main.sch", 4, "test", "$x", effective)
            identity = next(d["id"] for (k, i), d in scopes.declarations.items() if k == key)
            if wrong_candidate:
                identity = next(d["id"] for (k, i), d in scopes.declarations.items() if k != key)
            return audit.classify_expression("$x", variable("x", identity), lambda: {}, {}, callback)
        self.assertEqual(run()[0], "mapped-exact")
        self.assertEqual(run(wrong_candidate=True)[0], "different")
        for kwargs in [{"comment": ""}, {"comment": xb.COMMON_COMMENT + "."},
                       {"witness": False}, {"owner": "other-pattern"}, {"member": "other.sch"},
                       {"nested": True}, {"elsewhere": True}]:
            status, reason, _ = run(**kwargs)
            self.assertEqual(status, "unexamined")
            self.assertEqual(reason, "unresolved source binding" if kwargs.get("owner") or kwargs.get("member")
                             else "missing independent proof for common-global binding")

    def test_duplicate_rule_lets_are_unsupported(self):
        self.prepare('<sch:pattern><sch:rule context="a"><sch:let name="x" value="1"/>'
                     '<sch:let name="x" value="2"/><sch:assert id="use" test="$x"/></sch:rule></sch:pattern>')
        status, reason, evidence = audit.classify_expression(
            "$x", variable("x", "anything"), lambda: {}, {},
            lambda tree, effective=False: self.scopes.resolve(tree, KEY, self.index("use"), "test", "$x", effective))
        self.assertEqual(status, "unsupported")
        self.assertIn("multiply defined host binding", evidence["structure_error"])

    def test_forward_rule_shadowing(self):
        self.prepare('<sch:let id="outer" name="x" value="1"/><sch:pattern><sch:rule context="a">'
                     '<sch:assert id="before" test="$x"/><sch:let id="later" name="x" value="2"/>'
                     '<sch:assert id="after" test="$x"/></sch:rule></sch:pattern>')
        for marker, decl in [("before", "outer"), ("after", "later")]:
            self.assertTrue(self.resolve(marker, "test")["resolved"])
            self.assertEqual(self.refs(marker, "test"), [self.identity(decl)])
            status, _, _ = audit.classify_expression("$x", variable("x", self.identity(decl)), lambda: {}, {},
                lambda tree, effective=False: self.scopes.resolve(tree, KEY, self.index(marker), "test", "$x", effective))
            self.assertEqual(status, "mapped-exact")

    def test_distinct_range_clause_identities(self):
        for operator, end in [("for", "return"), ("some", "satisfies"), ("every", "satisfies")]:
            text = f"{operator} $a in 1, $b in $a {end} $a + $b"
            result = xb.resolve(Parser(text).parse(), {}, {}, "attr", text)
            a = f"attr/expr:{text.index('$a') + 1}:a"
            b = f"attr/expr:{text.index('$b') + 1}:b"
            self.assertEqual([d["id"] for d in result["range_declarations"]], [a, b])
            self.assertTrue(result["resolved"])
            self.assertEqual([r["source_declaration"]["id"] for r in result["references"]], [a, a, b])
            candidate = {"kind": "bind", "operator": operator, "bindings": [
                {"name": qname("a"), "id": a, "value": {"kind": "number", "lexical_kind": "integer", "lexeme": "1"}},
                {"name": qname("b"), "id": b, "value": variable("a", a)}],
                "result": {"kind": "binary", "operator": "+", "left": variable("a", a), "right": variable("b", b)}}
            self.assertEqual(audit.classify_expression(text, candidate, lambda: {}, {},
                lambda tree: xb.resolve(tree, {}, {}, "attr", text))[0], "mapped-exact")

    def prepare(self, body):
        self.doc = fixture(body)
        self.scopes = xb.SourceScopes({KEY: self.doc})
        return self.scopes

    def index(self, marker):
        return next(i for i, n in enumerate(self.doc["nodes"]) if n["attributes"].get("id") == marker)

    def identity(self, marker):
        return self.scopes.declarations[KEY, self.index(marker)]["id"]

    def resolve(self, marker, attr="select", global_patterns=False):
        i = self.index(marker)
        text = self.doc["nodes"][i]["attributes"][attr]
        return self.scopes.resolve(Parser(text).parse(), KEY, i, attr, text, global_patterns)

    def refs(self, marker, attr="select"):
        return [r["source_declaration"]["id"] if r["source_declaration"] else None
                for r in self.resolve(marker, attr)["references"]]

    def test_host_shadowing_and_initializer_exclusion(self):
        self.prepare('''<sch:let id="global" name="x" value="1"/>
          <xsl:function name="f"><xsl:param id="param" name="x"/>
            <xsl:variable id="local" name="x" select="$x"/>
            <xsl:choose><xsl:when test="true()">
              <xsl:variable id="nested" name="x" select="$x"/>
              <xsl:sequence id="inside" select="$x"/>
            </xsl:when><xsl:otherwise><xsl:sequence id="outside" select="$x"/></xsl:otherwise></xsl:choose>
          </xsl:function><xsl:function name="g"><xsl:sequence id="other" select="$x"/></xsl:function>''')
        for use, declaration in [("local", "param"), ("nested", "local"), ("inside", "nested"),
                                 ("outside", "local"), ("other", "global")]:
            self.assertEqual(self.refs(use), [self.identity(declaration)])

    def test_forward_locals_and_content_boundaries(self):
        self.prepare('''<xsl:function name="f"><xsl:sequence id="forward" select="$x"/>
          <xsl:variable name="x" id="x"><xsl:variable name="y" select="1"/>
            <xsl:sequence id="self" select="$x"/></xsl:variable>
          <xsl:sequence id="after" select="$y"/></xsl:function>''')
        for marker in ["forward", "self", "after"]:
            self.assertFalse(self.resolve(marker)["resolved"])

    def test_schema_pattern_rule_visibility(self):
        self.prepare('''<sch:let name="g" id="g" value="$later"/><sch:let name="later" id="later" value="1"/>
          <sch:pattern><sch:let name="p" id="p" value="1"/>
            <sch:rule id="context" context="$r"><sch:let id="r" name="r" value="$p"/>
              <sch:let id="self" name="z" value="$z"/><sch:assert id="use" test="$g + $p + $r"/>
            </sch:rule></sch:pattern>
          <sch:pattern><sch:rule context="a"><sch:assert id="cross" test="$p"/></sch:rule></sch:pattern>''')
        self.assertEqual(self.refs("g", "value"), [self.identity("later")])
        self.assertEqual(self.refs("use", "test"), [self.identity(x) for x in ["g", "p", "r"]])
        self.assertEqual(self.refs("r", "value"), [self.identity("p")])
        for marker, attr in [("context", "context"), ("self", "value"), ("cross", "test")]:
            self.assertFalse(self.resolve(marker, attr)["resolved"])
        self.assertFalse(self.resolve("cross", "test", True)["resolved"])

    def test_expanded_names_duplicates_and_unknown_prefix(self):
        self.prepare('''<sch:let name="a:x" id="decl" value="1"/>
          <xsl:function name="f"><xsl:sequence id="use" select="$b:x"/></xsl:function>''')
        self.assertEqual(self.refs("use"), [self.identity("decl")])
        for body in ['<sch:let name="x" value="1"/><sch:let name="x" value="2"/>',
                     '<xsl:function name="f"><xsl:param name="x"/><xsl:param name="x"/>']:
            self.prepare(body + '<xsl:sequence select="1"/>' + ('</xsl:function>' if 'function' in body else ''))
            with self.assertRaisesRegex(Unsupported, "multiply defined"):
                self.scopes.environment(KEY, len(self.doc["nodes"]) - 1, "test")
        with self.assertRaisesRegex(Unsupported, "unbound variable prefix"):
            xb.resolve(Parser("$u:x").parse(), {}, {}, "attr", "$u:x")

    def test_include_association_and_fail_closed_edges(self):
        doc = fixture('<sch:include href="common.sch"/><sch:pattern><sch:rule context="a"/></sch:pattern>')
        common = audit.parse_xml(('<sch:pattern xmlns:sch="' + audit.S +
                                  '"><sch:let name="p" value="1"/></sch:pattern>').encode(),
                                 "fixture.zip", "common.sch", "a", "l")
        scopes = xb.SourceScopes({KEY: doc, "fixture.zip!common.sch": common})
        self.assertEqual(scopes.owner("fixture.zip!common.sch"), KEY)
        self.assertEqual(scopes.environment(KEY, 3, "test"), {})
        self.assertNotIn(("", "p"), scopes.environment(KEY, 3, "test", True))
        for href in ["missing.sch", "https://example.com/a", "schema.sch"]:
            bad = fixture('<sch:include href="' + href + '"/>')
            with self.assertRaises(Unsupported):
                xb.SourceScopes({KEY: bad}).members(KEY)
        repeated = fixture('<sch:include href="common.sch"/><sch:include href="common.sch"/>')
        with self.assertRaisesRegex(Unsupported, "repeated include"):
            xb.SourceScopes({KEY: repeated, "fixture.zip!common.sch": common}).members(KEY)

    def test_multiple_ranges_shadowing_capture_and_restoration(self):
        env = {("", "x"): {"id": "outer", "name": qname("x")}}
        for operator, end in [("for", "return"), ("some", "satisfies"), ("every", "satisfies")]:
            text = f"{operator} $x in $x, $x in $x {end} $x"
            result = xb.resolve(Parser(text).parse(), env, {}, "attr", text)
            ids = [d["id"] for d in result["range_declarations"]]
            self.assertEqual([r["source_declaration"]["id"] for r in result["references"]], ["outer", *ids])
        text = "(for $x in 1 return (for $x in $x return $x)), $x"
        result = xb.resolve(Parser(text).parse(), env, {}, "attr", text)
        self.assertEqual([r["source_declaration"]["id"] for r in result["references"]],
                         [d["id"] for d in result["range_declarations"]] + ["outer"])
        for text in ["for $x in $x return $x", "(for $x in 1 return $x), $x"]:
            self.assertFalse(xb.resolve(Parser(text).parse(), {}, {}, "attr", text)["resolved"])
        text = "for (: é :) $x in 1 return $x"
        self.assertEqual(xb.resolve(Parser(text).parse(), {}, {}, "attr", text)["range_declarations"][0]["id"],
                         "attr/expr:14:x")

    def test_candidate_status_mutants_at_specific_facet(self):
        self.prepare('''<sch:let name="x" id="global" value="1"/>
          <xsl:function name="f"><xsl:param name="x" id="param"/>
            <xsl:sequence id="use" select="$x"/>
          </xsl:function><xsl:function name="g"><xsl:param name="x" id="other"/></xsl:function>''')
        i = self.index("use")
        callback = lambda tree: self.scopes.resolve(tree, KEY, i, "select", "$x")
        correct = variable("x", self.identity("param"))
        for label, identity in [("wrong declaration", "wrong"), ("same name other scope", self.identity("other")),
                                ("shadowing swapped", self.identity("global"))]:
            with self.subTest(facet="expression AST:select", mutant=label):
                self.assertEqual(audit.classify_expression("$x", correct, lambda: {}, {}, callback)[0], "mapped-exact")
                self.assertEqual(audit.classify_expression("$x", variable("x", identity), lambda: {}, {}, callback)[0], "different")
        text = "for $x in $x return $x"
        binder = "attr/expr:5:x"
        env = {("", "x"): {"id": "outer", "name": qname("x")}}
        callback = lambda tree: xb.resolve(tree, env, {}, "attr", text)
        correct = {"kind": "bind", "operator": "for", "bindings": [
            {"name": qname("x"), "id": binder, "value": variable("x", "outer")}],
            "result": variable("x", binder)}
        capture, declaration_only, changed = (copy.deepcopy(correct) for _ in range(3))
        capture["bindings"][0]["value"]["binding"] = binder
        declaration_only["bindings"][0]["id"] = binder + "/wrong"
        changed["bindings"][0]["id"] = binder + "/wrong"
        changed["result"]["binding"] = binder + "/wrong"
        for mutant in [capture, declaration_only, changed]:
            with self.subTest(facet="expression AST:value", mutant=mutant):
                self.assertEqual(audit.classify_expression(text, correct, lambda: {}, {}, callback)[0], "mapped-exact")
                self.assertEqual(audit.classify_expression(text, mutant, lambda: {}, {}, callback)[0], "different")

    def test_cross_pattern_status_and_unresolved_fail_closed(self):
        self.prepare('''<sch:pattern><sch:let name="x" id="x" value="1"/></sch:pattern>
          <sch:pattern><sch:rule context="a"><sch:assert id="use" test="$x"/></sch:rule></sch:pattern>''')
        i = self.index("use")
        callback = lambda tree, global_patterns=False: self.scopes.resolve(
            tree, KEY, i, "test", "$x", global_patterns)
        candidate = variable("x", self.identity("x"))
        status, reason, evidence = audit.classify_expression("$x", candidate, lambda: {}, {}, callback)
        self.assertEqual(status, "unexamined")
        self.assertEqual(reason, "unresolved source binding")
        self.assertFalse(evidence["resolved"])
        self.assertFalse(evidence["effective_common_global"]["resolved"])
        candidate["binding"] = "wrong"
        self.assertEqual(audit.classify_expression("$x", candidate, lambda: {}, {}, callback)[0], "unexamined")
        self.assertEqual(audit.classify_expression("$missing", variable("missing", "wrong"), lambda: {}, {}, callback)[0],
                         "unexamined")


if __name__ == "__main__":
    unittest.main()
