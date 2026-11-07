#![allow(dead_code)]
use einvoice_native_runtime::machine::{EntryFocus, MachineLimits, Prepared, Run};
use einvoice_native_runtime::{Limits, NumericKind};
use einvoice_source::SourceDocument;
use validation_ir::{Expr, ExprBinding, Expression, Location, Name, NodeTest, SequenceType};
pub const FN: &str = "http://www.w3.org/2005/xpath-functions";
pub const XS: &str = "http://www.w3.org/2001/XMLSchema";
pub fn name(ns: &str, local: &str) -> Name {
    Name {
        qualified: local.into(),
        namespace: ns.into(),
        local: local.into(),
    }
}
pub fn n(s: &str) -> Expr {
    Expr::Number {
        lexical_kind: NumericKind::Integer,
        lexeme: s.into(),
    }
}
pub fn decimal(s: &str) -> Expr {
    Expr::Number {
        lexical_kind: NumericKind::Decimal,
        lexeme: s.into(),
    }
}
pub fn s(value: &str) -> Expr {
    Expr::String {
        value: value.into(),
    }
}
pub fn seq(items: Vec<Expr>) -> Expr {
    Expr::Sequence { items }
}
pub fn call(local: &str, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        name: name(FN, local),
        arguments,
    }
}
pub fn cast(local: &str, operand: Expr) -> Expr {
    Expr::Call {
        name: name(XS, local),
        arguments: vec![operand],
    }
}
pub fn bin(operator: &str, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        operator: operator.into(),
        left: Box::new(left),
        right: Box::new(right),
    }
}
pub fn step(axis: &str, local: &str) -> Expr {
    Expr::Step {
        axis: axis.into(),
        test: if local == "*" {
            NodeTest::Any
        } else if local == "node()" || local == "text()" {
            NodeTest::Kind {
                name: local[..local.len() - 2].into(),
            }
        } else {
            NodeTest::Name {
                name: name("", local),
            }
        },
    }
}
pub fn path(parts: &[&str]) -> Expr {
    parts
        .iter()
        .fold(Expr::Root, |base, part| bin("/", base, step("child", part)))
}
pub fn filter(base: Expr, predicate: Expr) -> Expr {
    Expr::Filter {
        base: Box::new(base),
        predicate: Box::new(predicate),
    }
}
pub fn group(operand: Expr) -> Expr {
    Expr::Group {
        operand: Box::new(operand),
    }
}
pub fn cond(condition: Expr, then: Expr, otherwise: Expr) -> Expr {
    Expr::If {
        condition: Box::new(condition),
        then: Box::new(then),
        otherwise: Box::new(otherwise),
    }
}
pub fn var(id: &str) -> Expr {
    Expr::Variable {
        name: name("", id),
        binding: id.into(),
    }
}
pub fn bind(operator: &str, bindings: Vec<(&str, Expr)>, result: Expr) -> Expr {
    Expr::Bind {
        operator: operator.into(),
        bindings: bindings
            .into_iter()
            .map(|(id, value)| ExprBinding {
                id: id.into(),
                name: name("", id),
                value,
            })
            .collect(),
        result: Box::new(result),
    }
}
pub fn typeop(operator: &str, operand: Expr, local: &str, occurrence: Option<&str>) -> Expr {
    Expr::TypeOp {
        operator: operator.into(),
        operand: Box::new(operand),
        sequence_type: SequenceType {
            name: name(XS, local),
            occurrence: occurrence.map(str::to_owned),
        },
    }
}
pub fn bad() -> Expr {
    cast("integer", path(&["r", "bad"]))
}
pub fn location() -> Location {
    Location {
        resource: "independent-machine-probe".into(),
        xml_byte_start: 0,
        xml_byte_end: 1,
        line: 1,
        column: 1,
        attribute: Some("select".into()),
    }
}
pub fn expression(tree: Expr) -> Expression {
    Expression {
        text: "authored AST (no test parser)".into(),
        source: location(),
        tree,
    }
}
pub fn machine_limits() -> MachineLimits {
    MachineLimits {
        objects: 500_000,
        live_bytes: 100_000_000,
        total_bytes: 500_000_000,
        frames: 100_000,
        work: 100_000_000,
    }
}
pub fn atomic_limits() -> Limits {
    Limits::new(
        16384,
        65536,
        8192,
        100_000_000_000,
        100_000_000,
        64_000_000,
        16_000_000,
        100_000,
        500_000_000,
    )
    .unwrap()
}
pub fn evaluate(tree: Expr) -> einvoice_native_runtime::Result<Vec<String>> {
    let e = expression(tree);
    let p = Prepared::new(&e, &[], machine_limits())?;
    let d = SourceDocument::parse(include_bytes!("../evaluator-fixtures/document.xml")).unwrap();
    let mut r = Run::new(
        &p,
        &d,
        EntryFocus::Document,
        atomic_limits(),
        machine_limits(),
        1_000_000,
        1_000_000,
    )?;
    let c = r.cursor()?;
    let mut out = vec![];
    while let Some(i) = r.next(c)? {
        out.push(r.render(i)?.as_str().into());
    }
    Ok(out)
}
pub fn decode(s: &str) -> String {
    String::from_utf8(
        s.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
