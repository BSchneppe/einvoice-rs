use einvoice_native_runtime::{
    Arithmetic as A, Atomic, AtomicType as T, Comparison as C, Error, Failure, Fault, Limits,
    NumericKind, Scope,
};
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
fn text(s: &mut Scope, a: &Atomic) -> String {
    s.render(a).unwrap().as_str().to_owned()
}
#[test]
fn tags_literals_and_exact_operations_do_not_use_binary_money() {
    let mut s = scope();
    let i = s
        .literal(&NumericKind::Integer, "9007199254740993")
        .unwrap();
    let d = s
        .literal(&NumericKind::Decimal, "9007199254740993.0")
        .unwrap();
    let binary = s
        .literal(&NumericKind::Double, "9007199254740993e0")
        .unwrap();
    assert_eq!(i.atomic_type(), T::Integer);
    assert_eq!(d.atomic_type(), T::Decimal);
    assert!(s.numeric_compare(C::Equal, &d, &binary).unwrap()); // Required double promotion.
    assert_eq!(text(&mut s, &i), "9007199254740993");
    let zero = s.lexical(T::Integer, "0").unwrap();
    assert_eq!(
        s.arithmetic(A::Add, &i, &zero).unwrap().atomic_type(),
        T::Integer
    );
    assert_eq!(
        s.arithmetic(A::Add, &d, &zero).unwrap().atomic_type(),
        T::Decimal
    );
    assert_eq!(
        s.arithmetic(A::Divide, &i, &i).unwrap().atomic_type(),
        T::Decimal
    );
    assert_eq!(
        s.arithmetic(A::IntegerDivide, &d, &i)
            .unwrap()
            .atomic_type(),
        T::Integer
    );
    let rounded = s.round(&d, 0).unwrap();
    assert_eq!(rounded.atomic_type(), T::Decimal);
    let large = format!("{}.01", "9".repeat(1024));
    let a = s.lexical(T::Decimal, &large).unwrap();
    let b = s.lexical(T::Decimal, &"9".repeat(1024)).unwrap();
    let c = s.arithmetic(A::Subtract, &a, &b).unwrap();
    assert_eq!(text(&mut s, &c), "0.01");
    let product = s.arithmetic(A::Multiply, &a, &c).unwrap();
    let quotient = s.arithmetic(A::IntegerDivide, &product, &c).unwrap();
    assert_eq!(text(&mut s, &quotient), "9".repeat(1024));
}
#[test]
fn promotion_asymmetric_casts_and_nan_six_relations() {
    let mut s = scope();
    let f = s.lexical(T::Float, "16777216").unwrap();
    let one = s.lexical(T::Integer, "1").unwrap();
    let sum = s.arithmetic(A::Add, &f, &one).unwrap();
    assert_eq!(sum.atomic_type(), T::Float);
    assert!(s.numeric_compare(C::Equal, &sum, &f).unwrap());
    let n = s.lexical(T::Double, "NaN").unwrap();
    let z = s.lexical(T::Decimal, "0").unwrap();
    for (op, want) in [
        (C::Equal, false),
        (C::NotEqual, true),
        (C::Less, false),
        (C::LessEqual, false),
        (C::Greater, false),
        (C::GreaterEqual, false),
    ] {
        assert_eq!(s.numeric_compare(op, &n, &z).unwrap(), want);
    }
    let untyped = s.lexical(T::UntypedAtomic, "9007199254740993").unwrap();
    let n = s.number(Some(&untyped)).unwrap();
    assert_eq!(text(&mut s, &n), "9.007199254740992E15");
    let cast = s.cast(&untyped, T::Decimal).unwrap();
    assert_eq!(text(&mut s, &cast), "9007199254740993");
    let arithmetic = s.arithmetic(A::Add, &untyped, &z).unwrap();
    assert_eq!(arithmetic.atomic_type(), T::Double);
    assert_eq!(
        s.numeric_compare(C::Equal, &untyped, &z),
        Err(Error::XPath(Fault::Xpty0004))
    );
    let string = s.lexical(T::String, "1").unwrap();
    assert!(matches!(
        s.arithmetic(A::Add, &string, &one),
        Err(Error::XPath(Fault::Xpty0004))
    ));
    let d = s.lexical(T::Double, "0.1").unwrap();
    let exact = s.cast(&d, T::Decimal).unwrap();
    assert_eq!(
        text(&mut s, &exact),
        "0.1000000000000000055511151231257827021181583404541015625"
    );
    let f = s.cast(&d, T::Float).unwrap();
    let back = s.cast(&f, T::Double).unwrap();
    assert_ne!(back.double_bits(), d.double_bits());
}
#[test]
fn malformed_nonfinite_and_number_have_different_error_channels() {
    let mut s = scope();
    for bad in ["", "1e2", "1_0", "\u{a0}1", "NaN", "INF", "+.", "١"] {
        assert!(matches!(
            s.lexical(T::Decimal, bad),
            Err(Error::XPath(Fault::Forg0001))
        ));
    }
    for bad in ["bad", "", "\u{a0}1", "+NaN", "Infinity", "0x1p0"] {
        let string = s.lexical(T::String, bad).unwrap();
        let result = s.number(Some(&string)).unwrap();
        assert_eq!(text(&mut s, &result), "NaN");
    }
    let absent = s.number(None).unwrap();
    assert_eq!(text(&mut s, &absent), "NaN");
    let string = s.lexical(T::String, "false").unwrap();
    assert_eq!(s.cast(&string, T::Boolean).unwrap().boolean(), Some(false));
    for nonfinite in ["NaN", "INF", "-INF"] {
        let x = s.lexical(T::Double, nonfinite).unwrap();
        assert!(matches!(
            s.cast(&x, T::Decimal),
            Err(Error::XPath(Fault::Foca0002))
        ));
        assert!(!s.castable(&x, T::Integer).unwrap());
    }
    for ty in ["int", "anyAtomicType", "date", "QName", "duration"] {
        assert!(matches!(
            Scope::resolve_type("http://www.w3.org/2001/XMLSchema", ty),
            Err(Error::Fatal(Failure::Capability(_)))
        ));
    }
}
#[test]
fn signed_zero_and_binary_boundaries() {
    let mut s = scope();
    for t in [T::Float, T::Double] {
        let z = s.lexical(t, "-1e-999999999999999999999999").unwrap();
        assert_eq!(text(&mut s, &z), "-0");
        let abs = s.abs(&z).unwrap();
        assert_eq!(text(&mut s, &abs), "0");
        let neg = s.negate(&z).unwrap();
        assert_eq!(text(&mut s, &neg), "0");
        let r = s.round(&z, 0).unwrap();
        assert_eq!(text(&mut s, &r), "-0");
        let one = s.lexical(T::Integer, "1").unwrap();
        let reciprocal = s.arithmetic(A::Divide, &one, &r).unwrap();
        assert_eq!(text(&mut s, &reciprocal), "-INF");
        let dec = s.cast(&z, T::Decimal).unwrap();
        assert_eq!(text(&mut s, &dec), "0");
    }
    let sub = s.lexical(T::Double, "4.9e-324").unwrap();
    assert_eq!(sub.double_bits(), Some(1));
    let dec = s.cast(&sub, T::Decimal).unwrap();
    let back = s.cast(&dec, T::Double).unwrap();
    assert_eq!(back.double_bits(), Some(1));
    let max = s.lexical(T::Double, "1.7976931348623157e308").unwrap();
    let two = s.lexical(T::Integer, "2").unwrap();
    let inf = s.arithmetic(A::Multiply, &max, &two).unwrap();
    assert_eq!(text(&mut s, &inf), "INF");
}
#[test]
fn exact_division_boundaries_and_signed_quotient_identity() {
    let mut s = scope();
    for (a, b) in [
        ("-5.5", "2"),
        ("5.5", "-2"),
        ("-5.5", "-2"),
        ("1.99999999999999999999999999999", "2"),
    ] {
        let a = s.lexical(T::Decimal, a).unwrap();
        let b = s.lexical(T::Decimal, b).unwrap();
        let q = s.arithmetic(A::IntegerDivide, &a, &b).unwrap();
        let r = s.arithmetic(A::Modulo, &a, &b).unwrap();
        let p = s.arithmetic(A::Multiply, &q, &b).unwrap();
        let sum = s.arithmetic(A::Add, &p, &r).unwrap();
        assert!(s.numeric_compare(C::Equal, &a, &sum).unwrap());
    }
    let two = s.lexical(T::Integer, "2").unwrap();
    let three = s.lexical(T::Integer, "3").unwrap();
    let div = s.arithmetic(A::Divide, &two, &three).unwrap();
    let times = s.arithmetic(A::Multiply, &div, &three).unwrap();
    assert_eq!(text(&mut s, &times), "2.000000000000000001");
    let hundred = s.lexical(T::Decimal, "100.000").unwrap();
    let d = s.arithmetic(A::Divide, &hundred, &three).unwrap();
    assert_eq!(text(&mut s, &d), "33.333333333333333333");
}
