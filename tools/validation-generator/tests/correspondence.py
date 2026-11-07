#!/usr/bin/env python3
"""Independent pinned-source preservation accounting, never runtime admission.

Each source occurrence/facet is an obligation. Exact lexical/constructor matches
are separate from pending AST, dispatch, helper and policy effects. Only named
constructor projections are allowed; competing CII-SR-282 evidence stays intact.
"""

import argparse
import collections
import hashlib
import io
import json
import posixpath
import re
import stat
import zipfile
from pathlib import Path, PurePosixPath
from xml.parsers import expat

import candidate as independent_candidate
import inventory as old
import occurrences as lex

HERE = Path(__file__).resolve().parents[1]
S = "http://purl.oclc.org/dsdl/schematron"
X = "http://www.w3.org/1999/XSL/Transform"
V = "http://purl.oclc.org/dsdl/svrl"
C = "http://www.xoev.de/de/validator/framework/1/scenarios"
XML = "http://www.w3.org/XML/1998/namespace"
ARCHIVES = {"cen.zip", "xr.zip", "config.zip", "validator.jar", "cen-source.zip"}
# Original model/codelist occurrences are obligations even though the candidate
# does not retain their original-instance provenance. Never derive scope from IR.
ORIGINALS = {
    "cen.zip/schematron/EN16931-CII-validation.sch",
    "cen.zip/schematron/preprocessed/EN16931-CII-validation-preprocessed.sch",
    "cen.zip/schematron/abstract/EN16931-CII-model.sch",
    "cen.zip/schematron/abstract/EN16931-CII-syntax.sch",
    "cen.zip/schematron/CII/EN16931-CII-model.sch",
    "cen.zip/schematron/CII/EN16931-CII-syntax.sch",
    "cen.zip/schematron/codelist/EN16931-CII-codes.sch",
    "cen.zip/xslt/EN16931-CII-validation.xslt",
    "xr.zip/schematron/cii/XRechnung-CII-validation.sch",
    "xr.zip/schematron/common.sch",
    "config.zip/scenarios.xml",
    "config.zip/resources/cii/16b/xsl/EN16931-CII-validation.xsl",
    "config.zip/resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl",
    "config.zip/resources/default-report.xsl",
    "config.zip/resources/xrechnung-report.xsl",
}


def require(condition, reason="correspondence invariant failed"):
    if not condition:
        raise ValueError(reason)


def check_occurrences(text, tree):
    # The shared check uses assert for equality. Keep this boundary active under -O.
    expected = lex.source_occurrences(text)
    actual = lex.tree_occurrences(tree)
    require(expected == actual,
            f"XPath occurrence mismatch at None: missing={dict(expected - actual)!r}; extra={dict(actual - expected)!r}")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def enc(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def canonical(value):
    """Sorted JSON + LF; each non-exact occurrence is one unshortened JSON row."""
    fields = []
    for key, item in sorted(value.items()):
        if key == "non_exact":
            body = "[\n" + ",\n".join("  " + enc(row).decode() for row in item) + "\n]"
        else:
            body = json.dumps(item, ensure_ascii=False, sort_keys=True, indent=2, allow_nan=False)
        fields.append("  " + json.dumps(key) + ": " + body.replace("\n", "\n  "))
    return ("{\n" + ",\n".join(fields) + "\n}\n").encode()


def unique_pairs(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, "duplicate JSON key: " + key)
        value[key] = item
    return value


def read_json(data):
    return json.loads(
        data,
        object_pairs_hook=unique_pairs,
        parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite JSON")),
    )


def authenticate_candidate(directory):
    expected = read_json((HERE / "CANDIDATE-ACCOUNTING.json").read_bytes())
    paths = sorted(directory.rglob("*"))
    require(not any(p.is_symlink() for p in paths), "candidate symlink")
    snapshot = {str(p.relative_to(directory)): p.read_bytes() for p in paths if p.is_file()}
    actual = {p: {"sha256": sha(b), "size": len(b)} for p, b in snapshot.items()}
    require(actual == expected["files"], "candidate differs from reviewed CANDIDATE-ACCOUNTING.json")
    value = read_json(snapshot["resolved-candidate.json"])
    require(sha(snapshot["resolved-candidate.json"]) == expected["candidate_sha256"], "candidate digest")
    return value, expected


def read_archive(data, pin, member_pins):
    """Authenticate one immutable byte snapshot, including additive JAR pins."""
    require(len(data) <= 128 * 1024 * 1024, "archive byte budget")
    require(sha(data) == pin["sha256"], "archive digest: " + pin["file"])
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        require(len(entries) <= 20000, "archive entry budget")
        require(sum(i.file_size for i in entries) <= 512 * 1024 * 1024, "expanded archive budget")
        regular = [i for i in entries if not i.is_dir()]
        names = [i.filename for i in regular]
        require(len(names) == len(set(names)), "duplicate ZIP member")
        if "members" in pin:
            require(set(names) == set(member_pins), "exact ZIP member inventory")
        else:
            require(set(member_pins) <= set(names), "missing JAR closure member")
        for entry in entries:
            path = PurePosixPath(entry.filename)
            require(not path.is_absolute() and ".." not in path.parts and "\\" not in entry.filename, "unsafe ZIP path")
            require(not stat.S_ISLNK(entry.external_attr >> 16), "ZIP symlink")
            require(entry.file_size <= 64 * 1024 * 1024, "member byte budget")
        result = {}
        for entry in regular:
            if entry.filename not in member_pins:
                continue
            content = archive.read(entry)
            require(len(content) == entry.file_size, "member size")
            require(sha(content) == member_pins[entry.filename], "member digest: " + entry.filename)
            result[entry.filename] = content
        return result


def load_inputs(archives, directory, reverse=False):
    value, expected = authenticate_candidate(directory)
    lock_bytes = (HERE.parent / "oracle/artifacts.lock.json").read_bytes()
    lock = read_json(lock_bytes)
    closure_bytes = (HERE / "jar-closure.lock.json").read_bytes()
    closure = read_json(closure_bytes)
    pins = {p["file"]: p for p in lock["artifacts"] if p["file"] in ARCHIVES}
    require(set(pins) == ARCHIVES, "archive pin inventory")
    require(closure["parent_archive"] == "validator.jar", "closure parent archive")
    require(closure["parent_sha256"] == pins["validator.jar"]["sha256"], "closure parent digest")
    loaded, identities = {}, {}
    for name in sorted(pins, reverse=reverse):
        pin = pins[name]
        member_pins = pin.get("members", closure["members"] if name == "validator.jar" else {})
        require(member_pins, "no member authentication")
        members = read_archive((archives / name).read_bytes(), pin, member_pins)
        identities[name] = {
            "sha256": pin["sha256"],
            "selected_members": {m: {"sha256": sha(b), "size": len(b)} for m, b in members.items()},
        }
        loaded.update((name + "/" + m, b) for m, b in members.items())
    selected = ORIGINALS | {r for r in value["effective_package"]["resources"] if not r.startswith("standalone/")}
    payload = {r.replace("/", "!", 1): loaded[r] for r in sorted(selected)}
    ident = {
        "archives": identities,
        "artifact_lock_sha256": sha(lock_bytes),
        "jar_closure_lock_sha256": sha(closure_bytes),
        "candidate_accounting_sha256": sha((HERE / "CANDIDATE-ACCOUNTING.json").read_bytes()),
        "candidate_sha256": expected["candidate_sha256"],
    }
    docs = {
        k: parse_xml(b, *k.split("!", 1), identities[k.split("!", 1)[0]]["sha256"], sha(lock_bytes))
        for k, b in payload.items()
        if k.endswith((".sch", ".xsl", ".xslt", ".xsd", ".xml"))
    }
    for key in [
        "config.zip!resources/cii/16b/xsl/EN16931-CII-validation.xsl",
        "config.zip!resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl",
    ]:
        docs[key]["executable"] = executable_view(docs[key])
    return value, docs, payload, ident


def parse_xml(data, archive, member, archive_digest, lock_digest):
    """Build ordered XML constructors and raw byte ranges with stdlib Expat.

    DTDs/entities are refused. XML decoding is Expat's, independently of Rust's
    roxmltree; literal text and attribute spelling are never normalized by us.
    """
    require(b"\x00" not in data, "unsupported XML byte encoding")
    parser = expat.ParserCreate(namespace_separator="}")
    nodes, stack, pending_ns = [], [], []
    member_digest = sha(data)

    def expanded(name):
        uri, separator, local = name.rpartition("}")
        return [uri, local] if separator else ["", name]

    def content(token):
        if stack:
            target = nodes[stack[-1]]["content"]
            if "text" in token and target and "text" in target[-1]:
                target[-1]["text"] += token["text"]
            else:
                target.append(token)

    def start(name, attrs):
        require(len(nodes) < 250000, "XML node budget")
        tag = expanded(name)
        parent = nodes[stack[-1]] if stack else None
        ordinal = 1 + sum(nodes[i]["name"] == tag for i in parent["children"]) if parent else 1
        path = (parent["path"] if parent else []) + [[*tag, ordinal]]
        scope = dict(parent["namespaces"]) if parent else {"xml": XML}
        scope.update(pending_ns)
        pending_ns.clear()
        attributes = {}
        for key, value in attrs.items():
            uri, local = expanded(key)
            attributes["{" + uri + "}" + local if uri else local] = value
        offset = parser.CurrentByteIndex
        opening_end = old.opening_end(data, offset) + 1
        raw_attributes = {}
        for match in re.finditer(rb'([^\s=<>/]+)\s*=\s*(?:"[^"]*"|\x27[^\x27]*\x27)', data[offset:opening_end]):
            begin, end = offset + match.start(), offset + match.end()
            raw_attributes[match[1].decode("ascii")] = {"span": [begin, end], "sha256": sha(data[begin:end])}
        node = {
            "site_id": sha(
                enc(
                    ["einvoice-correspondence-source-site/1", lock_digest, archive_digest, member, member_digest, path]
                )
            ),
            "name": tag,
            "attributes": attributes,
            "namespaces": scope,
            "path": path,
            "parent": stack[-1] if stack else None,
            "children": [],
            "content": [],
            "line": parser.CurrentLineNumber,
            "raw_span": [offset, opening_end],
            "raw_attributes": raw_attributes,
        }
        index = len(nodes)
        if parent:
            parent["children"].append(index)
        content({"element": index})
        nodes.append(node)
        stack.append(index)

    def end(_):
        node = nodes[stack.pop()]
        begin, opening_end = node["raw_span"]
        if not data[begin:opening_end].rstrip().endswith(b"/>"):
            node["raw_span"][1] = data.index(b">", parser.CurrentByteIndex) + 1
        node["raw_sha256"] = sha(data[begin : node["raw_span"][1]])

    def refuse(*_):
        raise ValueError("unsupported XML DTD/entity declaration")

    parser.StartNamespaceDeclHandler = lambda prefix, uri: pending_ns.append((prefix or "", uri or ""))
    parser.StartElementHandler = start
    parser.EndElementHandler = end
    parser.CharacterDataHandler = lambda value: content({"text": value})
    parser.CommentHandler = lambda value: content({"comment": value})
    parser.ProcessingInstructionHandler = lambda target, value: content({"pi": [target, value]})
    parser.StartDoctypeDeclHandler = refuse
    parser.EntityDeclHandler = refuse
    parser.ExternalEntityRefHandler = refuse
    parser.SkippedEntityHandler = refuse
    parser.SetParamEntityParsing(expat.XML_PARAM_ENTITY_PARSING_NEVER)
    parser.Parse(data, True)
    require(nodes, "empty selected XML")
    return {"nodes": nodes}


def ancestors(doc, index):
    parent = doc["nodes"][index]["parent"]
    result = []
    while parent is not None:
        result.append(parent)
        parent = doc["nodes"][parent]["parent"]
    return list(reversed(result))


def indexes(doc, ns, names):
    return [i for i, node in enumerate(doc["nodes"]) if node["name"][0] == ns and node["name"][1] in names]


def static_text(doc, index):
    """Only direct text/xsl:text is understood; arbitrary constructors stay unknown."""
    if "select" in doc["nodes"][index]["attributes"]:
        return None
    pieces = []
    for token in doc["nodes"][index]["content"]:
        if "text" in token:
            pieces.append(token["text"])
        elif "element" in token:
            child = doc["nodes"][token["element"]]
            if child["name"] != [X, "text"] or child["children"]:
                return None
            pieces.extend(t["text"] for t in child["content"] if "text" in t)
        else:
            return None
    return "".join(pieces)


def extension_instruction(doc, index):
    """Conservative scope check only; never execute an extension or its fallback."""
    node = doc["nodes"][index]
    for ancestor in ancestors(doc, index) + [index]:
        owner = doc["nodes"][ancestor]
        attribute = "extension-element-prefixes" if owner["name"][0] == X else "{" + X + "}extension-element-prefixes"
        for prefix in owner["attributes"].get(attribute, "").split():
            namespace = owner["namespaces"].get("" if prefix == "#default" else prefix)
            # An unbound declaration is unsupported, not evidence of a literal child.
            if not namespace or node["name"][0] == namespace:
                return True
    return False


def literal_field(doc, index, field):
    node = doc["nodes"][index]
    constructors = [
        i
        for i in node["children"]
        if doc["nodes"][i]["name"] == [X, "attribute"]
        and doc["nodes"][i]["attributes"].get("name") == field
        and doc["nodes"][i]["attributes"].get("namespace", "") == ""
    ]
    # Unknown dynamic attribute names can overwrite any literal attribute.
    dynamic_names = [
        i
        for i in node["children"]
        if doc["nodes"][i]["name"] == [X, "attribute"]
        and (
            "{" in doc["nodes"][i]["attributes"].get("name", "")
            or "name" not in doc["nodes"][i]["attributes"]
            or ":" in doc["nodes"][i]["attributes"].get("name", "")
            or "{" in doc["nodes"][i]["attributes"].get("namespace", "")
        )
    ]
    indirect = [
        i
        for i in node["children"]
        if doc["nodes"][i]["name"][0] == X and doc["nodes"][i]["name"][1] not in ("attribute", "text", "value-of")
    ]
    value = node["attributes"].get(field)
    if (
        dynamic_names
        or indirect
        or any(extension_instruction(doc, i) for i in node["children"])
        or "{" + X + "}use-attribute-sets" in node["attributes"]
        or len(constructors) > 1
        or (constructors and value is not None)
    ):
        return {"status": "ambiguous", "value": None, "constructors": constructors}
    if constructors:
        value = static_text(doc, constructors[0])
        status = "literal" if value is not None else "dynamic_unknown"
    elif value is None:
        status = "absent"
    else:
        status = "dynamic_unknown" if "{" in value or "}" in value else "literal"
        if status != "literal":
            value = None
    return {"status": status, "value": value, "constructors": constructors}


def executable_view(doc):
    nodes = doc["nodes"]
    records = []
    for i in indexes(
        doc, V, {"failed-assert", "successful-report", "fired-rule", "suppressed-rule", "active-pattern"}
    ):
        chain = ancestors(doc, i)
        templates = [a for a in chain if nodes[a]["name"] == [X, "template"]]
        template = templates[-1] if templates else None
        control = []
        for a in chain:
            if nodes[a]["name"][0] != X:
                continue
            entry = {"node": a, "kind": nodes[a]["name"][1], "attributes": nodes[a]["attributes"]}
            if nodes[a]["name"][1] in ("when", "otherwise"):
                siblings = nodes[nodes[a]["parent"]]["children"] if nodes[a]["parent"] is not None else []
                entry["preceding_when_branches"] = [
                    {"node": s, "attributes": nodes[s]["attributes"]}
                    for s in siblings[: siblings.index(a)]
                    if nodes[s]["name"] == [X, "when"]
                ]
            control.append(entry)
        records.append(
            {
                "node": i,
                "kind": nodes[i]["name"][1],
                "template": template,
                "template_attributes": nodes[template]["attributes"] if template is not None else None,
                "ancestor_chain": chain,
                "control": control,
                "fields": {f: literal_field(doc, i, f) for f in ("id", "test", "flag", "role", "context")},
                "constructor_node": i,
            }
        )
    return {"templates": indexes(doc, X, {"template"}), "emitters": records}


def enumerate_obligations(candidate, docs, payload, ident):
    """Compare source-derived occurrences, preserving every pending facet."""
    package = candidate["effective_package"]
    require(candidate["backend"] == package["backend"] == package["policy"]["execution"] == "Unavailable", "backend state")
    require(candidate["correspondence_audit"] == "PendingFullPredicateContextHelperMessagePolicyCorrespondence", "audit state")

    def resource(k):
        return k.replace("!", "/", 1)

    def node(k, i):
        return docs[k]["nodes"][i]

    def children(k, i, ns=None, local=None):
        return [
            j
            for j in node(k, i)["children"]
            if (ns is None or node(k, j)["name"][0] == ns) and (local is None or node(k, j)["name"][1] == local)
        ]

    def ancestors(k, i):
        out = []
        p = node(k, i)["parent"]
        while p is not None:
            out.append(p)
            p = node(k, p)["parent"]
        return list(reversed(out))

    def text(k, i):
        return "".join(t["text"] for t in node(k, i)["content"] if "text" in t)

    def occurrence(k, i):
        n = node(k, i)
        a, m = k.split("!", 1)
        chain = ancestors(k, i) + [i]
        identity = {}
        for kind in ["pattern", "rule", "assert", "param", "let", "function", "variable", "template"]:
            hits = [j for j in chain if node(k, j)["name"][1] == kind and node(k, j)["name"][0] in [S, X]]
            if hits:
                j = hits[-1]
                kind_ns = node(k, j)["name"][0]
                p = node(k, j)["parent"]
                peers = children(k, p, kind_ns, kind) if p is not None else [j]
                identity[kind] = {
                    "index": peers.index(j),
                    "id": node(k, j)["attributes"].get("id"),
                    "name": node(k, j)["attributes"].get("name"),
                }
        return {
            "archive": a,
            "archive_sha256": ident["archives"][a]["sha256"],
            "member": m,
            "member_sha256": ident["archives"][a]["selected_members"][m]["sha256"],
            "node_index": i,
            "xml_id": n["attributes"].get("id"),
            "site_id": n["site_id"],
            "element_path": n["path"],
            "raw_span": n["raw_span"],
            "raw_sha256": n["raw_sha256"],
            "line": n["line"],
            "occurrence": identity,
        }

    def shape(k, i, strip_space=False):
        n = node(k, i)
        parts = []
        for t in n["content"]:
            if "element" in t:
                parts.append(shape(k, t["element"], strip_space))
            elif "text" in t and (not strip_space or t["text"].strip()):
                parts.append({"text": t["text"]})
        return {"name": n["name"], "attributes": n["attributes"], "content": parts}

    def xmlinstr(k, i):
        n = node(k, i)
        parts = []
        for token in n["content"]:
            if "element" in token:
                parts.append(xmlinstr(k, token["element"]))
            elif "text" in token and token["text"].strip():
                parts.append({"text": token["text"]})
        return {"tag": n["name"][1], "attrs": n["attributes"], "children": parts}

    def xmlhelper(k, i):
        attrs = node(k, i)["attributes"]
        return {
            "name": attrs["name"],
            "as": attrs.get("as"),
            "parameters": [xmlinstr(k, j) for j in children(k, i, X, "param")],
            "body": [xmlinstr(k, j) for j in children(k, i) if node(k, j)["name"] != [X, "param"]],
        }

    def irhelper(h):
        return {
            "name": h["name"]["qualified"],
            "as": old.typename(h["result_type"]),
            "parameters": [old.ir_binding(b, True) for b in h["parameters"]],
            "body": [old.ir_instruction(b) for b in h["body"]],
        }

    def walk(v, p=""):
        yield p, v
        if isinstance(v, dict):
            for k, c in v.items():
                yield from walk(c, p + "/" + k.replace("~", "~0").replace("/", "~1"))
        elif isinstance(v, list):
            for i, c in enumerate(v):
                yield from walk(c, p + "/" + str(i))

    objects = list(walk(candidate))
    bysource = collections.defaultdict(list)
    for p, v in objects:
        if isinstance(v, dict) and isinstance(v.get("source"), dict):
            s = v["source"]
            bysource[(s["resource"], s["xml_byte_start"], s["attribute"])].append((p, v))

    def linked(k, i, attr=None, kind=None):
        n = node(k, i)
        start = n["raw_span"][0] if attr is None else n["raw_attributes"][attr]["span"][0]
        found = bysource[(resource(k), start, attr)]
        return [(p, v) for p, v in found if p.startswith("/effective_package/") and (kind is None or kind(v))]

    rows = []
    excluded_scaffolding = []

    def add(k, i, cat, facet, expected, actual=None, paths=None, status=None, reason=None, extra=None):
        if status is None:
            status = "mapped-exact" if expected == actual else "different"
        r = {
            "obligation_id": sha(enc([k, i, cat, facet]))[:20],
            "category": cat,
            "facet": facet,
            "source": occurrence(k, i),
            "candidate_locations": paths or [],
            "status": status,
            "evidence": {"source": expected, "candidate": actual},
            "reason": reason
            or (
                "exact field/constructor equality; no execution claim"
                if status == "mapped-exact"
                else "exact values differ; no normalization or authority decision"
            ),
        }
        if extra:
            r["evidence"].update(extra)
        rows.append(r)
        return r

    P = "predicate"
    CT = "context/phase-selection"
    H = "helper/variable binding"
    M = "message/diagnostic part"
    PS = "policy/severity override"
    N = "namespace"
    # Primary declarative source accounting. Every expression has a separate lexical and AST facet.
    primary = [
        "cen.zip!schematron/preprocessed/EN16931-CII-validation-preprocessed.sch",
        "xr.zip!schematron/cii/XRechnung-CII-validation.sch",
        "xr.zip!schematron/common.sch",
    ]
    source_binding_names = {
        n["attributes"].get("name")
        for pk in primary
        for n in docs[pk]["nodes"]
        if tuple(n["name"]) in [(S, "let"), (X, "param"), (X, "variable")]
    }
    site_locations = {}
    rule_locations = {}
    for si, s in enumerate(package["schemas"]):
        for pi, p in enumerate(s["patterns"]):
            for ri, r in enumerate(p["rules"]):
                rp = f"/effective_package/schemas/{si}/patterns/{pi}/rules/{ri}"
                rule_locations[r["source"]["resource"], r["source"]["xml_byte_start"]] = (rp, r)
                for ai, a in enumerate(r["sites"]):
                    site_locations[a["source"]["resource"], a["source"]["xml_byte_start"]] = (
                        rp + f"/sites/{ai}",
                        a,
                        rp,
                        r,
                    )

    def msg(k, i):
        out = []
        for t in node(k, i)["content"]:
            if "text" in t:
                out.append({"kind": "text", "text": t["text"]})
            elif "element" in t:
                c = node(k, t["element"])
                local = c["name"][1]
                if c["name"] == [S, "name"]:
                    out.append({"kind": "name"})
                elif c["name"] == [S, "value-of"]:
                    out.append({"kind": "value_of", "text": c["attributes"]["select"]})
                else:
                    out.append({"kind": "unsupported", "constructor": shape(k, t["element"])})
        return out

    def irmsg(parts):
        return [
            {"kind": "text", "text": p["text"]}
            if p["kind"] == "text"
            else {"kind": "name"}
            if p["kind"] == "name"
            else {"kind": "value_of", "text": p["expression"]["text"]}
            for p in parts
        ]

    # Attributes here are compared above/below or in the enclosing constructor.
    # sch:title is descriptive metadata, explicitly excluded from obligations.
    primary_attributes = {
        S: {
            "schema": {"queryBinding", "defaultPhase"},
            "pattern": {"id"},
            "phase": {"id"},
            "active": {"pattern"},
            "include": {"href"},
            "ns": {"prefix", "uri"},
            "rule": {"context", "flag"},
            "assert": {"id", "flag", "test"},
            "let": {"name", "value"},
            "name": set(),
            "value-of": {"select"},
            "title": set(),
        },
        X: {
            "function": {"name", "as"},
            "param": {"name", "as", "select"},
            "variable": {"name", "as", "select"},
            "sequence": {"select"},
            "choose": set(),
            "when": {"test"},
            "otherwise": set(),
        },
    }
    # Reuse existing independent constructor projection, not any compiler parser.
    for k in primary:
        for i, n in enumerate(docs[k]["nodes"]):
            ns, local = n["name"]
            attrs = n["attributes"]
            handled = primary_attributes.get(ns, {}).get(local)
            if handled is None:
                add(k, i, H, "unsupported primary element", shape(k, i),
                    status="unsupported", reason="element has no explicit correspondence handler or exclusion")
            for attr in sorted(attrs.keys() - (handled or set())):
                add(k, i, H, "unsupported primary attribute:" + attr,
                    {"name": attr, "value": attrs[attr], "raw_attributes": n["raw_attributes"]},
                    status="unsupported", reason="attribute has no explicit correspondence handler or exclusion")
            if handled is None:
                continue  # Unknown local names must never enter a known element's handler.
            if ns == S and local == "assert":
                p, a, rp, r = site_locations[resource(k), n["raw_span"][0]]
                expected = msg(k, i)
                actual = irmsg(a["message"])
                for j, part in enumerate(expected):
                    add(
                        k,
                        i,
                        M,
                        "part:" + str(j),
                        part,
                        actual[j] if j < len(actual) else None,
                        [p + "/message/" + str(j)] if j < len(actual) else [p + "/message"],
                        status="unsupported" if part["kind"] == "unsupported" else None,
                    )
                require(any(part["kind"] == "unsupported" for part in expected) or len(expected) == len(actual),
                        "message part count differs")
                add(
                    k,
                    i,
                    PS,
                    "assertion flag and severities",
                    {
                        "id": attrs["id"],
                        "raw_flag": attrs["flag"],
                        "base_severity": {"fatal": "error", "warning": "warning", "information": "information"}[
                            attrs["flag"]
                        ],
                        "effective_severity": next(
                            c["values"] for c in package["policy"]["clauses"] if c["kind"] == "overrides"
                        ).get(
                            attrs["id"],
                            {"fatal": "error", "warning": "warning", "information": "information"}[attrs["flag"]],
                        ),
                    },
                    {x: a[x] for x in ["id", "raw_flag", "base_severity", "effective_severity"]},
                    [p],
                )
            if ns == S and local == "rule":
                p, r = rule_locations[resource(k), n["raw_span"][0]]
                add(
                    k,
                    i,
                    CT,
                    "ordered rule/site membership",
                    {
                        "flag": attrs.get("flag"),
                        "assertion_starts": [node(k, ch)["raw_span"][0] for ch in children(k, i, S, "assert")],
                    },
                    {"flag": r["flag"], "assertion_starts": [a["source"]["xml_byte_start"] for a in r["sites"]]},
                    [p],
                )
            if ns == S and local in ["schema", "pattern", "phase", "active", "include", "ns"]:
                if local == "schema":
                    si = 0 if k.startswith("cen.zip") else 1
                    s = package["schemas"][si]
                    p = f"/effective_package/schemas/{si}"
                    add(
                        k,
                        i,
                        CT,
                        "phase default and selected entry",
                        {"declared_default_phase": attrs.get("defaultPhase"), "selection": "#ALL"},
                        {"declared_default_phase": s["declared_default_phase"], "selection": s["selection"]},
                        [p],
                    )
                    add(
                        k,
                        i,
                        CT,
                        "dispatch contract",
                        {
                            "ordered_patterns": [
                                node(k, j)["attributes"].get("id") or node(k, j)["attributes"].get("href")
                                for j in children(k, i)
                                if node(k, j)["name"] in [[S, "pattern"], [S, "include"]]
                            ]
                        },
                        {"patterns": [p["id"] for p in s["patterns"]], "dispatch": s["dispatch"]},
                        [p + "/dispatch"],
                        status="unexamined",
                        reason="source order and candidate dispatch retained; configured template dispatch/reachability not independently proved",
                    )
                elif local == "pattern":
                    matches = linked(k, i, kind=lambda v: "rules" in v)
                    require(len(matches) == 1, "unique pattern association")
                    p, v = matches[0]
                    add(
                        k,
                        i,
                        CT,
                        "ordered pattern activation/membership",
                        {
                            "id": attrs["id"],
                            "active": True,
                            "rules": [node(k, j)["raw_span"][0] for j in children(k, i, S, "rule")],
                        },
                        {
                            "id": v["id"],
                            "active": v["active"],
                            "rules": [r["source"]["xml_byte_start"] for r in v["rules"]],
                        },
                        [p],
                    )
                elif local == "phase":
                    p, v = linked(k, i, kind=lambda v: "active_patterns" in v)[0]
                    add(
                        k,
                        i,
                        CT,
                        "phase ordered active patterns",
                        {
                            "id": attrs["id"],
                            "active_patterns": [
                                node(k, j)["attributes"]["pattern"] for j in children(k, i, S, "active")
                            ],
                        },
                        {"id": v["id"], "active_patterns": v["active_patterns"]},
                        [p],
                    )
                elif local == "active":
                    pass  # included individually by ordered phase content
                elif local == "include":
                    si = 1
                    to = k.split("!", 1)[0] + "/" + posixpath.normpath(
                        posixpath.join(posixpath.dirname(k.split("!", 1)[1]), attrs["href"])
                    )
                    inc = package["schemas"][si]["includes"][0]
                    add(
                        k,
                        i,
                        CT,
                        "include edge",
                        {"from_start": n["raw_span"][0], "to": to},
                        {"from_start": inc["from"]["xml_byte_start"], "to": inc["to"]},
                        ["/effective_package/schemas/1/includes/0"],
                        extra={"href": attrs["href"]},
                    )
                elif local == "ns":
                    p, v = linked(k, i, kind=lambda v: "prefix" in v)[0]
                    add(k, i, N, "namespace declaration", attrs, {"prefix": v["prefix"], "uri": v["uri"]}, [p])
            if (ns == S and local == "let") or (ns == X and local in ["param", "variable"]):
                p, b = linked(k, i, kind=lambda v: "effective_scope" in v)[0]
                expected = {
                    "id": resource(k) + "@" + str(n["raw_span"][0]),
                    "name": attrs["name"],
                    "as": attrs.get("as"),
                    "value": attrs.get("value", attrs.get("select")),
                    "content": None,
                }
                actual = {
                    "id": b["id"],
                    "name": b["name"]["qualified"],
                    "as": old.typename(b["sequence_type"]),
                    "value": b["value"].get("expression", {}).get("text"),
                    "content": None,
                }
                if local == "variable" and "select" not in attrs:
                    expected["content"] = xmlinstr(k, i)["children"]
                    actual["content"] = old.ir_binding(b)["children"] if b["value"]["kind"] == "content" else None
                add(
                    k,
                    i,
                    H,
                    "binding declaration/initializer",
                    expected,
                    actual,
                    [p],
                    extra={
                        "declared_owner": b["declared_owner"],
                        "effective_scope": b["effective_scope"],
                        "initialization_context": b["initialization_context"],
                    },
                )
            if ns == X and local == "function":
                p, h = linked(k, i, kind=lambda v: "result_type" in v)[0]
                expected = xmlhelper(k, i)
                actual = irhelper(h) if "body" in h else None
                add(
                    k,
                    i,
                    H,
                    "helper ordered signature/constructor tree",
                    expected,
                    actual,
                    [p],
                    status=None if actual is not None else "unexamined",
                    reason=None if actual is not None else "candidate helper has no comparable body",
                    extra={
                        "source_raw_constructor": shape(k, i),
                        "whitespace_policy": "strip whitespace-only XSLT stylesheet text; preserve all significant literal text",
                    },
                )
            for attr in ["context", "test", "value", "select"]:
                if attr not in attrs:
                    continue
                cat = (
                    CT
                    if attr == "context"
                    else P
                    if ns == S and local == "assert"
                    else M
                    if ns == S and local == "value-of"
                    else H
                )
                found = linked(k, i, attr, lambda v: set(v) == {"text", "source", "tree"})
                # The candidate's one resolved test originates in the selected original concrete parameter.
                if not found and local == "assert":
                    sp, a, *_ = site_locations[resource(k), n["raw_span"][0]]
                    found = [(sp + "/test", a["test"])]
                if len(found) != 1:
                    add(
                        k,
                        i,
                        cat,
                        "expression lexical:" + attr,
                        attrs[attr],
                        None,
                        [p for p, v in found],
                        status="ambiguous" if found else "unsupported",
                    )
                    continue
                p, e = found[0]
                add(
                    k,
                    i,
                    cat,
                    "expression lexical:" + attr,
                    attrs[attr],
                    e["text"],
                    [p],
                    extra={"source_attribute": n["raw_attributes"][attr], "candidate_source": e["source"]},
                )
                try:
                    check_occurrences(attrs[attr], e["tree"])
                    lexical = "pass"
                except (AssertionError, KeyError, ValueError) as error:
                    lexical = "fail:" + repr(error)
                # Exact source AST preservation cannot be inferred from the lexical multiset check.
                add(
                    k,
                    i,
                    cat,
                    "expression AST:" + attr,
                    attrs[attr],
                    e["tree"],
                    [p + "/tree"],
                    status="unexamined" if lexical == "pass" else "different",
                    reason="lexical multiplicity check is "
                    + lexical
                    + "; association, precedence, grouping and full binding identity are not independently established",
                    extra={"lexical_check": lexical},
                )
    # Original CEN entry/abstract/concrete obligations: bounded textual parameter projection for discovery only.
    entry = "cen.zip!schematron/EN16931-CII-validation.sch"
    for i, n in enumerate(docs[entry]["nodes"]):
        local = n["name"][1]
        if local == "include":
            dest = "cen.zip/" + posixpath.normpath("schematron/" + n["attributes"]["href"])
            paths = [
                "/effective_package/source_graph/" + str(j)
                for j, e in enumerate(package["source_graph"])
                if e["from"] == resource(entry) and e["to"] == dest
            ]
            add(
                entry,
                i,
                CT,
                "original include occurrence",
                n["attributes"],
                [package["source_graph"][int(p.rsplit("/", 1)[1])] for p in paths],
                paths,
                status="mapped-exact" if paths else "unsupported",
                reason="bounded original include edge retained"
                if paths
                else "original model/codelist include not represented as an original-entry edge; preprocessed resource is distinct evidence",
            )
        elif local in ["phase", "ns"]:
            si = 0
            s = package["schemas"][si]
            if local == "ns":
                a = n["attributes"]
                add(
                    entry,
                    i,
                    N,
                    "original namespace declaration",
                    a,
                    {"prefix": a["prefix"], "uri": s["namespaces"].get(a["prefix"])},
                    ["/effective_package/schemas/0/namespaces"],
                )
            else:
                a = {
                    "id": n["attributes"]["id"],
                    "active_patterns": [
                        node(entry, ch)["attributes"]["pattern"] for ch in children(entry, i, S, "active")
                    ],
                }
                v = [{x: p[x] for x in ["id", "active_patterns"]} for p in s["phases"] if p["id"] == a["id"]]
                add(
                    entry,
                    i,
                    CT,
                    "original phase declaration",
                    a,
                    v[0] if len(v) == 1 else v,
                    ["/effective_package/schemas/0/phases"],
                )

    for kind in ["model", "syntax"]:
        ak = f"cen.zip!schematron/abstract/EN16931-CII-{kind}.sch"
        ck = f"cen.zip!schematron/CII/EN16931-CII-{kind}.sch"
        concrete = node(ck, 0)
        abstract = node(ak, 0)
        params = collections.defaultdict(list)
        for j in children(ck, 0, S, "param"):
            params[node(ck, j)["attributes"]["name"]].append(j)
        ps = [
            (i, p) for i, p in enumerate(package["schemas"][0]["patterns"]) if p["id"] == concrete["attributes"]["id"]
        ]
        require(len(ps) == 1, "unique original pattern")
        pi, pat = ps[0]
        pp = f"/effective_package/schemas/0/patterns/{pi}"
        add(
            ck,
            0,
            CT,
            "original is-a linkage",
            {"is-a": concrete["attributes"]["is-a"], "abstract_id": abstract["attributes"]["id"]},
            {"effective_pattern_id": pat["id"], "effective_pattern_source": pat["source"]},
            [pp],
            status="unexamined",
            reason="unique source abstract/concrete linkage and candidate pattern identity observed; complete original-instance provenance is represented only for the reviewed local resolution",
        )
        arules = children(ak, 0, S, "rule")
        require(len(arules) == len(pat["rules"]), "original rule count")
        uses = collections.defaultdict(list)
        for ri, (ar, cr) in enumerate(zip(arules, pat["rules"])):
            rp = pp + f"/rules/{ri}"
            for j, attr, actual, path in [(ar, "context", cr["context"]["text"], rp + "/context")] + [
                (ai, "test", ca["test"]["text"], rp + f"/sites/{aii}/test")
                for aii, (ai, ca) in enumerate(zip(children(ak, ar, S, "assert"), cr["sites"], strict=True))
            ]:
                original = node(ak, j)["attributes"][attr]
                refs = (
                    [original[1:]]
                    if original.startswith("$") and original[1:] in params
                    else re.findall(r"\$([A-Za-z_][A-Za-z_0-9.-]*)", original)
                )
                alternatives = [original]
                ambiguous = False
                for name in refs:
                    indices = params[name]
                    require(indices, (ak, j, name))
                    ambiguous |= len(indices) > 1
                    alternatives = [
                        a.replace("$" + name, node(ck, pj)["attributes"]["value"])
                        for a in alternatives
                        for pj in indices
                    ]
                    for pj in indices:
                        uses[pj].append(
                            {
                                "source_node": j,
                                "attribute": attr,
                                "candidate_location": path,
                                "candidate_text": actual,
                                "expanded_alternatives": alternatives,
                            }
                        )
                expected = alternatives[0] if len(alternatives) == 1 else alternatives
                status = "ambiguous" if ambiguous else "mapped-exact" if expected == actual else "different"
                add(
                    ak,
                    j,
                    CT if attr == "context" else P,
                    "original parameter substitution:" + attr,
                    {"abstract_text": original, "expanded": expected},
                    actual,
                    [path],
                    status=status,
                    reason="duplicate parameter authority remains a blocker; ordered alternatives retained"
                    if ambiguous
                    else "bounded textual source substitution compared without whitespace normalization",
                    extra={
                        "concrete_resource": resource(ck),
                        "parameter_occurrences": [occurrence(ck, pj) for name in refs for pj in params[name]],
                    },
                )
            for ai, ca in zip(children(ak, ar, S, "assert"), cr["sites"], strict=True):
                aid = cr["sites"].index(ca)
                path = rp + f"/sites/{aid}"
                add(ak, ai, M, "original ordered message", msg(ak, ai), irmsg(ca["message"]), [path + "/message"])
                add(
                    ak,
                    ai,
                    PS,
                    "original assertion flag",
                    node(ak, ai)["attributes"]["flag"],
                    ca["raw_flag"],
                    [path + "/raw_flag"],
                )
        for pj in children(ck, 0, S, "param"):
            n = node(ck, pj)
            u = uses[pj]
            name = n["attributes"]["name"]
            paths = [x["candidate_location"] for x in u]
            # A duplicate is still ambiguous even if one occurrence exactly agrees with a materialized field.
            st = (
                "ambiguous"
                if len(params[name]) > 1
                else "mapped-exact"
                if u
                and all(
                    n["attributes"]["value"] in x["expanded_alternatives"]
                    and x["candidate_text"] in x["expanded_alternatives"]
                    for x in u
                )
                else "unsupported"
                if not u
                else "different"
            )
            add(
                ck,
                pj,
                H,
                "original parameter occurrence",
                n["attributes"],
                u,
                paths,
                status=st,
                reason="duplicate parameter; no first/last-wins inference"
                if st == "ambiguous"
                else "all referenced expansions compared exactly"
                if st == "mapped-exact"
                else "no source reference maps this parameter"
                if not u
                else "expanded parameter and candidate differ",
            )
    # Original codelist is an identity-preserved inlined pattern, verify full ordered rule/assert structure.
    k = "cen.zip!schematron/codelist/EN16931-CII-codes.sch"
    pat = package["schemas"][0]["patterns"][2]
    pp = "/effective_package/schemas/0/patterns/2"
    for ri, (i, r) in enumerate(zip(children(k, 0, S, "rule"), pat["rules"], strict=True)):
        rp = pp + f"/rules/{ri}"
        add(
            k,
            i,
            CT,
            "original codelist rule context",
            node(k, i)["attributes"]["context"],
            r["context"]["text"],
            [rp + "/context"],
        )
        for ai, (j, a) in enumerate(zip(children(k, i, S, "assert"), r["sites"], strict=True)):
            ap = rp + f"/sites/{ai}"
            add(
                k,
                j,
                P,
                "original codelist predicate",
                node(k, j)["attributes"]["test"],
                a["test"]["text"],
                [ap + "/test"],
            )
            add(k, j, M, "original codelist message", msg(k, j), irmsg(a["message"]), [ap + "/message"])
            add(
                k, j, PS, "original codelist flag", node(k, j)["attributes"]["flag"], a["raw_flag"], [ap + "/raw_flag"]
            )
    # Explicit source-derived static defaults and XML helper namespace scopes.
    for si, schema in enumerate(package["schemas"]):
        k = schema["source"]["resource"].replace("/", "!", 1)
        expected = dict(old.DEFAULTS)
        expected.update(
            (n["attributes"]["prefix"], n["attributes"]["uri"]) for n in docs[k]["nodes"] if n["name"] == [S, "ns"]
        )
        add(
            k,
            0,
            N,
            "effective XPath static namespace/default context",
            {"prefixes": expected, "element_default": "", "function_default": old.DEFAULTS["fn"]},
            {
                "prefixes": schema["namespaces"],
                "element_default": schema["xpath_element_default_namespace"],
                "function_default": schema["xpath_function_default_namespace"],
            },
            [f"/effective_package/schemas/{si}/namespaces"],
        )
        for hi, h in enumerate(schema["helpers"]):
            sk = h["source"]["resource"].replace("/", "!", 1)
            j = next(j for j, n in enumerate(docs[sk]["nodes"]) if n["raw_span"][0] == h["source"]["xml_byte_start"])
            bindings = {pre: uri for pre, uri in node(sk, j)["namespaces"].items() if pre in expected}
            add(
                sk,
                j,
                N,
                "helper XML prefix bindings",
                bindings,
                {pre: schema["namespaces"].get(pre) for pre in bindings},
                [f"/effective_package/schemas/{si}/helpers/{hi}"],
                extra={"unprefixed_XPath_node_namespace": "", "built_in_missing_xs_is_static_default": True},
            )

    # Configured XSLT remains comparison evidence. Join is only a candidate association until all facets pass.
    site_by_id = collections.defaultdict(list)
    for p, a, rp, r in site_locations.values():
        site_by_id[a["id"]].append((p, a, rp, r))
    configured_keys = [
        "config.zip!resources/cii/16b/xsl/EN16931-CII-validation.xsl",
        "config.zip!resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl",
    ]
    for k in configured_keys:
        ex = docs[k]["executable"]
        emitters = [e for e in ex["emitters"] if e["kind"] in ["failed-assert", "successful-report"]]
        ids = collections.Counter(e["fields"]["id"]["value"] for e in emitters)
        routing = {}
        for e in emitters:
            i = e["node"]
            idfield = e["fields"]["id"]
            aid = idfield["value"]
            matches = site_by_id.get(aid, [])
            if idfield["status"] != "literal" or ids[aid] != 1 or len(matches) != 1:
                add(
                    k,
                    i,
                    P,
                    "configured association",
                    e["fields"],
                    None,
                    [p for p, *_ in matches],
                    status="ambiguous",
                    reason="no unique literal-ID association; no source authority selected",
                )
                continue
            p, a, rp, r = matches[0]
            template = e["template"]
            routing[template] = (rp, r, e)
            for field, cat, actual, path in [
                ("test", P, a["test"]["text"], p + "/test"),
                ("flag", PS, a["raw_flag"], p + "/raw_flag"),
                ("role", PS, a.get("raw_role"), p),
            ]:
                f = e["fields"][field]
                status = None if f["status"] in ["literal", "absent"] else "ambiguous"
                add(
                    k,
                    i,
                    cat,
                    "configured diagnostic metadata:" + field,
                    f["value"],
                    actual,
                    [path],
                    status=status,
                    extra={
                        "field_status": f["status"],
                        "association": "unique raw ID plus explicit field comparisons; not a verified effective-instance identity",
                    },
                )
            guards = [g for g in e["control"] if g["kind"] in ["if", "when"] and "test" in g["attributes"]]
            if guards:
                guard = guards[-1]
                gi = guard["node"]
                expected = guard["attributes"]["test"]
                actual = "not(" + a["test"]["text"] + ")"
                add(
                    k,
                    gi,
                    P,
                    "configured failure guard",
                    expected,
                    actual,
                    [p + "/test"],
                    extra={
                        "full_control_ancestors": e["control"],
                        "emitter_node": i,
                        "comparison": "exact not(...) wrapper only; no algebraic normalization",
                    },
                )
            else:
                add(
                    k,
                    i,
                    P,
                    "configured failure guard",
                    e["control"],
                    None,
                    [p + "/test"],
                    status="unsupported",
                    reason="no recognized direct failure-guard wrapper",
                )
            texts = children(k, i, V, "text")
            parts = []
            unknown = []

            def message_parts(j, k=k, parts=parts, unknown=unknown):
                nn = node(k, j)
                for t in nn["content"]:
                    if "text" in t:
                        if t["text"].strip():
                            parts.append({"kind": "text", "text": t["text"]})
                    elif "element" in t:
                        ch = t["element"]
                        cn = node(k, ch)
                        if cn["name"] == [X, "text"]:
                            parts.append({"kind": "text", "text": text(k, ch)})
                        elif cn["name"] == [X, "value-of"]:
                            sel = cn["attributes"].get("select")
                            parts.append(
                                {"kind": "name"} if sel in ["name(.)", "name()"] else {"kind": "value_of", "text": sel}
                            )
                        else:
                            unknown.append(shape(k, ch))

            for j in texts:
                message_parts(j)
            add(
                k,
                texts[0] if len(texts) == 1 else i,
                M,
                "configured ordered message constructor",
                parts,
                irmsg(a["message"]),
                [p + "/message"],
                status="unsupported" if unknown or len(texts) != 1 else None,
                extra={
                    "unknown_constructors": unknown,
                    "whitespace_policy": "strip whitespace-only literal stylesheet text; xsl:text always exact",
                    "name_constructor_rule": "sch:name maps to xsl:value-of select=name() or name(.); exact select retained in full_constructor",
                    "full_constructor": shape(k, texts[0]) if len(texts) == 1 else [shape(k, j) for j in texts],
                },
            )
        for ti, (rp, r, e) in sorted(routing.items()):
            n = node(k, ti)
            add(
                k,
                ti,
                CT,
                "configured template match",
                n["attributes"].get("match"),
                r["context"]["text"],
                [rp + "/context"],
                extra={"template_attributes": n["attributes"]},
            )
            add(
                k,
                ti,
                CT,
                "configured mode/priority/suppression dispatch",
                {"template": n["attributes"], "control": e["control"]},
                {"schema_dispatch": package["schemas"][0 if "cii/16b" in k else 1]["dispatch"]},
                [rp],
                status="unexamined",
                reason="candidate has source-order rule dispatch, no explicit configured template/mode/priority/suppression graph; bounded structural dispatch correspondence remains to be audited",
            )
        # Compare actual compiled binding initializers with global/rule declarations; do not infer effects.
        for i, n in enumerate(docs[k]["nodes"]):
            if n["name"][0] != X or n["name"][1] not in ["variable", "param"]:
                continue
            if any(node(k, j)["name"] == [X, "function"] for j in ancestors(k, i)):
                continue
            name = n["attributes"].get("name")
            matches = [
                (p, v)
                for p, v in objects
                if p.startswith("/effective_package/")
                and isinstance(v, dict)
                and v.get("effective_scope") in ["xslt2_schema_global", "rule_local"]
                and v["name"]["qualified"] == name
            ]
            if not matches:
                # Exclusion must not depend on candidate scope: a counterpart anywhere keeps the obligation.
                elsewhere = [
                    p
                    for p, v in objects
                    if p.startswith("/effective_package/")
                    and isinstance(v, dict)
                    and "effective_scope" in v
                    and v["name"]["qualified"] == name
                ]
                if elsewhere or name in source_binding_names:
                    add(
                        k,
                        i,
                        H,
                        "configured binding association",
                        shape(k, i, True),
                        None,
                        elsewhere,
                        status="unsupported",
                        reason="configured binding has a source or candidate counterpart outside the compared global/rule-local scopes",
                    )
                    continue
                excluded_scaffolding.append({"name": name, "source": occurrence(k, i)})
                continue
            if len(matches) != 1:
                add(
                    k,
                    i,
                    H,
                    "configured binding association",
                    shape(k, i, True),
                    None,
                    [p for p, v in matches],
                    status="ambiguous",
                    reason="repeated source binding names require owner/context resolution",
                )
                continue
            bp, b = matches[0]
            expected = {
                "name": name,
                "initializer": n["attributes"].get("select"),
                "declared_type": n["attributes"].get("as"),
            }
            actual = {
                "name": b["name"]["qualified"],
                "initializer": b["value"].get("expression", {}).get("text"),
                "declared_type": old.typename(b["sequence_type"]),
            }
            add(
                k,
                i,
                H,
                "configured binding initializer",
                expected,
                actual,
                [bp],
                extra={
                    "configured_kind": n["name"][1],
                    "candidate_scope": b["effective_scope"],
                    "ancestor_templates": [
                        node(k, j)["attributes"] for j in ancestors(k, i) if node(k, j)["name"] == [X, "template"]
                    ],
                },
            )
            if n["name"][1] == "param":
                add(
                    k,
                    i,
                    H,
                    "configured global parameter override effects",
                    shape(k, i),
                    b,
                    [bp],
                    status="unexamined",
                    reason="configured xsl:param has a default initializer; candidate source let has no external override contract; actual engine supplied parameter policy not established",
                )

        # Helper bodies/signatures and globals in configured view, including compiler-injected helpers.
        for i, n in enumerate(docs[k]["nodes"]):
            if n["name"] != [X, "function"]:
                continue
            qualified = n["attributes"].get("name")
            matches = [
                (p, v)
                for p, v in objects
                if p.startswith("/effective_package/")
                and isinstance(v, dict)
                and "result_type" in v
                and v["name"]["qualified"] == qualified
            ]
            expected = xmlhelper(k, i)
            if len(matches) == 1:
                p, h = matches[0]
                actual = irhelper(h) if "body" in h else None
                add(
                    k,
                    i,
                    H,
                    "configured helper declaration/constructor",
                    expected,
                    actual,
                    [p],
                    status=None if actual is not None else "unexamined",
                    reason=None if actual is not None else "candidate helper has no comparable body",
                    extra={
                        "constructor_projection": "strip whitespace-only XSLT stylesheet text",
                        "source_raw_constructor": shape(k, i),
                    },
                )
            else:
                add(
                    k,
                    i,
                    H,
                    "configured injected helper",
                    shape(k, i, True),
                    None,
                    ["/effective_package/schemas"],
                    status="unsupported",
                    reason="compiler-injected location helper has no source-level Helper counterpart; diagnostic location generation is not represented by candidate MessagePart",
                    extra={
                        "constructor_projection": "strip whitespace-only XSLT stylesheet text",
                        "source_raw_constructor": shape(k, i),
                    },
                )
    # Release view of the known conflict, using the existing independent extractor.
    k = "cen.zip!xslt/EN16931-CII-validation.xslt"
    ex = executable_view(docs[k])
    es = [e for e in ex["emitters"] if e["kind"] == "failed-assert" and e["fields"]["id"]["value"] == "CII-SR-282"]
    require(len(es) == 1, "unique release CII-SR-282 emitter")
    e = es[0]
    p, a, rp, r = site_by_id["CII-SR-282"][0]
    for field, cat, actual, path in [
        ("test", P, a["test"]["text"], p + "/test"),
        ("flag", PS, a["raw_flag"], p + "/raw_flag"),
    ]:
        f = e["fields"][field]
        add(
            k,
            e["node"],
            cat,
            "release CII-SR-282 metadata:" + field,
            f,
            actual,
            [path],
            status="different"
            if f["status"] in ["literal", "absent"] and f["value"] != actual
            else "mapped-exact"
            if f["value"] == actual
            else "ambiguous",
            reason="release comparison evidence only; competing source authority remains a blocker",
            extra={"full_control_ancestors": e["control"]},
        )
    add(
        k,
        e["template"],
        CT,
        "release CII-SR-282 template match",
        e["template_attributes"]["match"],
        r["context"]["text"],
        [rp + "/context"],
        extra={"template_attributes": e["template_attributes"]},
    )
    for control in e["control"]:
        for branch in control.get("preceding_when_branches", []):
            add(
                k,
                branch["node"],
                P,
                "release CII-SR-282 success guard",
                branch["attributes"]["test"],
                a["test"]["text"],
                [p + "/test"],
                reason="actual release success guard compared; release is comparison evidence, never a frontend",
            )

    # Scenario: all semantic selected children, each override, and namespace context.
    k = "config.zip!scenarios.xml"
    require(text(k, 177) == package["policy"]["scenario"], "selected scenario")
    for i in [184, 185, 186]:
        n = node(k, i)
        prefix = n["attributes"]["prefix"]
        add(
            k,
            i,
            N,
            "policy namespace",
            {"prefix": prefix, "uri": text(k, i)},
            {"prefix": prefix, "uri": package["policy"]["namespaces"].get(prefix)},
            ["/effective_package/policy/namespaces"],
        )
    for i, field in [(187, "selection"), (213, "acceptance")]:
        e = package["policy"][field]
        add(
            k,
            i,
            PS,
            "policy expression lexical:" + field,
            text(k, i),
            e["text"],
            ["/effective_package/policy/" + field],
        )
        check_occurrences(text(k, i), e["tree"])
        add(
            k,
            i,
            PS,
            "policy expression AST:" + field,
            text(k, i),
            e["tree"],
            ["/effective_package/policy/" + field + "/tree"],
            status="unexamined",
            reason="exact selector text and lexical occurrences checked; AST grouping/association not independently established",
        )
    ovidx = next(j for j, c in enumerate(package["policy"]["clauses"]) if c["kind"] == "overrides")
    values = package["policy"]["clauses"][ovidx]["values"]
    for i in range(204, 213):
        n = node(k, i)
        aid = text(k, i)
        add(
            k,
            i,
            PS,
            "scenario severity override",
            {"id": aid, "level": n["attributes"]["level"]},
            {"id": aid, "level": values.get(aid)},
            [f"/effective_package/policy/clauses/{ovidx}/values/{aid}"],
        )
    scenario = node(k, 188)["parent"]
    stage_nodes = children(k, scenario, C)
    source_stages = []
    for i in stage_nodes:
        local = node(k, i)["name"][1]
        if local not in ["validateWithXmlSchema", "validateWithSchematron", "createReport"]:
            continue
        loc = next(j for j in children(k, children(k, i, C, "resource")[0], C, "location"))
        target = resource(k).split("/")[0] + "/" + text(k, loc)
        if local == "validateWithSchematron":
            label = {resource(configured_keys[0]): "cen", resource(configured_keys[1]): "xrechnung"}.get(target, target)
        else:
            label = {"validateWithXmlSchema": "cii-xsd", "createReport": "report"}[local]
        source_stages.append(label)
        retained = package["resources"].get(target)
        actual_target = retained["archive"] + "/" + retained["member"] if retained else None
        add(
            k,
            i,
            PS,
            "scenario resource presence",
            target,
            actual_target,
            ["/effective_package/resources"],
        )
    add(k, scenario, PS, "scenario ordered stages", source_stages,
        package["policy"]["clauses"][0]["stages"], ["/effective_package/policy/clauses/0/stages"],
        reason="complete scenario stage order compared to candidate stages; extra engine stages need separate source proof")
    # Report policy and classfile clauses remain typed claims with pinned evidence, not a source proof.
    for ci, cl in enumerate(package["policy"]["clauses"]):
        for ei, e in enumerate(cl["evidence"]):
            a, m = e["resource"].split("/", 1)
            key = a + "!" + m
            paths = [f"/effective_package/policy/clauses/{ci}"]
            if key in docs:
                # A citation is not itself a proof. Preserve exact cited lines and a source occurrence.
                match = re.search(r"lines? (\d+)-(\d+)", e["locator"])
                b = payload[key]
                lo, hi = (int(match[1]), int(match[2])) if match else (1, len(b.splitlines()))
                lines = b.decode("utf-8").splitlines(keepends=True)
                fragment = "".join(lines[lo - 1 : hi])
                indices = [j for j, n in enumerate(docs[key]["nodes"]) if lo <= n["line"] <= hi]
                idx = indices[0] if indices else 0
                add(
                    key,
                    idx,
                    PS,
                    "typed policy clause evidence:" + cl["kind"] + ":" + str(ei),
                    {"locator": e["locator"], "exact_lines": fragment},
                    cl,
                    paths,
                    status="unexamined",
                    reason="evidence bytes authenticated; whole policy effects/control flow not proved by typed constants or line citations",
                )
            else:
                # Pinned classfile identity, no decompilation/execution claim.
                r = {
                    "obligation_id": sha(enc([key, cl["kind"], ei]))[:20],
                    "category": PS,
                    "facet": "engine classfile policy evidence:" + cl["kind"],
                    "source": {
                        "archive": a,
                        "archive_sha256": ident["archives"][a]["sha256"],
                        "member": m,
                        "member_sha256": ident["archives"][a]["selected_members"][m]["sha256"],
                        "occurrence": {"method_locator": e["locator"]},
                    },
                    "candidate_locations": paths,
                    "status": "unexamined",
                    "evidence": {"source": e, "candidate": cl},
                    "reason": "classfile bytes authenticated against additive JAR closure; method control flow/effect correspondence unexamined",
                }
                rows.append(r)
    # Independent source-owned policy denominator: complete non-HTML report template/function
    # and top-level binding trees, not just the candidate's selected evidence citations.
    for k in ["config.zip!resources/default-report.xsl", "config.zip!resources/xrechnung-report.xsl"]:
        for i in children(k, 0, X):
            n = node(k, i)
            local = n["name"][1]
            attrs = n["attributes"]
            if local in ["template", "function"]:
                if attrs.get("name", "").startswith("html:") or attrs.get("mode", "").startswith("html:"):
                    continue
            elif local not in ["param", "variable", "import"]:
                continue
            refs = []
            # Match citations by the actual owner span's source lines, preserving uncertainty.
            for ci, cl in enumerate(package["policy"]["clauses"]):
                for e in cl["evidence"]:
                    if e["resource"] != resource(k):
                        continue
                    nums = [int(x) for x in re.findall(r"(?<![A-Za-z])\b[0-9]+\b", e["locator"])]
                    last_line = payload[k][: n["raw_span"][1]].count(b"\n") + 1
                    if any(n["line"] <= x <= last_line for x in nums):
                        refs.append(f"/effective_package/policy/clauses/{ci}")
            refs = sorted(set(refs))
            rp = "/effective_package/resources/" + resource(k).replace("~", "~0").replace("/", "~1")
            if local == "import":
                target = "config.zip/" + posixpath.normpath("resources/" + attrs["href"])
                edges = [
                    (j, e)
                    for j, e in enumerate(package["source_graph"])
                    if e["from"] == resource(k) and e["to"] == target and e["relation"] == "opaque-xslt-import"
                ]
                add(
                    k,
                    i,
                    PS,
                    "report import dependency",
                    {"href": attrs["href"], "resolved_target": target},
                    [e for j, e in edges],
                    [f"/effective_package/source_graph/{j}" for j, e in edges],
                    status="mapped-exact" if len(edges) == 1 else "unsupported",
                    reason="exact same-archive import retained; no report execution claim",
                )
            else:
                add(
                    k,
                    i,
                    PS,
                    "source-owned report policy constructor:" + local,
                    shape(k, i),
                    {
                        "related_typed_clauses": [
                            package["policy"]["clauses"][int(p.rsplit("/", 1)[1])] for p in refs
                        ],
                        "opaque_resource": package["resources"][resource(k)],
                    },
                    refs + [rp],
                    status="unexamined" if refs else "unsupported",
                    reason="complete non-HTML source owner tree retained; citation overlap does not prove typed policy effects"
                    if refs
                    else "source report constructor is retained opaquely but has no typed policy counterpart; report program not representable by current clauses",
                )
        add(
            k,
            0,
            N,
            "opaque report namespace context",
            node(k, 0)["namespaces"],
            package["policy"]["namespaces"],
            ["/effective_package/policy/namespaces"],
            status="unexamined",
            reason="report XML/XPath static contexts are retained in opaque bytes; the selector namespace map is a different context, not a report-program namespace model",
        )

    # Source dialect is enforced by the compiler but has no explicit per-schema IR field.
    for k in [primary[0], primary[1], entry]:
        add(
            k,
            0,
            CT,
            "declared XPath query binding",
            node(k, 0)["attributes"].get("queryBinding"),
            {"format": package["format"], "per_schema_queryBinding_field": False},
            ["/effective_package/format"],
            status="unexamined",
            reason="xslt2 source dialect is an implicit compiler/IR contract; no separate candidate queryBinding field, and no execution semantics claim",
        )

    # Every expression-associated AST remains pending except the separately authored selected tree.
    # Add selected original predicate proof without overriding the raw source conflict.
    r = candidate["resolutions"][0]
    src = r["effective"]["source"]
    k = src["resource"].replace("/", "!", 1)
    i = next(
        i
        for i, n in enumerate(docs[k]["nodes"])
        if n["raw_attributes"].get("value", {}).get("span", [None])[0] == src["xml_byte_start"]
    )
    add(
        k,
        i,
        P,
        "selected original predicate authored AST",
        node(k, i)["attributes"]["value"],
        r["effective"]["tree"],
        ["/resolutions/0/effective/tree"]
        + [p + "/test/tree" for p, a, _, _ in site_locations.values() if a["id"] == "CII-SR-282"],
        status="mapped-exact" if r["effective"]["tree"] == independent_candidate.BUYER_TREE else "different",
        reason="compared with independently authored BUYER_TREE in tests/candidate.py; this does not resolve competing raw/release authority",
        extra={"independently_authored_tree": independent_candidate.BUYER_TREE},
    )
    # All candidate locations have actual exact source ranges (including original selected value).
    location_count = 0
    for p, v in objects:
        if not isinstance(v, dict) or set(v) != {
            "resource",
            "xml_byte_start",
            "xml_byte_end",
            "line",
            "column",
            "attribute",
        }:
            continue
        key = v["resource"].replace("/", "!", 1)
        ns = docs[key]["nodes"]
        attr = v["attribute"]
        start = v["xml_byte_start"]
        matched = [
            n
            for n in ns
            if (
                n["raw_span"][0] == start
                if attr is None
                else n["raw_attributes"].get(attr, {}).get("span", [None])[0] == start
            )
        ]
        require(len(matched) == 1, (p, "source location not unique"))
        n = matched[0]
        expected = n["raw_span"] if attr is None else n["raw_attributes"][attr]["span"]
        require(expected == [v["xml_byte_start"], v["xml_byte_end"]], (p, "source range drift"))
        prefix = payload[key][:start].decode("utf-8")
        require(v["line"] == prefix.count("\n") + 1 and v["column"] == len(prefix.rsplit("\n", 1)[-1]) + 1,
                (p, "source line/column drift"))
        location_count += 1

    require(len({r["obligation_id"] for r in rows}) == len(rows), "duplicate obligation")
    rows.sort(
        key=lambda r: (
            r["category"],
            r["source"]["archive"],
            r["source"]["member"],
            r["source"].get("node_index", -1),
            r["facet"],
        )
    )
    coverage = {
        k: dict(collections.Counter(n["name"][1] for n in docs[k]["nodes"]))
        for k in primary
        + [
            entry,
            "cen.zip!schematron/abstract/EN16931-CII-model.sch",
            "cen.zip!schematron/abstract/EN16931-CII-syntax.sch",
            "cen.zip!schematron/CII/EN16931-CII-model.sch",
            "cen.zip!schematron/CII/EN16931-CII-syntax.sch",
            "cen.zip!schematron/codelist/EN16931-CII-codes.sch",
        ]
    }
    coverage["excluded_configured_scaffolding"] = sorted(
        excluded_scaffolding, key=lambda row: (row["name"], row["source"]["archive"],
                                              row["source"]["member"], row["source"]["node_index"])
    )
    for row in rows:
        for pointer in row["candidate_locations"]:
            resolve_pointer(candidate, pointer)
    return rows, coverage, location_count


def resolve_pointer(value, pointer):
    require(pointer.startswith("/"), "invalid candidate JSON pointer")
    for part in pointer[1:].split("/"):
        key = part.replace("~1", "/").replace("~0", "~")
        value = value[int(key)] if isinstance(value, list) else value[key]
    return value


def summarize(value, rows, coverage, location_count, payload, ident):
    """Bind the complete ordered denominator; retain all non-exact evidence."""
    require(len({r["obligation_id"] for r in rows}) == len(rows), "duplicate obligation")
    counts = dict(sorted(collections.Counter(r["status"] for r in rows).items()))
    categories = {
        category: dict(sorted(collections.Counter(r["status"] for r in rows if r["category"] == category).items()))
        for category in sorted({r["category"] for r in rows})
    }
    exact = [r for r in rows if r["status"] == "mapped-exact"]
    return {
        "format": "einvoice-correspondence-accounting/1",
        "result": "incomplete",
        "claim": "bounded exact field/constructor preservation; not full correspondence or execution equivalence",
        "candidate_sha256": ident["candidate_sha256"],
        "candidate_accounting_sha256": ident["candidate_accounting_sha256"],
        "artifact_lock_sha256": ident["artifact_lock_sha256"],
        "jar_closure_lock_sha256": ident["jar_closure_lock_sha256"],
        "source_package": value["source_package"],
        "backend": value["backend"],
        "correspondence_audit": value["correspondence_audit"],
        "scope": {
            "included": "CEN original/preprocessed declarations; release CII-SR-282 conflict only; XR/common declarations; configured CEN/XR metadata, failure guards, messages, routing and helpers; selected CII scenario, non-HTML report owner trees and typed policy evidence",
            "excluded": "XSD semantics, HTML presentation, unrelated scenarios, arbitrary compiler scaffolding, full AST/dispatch/helper-effect/policy-effect proofs, XSLT or invoice execution, runtime/backend, AOT and performance",
            "configured_xslt": "comparison evidence only; never executed",
            "granularity": "one entry per occurrence/facet; lexical and AST facets and original/preprocessed/configured views have separate obligations",
        },
        "archives": {a: v["sha256"] for a, v in sorted(ident["archives"].items())},
        "members": {k: {"sha256": sha(b), "size": len(b)} for k, b in sorted(payload.items())},
        "coverage": {k: v for k, v in coverage.items() if k != "excluded_configured_scaffolding"},
        "excluded_configured_scaffolding": coverage["excluded_configured_scaffolding"],
        "candidate_source_locations_checked": location_count,
        "denominator": len(rows),
        "counts": counts,
        "categories": categories,
        "obligations_sha256": sha(enc(rows)),
        "mapped_exact": {"count": len(exact), "canonical_list_sha256": sha(enc(exact))},
        "non_exact": [r for r in rows if r["status"] != "mapped-exact"],
    }


def check(archives, directory, reverse=False):
    value, docs, payload, ident = load_inputs(archives, directory, reverse)
    rows, coverage, location_count = enumerate_obligations(value, docs, payload, ident)
    return summarize(value, rows, coverage, location_count, payload, ident)


def check_accounting(actual, expected):
    require(actual == expected, "reviewed correspondence accounting changed (including candidate/summary pairing)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archives", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--summary", type=Path, required=True)
    args = parser.parse_args()
    summary = check(args.archives, args.candidate)
    args.summary.write_bytes(canonical(summary))
    print(json.dumps({k: summary[k] for k in ["result", "denominator", "counts"]}, sort_keys=True))
