"""Bounded, read-only XML source accounting. No XPath evaluation or compilation."""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import sys
from urllib.parse import urlsplit
from xml.parsers import expat
import zipfile

ROOT = Path(__file__).resolve().parents[2]
LOCK = ROOT / "tools/oracle/artifacts.lock.json"
LOCK_SHA256 = "5dec502919d8ee137cd2c1453be8c65c1176e8bc707e358f646cef3fb1f55175"
FORMAT = "bsc10-source-inventory-v1"
S = "http://purl.oclc.org/dsdl/schematron"
X = "http://www.w3.org/1999/XSL/Transform"
V = "http://purl.oclc.org/dsdl/svrl"
D = "http://www.w3.org/2001/XMLSchema"
C = "http://www.xoev.de/de/validator/framework/1/scenarios"
XML = "http://www.w3.org/XML/1998/namespace"
CEN_PRE = "schematron/preprocessed/EN16931-CII-validation-preprocessed.sch"
XR_ROOT = "schematron/cii/XRechnung-CII-validation.sch"
CEN_XSL = "resources/cii/16b/xsl/EN16931-CII-validation.xsl"
XR_XSL = "resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl"
RELEASE_XSL = "xslt/EN16931-CII-validation.xslt"
ALLOWLIST = {
    "cen.zip": (
        "schematron/EN16931-CII-validation.sch", CEN_PRE,
        "schematron/abstract/EN16931-CII-model.sch",
        "schematron/abstract/EN16931-CII-syntax.sch",
        "schematron/CII/EN16931-CII-model.sch",
        "schematron/CII/EN16931-CII-syntax.sch",
        "schematron/codelist/EN16931-CII-codes.sch",
    ),
    "xr.zip": (XR_ROOT, "schematron/common.sch"),
    "config.zip": (
        "scenarios.xml", CEN_XSL, XR_XSL,
        "resources/cii/16b/xsd/CrossIndustryInvoice_100pD16B.xsd",
        "resources/cii/16b/xsd/CrossIndustryInvoice_QualifiedDataType_100pD16B.xsd",
        "resources/cii/16b/xsd/CrossIndustryInvoice_ReusableAggregateBusinessInformationEntity_100pD16B.xsd",
        "resources/cii/16b/xsd/CrossIndustryInvoice_UnqualifiedDataType_100pD16B.xsd",
    ),
}
CLAIMS = {
    "runtime_coverage": "unavailable", "native_tested_coverage": "unavailable",
    "semantic_equivalence": "unavailable", "conformance": False,
    "resource_attestation": False, "native_attestation": False,
    "release_evidence": False,
}


class InventoryError(ValueError):
    """A bounded diagnostic; never interpolate unselected archive names or XML."""


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False, allow_nan=False).encode("utf-8")


def json_bytes(value):
    return json.dumps(value, sort_keys=True, indent=2, ensure_ascii=False,
                      allow_nan=False).encode("utf-8") + b"\n"


def expanded(name):
    parts = name.split("}")
    return (parts[0], parts[1], parts[2] if len(parts) == 3 else "") if len(parts) > 1 else ("", name, "")


def parse_xml(data, archive, member, archive_digest, lock_digest):
    """Expat cannot fetch resources; all DTD/entity declarations are refused."""
    if b"\x00" in data or data.startswith((b"\xff\xfe", b"\xfe\xff")):
        raise InventoryError("unsupported XML byte encoding")
    parser = expat.ParserCreate(namespace_separator="}")
    parser.namespace_prefixes = True
    nodes, stack, pending_ns = [], [], []
    member_digest = sha(data)
    xml_declaration = {"version": "1.0", "encoding": None, "standalone": -1}

    def refuse(*_):
        raise InventoryError("unsupported XML DTD/entity declaration")

    def declaration(version, encoding, standalone):
        if version != "1.0" or (encoding and encoding.lower() not in ("utf-8", "utf8", "iso-8859-1")):
            raise InventoryError("unsupported XML version or declared encoding")
        xml_declaration.update(version=version, encoding=encoding, standalone=standalone)

    def content(token):
        if stack:
            target = nodes[stack[-1]]["content"]
            if "text" in token and target and "text" in target[-1]:
                target[-1]["text"] += token["text"]
            else:
                target.append(token)

    def start(name, attrs):
        ns, local, prefix = expanded(name)
        parent = nodes[stack[-1]] if stack else None
        ordinal = 1
        if parent:
            ordinal += sum(nodes[i]["name"] == [ns, local] for i in parent["children"])
        path = (parent["path"] if parent else []) + [[ns, local, ordinal]]
        scope = dict(parent["namespaces"]) if parent else {"xml": XML}
        scope.update(pending_ns)
        pending_ns.clear()
        attributes, lexical = {}, {}
        for key, value in attrs.items():
            uri, attr, pre = expanded(key)
            qualified = "{" + uri + "}" + attr if uri else attr
            attributes[qualified] = value
            lexical[qualified] = (pre + ":" if pre else "") + attr
        identity = {"domain": "bsc10-source-site-v1", "artifact_lock_sha256": lock_digest,
                    "archive_sha256": archive_digest, "member": member,
                    "member_sha256": member_digest, "element_path": path,
                    "kind": [ns, local]}
        node = {"site_id": sha(canonical(identity)), "name": [ns, local],
                "original_id": attributes.get("id"),
                "lexical_name": (prefix + ":" if prefix else "") + local,
                "attributes": attributes, "lexical_attributes": lexical,
                "namespaces": scope, "path": path, "ordinal": len(nodes) + 1,
                "parent": stack[-1] if stack else None, "children": [], "content": [],
                "line": parser.CurrentLineNumber, "byte_span": "unavailable"}
        index = len(nodes)
        if parent:
            parent["children"].append(index)
        content({"element": index})
        nodes.append(node)
        stack.append(index)

    parser.XmlDeclHandler = declaration
    parser.StartNamespaceDeclHandler = lambda prefix, uri: pending_ns.append((prefix or "", uri or ""))
    parser.StartElementHandler = start
    parser.EndElementHandler = lambda _: stack.pop()
    parser.CharacterDataHandler = lambda text: content({"text": text})
    parser.CommentHandler = lambda text: content({"comment": text})
    parser.ProcessingInstructionHandler = lambda target, text: content({"pi": [target, text]})
    parser.StartDoctypeDeclHandler = refuse
    parser.EntityDeclHandler = refuse
    parser.ExternalEntityRefHandler = refuse
    parser.SkippedEntityHandler = refuse
    parser.SetParamEntityParsing(expat.XML_PARAM_ENTITY_PARSING_NEVER)
    try:
        parser.Parse(data, True)
    except expat.ExpatError:
        raise InventoryError("malformed selected XML") from None
    if not nodes:
        raise InventoryError("empty selected XML")
    return {"archive": archive, "member": member, "sha256": member_digest,
            "bytes": len(data), "xml_declaration": xml_declaration, "nodes": nodes}


def read_archive(data, pin, members):
    """Hash and consume a single immutable byte snapshot, never reopen the path."""
    if sha(data) != pin["sha256"]:
        raise InventoryError("archive digest mismatch")
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            regular = [i.filename for i in archive.infolist() if not i.is_dir()]
            if len(set(regular)) != len(regular) or set(regular) != set(pin["members"]):
                raise InventoryError("archive member-set mismatch")
            if not set(members) <= set(pin["members"]):
                raise InventoryError("required source member missing")
            result = {}
            for member in members:
                payload = archive.read(member)
                if sha(payload) != pin["members"][member]:
                    raise InventoryError("selected member digest mismatch")
                result[member] = payload
            return result
    except (zipfile.BadZipFile, RuntimeError, KeyError, NotImplementedError):
        raise InventoryError("unreadable selected archive") from None


def ancestors(doc, index):
    parent = doc["nodes"][index]["parent"]
    result = []
    while parent is not None:
        result.append(parent)
        parent = doc["nodes"][parent]["parent"]
    return list(reversed(result))


def indexes(doc, ns, names):
    return [i for i, node in enumerate(doc["nodes"])
            if node["name"][0] == ns and node["name"][1] in names]


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
        attribute = ("extension-element-prefixes" if owner["name"][0] == X
                     else "{" + X + "}extension-element-prefixes")
        for prefix in owner["attributes"].get(attribute, "").split():
            namespace = owner["namespaces"].get("" if prefix == "#default" else prefix)
            # An unbound declaration is unsupported, not evidence of a literal child.
            if not namespace or node["name"][0] == namespace:
                return True
    return False


def literal_field(doc, index, field):
    node = doc["nodes"][index]
    constructors = [i for i in node["children"]
                    if doc["nodes"][i]["name"] == [X, "attribute"]
                    and doc["nodes"][i]["attributes"].get("name") == field
                    and doc["nodes"][i]["attributes"].get("namespace", "") == ""]
    # Unknown dynamic attribute names can overwrite any literal attribute.
    dynamic_names = [i for i in node["children"] if doc["nodes"][i]["name"] == [X, "attribute"]
                     and ("{" in doc["nodes"][i]["attributes"].get("name", "")
                          or "name" not in doc["nodes"][i]["attributes"]
                          or ":" in doc["nodes"][i]["attributes"].get("name", "")
                          or "{" in doc["nodes"][i]["attributes"].get("namespace", ""))]
    indirect = [i for i in node["children"] if doc["nodes"][i]["name"][0] == X
                and doc["nodes"][i]["name"][1] not in ("attribute", "text", "value-of")]
    value = node["attributes"].get(field)
    if (dynamic_names or indirect or any(extension_instruction(doc, i) for i in node["children"])
            or "{" + X + "}use-attribute-sets" in node["attributes"]
            or len(constructors) > 1 or (constructors and value is not None)):
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
    for i in indexes(doc, V, {"failed-assert", "successful-report", "fired-rule", "suppressed-rule", "active-pattern"}):
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
                    for s in siblings[:siblings.index(a)] if nodes[s]["name"] == [X, "when"]]
            control.append(entry)
        records.append({"node": i, "kind": nodes[i]["name"][1], "template": template,
                        "template_attributes": nodes[template]["attributes"] if template is not None else None,
                        "ancestor_chain": chain,
                        "control": control,
                        "fields": {f: literal_field(doc, i, f) for f in ("id", "test", "flag", "role", "context")},
                        "constructor_node": i})
    return {"templates": indexes(doc, X, {"template"}), "emitters": records}


def compare(source, target):
    """Limited raw-ID join. Missing/duplicate/dynamic identities never choose a winner."""
    declarations = indexes(source, S, {"assert", "report"})
    emitters = [e for e in target["executable"]["emitters"]
                if e["kind"] in ("failed-assert", "successful-report")]
    left, right = defaultdict(list), defaultdict(list)
    for i in declarations:
        left[source["nodes"][i]["attributes"].get("id")].append(i)
    for emitter in emitters:
        field = emitter["fields"]["id"]
        if field["status"] == "literal":
            right[field["value"]].append(emitter)
    associations = []
    for i in declarations:
        node = source["nodes"][i]
        ident = node["attributes"].get("id")
        matches = right.get(ident, []) if ident is not None else []
        status = ("missing_id" if ident is None else "ambiguous" if len(left[ident]) > 1 or len(matches) > 1
                  else "unmapped" if not matches else "unique_raw_id")
        rec = {"source_node": i, "original_id": ident, "status": status,
               "candidate_emitters": [e["node"] for e in matches]}
        if status == "unique_raw_id":
            emitter = matches[0]
            rules = [a for a in ancestors(source, i) if source["nodes"][a]["name"] == [S, "rule"]]
            context = source["nodes"][rules[-1]]["attributes"].get("context") if rules else None
            template = emitter["template_attributes"]
            fields = {}
            for field in ("test", "flag", "role", "context"):
                a = context if field == "context" else node["attributes"].get(field)
                metadata = emitter["fields"][field]
                b = template.get("match") if field == "context" and template is not None else metadata["value"]
                known = (template is not None and "match" in template) if field == "context" else metadata["status"] in ("literal", "absent")
                fields[field] = {"source": a, "configured": b,
                                 "comparison": "unknown" if not known else "exact" if a == b else
                                 "boundary_whitespace_only" if isinstance(a, str) and isinstance(b, str) and a.strip(" \t\r\n") == b.strip(" \t\r\n")
                                 else "different"}
            rec["fields"] = fields
            rec["emitter_kind_agrees"] = emitter["kind"] == ("failed-assert" if node["name"][1] == "assert" else "successful-report")
        associations.append(rec)
    mapped = {a["candidate_emitters"][0] for a in associations if a["status"] == "unique_raw_id"}
    return {"method": "raw-ID lexical association only; no semantic equivalence",
            "source": source["key"], "configured": target["key"], "associations": associations,
            "unknown_id_emitters": [e["node"] for e in emitters if e["fields"]["id"]["status"] not in ("literal", "absent")],
            "unassociated_emitters": [e["node"] for e in emitters if e["node"] not in mapped]}


def resolve_member(member, href):
    if not href:
        return member
    if any(ord(char) <= 32 or ord(char) == 127 for char in href):
        return None
    try:
        uri = urlsplit(href)
    except ValueError:
        return None
    if uri.scheme or uri.netloc or any(char in href for char in "?#%\\") or uri.path.startswith("/"):
        return None
    parts = list(PurePosixPath(member).parent.parts)
    for part in uri.path.split("/"):
        if part == "..":
            if not parts:
                return None
            parts.pop()
        elif part not in ("", "."):
            parts.append(part)
    return "/".join(parts)


def relations(documents):
    edges = []
    for key, doc in documents.items():
        for i, node in enumerate(doc["nodes"]):
            ns, kind = node["name"]
            attrs = node["attributes"]
            if ns == S and kind in ("param", "phase"):
                edges.append({"source": key, "node": i, "kind": kind,
                              "parent": node["parent"], "attributes": attrs,
                              "status": "unexpanded_parameter_occurrence" if kind == "param" else "declared_phase"})
            attr = ("href" if ns in (S, X) and kind in ("include", "import") else
                    "schemaLocation" if ns == D and kind in ("include", "import", "redefine", "override") else None)
            if attr:
                raw = attrs.get(attr)
                member = resolve_member(doc["member"], raw) if raw is not None else None
                target = doc["archive"] + "!" + member if member is not None else None
                status = "selected" if target in documents else "missing_location" if raw is None else "outside_selected_scope"
                edge = {"source": key, "node": i, "kind": kind, "raw": raw,
                        "target": target if target in documents else None, "status": status}
                # xml:base requires a resolver we intentionally do not implement.
                if any("{" + XML + "}base" in doc["nodes"][a]["attributes"] for a in ancestors(doc, i) + [i]):
                    edge.update(status="xml_base_unresolved", target=None)
                if ns == D and kind == "import" and edge["target"]:
                    target_ns = documents[edge["target"]]["nodes"][0]["attributes"].get("targetNamespace")
                    edge["namespace_comparison"] = {"declared": attrs.get("namespace"), "target": target_ns,
                                                    "equal": attrs.get("namespace") == target_ns}
                edges.append(edge)
            for attr, target_kind in (("is-a", "pattern"), ("pattern", "pattern"), ("rule", "rule")):
                if ns != S or attr not in attrs or (attr == "pattern" and kind != "active") or (attr == "rule" and kind != "extends"):
                    continue
                candidates = [{"document": k, "node": n} for k, d in documents.items() if d["archive"] == doc["archive"]
                              for n in indexes(d, S, {target_kind}) if d["nodes"][n]["attributes"].get("id") == attrs[attr]]
                edges.append({"source": key, "node": i, "kind": attr, "raw": attrs[attr],
                              "candidates": candidates, "status": "unresolved_static_candidates",
                              "note": "no include/phase/abstract expansion"})
    # Ordered edges retain duplicate include occurrences. Back-edges report cycles without expansion.
    adjacency = defaultdict(list)
    for index, edge in enumerate(edges):
        if edge.get("target"):
            adjacency[edge["source"]].append((edge["target"], index))
    visited, active, cycles = set(), set(), []

    def visit(key):
        visited.add(key)
        active.add(key)
        for target, index in adjacency[key]:
            if target in active:
                cycles.append({"back_edge": index, "source": key, "target": target})
            elif target not in visited:
                visit(target)
        active.remove(key)

    for key in documents:
        if key not in visited:
            visit(key)
    return {"edges": edges, "cycle_back_edges": cycles}


def describe(doc):
    counts = Counter("{" + n["name"][0] + "}" + n["name"][1] for n in doc["nodes"])
    params = defaultdict(list)
    for i in indexes(doc, S, {"param"}):
        node = doc["nodes"][i]
        params[(node["parent"], node["attributes"].get("name"))].append(i)
    doc["counts"] = dict(sorted(counts.items()))
    doc["duplicate_parameters"] = [{"parent": parent, "name": name, "occurrences": occurrences}
                                   for (parent, name), occurrences in params.items() if len(occurrences) > 1]
    if doc["nodes"][0]["name"] in ([X, "stylesheet"], [X, "transform"]):
        doc["executable"] = executable_view(doc)


def build(cache, release=False, lock_path=LOCK):
    lock_bytes = lock_path.read_bytes()
    if sha(lock_bytes) != LOCK_SHA256:
        raise InventoryError("baseline artifact lock anchor mismatch")
    pins = json.loads(lock_bytes)["artifacts"]
    documents, archives = {}, {}
    for archive, allowed in ALLOWLIST.items():
        matches = [p for p in pins if p["file"] == archive]
        if len(matches) != 1:
            raise InventoryError("ambiguous required artifact pin")
        pin = matches[0]
        chosen = allowed + ((RELEASE_XSL,) if release and archive == "cen.zip" else ())
        snapshot = (cache / "downloads" / archive).read_bytes()
        payloads = read_archive(snapshot, pin, chosen)
        archives[archive] = {"sha256": sha(snapshot), "member_set_verified": True,
                             "selected_members": len(payloads)}
        for member, payload in payloads.items():
            key = archive + "!" + member
            doc = parse_xml(payload, archive, member, archives[archive]["sha256"], sha(lock_bytes))
            expected = ([X, "stylesheet"], [X, "transform"]) if member.endswith((".xsl", ".xslt")) else (
                ([S, "schema"], [S, "pattern"]) if member.endswith(".sch") else
                ([D, "schema"],) if member.endswith(".xsd") else ([C, "scenarios"],))
            if doc["nodes"][0]["name"] not in expected:
                raise InventoryError("unsupported required XML root shape")
            doc["key"] = key
            doc["representation"] = ("preprocessed_schematron" if member == CEN_PRE else
                                     "original_schematron" if member.endswith(".sch") else
                                     "release_provenance_xslt" if member == RELEASE_XSL else
                                     "configured_xslt" if member in (CEN_XSL, XR_XSL) else
                                     "invoice_xsd" if member.endswith(".xsd") else "configuration")
            describe(doc)
            documents[key] = doc
    pairs = [("cen.zip!" + CEN_PRE, "config.zip!" + CEN_XSL),
             ("xr.zip!" + XR_ROOT, "config.zip!" + XR_XSL)]
    associations = [compare(documents[a], documents[b]) for a, b in pairs]
    if release:
        associations.append(compare(documents["cen.zip!" + CEN_PRE], documents["cen.zip!" + RELEASE_XSL]))
    policy = {"allowlist": ALLOWLIST, "release_provenance": release,
              "optional_release_member": RELEASE_XSL, "claims": CLAIMS, "format": FORMAT}
    return {"format": FORMAT, "artifact_lock_sha256": sha(lock_bytes),
            "inspector_sha256": sha(Path(__file__).read_bytes()), "policy_sha256": sha(canonical(policy)),
            "claims": dict(CLAIMS), "archives": archives, "documents": documents,
            "relations": relations(documents), "associations": associations}


def summary(report):
    documents = report["documents"]
    selected = [documents["cen.zip!" + CEN_PRE], documents["xr.zip!" + XR_ROOT], documents["xr.zip!schematron/common.sch"]]
    originals = [d for k, d in documents.items() if k.startswith("cen.zip!schematron/") and
                 k != "cen.zip!" + CEN_PRE] + selected[1:]
    configured = [documents["config.zip!" + n] for n in (CEN_XSL, XR_XSL)]
    comparisons = []
    for join in report["associations"]:
        counts = {field: dict(sorted(Counter(a["fields"][field]["comparison"] for a in join["associations"] if "fields" in a).items()))
                  for field in ("test", "context", "flag", "role")}
        differences = [dict(a) for a in join["associations"] if "fields" not in a or not a["emitter_kind_agrees"] or
                       any(f["comparison"] == "different" or f["comparison"] == "unknown" for f in a["fields"].values())]
        emitters = {e["node"]: e for e in documents[join["configured"]]["executable"]["emitters"]}
        for difference in differences:
            difference["source_site_id"] = documents[join["source"]]["nodes"][difference["source_node"]]["site_id"]
            difference["candidate_controls"] = [emitters[i]["control"] for i in difference["candidate_emitters"]]
        comparisons.append({"source": join["source"], "configured": join["configured"],
                            "statuses": dict(Counter(a["status"] for a in join["associations"])),
                            "fields": counts, "non_whitespace_differences": differences,
                            "unknown_id_emitters": join["unknown_id_emitters"],
                            "unassociated_emitters": join["unassociated_emitters"]})
    return {"format": FORMAT + "-summary", "report_sha256": sha(json_bytes(report)),
            **{f: report[f] for f in ("artifact_lock_sha256", "inspector_sha256", "policy_sha256", "claims", "archives")},
            "denominators": {
                "original_declarations": {kind: sum(len(indexes(d, S, {kind})) for d in originals) for kind in ("rule", "assert", "report")},
                "selected_source_sites": {kind: sum(len(indexes(d, S, {kind})) for d in selected) for kind in ("rule", "assert", "report")},
                "configured_emitters": dict(sorted(Counter(e["kind"] for d in configured for e in d["executable"]["emitters"]).items()))},
            "members": {k: {"sha256": d["sha256"], "bytes": d["bytes"],
                            "representation": d["representation"],
                            "xml_declaration": d["xml_declaration"],
                            "declarations": {kind: len(indexes(d, S, {kind})) for kind in ("pattern", "rule", "assert", "report", "param", "phase", "active")},
                            "duplicate_parameters": [{"name": p["name"],
                                "occurrences": [{"site_id": d["nodes"][i]["site_id"], "attributes": d["nodes"][i]["attributes"]} for i in p["occurrences"]]}
                                for p in d["duplicate_parameters"]]} for k, d in documents.items()},
            "comparisons": comparisons,
            "relations": {"statuses": dict(Counter(e["status"] for e in report["relations"]["edges"])),
                          "cycle_back_edges": report["relations"]["cycle_back_edges"],
                          "xsd_import_namespace_checks": [e["namespace_comparison"] for e in report["relations"]["edges"] if "namespace_comparison" in e]}}


def write_fresh(path, data, cache, root=ROOT):
    """Only fresh files; protect repo evidence, implementation and the entire cache."""
    if path.is_symlink():
        raise InventoryError("output symlink alias refused")
    try:
        target, repo, cache_root = path.resolve(), root.resolve(), cache.resolve()
    except (OSError, RuntimeError):
        raise InventoryError("output path resolution failed") from None
    if (target.is_relative_to(cache_root) or
            (target.is_relative_to(repo) and not target.is_relative_to(repo / ".scratch"))):
        raise InventoryError("protected output destination")
    try:
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
    except OSError:
        raise InventoryError("output must be a fresh file in an existing writable directory") from None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "verify"))
    parser.add_argument("--cache", required=True, type=Path)
    parser.add_argument("--release-provenance", action="store_true")
    parser.add_argument("--output", type=Path, help="optional fresh full-report file")
    parser.add_argument("--expected-summary", type=Path, default=Path(__file__).with_name("evidence-summary.json"))
    args = parser.parse_args(argv)
    try:
        report = build(args.cache, args.release_provenance)
        result = summary(report)
        if args.command == "verify" and args.expected_summary.read_bytes() != json_bytes(result):
            raise InventoryError("checked summary differs; inspect all source differences")
        if args.output:
            write_fresh(args.output, json_bytes(report), args.cache)
        sys.stdout.buffer.write(json_bytes(result))
        return 0
    except (InventoryError, OSError, KeyError, json.JSONDecodeError):
        # Exception text from dependencies may contain filesystem or unselected names.
        error = sys.exc_info()[1]
        print("source-inventory: " + (str(error) if isinstance(error, InventoryError) else "required source or summary unavailable/invalid"), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
