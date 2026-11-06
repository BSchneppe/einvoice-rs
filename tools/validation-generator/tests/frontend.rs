use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Write},
};
use validation_generator::{ir::*, schematron, source, xpath};
fn fixture(text: &str) -> validation_generator::Result<Schema> {
    schematron::parse(
        &BTreeMap::from([
            ("main.sch".into(), text.into()),
            (
                "common.sch".into(),
                include_str!("fixtures/common.sch").into(),
            ),
        ]),
        "main.sch",
        Some("chosen"),
    )
}
fn context() -> xpath::Context {
    let mut c = xpath::Context::xslt2();
    c.namespaces.insert("t".into(), "urn:fixture".into());
    c
}
fn assertion_schema(test: &str) -> validation_generator::Result<Schema> {
    let xml = format!(
        r#"<schema xmlns="http://purl.oclc.org/dsdl/schematron" queryBinding="xslt2"><pattern id="p"><rule context="root"><assert id="a" flag="fatal" test="{test}">message</assert></rule></pattern></schema>"#
    );
    schematron::parse(
        &BTreeMap::from([("main.sch".into(), xml)]),
        "main.sch",
        None,
    )
}
#[test]
fn axis_predicates_keep_group_boundaries() {
    let c = context();
    for axis in ["ancestor", "preceding-sibling"] {
        let direct = xpath::parse(&format!("{axis}::*[1]"), &c).unwrap();
        let grouped = xpath::parse(&format!("({axis}::*)[1]"), &c).unwrap();
        let step = Expr::Step {
            axis: axis.into(),
            test: NodeTest::Any,
        };
        let one = Expr::Number {
            lexical_kind: NumericKind::Integer,
            lexeme: "1".into(),
        };
        assert_eq!(
            direct,
            Expr::Filter {
                base: Box::new(step.clone()),
                predicate: Box::new(one.clone())
            }
        );
        assert_eq!(
            grouped,
            Expr::Filter {
                base: Box::new(Expr::Group {
                    operand: Box::new(step)
                }),
                predicate: Box::new(one)
            }
        );
        assert_ne!(direct, grouped);
        let stacked = assertion_schema(&format!("{axis}::*[1][1]")).unwrap();
        let nested = assertion_schema(&format!("(({axis}::*))[1][1]")).unwrap();
        assert_ne!(
            stacked.patterns[0].rules[0].sites[0].test.tree,
            nested.patterns[0].rules[0].sites[0].test.tree
        );
    }
    // This hand-authored expectation also protects ordinary operator precedence.
    let Expr::Binary {
        operator, right, ..
    } = xpath::parse("1 + 2 * 3", &c).unwrap()
    else {
        panic!()
    };
    assert_eq!(operator, "+");
    assert!(matches!(*right, Expr::Binary { operator, .. } if operator == "*"));
}
#[test]
fn source_compilation_enforces_path_cast_and_xpath2_arity() {
    for invalid in [
        "a/-b",
        "a/+b",
        "a / / b",
        "a// /b",
        "1 castable as xs:integer cast as xs:string",
        "1 cast as xs:integer / a",
        "round(1,2)",
    ] {
        assert!(assertion_schema(invalid).is_err(), "accepted {invalid}");
    }
    for valid in [
        "a/(-b)",
        "a/(+b)",
        "a/(/b)",
        "a//(/b)",
        "a/(b)[1]",
        "/",
        "-1",
        "1 cast as xs:integer castable as xs:string",
        "(1 castable as xs:integer) cast as xs:string",
        "(1 cast as xs:integer) cast as xs:string",
        "round(1)",
        "sum((1,2),0)",
        "round-half-to-even(1,2)",
    ] {
        assertion_schema(valid).unwrap_or_else(|e| panic!("rejected {valid}: {e}"));
    }
}
#[test]
fn helper_namespace_conflicts_fail_at_any_depth() {
    let template = r#"<schema xmlns="http://purl.oclc.org/dsdl/schematron" xmlns:xsl="http://www.w3.org/1999/XSL/Transform" xmlns:u="urn:outer" queryBinding="xslt2"><ns prefix="u" uri="urn:outer"/><ns prefix="n" uri="urn:nodes"/><xsl:function name="u:f" as="xs:string"><xsl:param name="arg"/><xsl:sequence select="string($arg/n:item)"/></xsl:function><pattern id="p"><rule context="root"><assert id="a" flag="fatal" test="u:f(.) = 'ok'">message</assert></rule></pattern></schema>"#;
    let parse = |xml: String| {
        schematron::parse(
            &BTreeMap::from([("main.sch".into(), xml)]),
            "main.sch",
            None,
        )
    };
    parse(template.into()).unwrap(); // Built-in xs remains available without xmlns:xs.
    for xml in [
        template.replace("xmlns:u=\"urn:outer\"", "xmlns:u=\"urn:inner\""),
        template.replace("<xsl:function ", "<xsl:function xmlns:u=\"urn:inner\" "),
        template.replace(
            "<xsl:function ",
            "<xsl:function xmlns:xs=\"urn:not-schema\" ",
        ),
        template.replace("<xsl:param ", "<xsl:param xmlns:u=\"urn:inner\" "),
        template.replace(
            "<xsl:sequence ",
            "<xsl:sequence xmlns:n=\"urn:other-nodes\" ",
        ),
    ] {
        let error = parse(xml).unwrap_err().to_string();
        assert!(error.contains("namespace rebinding"), "{error}");
        assert!(error.contains("main.sch:"), "{error}");
    }
}
#[test]
fn requirements_distinguish_node_tests_from_literal_text() {
    let baseline: Package = serde_json::from_str(include_str!("fixtures/inspection.json")).unwrap();
    let mut sets = Vec::new();
    for name in ["node", "text", "comment", "processing-instruction"] {
        let mut package = baseline.clone();
        package.schemas = vec![assertion_schema(&format!("{name}()")).unwrap()];
        package.policy = None;
        package.requirements.clear();
        let set = validation_generator::bundle::requirements(&package).unwrap();
        assert!(set.contains(&Requirement {
            category: "node-test-kind".into(),
            name: name.into()
        }));
        sets.push(set);
    }
    for pair in sets.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
}
#[test]
fn phases_dispatch_scopes_and_mixed_messages() {
    let s = fixture(include_str!("fixtures/scope.sch")).unwrap();
    assert_eq!(
        s.patterns.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["globals", "first", "second", "inactive"]
    );
    assert_eq!(
        s.patterns.iter().map(|p| p.active).collect::<Vec<_>>(),
        [false, true, true, false]
    );
    assert_eq!(
        s.dispatch,
        Dispatch::DocumentTraversalFirstMatchingRulePerPatternInSourceOrder
    );
    let common = &s.patterns[0].bindings[0];
    assert_eq!(common.effective_scope, Scope::Xslt2SchemaGlobal);
    assert_eq!(common.initialization_context, "document-root");
    let r = &s.patterns[1].rules[0];
    let mut vars = BTreeSet::new();
    let mut calls = BTreeSet::new();
    xpath::dependencies(&r.context.tree, &mut vars, &mut calls);
    assert!(vars.contains(&s.globals[0].id));
    let BindingValue::Select { expression } = &r.bindings[0].value else {
        panic!()
    };
    vars.clear();
    xpath::dependencies(&expression.tree, &mut vars, &mut calls);
    assert_eq!(vars, BTreeSet::from([s.globals[0].id.clone()]));
    let site = &r.sites[0];
    assert_eq!(site.message.len(), 5);
    assert!(matches!(&site.message[0],MessagePart::Text { text } if text == "before "));
    let MessagePart::ValueOf { expression } = &site.message[3] else {
        panic!()
    };
    assert!(
        matches!(&expression.tree,Expr::Variable { binding,.. } if binding == &r.bindings[0].id)
    );
    let Expr::Bind {
        bindings, result, ..
    } = &site.test.tree
    else {
        panic!()
    };
    assert_ne!(bindings[0].id, r.bindings[0].id);
    vars.clear();
    xpath::dependencies(result, &mut vars, &mut calls);
    assert!(vars.contains(&bindings[0].id));
    assert_eq!(s.helpers.len(), 2);
    assert_eq!(s.helpers[0].result_type.name.local, "integer");
    let Instruction::Choose {
        branches,
        otherwise,
        ..
    } = &s.helpers[0].body[0]
    else {
        panic!()
    };
    assert_eq!(branches.len(), 1);
    assert_eq!(otherwise.len(), 3);
    vars.clear();
    calls.clear();
    xpath::dependencies(
        &match &s.helpers[1].body[0] {
            Instruction::Sequence { expression } => expression.tree.clone(),
            _ => panic!(),
        },
        &mut vars,
        &mut calls,
    );
    assert!(vars.contains(&common.id));
}
#[test]
fn arbitrary_precision_lexemes_survive_json() {
    for (literal, kind) in [
        (
            "12345678901234567890123456789012345678901234567890",
            NumericKind::Integer,
        ),
        (
            "00000.1234567890123456789012345678901234567890",
            NumericKind::Decimal,
        ),
        ("1.0000e+999999999999999999999999", NumericKind::Double),
        (".00000000000000000000000000000000001", NumericKind::Decimal),
        ("42.", NumericKind::Decimal),
    ] {
        let tree = xpath::parse(literal, &context()).unwrap();
        assert_eq!(
            tree,
            Expr::Number {
                lexical_kind: kind,
                lexeme: literal.into()
            }
        );
        let json = serde_json::to_vec(&tree).unwrap();
        assert!(String::from_utf8_lossy(&json).contains(literal));
        assert_eq!(serde_json::from_slice::<Expr>(&json).unwrap(), tree);
    }
    assert_ne!(
        xpath::parse("0.1234567890123456789012345678901", &context()).unwrap(),
        xpath::parse("0.1234567890123456789012345678902", &context()).unwrap()
    );
}
#[test]
fn complete_trees_and_match_pattern_entry() {
    let c = context();
    for text in [
        "if (true()) then (for $i in 1 to 3 return if ($i > 1) then $i else 0) else 0",
        "some $i in (1,2), $j in $i satisfies every $k in $j satisfies $k castable as xs:integer",
        "/t:root/t:line[1]/@amount/ xs:decimal(.)",
        "ancestor::t:root/preceding-sibling::*[last() - 1] | self::t:*",
        "'it''s a string'",
        "- - 1 + 2 * 3",
        "(: comment (: nested :) :) t:*",
        "()",
        "*:code",
    ] {
        xpath::parse(text, &c).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
    for text in [
        "t:item[if (@x) then true() else false()]",
        "/t:root//t:* | t:item[@x = 'y']",
    ] {
        xpath::parse_pattern(text, &c).unwrap();
    }
    for text in [
        "1 + 2",
        "t:item/xs:decimal(.)",
        "ancestor::t:item",
        "t:a/(t:b | t:c)",
        "(t:a | t:b)/t:c",
        "(t:a/t:b)[1]",
        "some $x in 1 satisfies $x",
    ] {
        assert!(xpath::parse_pattern(text, &c).is_err(), "{text}");
    }
    let Expr::Binary {
        operator,
        left,
        right,
    } = xpath::parse("1 + 2 * 3", &c).unwrap()
    else {
        panic!()
    };
    assert_eq!(operator, "+");
    assert!(matches!(*left, Expr::Number { .. }));
    assert!(matches!(*right,Expr::Binary { operator,.. } if operator == "*"));
}
#[test]
fn rejects_unknown_syntax_functions_types_names() {
    for text in [
        "1 trailing",
        "1and 2",
        "t :item",
        "t: item",
        "$t :var",
        "child::count(.)",
        "1 cast as xs:integer[1]",
        "/[1]",
        "1e+",
        "'unclosed",
        "(1",
        "1 +",
        "foo:bar",
        "t:a[ ]",
        "for $i in 1 return",
        "if (1) then 2",
        "1 = 2 = 3",
        "1 to 2 to 3",
        "unknown-function()",
        "count(1,2)",
        "xs:decimal()",
        "1 cast as xs:madeUp",
        "1 cast as t:string",
        "$unknown",
        "1 instance of xs:integer",
        "t:a/@",
        "t:a ! 2",
    ] {
        let result = xpath::parse(text, &context())
            .and_then(|mut tree| xpath::resolve(&mut tree, &BTreeMap::new(), "fixture"));
        assert!(result.is_err(), "accepted {text}");
    }
    for t in [
        "xs:string junk",
        "xs:madeUp",
        "unknown:string",
        "xs:integer??",
    ] {
        assert!(xpath::parse_type(t, &context()).is_err(), "{t}");
    }
}
#[test]
fn strict_xml_include_and_static_failures() {
    let base = include_str!("fixtures/scope.sch");
    for (from, to) in [
        ("<title>", "<title unknown=\"x\">"),
        ("<title>", "<title xmlns=\"urn:unknown\">"),
        ("<name/>", "<name select=\".\"/>"),
        ("<name/>", "<unknown/>"),
        ("href=\"common.sch\"", "href=\"https://example.org/a\""),
        ("href=\"common.sch\"", "href=\"../../escape.sch\""),
        ("href=\"common.sch\"", "href=\"missing.sch\""),
        ("href=\"common.sch\"", "href=\"main.sch\""),
        ("pattern=\"second\"", "pattern=\"missing\""),
        (
            "<xsl:sequence select=\"0\"/>",
            "<xsl:copy-of select=\"0\"/>",
        ),
        ("select=\"u:recursive($local)\"", "select=\"u:recursive()\""),
        ("select=\"$later\"", "select=\"$local\""),
        ("value=\"$later\"", "value=\"$forward\""),
        ("select=\"$value - 1\"", "select=\"$local - 1\""),
        ("test=\"true()\"", "test=\"$local\""),
    ] {
        let text = base.replace(from, to);
        assert_ne!(text, base, "fixture replacement {from}");
        assert!(
            fixture(&text).is_err(),
            "accepted replacement {from} -> {to}"
        );
    }
    let error = fixture(&base.replace("test=\"true()\"", "test=\"1 &lt;\""))
        .unwrap_err()
        .to_string();
    assert!(error.contains("main.sch:"));
    assert!(error.contains("decoded XPath UTF-8 offset"));
    let resources = BTreeMap::from([("main.sch".into(), base.into())]);
    assert!(schematron::parse(&resources, "main.sch", None).is_err());
}
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        z.start_file(*name, zip::write::FileOptions::default())
            .unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}
#[test]
fn offline_archive_integrity_and_exact_inventory() {
    let bytes = archive(&[("a.sch", b"<a/>")]);
    let pin = source::Pin {
        file: "fixture.zip".into(),
        version: "1".into(),
        sha256: source::hash(&bytes),
        members: Some(BTreeMap::from([("a.sch".into(), source::hash(b"<a/>"))])),
    };
    assert!(source::verify_archive(bytes.clone(), pin.clone()).is_ok());
    let mut tampered = bytes.clone();
    tampered[0] ^= 1;
    assert!(source::verify_archive(tampered, pin.clone()).is_err());
    let mut wrong_member = pin.clone();
    wrong_member
        .members
        .as_mut()
        .unwrap()
        .insert("a.sch".into(), "0".repeat(64));
    assert!(source::verify_archive(bytes.clone(), wrong_member).is_err());
    for entries in [
        vec![("../a.sch", b"<a/>".as_slice())],
        vec![("a.sch", b"<a/>".as_slice()), ("a.sch", b"<a/>".as_slice())],
        vec![("a.sch", b"<a/>".as_slice()), ("b.sch", b"<b/>".as_slice())],
        vec![("b.sch", b"<b/>".as_slice())],
    ] {
        let bytes = archive(&entries);
        let mut p = pin.clone();
        p.sha256 = source::hash(&bytes);
        assert!(source::verify_archive(bytes, p).is_err());
    }
    let cross_archive = BTreeSet::from(["other/a.xsd".into()]);
    assert!(source::relative("archive/root.xsd", "../other/a.xsd", &cross_archive).is_err());
    for path in ["/a", "a/../b", "a\\b", "a:b", "a//b", "./a"] {
        assert!(source::safe_member(path).is_err());
    }
    let allowed = BTreeSet::from(["archive/schema/a.xsd".into()]);
    assert!(source::relative("archive/schema/root.xsd", "https://a/a.xsd", &allowed).is_err());
    assert!(source::relative("archive/schema/root.xsd", "../../outside", &allowed).is_err());
}
fn package(target: &str, revision: &str, schema: Schema) -> Package {
    Package {
        format: FORMAT.into(),
        target: target.into(),
        source_package: revision.into(),
        backend: BackendState::Unavailable,
        schemas: vec![schema],
        resources: BTreeMap::new(),
        source_graph: Vec::new(),
        policy: None,
        requirements: BTreeSet::new(),
    }
}
#[test]
fn exact_identities_coexist_and_fixture_encoding_is_deterministic() {
    let a = package(
        "fixture:one",
        "revision:1",
        fixture(include_str!("fixtures/scope.sch")).unwrap(),
    );
    let b = package("fixture:two", "revision:2", a.schemas[0].clone());
    let packages = [a, b];
    assert_eq!(
        select(&packages, "fixture:one", "revision:1")
            .unwrap()
            .target,
        "fixture:one"
    );
    assert_eq!(
        select(&packages, "fixture:two", "revision:2")
            .unwrap()
            .target,
        "fixture:two"
    );
    assert!(select(&packages, "fixture:one", "revision:2").is_err());
    assert!(select(&packages, "latest", "revision:1").is_err());
    assert!(select(
        &[packages[0].clone(), packages[0].clone()],
        "fixture:one",
        "revision:1"
    )
    .is_err());
    let first = validation_generator::emit::canonical(&packages[0]).unwrap();
    let second = validation_generator::emit::canonical(&package(
        "fixture:one",
        "revision:1",
        fixture(include_str!("fixtures/scope.sch")).unwrap(),
    ))
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(
        serde_json::from_slice::<Package>(&first).unwrap(),
        packages[0]
    );
}

#[test]
fn checked_in_fixture_is_current() {
    let mut p = package(
        "fixture:one",
        "revision:1",
        fixture(include_str!("fixtures/scope.sch")).unwrap(),
    );
    p.requirements = validation_generator::bundle::requirements(&p).unwrap();
    assert_eq!(
        validation_generator::emit::canonical(&p).unwrap(),
        include_bytes!("fixtures/inspection.json")
    );
}

#[test]
fn explicit_schema_closure_rejects_import_discovery_and_retains_cycles() {
    let root = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:one"><xs:import namespace="urn:two" schemaLocation="two.xsd"/></xs:schema>"#;
    let other = r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="urn:two"><xs:import namespace="urn:one" schemaLocation="one.xsd"/></xs:schema>"#;
    let resources = BTreeMap::from([
        ("fixture/one.xsd".into(), root.into()),
        ("fixture/two.xsd".into(), other.into()),
    ]);
    let edges = validation_generator::schema::graph(&resources, &BTreeMap::new()).unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].from, edges[1].to);
    for (from, to) in [
        ("two.xsd", "https://example.org/two.xsd"),
        ("two.xsd", "../../two.xsd"),
        ("two.xsd", "missing.xsd"),
        ("namespace=\"urn:two\"", "namespace=\"urn:unknown\""),
        ("schemaLocation=\"two.xsd\"", ""),
        ("xs:import", "xs:include"),
    ] {
        let mut changed = resources.clone();
        changed.insert("fixture/one.xsd".into(), root.replace(from, to));
        assert!(
            validation_generator::schema::graph(&changed, &BTreeMap::new()).is_err(),
            "{to}"
        );
    }
    let mut locationless = resources;
    locationless.insert(
        "fixture/one.xsd".into(),
        root.replace(" schemaLocation=\"two.xsd\"", ""),
    );
    let mappings = BTreeMap::from([(
        ("fixture/one.xsd".into(), "urn:two".into()),
        "fixture/two.xsd".into(),
    )]);
    assert!(validation_generator::schema::graph(&locationless, &mappings).is_ok());
}

#[test]
fn global_cycle_through_helper_and_expression_initializer_scope() {
    let text = include_str!("fixtures/scope.sch")
        .replace("value=\"$later\"", "value=\"u:unused()\"")
        .replace("select=\"$later\"", "select=\"$forward\"");
    assert!(fixture(&text)
        .unwrap_err()
        .to_string()
        .contains("global initialization cycle"));
    let mut tree = xpath::parse("for $x in $x return $x", &context()).unwrap();
    assert!(xpath::resolve(&mut tree, &BTreeMap::new(), "fixture").is_err());
    let text = include_str!("fixtures/scope.sch").replace(
        "queryBinding=\"xslt2\"",
        "queryBinding=\"xslt2\" defaultPhase=\"other\"",
    );
    let parsed = fixture(&text).unwrap();
    assert_eq!(parsed.declared_default_phase.as_deref(), Some("other"));
    assert_eq!(parsed.selection, "chosen");
}

#[test]
fn parser_budgets_fail_without_truncating_a_tree() {
    let chain = std::iter::repeat("1")
        .take(100)
        .collect::<Vec<_>>()
        .join(" + ");
    assert!(xpath::parse(&chain, &context())
        .unwrap_err()
        .to_string()
        .contains("budget"));
    let nested = format!("{}1{}", "(".repeat(200), ")".repeat(200));
    assert!(xpath::parse(&nested, &context()).is_err());
    let sequence = std::iter::repeat("1")
        .take(3000)
        .collect::<Vec<_>>()
        .join(",");
    assert!(xpath::parse(&sequence, &context()).is_err());
}
