"""Authored grammar fixtures and direct structural status regressions."""

import unittest

import correspondence as audit
from xpath_structure import AXES, BindingIdentityRequired, FN, Parser, Unsupported, qualify
from xpath_candidate import adapt


def step(name, axis="child"):
    return ("step", axis, ("name", name))


def number(lexeme, kind="integer"):
    return ("number", kind, lexeme)


def binary(op, left, right):
    return ("binary", op, left, right)


def qname(name, namespace=""):
    return {"qualified": name, "namespace": namespace, "local": name.split(":")[-1]}


def ir_step(name="a", axis="child"):
    return {"kind": "step", "axis": axis, "test": {"kind": "name", "name": qname(name)}}


def ir_number(lexeme, kind="integer"):
    return {"kind": "number", "lexical_kind": kind, "lexeme": lexeme}


def ir_binary(op, left, right):
    return {"kind": "binary", "operator": op, "left": left, "right": right}


# Each candidate fixture is authored explicitly, not constructed by the parser.
# Numeric witnesses are variable-free: the pinned 0.02 row contains a variable.
def mutant_fixtures():
    a, b, c = ir_step("a"), ir_step("b"), ir_step("c")
    return [
        ("reassociation", "a - b - c", ir_binary("-", ir_binary("-", a, b), c),
         ir_binary("-", a, ir_binary("-", b, c))),
        ("moved_group", "(a + b)", {"kind": "group", "operand": ir_binary("+", a, b)},
         ir_binary("+", {"kind": "group", "operand": a}, b)),
        ("moved_predicate", "a/b[1]",
         ir_binary("/", a, {"kind": "filter", "base": b, "predicate": ir_number("1")}),
         {"kind": "filter", "base": ir_binary("/", a, b), "predicate": ir_number("1")}),
        ("swapped_axis", "a", a, ir_step("a", "descendant")),
        ("wrong_arity", "f(1,2)",
         {"kind": "call", "name": qname("f", FN), "arguments": [ir_number("1"), ir_number("2")]},
         {"kind": "call", "name": qname("f", FN), "arguments": [ir_number("1")]}),
        ("numeric_lexeme", "0.02", ir_number("0.02", "decimal"), ir_number(".02", "decimal")),
        ("numeric_kind", "0.02", ir_number("0.02", "decimal"), ir_number("0.02", "double")),
    ]


class GrammarTests(unittest.TestCase):
    def test_authored_constructs(self):
        a, b, c = step("a"), step("b"), step("c")
        n1, n2, n3 = number("1"), number("2"), number("3")
        fixtures = [
            ("a - b - c", binary("-", binary("-", a, b), c)),
            ("a or b and c", binary("or", a, binary("and", b, c))),
            ("1 + 2 * 3", binary("+", n1, binary("*", n2, n3))),
            ("1 to 2 + 3", binary("to", n1, binary("+", n2, n3))),
            ("a | b intersect c", binary("|", a, binary("intersect", b, c))),
            ("-1 + +2", binary("+", ("unary", "-", n1), ("unary", "+", n2))),
            ("--a/b", ("unary", "-", ("unary", "-", binary("/", a, b)))),
            ("a cast as xs:integer", ("type", "cast", a, "xs:integer", None)),
            ("a castable as xs:integer?", ("type", "castable", a, "xs:integer", "?")),
            ("a instance of xs:string*", ("type", "instance", a, "xs:string", "*")),
            ("a treat as xs:string+", ("type", "treat", a, "xs:string", "+")),
            ("1 = 1 to 2", binary("=", n1, binary("to", n1, n2))),
            ("a cast as xs:integer + 1", binary("+", ("type", "cast", a, "xs:integer", None), n1)),
            ("a cast as xs:integer * 2", binary("*", ("type", "cast", a, "xs:integer", None), n2)),
            ("a cast as xs:integer castable as xs:string",
             ("type", "castable", ("type", "cast", a, "xs:integer", None), "xs:string", None)),
            ("(a castable as xs:string) cast as xs:integer",
             ("type", "cast", ("group", ("type", "castable", a, "xs:string", None)), "xs:integer", None)),
            ("/ = /", binary("=", ("root",), ("root",))),
            ("/ + 2", binary("+", ("root",), n2)),
            ("/", ("root",)),
            ("//a/b", binary("/", binary("//", ("root",), a), b)),
            ("a//b", binary("//", a, b)),
            ("/a", binary("/", ("root",), a)),
            ("a/.", binary("/", a, ("context",))),
            ("../@x", binary("/", ("step", "parent", ("kind", "node")), step("x", "attribute"))),
            ("a/* * 2", binary("*", binary("/", a, ("step", "child", ("any",))), n2)),
            ("ram:*", ("step", "child", ("namespace", "ram"))),
            ("*:x", ("step", "child", ("local", "x"))),
            ("a[b[c]]", ("filter", a, ("filter", b, c))),
            ("a[1][2]", ("filter", ("filter", a, n1), n2)),
            ("a/b[1]", binary("/", a, ("filter", b, n1))),
            ("(a/b)[1]", ("filter", ("group", binary("/", a, b)), n1)),
            ("((a))", ("group", ("group", a))),
            ("()", ("sequence", ())),
            ("1,2,3", ("sequence", (n1, n2, n3))),
            ("if(a) then f((1,2),3) else 0.02",
             ("if", a, ("call", "f", (("group", ("sequence", (n1, n2))), n3)),
              number("0.02", "decimal"))),
            ("if((a)) then f() else g(a)",
             ("if", ("group", a), ("call", "f", ()), ("call", "g", (a,)))),
            ("a/f(b)", binary("/", a, ("call", "f", (b,)))),
            ("and/or", binary("/", step("and"), step("or"))),
            ("a-b", step("a-b")),
            ("a (: outer (: nested :) :) / b", binary("/", a, b)),
            ("'a''b'", ("string", "a'b")),
            ('"a""b"', ("string", 'a"b')),
            ("'$x (: text :)'", ("string", "$x (: text :)")),
            ("a (: $x :) ", a),
            ("1e-2", number("1e-2", "double")),
            ("1.E+2", number("1.E+2", "double")),
            (".02", number(".02", "decimal")),
            ("2.", number("2.", "decimal")),
        ]
        for text, expected in fixtures:
            with self.subTest(text=text):
                self.assertEqual(Parser(text).parse(), expected)
        for axis in sorted(AXES):
            self.assertEqual(Parser(axis + "::a").parse(), step("a", axis))
        for kind in ["node", "text", "comment", "processing-instruction"]:
            self.assertEqual(Parser(kind + "()").parse(), ("step", "child", ("kind", kind)))

    def test_operator_spellings_and_left_association(self):
        a, b, c = step("a"), step("b"), step("c")
        repeating = "or and + - * div idiv mod | union intersect except / //".split()
        single = "= != < <= > >= eq ne lt le gt ge is << >> to".split()
        for op in repeating + single:
            with self.subTest(op=op):
                self.assertEqual(Parser(f"a {op} b").parse(), binary(op, a, b))
                if op in repeating:
                    self.assertEqual(Parser(f"a {op} b {op} c").parse(), binary(op, binary(op, a, b), c))
                else:
                    with self.assertRaises(Unsupported):
                        Parser(f"a {op} b {op} c").parse()

    def test_negative_subset_boundaries(self):
        for text in ["", "a[", "a[]", "a/", "//", "(: unclosed", "'unclosed", "1e",
                     "1div 2", "a = b != c", "a cast as xs:integer*", "a cast as 1",
                     "a cast as xs:integer cast as xs:string",
                     "a castable as xs:string cast as xs:integer", "a instance of xs:string cast as xs:string",
                     "a cast as xs:integer+", "a instance of item()",
                     "a treat as element(a)", "element(a)", "processing-instruction('x')",
                     "unknown::a", "namespace::a", "a/@1", "a/::b", "f(,)", "f(1,)",
                     "if(a) then b", "if() then a else b", "a ! b", "a => f()", "a?b",
                     "a \"or\" b", "a \"/\" b", "@'x'", "ram:*(1)", "\u03bb"]:
            with self.subTest(text=text), self.assertRaises(Unsupported):
                Parser(text).parse()
        for text in ["$x", "for $x in a return $x", "some $x in a satisfies $x",
                     "every $x in a satisfies $x"]:
            with self.subTest(text=text), self.assertRaisesRegex(BindingIdentityRequired, "binding identity"):
                Parser(text).parse()

    def test_source_namespace_context(self):
        ns = {"ram": "urn:ram", "u": "urn:helper", "xs": "http://www.w3.org/2001/XMLSchema", "": "urn:xml"}
        self.assertEqual(qualify(Parser("ram:a").parse(), ns),
                         ("step", "child", ("name", qname("ram:a", "urn:ram"))))
        self.assertEqual(qualify(Parser("a").parse(), ns), ("step", "child", ("name", qname("a"))))
        self.assertEqual(qualify(Parser("ram:*").parse(), ns), ("step", "child", ("namespace", "ram", "urn:ram")))
        self.assertEqual(qualify(Parser("f()").parse(), ns), ("call", qname("f", FN), ()))
        self.assertEqual(qualify(Parser("u:f()").parse(), ns), ("call", qname("u:f", "urn:helper"), ()))
        self.assertEqual(qualify(Parser("a cast as xs:string").parse(), ns),
                         ("type", "cast", ("step", "child", ("name", qname("a"))),
                          qname("xs:string", ns["xs"]), None))
        for text in ["p:a", "p:*", "p:f()", "a cast as p:t"]:
            with self.subTest(text=text), self.assertRaisesRegex(Unsupported, "unbound prefix"):
                qualify(Parser(text).parse(), ns)

    def test_adapter_boundaries(self):
        for candidate in [{"kind": "unknown"}, {"kind": "binary"}, None]:
            with self.subTest(candidate=candidate), self.assertRaises(Unsupported):
                adapt(candidate)


class StructuralStatusTests(unittest.TestCase):
    def test_structural_mutants(self):
        for label, text, candidate, mutant in mutant_fixtures():
            with self.subTest(mutation=label, facet="expression AST:value"):
                cache = {}
                status, _, evidence = audit.classify_expression(text, candidate, lambda: {}, cache)
                self.assertEqual(status, "mapped-exact")
                self.assertEqual(evidence["source_tree"], evidence["candidate_tree"])
                status, _, evidence = audit.classify_expression(text, mutant, lambda: {}, cache)
                self.assertEqual(status, "different")
                self.assertNotEqual(evidence["source_tree"], evidence["candidate_tree"])
                self.assertEqual(len(cache), 1)

    def test_order_values_and_test_kinds(self):
        for text, candidate, mutant in [
            ("1,2", {"kind": "sequence", "items": [ir_number("1"), ir_number("2")]},
             {"kind": "sequence", "items": [ir_number("2"), ir_number("1")]}),
            ("' padded '", {"kind": "string", "value": " padded "}, {"kind": "string", "value": "padded"}),
            ("a[1][2]", {"kind": "filter", "base": {"kind": "filter", "base": ir_step(),
                         "predicate": ir_number("1")}, "predicate": ir_number("2")},
             {"kind": "filter", "base": {"kind": "filter", "base": ir_step(),
              "predicate": ir_number("2")}, "predicate": ir_number("1")}),
            ("a castable as xs:integer?",
             {"kind": "type_op", "operator": "castable", "operand": ir_step(),
              "sequence_type": {"name": qname("xs:integer", "http://www.w3.org/2001/XMLSchema"), "occurrence": "?"}},
             {"kind": "type_op", "operator": "castable", "operand": ir_step(),
              "sequence_type": {"name": qname("xs:integer", "http://www.w3.org/2001/XMLSchema"), "occurrence": None}}),
            ("text()", {"kind": "step", "axis": "child", "test": {"kind": "kind", "name": "text"}},
             {"kind": "step", "axis": "child", "test": {"kind": "name", "name": qname("text")}}),
        ]:
            with self.subTest(text=text):
                ns = lambda: {"xs": "http://www.w3.org/2001/XMLSchema"}
                self.assertEqual(audit.classify_expression(text, candidate, ns, {})[0], "mapped-exact")
                self.assertEqual(audit.classify_expression(text, mutant, ns, {})[0], "different")

    def test_binding_and_parse_failures(self):
        for text in ["$x", "for $x in a return $x"]:
            status, reason, _ = audit.classify_expression(text, {}, lambda: self.fail("binding context resolved"), {})
            self.assertEqual(status, "unexamined")
            self.assertEqual(reason, "binding identity requires independent resolution")
        for text in ["a[", "p:a"]:
            status, _, evidence = audit.classify_expression(text, ir_step(), lambda: {}, {})
            self.assertEqual(status, "unsupported")
            self.assertTrue(evidence["structure_error"])
        status, _, _ = audit.classify_expression(
            "a", {"kind": "variable", "name": qname("x"), "binding": "wrong"}, lambda: {}, {})
        self.assertEqual(status, "different")


if __name__ == "__main__":
    unittest.main()
