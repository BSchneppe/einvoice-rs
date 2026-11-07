"""Independent, bounded XPath 2.0 syntax parser; no candidate or Rust imports.

Authority: W3C XPath 2.0 (second edition), Appendix A.1/A.2/A.4.
Admitted forms: ASCII QNames, literals, sequences, groups, calls, conditionals,
binary/unary operators, paths, axes, predicates and atomic QName type operands.
Kind tests take no arguments. Other type/kind tests and binding resolution are
explicitly unsupported. Trees preserve syntax, not evaluation equivalence.
"""

import re

FN = "http://www.w3.org/2005/xpath-functions"
NAME = r"[A-Za-z_][A-Za-z0-9_.-]*"
QNAME = rf"{NAME}(?::{NAME})?"
TOKEN = re.compile(
    r"(?P<number>(?:[0-9]+\.[0-9]*|\.[0-9]+|[0-9]+)(?:[eE][+-]?[0-9]+)?)|"
    rf"(?P<name>{NAME}:\*|\*:{NAME}|{QNAME})|"
    r"(?P<symbol>//|::|\.\.|!=|<=|>=|<<|>>|[/@.$*+\-=<>|(),\[\]?])"
)
# Binding strength increases here; path and predicate productions are separate.
LEVELS = ["or", "and", "= != < <= > >= eq ne lt le gt ge is << >>", "to",
          "+ -", "* div idiv mod", "| union", "intersect except",
          "instance", "treat", "castable", "cast"]
POWER = {op: i + 1 for i, level in enumerate(LEVELS) for op in level.split()}
KINDS = {"node", "text", "comment", "processing-instruction"}
RESERVED = set("attribute document-node element empty-sequence if item schema-attribute "
               "schema-element typeswitch".split())
AXES = set("child descendant attribute self descendant-or-self following-sibling following "
           "parent ancestor preceding-sibling preceding ancestor-or-self".split())


class Unsupported(ValueError):
    """Source syntax or static context outside this check's admitted subset."""


class BindingIdentityRequired(Unsupported):
    """A variable reference or binder prevents whole-facet admission."""


def lex(text):
    """Decode doubled string quotes and discard nested comments, independently."""
    tokens, i = [], 0
    while i < len(text):
        if text[i] in " \t\r\n":
            i += 1
        elif text.startswith("(:", i):
            depth = 1
            i += 2
            while depth:
                if i == len(text):
                    raise Unsupported("unterminated comment")
                if text.startswith("(:", i):
                    depth += 1
                    i += 2
                elif text.startswith(":)", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
        elif text[i] in "'\"":
            start, quote, value = i, text[i], []
            i += 1
            while True:
                if i == len(text):
                    raise Unsupported("unterminated string")
                ch = text[i]
                i += 1
                if ch == quote:
                    if i == len(text) or text[i] != quote:
                        break
                    i += 1
                value.append(ch)
            tokens.append(("string", "".join(value), start))
        else:
            match = TOKEN.match(text, i)
            if not match:
                raise Unsupported(f"unrecognized token at {i}: {text[i:i+20]!r}")
            i = match.end()
            if match.lastgroup == "number" and i < len(text) and re.match(r"[A-Za-z_:]", text[i]):
                raise Unsupported(f"unseparated name after number at {i}")
            tokens.append((match.lastgroup, match[0], match.start()))
    return tokens


class Parser:
    def __init__(self, text):
        self.tokens = lex(text)
        # Inspect tokens, never raw '$' substrings in strings or comments.
        if any(kind == "symbol" and value == "$" for kind, value, _ in self.tokens):
            raise BindingIdentityRequired("binding identity requires independent resolution")
        self.tokens.append(("end", "<end>", len(text)))
        self.i = 0

    def at(self, value):
        kind, actual, _ = self.tokens[self.i]
        return kind not in {"string", "number"} and actual == value

    def take(self, expected=None):
        token = self.tokens[self.i]
        if token[0] == "end" or (expected is not None and not self.at(expected)):
            raise Unsupported(f"expected {expected or 'expression'}, got {token!r}")
        self.i += 1
        return token

    def parse(self):
        tree = self.expr()
        if not self.at("<end>"):
            raise Unsupported(f"unexpected trailing token {self.tokens[self.i]!r}")
        return tree

    def expr(self):
        items = [self.single()]
        while self.at(","):
            self.take(",")
            items.append(self.single())
        return items[0] if len(items) == 1 else ("sequence", tuple(items))

    def single(self):
        if self.at("if") and self.tokens[self.i + 1][1] == "(":
            self.take("if")
            self.take("(")
            condition = self.expr()
            self.take(")")
            self.take("then")
            yes = self.single()
            self.take("else")
            return ("if", condition, yes, self.single())
        return self.binary(1)

    def binary(self, minimum):
        if self.at("+") or self.at("-"):
            left = ("unary", self.take()[1], self.binary(13))
        else:
            left = self.path()
        used = set()
        previous = None
        while (self.tokens[self.i][0] in {"name", "symbol"}
               and self.tokens[self.i][1] in POWER
               and POWER[self.tokens[self.i][1]] >= minimum):
            op = self.take()[1]
            power = POWER[op]
            if power in {3, 4, 9, 10, 11, 12} and power in used:
                raise Unsupported("non-repeating grammar production: " + op)
            # Postfix type operators nest cast < castable < treat < instance; a tighter one after a looser needs parentheses.
            if previous is not None and power > previous:
                raise Unsupported("operator order requires parentheses: " + op)
            used.add(power)
            previous = power
            if op in {"instance", "treat", "castable", "cast"}:
                self.take("of" if op == "instance" else "as")
                kind, typ, _ = self.take()
                if kind != "name" or not re.fullmatch(QNAME, typ):
                    raise Unsupported("expected atomic QName type")
                # SingleType admits only '?'; a following '*' or '+' is an arithmetic operator.
                suffixes = ("?",) if op in {"cast", "castable"} else ("?", "*", "+")
                occurrence = self.take()[1] if any(self.at(s) for s in suffixes) else None
                left = ("type", op, left, typ, occurrence)
            else:
                left = ("binary", op, left, self.binary(power + 1))
        return left

    def path(self):
        if self.at("/") or self.at("//"):
            op = self.take()[1]
            left = ("root",)
            # Leading-lone-slash note: '/' alone unless a relative path can start; '*' always starts one.
            token_kind, token_value, _ = self.tokens[self.i]
            if any(self.at(s) for s in (")", "]", ",", "<end>")) or (
                    token_kind == "symbol" and token_value in POWER and token_value != "*"):
                if op == "//":
                    raise Unsupported("missing relative path")
                return left
            left = ("binary", op, left, self.step())
        else:
            left = self.step()
        while self.at("/") or self.at("//"):
            left = ("binary", self.take()[1], left, self.step())
        return left

    def step(self):
        token = self.take()
        kind, value, _ = token
        if kind == "number":
            numeric = "double" if "e" in value.lower() else "decimal" if "." in value else "integer"
            base = ("number", numeric, value)
        elif kind == "string":
            base = ("string", value)
        elif value == "(":
            base = ("sequence", ()) if self.at(")") else ("group", self.expr())
            self.take(")")
        elif value == ".":
            base = ("context",)
        elif value == "..":
            base = ("step", "parent", ("kind", "node"))
        elif kind == "name" and self.at("(") and value not in KINDS:
            if value in RESERVED or not re.fullmatch(QNAME, value):
                raise Unsupported("reserved function name / unsupported kind test: " + value)
            self.take("(")
            args = []
            if not self.at(")"):
                args.append(self.single())
                while self.at(","):
                    self.take(",")
                    args.append(self.single())
            self.take(")")
            base = ("call", value, tuple(args))
        else:
            axis = "child"
            if value == "@":
                axis, token = "attribute", self.take()
            elif self.at("::"):
                axis = value
                if kind != "name" or axis not in AXES:
                    raise Unsupported("unknown axis " + axis)
                self.take("::")
                token = self.take()
            kind, value, _ = token
            if kind == "name" and value in KINDS and self.at("("):
                self.take("(")
                self.take(")")
                test = ("kind", value)
            elif kind == "symbol" and value == "*":
                test = ("any",)
            elif kind == "name" and value.endswith(":*"):
                test = ("namespace", value[:-2])
            elif kind == "name" and value.startswith("*:"):
                test = ("local", value[2:])
            elif kind == "name" and re.fullmatch(QNAME, value):
                test = ("name", value)
            else:
                raise Unsupported("unsupported step " + repr(token))
            base = ("step", axis, test)
        while self.at("["):
            self.take("[")
            base = ("filter", base, self.expr())
            self.take("]")
        return base


def qualify(tree, namespaces):
    """Expand source QNames using source-owned static context only."""
    def name(qname, function=False):
        prefix, colon, local = qname.partition(":")
        if colon and prefix not in namespaces:
            raise Unsupported("unbound prefix " + prefix)
        uri = namespaces[prefix] if colon else FN if function else ""
        return {"qualified": qname, "namespace": uri, "local": local if colon else qname}

    def walk(node):
        kind = node[0]
        if kind == "name":
            return (kind, name(node[1]))
        if kind == "namespace":
            if node[1] not in namespaces:
                raise Unsupported("unbound prefix " + node[1])
            return (kind, node[1], namespaces[node[1]])
        if kind == "call":
            return (kind, name(node[1], True), tuple(walk(a) for a in node[2]))
        if kind == "type":
            return (kind, node[1], walk(node[2]), name(node[3]), node[4])
        if kind == "sequence":
            return (kind, tuple(walk(a) for a in node[1]))
        return tuple(walk(a) if isinstance(a, tuple) else a for a in node)

    return walk(tree)
