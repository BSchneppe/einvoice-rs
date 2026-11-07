"""Closed structural dispatch proof for the two pinned SchXslt CII documents.

Inputs are the correspondence audit's authenticated Expat trees. Derivation has
no candidate argument. Matching, XPath values, payload effects and XSLT execution
are outside this proof; the entry contracts additionally require exact wrapper
bytes. Unknown shapes raise Unsupported, including under python -O.
"""

import posixpath
import re

import xpath_bindings as bindings
import xpath_candidate
import xpath_structure as xpath

X = "http://www.w3.org/1999/XSL/Transform"
S = "http://purl.oclc.org/dsdl/schematron"
V = "http://purl.oclc.org/dsdl/svrl"
H = "https://doi.org/10.5281/zenodo.1495494"
P = "schxslt:patterns-matched"
CONFIGURED = (
    "config.zip!resources/cii/16b/xsl/EN16931-CII-validation.xsl",
    "config.zip!resources/xrechnung/3.0.2/xsl/XRechnung-CII-validation.xsl",
)
SOURCES = (
    "cen.zip!schematron/preprocessed/EN16931-CII-validation-preprocessed.sch",
    "xr.zip!schematron/cii/XRechnung-CII-validation.sch",
)
ENTRY_SHA256 = dict(zip(CONFIGURED, (
    "1d5aa3fe81f9d8d17d1ce7ad8d7faff9f50ae7a3484f16e5e533c76ad7d235da",
    "3da82ccd78b99665528129a1ca105c158c13c9223dc5dd42ec398acab7f4118a",
)))
RULE_FACET = "configured mode/priority/suppression dispatch"
CONTRACT_FACET = "dispatch contract"
CONTRACT = "DocumentTraversalFirstMatchingRulePerPatternInSourceOrder"
ENTRY_NOTE = ("root() is XSLT 3.0 pattern syntax; the exact pinned SchXslt entry "
              "wrapper is recognized, with no strict XSLT 2.0 portability or processor acceptance claim")


class Unsupported(ValueError):
    pass


class EntryUnsupported(Unsupported):
    pass


def require(ok, reason):
    if not ok:
        raise Unsupported(reason)


class Document:
    def __init__(self, key, parsed):
        self.key, self.parsed = key, parsed
        self.nodes = parsed["nodes"]

    def children(self, i):
        return self.nodes[i]["children"]

    def attrs(self, i):
        return self.nodes[i]["attributes"]

    def tag(self, i):
        return tuple(self.nodes[i]["name"])

    def named(self, ns, local):
        return [i for i, n in enumerate(self.nodes) if n["name"] == [ns, local]]

    def evidence(self, i):
        n = self.nodes[i]
        return {"archive_member": self.key, "node_index": i, "byte_span": n["raw_span"],
                "raw_sha256": n["raw_sha256"], "attributes": n["attributes"]}

    def expect(self, i, ns, local, attrs):
        require(self.tag(i) == (ns, local) and self.attrs(i) == attrs,
                f"{self.key}:{i}: expected {local} {attrs}")

    def only(self, i, ns, local, attrs):
        require(len(self.children(i)) == 1, f"{local}: one direct child required")
        child = self.children(i)[0]
        self.expect(child, ns, local, attrs)
        return child

    def leaf(self, i):
        require(not self.children(i) and not any(t.get("text", "").strip()
                for t in self.nodes[i]["content"]), "empty instruction required")


def admit_pattern(tree):
    """Only child/attribute paths and unions; predicates use the XPath parser."""
    def step(t):
        return step(t[1]) if t[0] == "filter" else t[0] == "step" and t[1] in ("child", "attribute")

    def path(t):
        return (t[0] == "root" or step(t) or
                (t[0] == "binary" and t[1] in ("/", "//") and path(t[2]) and step(t[3])))

    def pattern(t):
        return pattern(t[2]) and path(t[3]) if t[0] == "binary" and t[1] == "|" else path(t)

    require(pattern(tree), "rule match outside bounded XSLT 2.0 pattern subset")


def derive(key, parsed):
    """Inventory and rank configured rules BEFORE reading candidate dispatch."""
    from correspondence import extension_instruction, static_text

    d = Document(key, parsed)
    require(key in CONFIGURED, "unconfigured stylesheet")
    d.expect(0, X, "transform", {"version": "2.0"})
    top = {(X, n) for n in ("template", "function", "param", "variable", "output")}
    top.add(("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "Description"))
    require(all(d.tag(i) in top for i in d.children(0)), "unexpected top-level declaration/import/include")
    instructions = {"transform", "template", "function", "param", "variable", "output", "attribute",
                    "if", "when", "otherwise", "choose", "comment", "value-of", "with-param",
                    "next-match", "apply-templates", "for-each", "sequence", "call-template", "number"}
    for i, n in enumerate(d.nodes):
        require(n["namespaces"].get("schxslt") == H and
                n["namespaces"].get("xs") == "http://www.w3.org/2001/XMLSchema",
                "dispatch expression/type namespace rebinding")
        require(not any(a in n["attributes"] for a in (
            "use-when", "xpath-default-namespace", "extension-element-prefixes",
            "{" + X + "}use-when", "{" + X + "}extension-element-prefixes",
            "{" + X + "}use-attribute-sets")), "unsupported static context/conditional/extension")
        require(not extension_instruction(parsed, i), "extension instruction")
        require(n["name"][0] != X or n["name"][1] in instructions, "unsupported XSLT instruction")

    templates = d.named(X, "template")
    require(all(d.nodes[i]["parent"] == 0 for i in templates), "nested template")
    drivers = [i for i in templates if any(d.tag(j) == (H, "document") for j in d.children(i))]
    require(len(drivers) == 1, "one activation driver required")
    driver = drivers[0]
    name = d.attrs(driver).get("name")
    require(name is not None and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.-]*", name), "simple named activation driver required")
    d.expect(driver, X, "template", {"name": name})
    document = d.only(driver, H, "document", {})
    require(len(d.children(document)) >= 2, "activation and traversal required")
    patterns, calls, applies, nexts = [], [], [], []
    for i in d.children(document)[:-1]:
        require(d.tag(i) == (H, "pattern") and set(d.attrs(i)) == {"id"}, "literal pattern token required")
        token = d.attrs(i)["id"]
        require(re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.-]*", token), "simple internal token required")
        require(len(d.children(i)) == 2, "activation constructor shape")
        test, loop = d.children(i)
        d.expect(test, X, "if", {"test": "exists(base-uri(root()))"})
        d.leaf(d.only(test, X, "attribute", {"name": "documents", "select": "base-uri(root())"}))
        d.expect(loop, X, "for-each", {"select": "root()"})
        require(len(d.children(loop)) == 1, "one active marker required")
        active = d.children(loop)[0]
        pid = d.attrs(active).get("id")
        require(pid is not None and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.-]*", pid), "literal active pattern identity required")
        d.expect(active, V, "active-pattern", {"id": pid, "name": pid})
        d.leaf(d.only(active, X, "attribute", {"name": "documents", "select": "base-uri(.)"}))
        patterns.append({"id": pid, "token": token, "activation": d.evidence(i), "rules": []})
    require(len({p["token"] for p in patterns}) == len(patterns) == len({p["id"] for p in patterns}),
            "unique active identities required")
    launch = d.children(document)[-1]
    mode = d.attrs(launch).get("mode")
    require(mode is not None and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.-]*", mode), "single ordinary traversal mode required")
    d.expect(launch, X, "apply-templates", {"select": "root()", "mode": mode})
    d.leaf(launch)
    applies.append(launch)

    defaults = [i for i in templates if d.attrs(i).get("mode") == "#all"]
    expected = {"text() | @*": [], "/": ["node()"], "*": ["@*", "node()"]}
    require(len(defaults) == 3 and {d.attrs(i).get("match") for i in defaults} == set(expected),
            "three traversal defaults with complete node coverage required")
    for i in defaults:
        match = d.attrs(i)["match"]
        d.expect(i, X, "template", {"match": match, "mode": "#all", "priority": "-10"})
        require(len(d.children(i)) == len(expected[match]), "fallback recursion count")
        for j, select in zip(d.children(i), expected[match]):
            d.expect(j, X, "apply-templates", {"select": select, "mode": "#current"})
            d.leaf(j)  # No with-param, including tunnel parameters, reaches descendants.
            applies.append(j)

    # A changed entry wrapper cannot close either #ALL contract. Byte equality is
    # an additional gate below, even if the structural launch path still matches.
    try:
        named = [i for i in templates if d.attrs(i).get("name") == "schxslt.validate"]
        roots = [i for i in templates if d.attrs(i).get("match") == "root()"]
        require(len(named) == len(roots) == 1, "entry templates")
        named, root = named[0], roots[0]
        d.expect(named, X, "template", {"name": "schxslt.validate"})
        uri = "document($schxslt.validate.initial-document-uri)"
        j = d.only(named, X, "apply-templates", {"select": uri})
        d.leaf(j)
        applies.append(j)
        d.expect(root, X, "template", {"match": "root()"})
        require(len(d.children(root)) == 2, "root param and choose")
        param, choose = d.children(root)
        d.expect(param, X, "param", {"name": "schxslt.validate.recursive-call", "as": "xs:boolean", "select": "false()"})
        d.leaf(param)
        d.expect(choose, X, "choose", {})
        require(len(d.children(choose)) == 2, "entry choose branches")
        when, otherwise = d.children(choose)
        d.expect(when, X, "when", {"test": "not($schxslt.validate.recursive-call) and (normalize-space($schxslt.validate.initial-document-uri) != '')"})
        j = d.only(when, X, "apply-templates", {"select": uri})
        d.leaf(d.only(j, X, "with-param", {"name": "schxslt.validate.recursive-call", "as": "xs:boolean", "select": "true()"}))
        applies.append(j)
        d.expect(otherwise, X, "otherwise", {})
        require([d.tag(i) for i in d.children(otherwise)] == [(X, "variable")] * 3 + [(V, "schematron-output")],
                "root report construction")
        _, report, _, _ = d.children(otherwise)
        d.expect(report, X, "variable", {"name": "report", "as": "element(schxslt:report)"})
        call = d.only(d.only(report, H, "report", {}), X, "call-template", {"name": name})
        d.leaf(call)
        calls.append(call)
    except Unsupported as error:
        raise EntryUnsupported("unsupported entry wrapper: " + str(error)) from error

    by_token = {p["token"]: p for p in patterns}
    rules = []
    for i in templates:
        if i in defaults + [driver, named, root]:
            continue
        a = d.attrs(i)
        require(set(a) == {"match", "mode", "priority"} and a["mode"] == mode,
                "rule mode/template inventory")
        require(re.fullmatch(r"-?\d+", a["priority"]) and int(a["priority"]) > -10,
                "explicit rule priority above defaults required")
        children = d.children(i)
        require(len(children) >= 2, "rule parameter and guard required")
        d.expect(children[0], X, "param", {"name": P, "as": "xs:string*"})
        d.leaf(children[0])
        for j in children[1:-1]:
            require(d.tag(j) == (X, "variable") and set(d.attrs(j)) == {"name", "select"}
                    and bindings.expanded(bindings.qname(d.attrs(j)["name"], d.nodes[j]["namespaces"])) !=
                        (H, "patterns-matched"), "select-only pre-guard variable without suppression shadow required")
            d.leaf(j)
        choose = children[-1]
        d.expect(choose, X, "choose", {})
        require(len(d.children(choose)) == 2, "two rule branches required")
        when, otherwise = d.children(choose)
        require(d.tag(when) == (X, "when") and set(d.attrs(when)) == {"test"}, "suppression guard required")
        guard = re.fullmatch(r"\$schxslt:patterns-matched\[\. = '([^']+)'\]", d.attrs(when)["test"])
        require(guard and guard[1] in by_token, "exact suppression token guard required")
        token = guard[1]
        d.expect(otherwise, X, "otherwise", {})
        branches = []
        for branch, marker, select in ((when, "suppressed-rule", "$" + P),
                (otherwise, "fired-rule", f"(${P}, '{token}')")):
            require(len(d.children(branch)) == 2, "wrapper then next-match required")
            wrapper, nxt = d.children(branch)
            d.expect(wrapper, H, "rule", {"pattern": token})
            markers = [j for j in d.children(wrapper) if d.tag(j) == (V, marker)]
            require(len(markers) == 1, "one direct rule marker required")
            contexts = [j for j in d.children(markers[0]) if d.tag(j) == (X, "attribute")
                        and d.attrs(j).get("name") == "context"]
            require(len(contexts) == 1 and static_text(parsed, contexts[0]) == a["match"],
                    "marker context must equal match")
            if marker == "suppressed-rule":
                require([d.tag(j) for j in d.children(wrapper)] == [(X, "comment"), (V, marker)],
                        "suppressed payload must not fire")
            else:
                require(d.children(wrapper)[0] == markers[0], "fired marker first in rule wrapper")
            d.expect(nxt, X, "next-match", {})
            argument = d.only(nxt, X, "with-param", {"name": P, "as": "xs:string*", "select": select})
            d.leaf(argument)
            nexts.append(nxt)
            branches.append({"wrapper": d.evidence(wrapper), "next_match": d.evidence(nxt),
                             "with_param": d.evidence(argument)})
        admit_pattern(xpath.Parser(a["match"]).parse())
        rules.append({"template": d.evidence(i), "match": a["match"], "priority": int(a["priority"]),
                      "pattern_id": by_token[token]["id"], "token": token, "mode": mode,
                      "guard": d.evidence(when), "parameter": d.evidence(children[0]),
                      "suppressed": branches[0], "fired": branches[1]})
    require(len({r["priority"] for r in rules}) == len(rules), "unique rule priorities required")
    rules.sort(key=lambda r: -r["priority"])
    for r in rules:
        by_token[r["token"]]["rules"].append(r)
    for local, admitted in (("apply-templates", applies), ("call-template", calls), ("next-match", nexts)):
        require(set(d.named(X, local)) == set(admitted), "unexpected dispatch instruction " + local)
    require(len(d.named(V, "active-pattern")) == len(patterns) and
            len(d.named(V, "fired-rule")) == len(d.named(V, "suppressed-rule")) == len(rules),
            "closed marker inventory required")
    require(sum(n["attributes"].get("name") == P for n in d.nodes) == 3 * len(rules),
            "suppression binding shadow")
    return {"patterns": patterns, "rules": rules, "entry": d.evidence(root),
            "entry_bytes_equal": d.nodes[root]["raw_sha256"] == ENTRY_SHA256[key],
            "entry_note": ENTRY_NOTE, "driver": d.evidence(driver), "launch": d.evidence(launch),
            "driver_call": d.evidence(call), "defaults": [d.evidence(i) for i in defaults],
            "template_count": len(templates), "mode": mode}


def canonical_tree(tree):
    """Normalize expression-local binders only; preserve every host binding ID."""
    def walk(t, local):
        if not isinstance(t, tuple) or not t:
            return t
        if t[0] == "variable":
            return t[0], t[1], local.get(t[2], t[2])
        if t[0] == "bind":
            inner, result = dict(local), []
            for name, identity, value in t[2]:
                initial = walk(value, inner)
                alpha = "xpath-local:" + str(len(inner))
                inner[identity] = alpha
                result.append((name, alpha, initial))
            return t[0], t[1], tuple(result), walk(t[3], inner)
        return tuple(walk(a, local) if isinstance(a, tuple) else a for a in t)
    return walk(tree, {})


def source_patterns(docs, key):
    d = Document(key, docs[key])
    result = []
    for i in d.children(0):
        if d.tag(i) == (S, "pattern"):
            result.append((key, i))
        elif d.tag(i) == (S, "include"):
            target = key.split("!")[0] + "!" + posixpath.normpath(posixpath.join(
                posixpath.dirname(key.split("!")[1]), d.attrs(i)["href"]))
            require(target == bindings.COMMON and target in docs and
                    docs[target]["nodes"][0]["name"] == [S, "pattern"], "unsupported whole-pattern include")
            result.append((target, 0))
    return result


def compare(candidate, docs, key, table, si):
    """Source order is authoritative for comparison; candidate never drives extraction."""
    from correspondence import enc, sha

    scopes = bindings.SourceScopes({k: docs[k] for k in (*SOURCES, bindings.COMMON, *CONFIGURED)})
    d, sd = Document(key, docs[key]), Document(SOURCES[si], docs[SOURCES[si]])
    source = source_patterns(docs, sd.key)
    source_ids = [docs[k]["nodes"][i]["attributes"]["id"] for k, i in source]
    schema = candidate["effective_package"]["schemas"][si]
    phases = [{"id": sd.attrs(i)["id"], "active_patterns": [sd.attrs(j)["pattern"]
               for j in sd.children(i) if sd.tag(j) == (S, "active")]} for i in sd.named(S, "phase")]
    issues = []
    if source_ids != [p["id"] for p in table["patterns"]] or source_ids != [p["id"] for p in schema["patterns"]]:
        issues.append("ordered active pattern identities differ")
    if schema["selection"] != "#ALL" or schema["dispatch"] != CONTRACT or not all(p["active"] for p in schema["patterns"]):
        issues.append("candidate #ALL activation/dispatch differs")
    if phases != [{"id": p["id"], "active_patterns": p["active_patterns"]} for p in schema["phases"]]:
        issues.append("phase declarations differ")
    if schema["declared_default_phase"] != sd.attrs(0).get("defaultPhase"):
        issues.append("declared default phase differs")
    results = {}
    source_by_id = dict(zip(source_ids, source))
    require(len(source_by_id) == len(source_ids), "duplicate source pattern identity")
    candidate_by_id = {p["id"]: (pi, p) for pi, p in enumerate(schema["patterns"])}
    require(len(candidate_by_id) == len(schema["patterns"]), "duplicate candidate pattern identity")
    for pattern in table["patterns"]:
        pid = pattern["id"]
        if pid not in source_by_id or pid not in candidate_by_id:
            issues.append("unmatched configured pattern " + pid)
            continue
        sk, owner = source_by_id[pid]
        original = Document(sk, docs[sk])
        source_rules = [i for i in original.children(owner) if original.tag(i) == (S, "rule")]
        pi, cp = candidate_by_id[pid]
        if len(source_rules) != len(pattern["rules"]) or len(source_rules) != len(cp["rules"]):
            issues.append("rule multiplicity differs for " + pid)
            continue
        seen = set()
        for ri, (source_i, rule, crule) in enumerate(zip(source_rules, pattern["rules"], cp["rules"])):
            raw = original.attrs(source_i)["context"]
            resolved = scopes.resolve(xpath.Parser(raw).parse(), sk, source_i, "context", raw, True)
            require(resolved["resolved"], "source context binding not independently resolved")
            env = scopes.environment(sk, source_i, "context", True)
            globals_, witnesses = {}, []
            for ref in resolved["references"]:
                decl = ref["source_declaration"]
                if isinstance(decl["kind"], str) and decl["kind"].startswith("xpath:"):
                    continue
                qn = bindings.expanded(decl["name"])
                matches = [i for i in d.children(0) if d.tag(i) in ((X, "variable"), (X, "param")) and
                           bindings.expanded(bindings.qname(d.attrs(i)["name"], d.nodes[i]["namespaces"])) == qn]
                require(len(matches) == 1, "unique configured global binding witness required")
                globals_[qn] = env[qn]
                witnesses.append({"declarative": decl, "configured_global": d.evidence(matches[0])})
            ti = rule["template"]["node_index"]
            compiled = bindings.resolve(xpath.Parser(rule["match"]).parse(), globals_,
                                        d.nodes[ti]["namespaces"], "configured-match", rule["match"])
            require(compiled["resolved"], "configured match has unmatched free binding")
            expected, actual = canonical_tree(resolved["source_tree"]), canonical_tree(compiled["source_tree"])
            require(enc(actual) not in seen, "duplicate normalized match needs rule-body identity evidence")
            seen.add(enc(actual))
            adapted = canonical_tree(xpath_candidate.adapt(crule["context"]["tree"]))
            candidate_text = bindings.resolve(xpath.Parser(crule["context"]["text"]).parse(), env,
                scopes.namespaces(sk, source_i), "candidate-text", crule["context"]["text"])
            source_equal = crule["source"]["resource"] == sk.replace("!", "/", 1) and [
                crule["source"]["xml_byte_start"], crule["source"]["xml_byte_end"]] == original.nodes[source_i]["raw_span"]
            equal = source_equal and candidate_text["resolved"] and expected == actual == adapted == canonical_tree(candidate_text["source_tree"])
            results[ti] = {"status": "mapped-exact" if equal else "different", "configured_rule": rule,
                "source_rule": original.evidence(source_i), "global_binding_witnesses": witnesses,
                "candidate_location": f"/effective_package/schemas/{si}/patterns/{pi}/rules/{ri}",
                "source_tree_sha256": sha(enc(expected)), "configured_tree_sha256": sha(enc(actual)),
                "candidate_tree_sha256": sha(enc(adapted)), "candidate_source_identity_equal": source_equal}
    if issues:
        for result in results.values():
            result["status"] = "different"
    return {"rules": results, "issues": issues, "phases": phases,
            "contract_status": "different" if issues or any(r["status"] != "mapped-exact" for r in results.values()) else "mapped-exact"}


def derive_documents(docs):
    """Source-only preparation, also used before the audit enumerates its rows."""
    tables, errors = {}, {}
    for key in CONFIGURED:
        try:
            tables[key] = derive(key, docs[key])
        except (Unsupported, xpath.Unsupported) as error:
            errors[key] = error
    return tables, errors


def update_obligations(rows, candidate, docs, derivation=None):
    """One-to-one join to existing IDs, never create or remove an obligation."""
    from correspondence import enc, sha

    selected = [r for r in rows if r["facet"] in (RULE_FACET, CONTRACT_FACET)]
    require(len(selected) == 158 and len({r["obligation_id"] for r in selected}) == 158,
            "pinned 158 dispatch obligation identities required")
    # Finish both independent configured derivations before any candidate read.
    tables, errors = derive_documents(docs) if derivation is None else derivation
    errors = dict(errors)
    comparisons = {}
    for si, key in enumerate(CONFIGURED):
        if key not in errors:
            try:
                comparisons[key] = compare(candidate, docs, key, tables[key], si)
            except (Unsupported, xpath.Unsupported) as error:
                errors[key] = error
    joined = set()
    for row in selected:
        src = row["source"]
        source_key = src["archive"] + "!" + src["member"]
        is_contract = row["facet"] == CONTRACT_FACET
        key = CONFIGURED[SOURCES.index(source_key)] if is_contract else source_key
        require(key in CONFIGURED, "unconfigured dispatch obligation")
        actual = docs[source_key]["nodes"][src["node_index"]]
        require(src["raw_span"] == actual["raw_span"] and src["raw_sha256"] == actual["raw_sha256"],
                "dispatch obligation source occurrence drift")
        require(row["obligation_id"] == sha(enc([source_key, src["node_index"], row["category"], row["facet"]]))[:20],
                "dispatch obligation identity drift")
        join = (key, None if is_contract else src["node_index"])
        require(join not in joined, "duplicate dispatch occurrence join")
        joined.add(join)
        proof = {"scope": "structural dispatch only; no matching, evaluation or execution", "entry_note": ENTRY_NOTE}
        if key in tables:
            table = tables[key]
            proof.update({k: v for k, v in table.items() if k not in ("rules", "patterns")})
            proof["activation"] = [{k: v for k, v in p.items() if k != "rules"} for p in table["patterns"]]
        if key in errors:
            row["status"] = "unexamined" if is_contract and isinstance(errors[key], EntryUnsupported) else "unsupported"
            row["reason"] = "unsupported configured dispatch shape: " + str(errors[key])
            proof["unsupported_error"] = str(errors[key])
        else:
            comparison = comparisons[key]
            proof.update(issues=comparison["issues"], phases=comparison["phases"])
            if is_contract:
                row["status"] = comparison["contract_status"]
                if row["status"] == "mapped-exact" and not tables[key]["entry_bytes_equal"]:
                    row["status"] = "unexamined"
                row["reason"] = ("entry wrapper bytes differ from the pinned root() wrapper; " + ENTRY_NOTE
                    if row["status"] == "unexamined" else "bounded #ALL structural dispatch comparison; " + ENTRY_NOTE)
            else:
                result = comparison["rules"].get(src["node_index"])
                row["status"] = result["status"] if result else "different"
                row["reason"] = "independent configured priority/suppression chain compared to source and candidate rule order; no match evaluation"
                if result:
                    proof["rule"] = result
                    if row["candidate_locations"] != [result["candidate_location"]]:
                        row["status"] = "different"
                        proof["issues"] = proof["issues"] + ["existing association differs from priority-derived rule location"]
        row["evidence"]["dispatch"] = proof
    for key, table in tables.items():
        if key not in errors:
            require({i for k, i in joined if k == key and i is not None} ==
                    {r["template"]["node_index"] for r in table["rules"]}, "dispatch template join is not one-to-one")
