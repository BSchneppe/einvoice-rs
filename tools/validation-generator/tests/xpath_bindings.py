"""Source-only lexical binding identities for the bounded Schematron XPath audit.

XPath 2.0 3.1.2/3.7/3.9; XSLT 2.0 9.7; ISO 19757-3 parent scopes.
COMMON_GLOBAL admits only independently witnessed common variable-pattern lets.
Inputs are authenticated Expat documents. No candidate fields determine scope.
"""

import posixpath
import re

from xpath_structure import FN, QNAME, Unsupported, qualify

S = "http://purl.oclc.org/dsdl/schematron"
X = "http://www.w3.org/1999/XSL/Transform"
COMMON = "xr.zip!schematron/common.sch"
CONFIGURED = "config.zip!resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl"
COMMON_COMMENT = " This pattern solely serves for declaring global variables (in XSLT speak) "
COMMON_COMMENT_SHA256 = "ce5344d9e2a8c9f52de4ce69b1f5fb2ffb9db5661771ddc6996b5c1a4b3eced8"


def qname(raw, namespaces):
    if not re.fullmatch(QNAME, raw):
        raise Unsupported("unsupported binding QName " + raw)
    prefix, sep, local = raw.partition(":")
    if sep and prefix not in namespaces:
        raise Unsupported("unbound variable prefix " + prefix)
    return {"qualified": raw, "namespace": namespaces[prefix] if sep else "",
            "local": local if sep else raw}


def expanded(name):
    return name["namespace"], name["local"]


def contains_bindings(tree):
    return isinstance(tree, tuple) and bool(tree) and (tree[0] in ("variable", "bind") or
                                      any(contains_bindings(x) for x in tree if isinstance(x, tuple)))


class SourceScopes:
    def __init__(self, docs):
        self.proof_docs = docs
        self.docs = {k: d for k, d in docs.items() if k.endswith(".sch")}
        self.closures = {}
        self.owners = {}
        self.declarations = {}
        for key, doc in self.docs.items():
            for i, node in enumerate(doc["nodes"]):
                if node["name"] not in ([S, "let"], [X, "param"], [X, "variable"]):
                    continue
                self.declarations[key, i] = {
                    "source_site_id": node["site_id"], "resource": key.replace("!", "/", 1),
                    "node_index": i, "element_path": node["path"], "raw_span": node["raw_span"],
                    "raw_sha256": node["raw_sha256"], "kind": node["name"],
                    "name": qname(node["attributes"]["name"], self.namespaces(key, i)),
                    "id": key.replace("!", "/", 1) + "@" + str(node["raw_span"][0]),
                }

    def common_global(self, key, i):
        if key != COMMON:
            return None
        doc = self.docs[key]
        node = doc["nodes"][i]
        parent = doc["nodes"][node["parent"]]
        if parent["name"] != [S, "pattern"] or parent["attributes"].get("id") != "variable-pattern":
            return None
        comments = [c for c in doc.get("comments", [])
                    if parent["raw_span"][0] < c["raw_span"][0] < parent["raw_span"][1]
                    and not any(doc["nodes"][j]["raw_span"][0] <= c["raw_span"][0] < doc["nodes"][j]["raw_span"][1]
                                for j in parent["children"])
                    and c["text"] == COMMON_COMMENT and c["raw_sha256"] == COMMON_COMMENT_SHA256]
        witnesses = [n for n in self.proof_docs.get(CONFIGURED, {}).get("nodes", [])
                     if n["name"] == [X, "variable"] and n["parent"] == 0
                     and expanded(qname(n["attributes"]["name"], n["namespaces"])) ==
                         expanded(self.declarations[key, i]["name"])]
        if len(comments) != 1 or len(witnesses) != 1:
            return None
        witness = witnesses[0]
        return {"rule": "COMMON_GLOBAL", "comment": comments[0],
                "configured_variable": {"archive": "config.zip", "member": CONFIGURED.split("!", 1)[1],
                                        "raw_span": witness["raw_span"], "raw_sha256": witness["raw_sha256"]}}

    def namespaces(self, key, i):
        doc = self.docs[key]
        ns = {"xs": "http://www.w3.org/2001/XMLSchema", "fn": FN,
              **doc["nodes"][i]["namespaces"]}
        for node in doc["nodes"]:
            if "xpath-default-namespace" in node["attributes"]:
                raise Unsupported("xpath-default-namespace")
            if node["name"] == [S, "ns"]:
                ns[node["attributes"]["prefix"]] = node["attributes"]["uri"]
        return ns

    def members(self, key, active=()):
        if key in active:
            raise Unsupported("include cycle")
        if key in self.closures:
            return self.closures[key]
        result = [key]
        archive, member = key.split("!", 1)
        for node in self.docs[key]["nodes"]:
            if node["name"] != [S, "include"]:
                continue
            href = node["attributes"]["href"]
            if ":" in href or href.startswith("/") or "#" in href:
                raise Unsupported("unadmitted include")
            target = archive + "!" + posixpath.normpath(posixpath.join(posixpath.dirname(member), href))
            if target not in self.docs:
                raise Unsupported("missing included member " + target)
            result.extend(self.members(target, active + (key,)))
        if len(result) != len(set(result)):
            raise Unsupported("repeated include instance needs occurrence identity")
        self.closures[key] = result
        return result

    def owner(self, key):
        if key not in self.owners:
            if self.docs[key]["nodes"][0]["name"] == [S, "schema"]:
                self.owners[key] = key
            else:
                owners = [k for k, d in self.docs.items() if d["nodes"][0]["name"] == [S, "schema"]
                          and key in self.members(k)]
                if len(owners) != 1:
                    raise Unsupported("non-unique source schema owner " + key)
                self.owners[key] = owners[0]
        return self.owners[key]

    def environment(self, key, i, attr, global_patterns=False):
        doc = self.docs[key]
        chain, current = [], i
        while current is not None:
            chain.insert(0, current)
            current = doc["nodes"][current]["parent"]
        members = self.members(self.owner(key))
        env = {}

        def add(decl, scope):
            name = expanded(decl["name"])
            if name in scope:
                raise Unsupported("multiply defined host binding " + repr(name))
            scope[name] = decl

        for (dk, di), decl in self.declarations.items():
            if dk not in members:
                continue
            dn = self.docs[dk]["nodes"][di]
            parent = self.docs[dk]["nodes"][dn["parent"]]
            if dn["name"] == [S, "let"] and (parent["name"] == [S, "schema"] or
                    (global_patterns and self.common_global(dk, di))):
                if (dk, di) != (key, i):
                    proof = self.common_global(dk, di) if global_patterns else None
                    add({**decl, "effective_scope_proof": proof} if proof else decl, env)
        pattern = {}
        for ancestor in chain:
            if doc["nodes"][ancestor]["name"] == [S, "pattern"]:
                for j in doc["nodes"][ancestor]["children"]:
                    if doc["nodes"][j]["name"] == [S, "let"] and j != i:
                        add(self.declarations[key, j], pattern)
        env.update(pattern)
        if attr != "context":
            for ancestor in chain:
                if doc["nodes"][ancestor]["name"] == [S, "rule"]:
                    rule = {}
                    for j in doc["nodes"][ancestor]["children"]:
                        if j >= i:
                            break
                        if doc["nodes"][j]["name"] == [S, "let"]:
                            add(self.declarations[key, j], rule)
                    env.update(rule)
        # Only function sequence constructors are admitted; caller/rule locals
        # cannot enter a function. Preceding siblings exclude each initializer.
        for current in chain:
            node = doc["nodes"][current]
            if node["name"] == [X, "function"]:
                params = {}
                for j in node["children"]:
                    if doc["nodes"][j]["name"] == [X, "param"]:
                        add(self.declarations[key, j], params)
            if node["parent"] is None:
                continue
            for sibling in doc["nodes"][node["parent"]]["children"]:
                if sibling == current:
                    break
                if doc["nodes"][sibling]["name"] in ([X, "param"], [X, "variable"]):
                    decl = self.declarations[key, sibling]
                    env[expanded(decl["name"])] = decl
        return env

    def resolve(self, tree, key, i, attr, text, global_patterns=False):
        node = self.docs[key]["nodes"][i]
        prefix = key.replace("!", "/", 1) + "@" + str(node["raw_attributes"][attr]["span"][0])
        result = resolve(tree, self.environment(key, i, attr, global_patterns),
                         self.namespaces(key, i), prefix, text)
        if global_patterns and not result["resolved"] and COMMON in self.members(self.owner(key)):
            eligible = set()
            for (dk, di), declaration in self.declarations.items():
                if dk != COMMON or declaration["kind"] != [S, "let"]:
                    continue
                parent_index = self.docs[dk]["nodes"][di]["parent"]
                if parent_index is None:
                    continue
                parent = self.docs[dk]["nodes"][parent_index]
                if parent["name"] == [S, "pattern"] and parent["attributes"].get("id") == "variable-pattern":
                    eligible.add(expanded(declaration["name"]))
            for ref in result["references"]:
                if ref["source_declaration"] is None and expanded(ref["name"]) in eligible:
                    ref["missing_common_global_proof"] = True
        return result


def resolve(tree, environment, namespaces, prefix, text):
    references, declarations = [], []

    def walk(node, scope):
        kind = node[0]
        if kind == "variable":
            name = qname(node[1], namespaces)
            decl = scope.get(expanded(name))
            references.append({"name": name, "decoded_name_char_offset": node[2],
                               "source_declaration": decl})
            return (kind, name, decl["id"] if decl else None)
        if kind == "bind":
            inner, bindings = dict(scope), []
            for raw, offset, value in node[2]:
                initial = walk(value, inner)
                name = qname(raw, namespaces)
                byte_offset = len(text[:offset].encode("utf-8"))
                identity = prefix + "/expr:" + str(byte_offset) + ":" + raw
                decl = {"kind": "xpath:" + node[1], "expression_attribute": prefix,
                        "decoded_name_char_offset": offset, "decoded_name_utf8_offset": byte_offset,
                        "name": name, "id": identity}
                declarations.append(decl)
                inner[expanded(name)] = decl
                bindings.append((name, identity, initial))
            return (kind, node[1], tuple(bindings), walk(node[3], inner))
        if kind == "call":
            return (kind, qualify((kind, node[1], ()), namespaces)[1],
                    tuple(walk(a, scope) for a in node[2]))
        if kind == "type":
            return (kind, node[1], walk(node[2], scope), qname(node[3], namespaces), node[4])
        if kind == "sequence":
            return (kind, tuple(walk(a, scope) for a in node[1]))
        if kind in ("name", "namespace"):
            return qualify(node, namespaces)
        return tuple(walk(a, scope) if isinstance(a, tuple) else a for a in node)

    source = walk(tree, environment)
    return {"source_tree": source, "references": references, "range_declarations": declarations,
            "resolved": all(r["source_declaration"] is not None for r in references)}


def reference_paths(tree, path=()):
    if isinstance(tree, tuple):
        if tree and tree[0] == "variable":
            yield path, tree
        for i, child in enumerate(tree):
            yield from reference_paths(child, path + (i,))


def compare(result, candidate):
    actual = dict(reference_paths(candidate))
    for ref, (path, expected) in zip(result["references"], reference_paths(result["source_tree"]), strict=True):
        found = actual.get(path)
        ref.update(tree_path=list(path), candidate_binding_id=found[2] if found else None,
                   candidate_variable=found, matching=expected == found)
    result["matching"] = result["resolved"] and result["source_tree"] == candidate
    return result
