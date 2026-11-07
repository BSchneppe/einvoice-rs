use einvoice_native_runtime::{Arithmetic, AtomicType, Error, Limits, Scope};

fn decode(s: &str) -> String {
    assert_eq!(s.len() % 2, 0);
    String::from_utf8(
        s.as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
fn kind(s: &str) -> AtomicType {
    Scope::resolve_type("http://www.w3.org/2001/XMLSchema", s).unwrap()
}
fn scope() -> Scope {
    Scope::new(
        Limits::new(
            16384,
            65536,
            8192,
            100_000_000_000,
            100_000_000,
            64_000_000,
            16_000_000,
            1024,
            32_000_000,
        )
        .unwrap(),
    )
}
fn check_fixture(fixture: &str, expected_count: usize) {
    let mut failures = vec![];
    let mut count = 0;
    for line in fixture.lines().filter(|s| !s.starts_with('#')) {
        let f: Vec<_> = line.split('\t').collect();
        assert_eq!(f.len(), 12);
        let mut s = scope();
        let a = decode(f[4]);
        let b = decode(f[6]);
        let run = |s: &mut Scope| -> einvoice_native_runtime::Result<(AtomicType, String)> {
            let a = if f[2] == "literal" {
                let kind = match f[3] {
                    "integer" => einvoice_native_runtime::NumericKind::Integer,
                    "decimal" => einvoice_native_runtime::NumericKind::Decimal,
                    "double" => einvoice_native_runtime::NumericKind::Double,
                    _ => panic!("literal kind"),
                };
                s.literal(&kind, &a)?
            } else {
                s.lexical(kind(f[3]), &a)?
            };
            let result = match f[2] {
                "lex" | "literal" => a,
                "cast" => s.cast(&a, kind(f[7]))?,
                "castable" => {
                    let result = s.castable(&a, kind(f[7]))?;
                    s.lexical(AtomicType::Boolean, if result { "true" } else { "false" })?
                }
                "number" => s.number(Some(&a))?,
                "round" => s.round(&a, f[7].parse().unwrap())?,
                op => {
                    let b = s.lexical(kind(f[5]), &b)?;
                    let op = match op {
                        "+" => Arithmetic::Add,
                        "-" => Arithmetic::Subtract,
                        "*" => Arithmetic::Multiply,
                        "div" => Arithmetic::Divide,
                        "idiv" => Arithmetic::IntegerDivide,
                        "mod" => Arithmetic::Modulo,
                        _ => panic!("fixture op"),
                    };
                    s.arithmetic(op, &a, &b)?
                }
            };
            Ok((
                result.atomic_type(),
                s.render(&result)?.as_str().to_string(),
            ))
        };
        let actual = run(&mut s);
        let expected = decode(f[10]);
        let valid = match &actual {
            Ok((t, v)) => {
                f[8] == "OK"
                    && f[9] == format!("{{http://www.w3.org/2001/XMLSchema}}{}", t.local_name())
                    && *v == expected
            }
            Err(Error::XPath(fault)) => {
                f[8] == "ERROR"
                    && f[9]
                        == format!(
                            "{{{}}}{}",
                            einvoice_native_runtime::XQT_ERRORS,
                            fault.local_name()
                        )
            }
            _ => false,
        };
        if !valid {
            failures.push(format!(
                "{}: {actual:?}, expected {} {} {:?}",
                f[0], f[8], f[9], expected
            ));
        }
        count += 1;
    }
    assert_eq!(count, expected_count);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pinned_development_scalar_operations() {
    check_fixture(include_str!("fixtures/development.tsv"), 162);
}
#[test]
fn independent_synthetic_binary_rendering_sweep() {
    check_fixture(include_str!("fixtures/rendering.tsv"), 228);
}

#[test]
fn independent_binary_integer_cast_and_division_boundaries() {
    check_fixture(include_str!("fixtures/boundaries.tsv"), 105);
}

#[test]
fn independently_replayed_rendering_and_cast_profile_corrections() {
    check_fixture(include_str!("fixtures/corrections.tsv"), 134);
}

#[test]
fn independently_replayed_subnormal_decimal_selection() {
    check_fixture(include_str!("fixtures/subnormal.tsv"), 96);
}
